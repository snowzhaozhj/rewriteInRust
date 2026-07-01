# MDR-013: danger 信号落入 migration-state（批次 C1）

- **状态**: 已决策
- **日期**: 2026-06-29
- **范围**: M3 遗留债批次 C1 —— `state populate-modules` 把 `classify_file()` 产出的 danger 类别透传进 `migration-state.json` 的 `ModuleState.danger`，为 plugin 后续消费（独立的 C2）打数据层地基。**仅改 CLI/core/docs，不碰 `plugin/`。**

## 背景

`lang::classify_file()` 已产出 `FileClassification{ file_kind, danger: Vec<DangerCategory> }`，6 类危险信号（`NumericPrecision`/`Concurrency`/`DynamicReflection`/`IoSideEffect`/`Ffi`/`SharedMutableGlobal`，见 `lang/mod.rs`）。但 `cmd_state_populate_modules` 此前**只用 `file_kind` 做 Batch/CoupledBatch 分流，`danger` 被丢弃**——下游（plugin/translator 的规则注入、定向测试）拿不到 danger 信号。

设计权威：`decomposition-redesign.md §(b)`（危险信号独立于机械门，命中即注入规则 + 加定向测试）/ PLAN-M3 `DEC-01`（机械/危险分类）。本批次只补数据层透传，不动分类逻辑。

## 决策

### 决策 1：state 只落**原始 danger 类别**，不落 concern 文案、不做 RULE 映射

`ModuleState.danger` 存 `DangerCategory` 的原始 snake_case 名（如 `"numeric_precision"`），**不**存 `concern()` 人读文案、**不**做 `RULE-NN` 映射。理由：concern 文案与规则映射会随规则目录演进而漂移，固化在核心层是负债；规则注入由 translator 依完整规则目录决定（对齐 `DangerCategory::concern` 注释立场「不在核心层固化可能漂移的规则映射」）。plugin/C2 消费原始类别名自行映射。

### 决策 2：字段形态用 `Vec<String>`（而非 `Vec<DangerCategory>`）

> **⚠️ 已被 M4-DEBT-02 取代（PR #57）**：形态改为 `Vec<DangerCategory>`（枚举上移 `types::common` 后无双向依赖问题，见文末「后续 TODO」收口）。下文是 C1 当时的务实取舍记录。

`ModuleState.danger: Vec<String>`，`#[serde(default, skip_serializing_if = "Vec::is_empty")]`。

- **为何不用 `Vec<DangerCategory>`**：`DangerCategory` 定义在 `crate::lang`，而 `lang` 已依赖 `crate::types::state`（`use ...::state::ModuleTier`）。让 `types::state` 反向依赖 `lang::DangerCategory` 会在模块间形成 `types ↔ lang` 双向依赖（Rust intra-crate 虽不报错，但破坏「types 是下层、不依赖 lang」的分层）。用 `String` 保持依赖单向（`lang → types`），types 层零 lang 依赖。
- **稳定映射**：给 `DangerCategory` 新增 `as_str() -> &'static str` 返回 snake_case 名（与 `#[serde(rename_all = "snake_case")]` 序列化一致），CLI 用它把类别透传为字符串。单点定义，避免散落字面量漂移。

### 决策 3：组并集 = 成员 danger 的并集（去重 + 字典序）

> **⚠️ 实现细节已被 M4-DEBT-02 更新（PR #57）**：去重改用 `BTreeSet<DangerCategory>`（按枚举身份去重），再 `sort_by_key(as_str())` 重排为字符串字典序——输出顺序与旧 `BTreeSet<String>` 字节级一致。语义不变（去重 + 字典序 + 确定性）。

composite 组的 `danger` = 各成员 `classify_file().danger` 的并集，去重 + 稳定字典序排序（用 `BTreeSet<String>`）。单文件模块 = 自身 danger。保证断点续传/重填的确定性输出。

### 决策 4：读失败文件 danger 保守视为空

源文件读取失败时（已有 `read_failures` 计数 + 高占比硬门禁，见 [MDR-012](./012-m3-debt-batch-a-deviations.md) 决策 1），该文件 danger 按空处理（不写 `danger_map`），不影响组并集。保守侧：宁可漏标也不虚构危险信号。

### 决策 5：仅 decompose 路径透传，`--no-decompose` 旧路径恒空

danger 收集复用 decompose 路径已有的「逐文件读源 + `classify_file`」循环（`lib.rs` populate 的 `file_kinds`/`self_sizes` 计算处），零额外 IO。`--no-decompose` 旧路径（SCC-only）不读源、不创建 adapter，故 `danger` 恒为空。理由：`--no-decompose` 是已弃用的兼容回退（默认走 decompose）；为它单独补一套读源/分类设施会引入重复 IO 与新的 `read_failures` 行为分支，得不偿失。如未来需要，可在 C2 之后单列任务补齐。

## 影响

- **CLI/state schema 新增**：`ModuleState.danger`（`string[]`，默认 `[]` 省略）；`state populate-modules`（默认 decompose）与 `state get` 输出含该字段。向后兼容（`serde(default)`，旧 state 文件无此字段时为空）。
- **core 新增**：`DangerCategory::as_str()`（snake_case 稳定标识）。
- **设计文档同步**：`docs/design/09-appendix-schemas.md` 的 ModuleState 字段说明补 `danger`。
- **不涉及 plugin**：plugin 消费 danger 是独立的 C2，本批次不动 `plugin/`。
- **测试**：新增 e2e `e2e_populate_danger_signals_into_state`——两个耦合 Python 文件分别触发 `numeric_precision`/`concurrency`，断言组 danger 为成员并集（去重 + 字典序）且经 `state get` serde 往返一致；新增 `as_str_matches_serde_snake_case` 单测锁死 `as_str()` 与 serde `rename_all` 一致性（防类别名漂移，C1 审查共识）。

## 后续 TODO（C1 审查类型设计视角，非阻塞）

> **M4-DEBT 收口（2026-06-30，PR #57）**：下列三项已在 M4 Sprint A 全部落地。决策 2（`Vec<String>` 形态）与决策 3（`BTreeSet<String>` 并集）已被 M4-DEBT-02 取代——形态改为 `Vec<DangerCategory>`、并集用 `BTreeSet<DangerCategory>` 去重后按 `as_str()` 字典序重排（保持输出顺序与旧版字节级一致）。

- ~~**`DangerCategory` 候选上移 `types` 层以恢复编译期值域安全**~~ → ✅ **M4-DEBT-02**：枚举上移 `types::common`，`ModuleState.danger: Vec<DangerCategory>`，保持 `lang → types` 单向依赖（`lang` 改 `use types::common::DangerCategory`）。加 `Deserialize` + `#[serde(other)] Unknown` 兜底（旧/跨版本 state 不硬失败）。
- ~~**`io_side_effect` 无对应 RULE**~~ → ✅ **M4-DEBT-01**：裁定**并入 RULE-10（标准库函数映射）的 IO 子节**，不新开 RULE（避免规则膨胀，保持 26 类目录）。io 横跨 RULE-3（错误处理）/RULE-10（标准库），独特关注点（副作用顺序/可见性/隔离）作为 RULE-10 IO 子节。TS/Python porting-template 补「标准库 IO 映射」节；translator.md 定向表 io 行改指 RULE-10。
- ~~**RULE-6/12/15 仅命名、M2 才完整展开**~~ → ✅ **M4-DEBT-03**：concurrency/ffi/shared_mutable_global 三类在 TS/Python porting-template 完整展开（并发模式/unsafe 使用策略/全局状态处理三节，含映射表 + 陷阱清单）；各模板 frontmatter bump `rule_version`。
