# 项目状态快照

> 每次会话结束前更新。新会话读此文件 → 找到 PLAN.md 对应任务 → 继续执行。

## 当前位置

- **Milestone**: M1 ✅ → M2 ✅ → **M3 多语言支持（Python 优先）✅ 全部完成** → **M3 遗留债清理（M4 地基）✅ 完成（2026-06-30）** → **M4「完善」执行中（双主线配比已拍板：双主线并行）——Sprint A 债务收口+Go前置 ✅ 完成（2026-06-30，待 PR 审查）**
- **M3 收尾（2026-06-29）**：Sprint A/B/C/D/E 全部合并，验收 M3-VAL-01~08 全达标；PR [#49](https://github.com/snowzhaozhj/rewriteInRust/pull/49)（ffi 测试修复）+ [#52](https://github.com/snowzhaozhj/rewriteInRust/pull/52)（source_root 探测加固）已合并；遗留 issue [#50](https://github.com/snowzhaozhj/rewriteInRust/issues/50)（source_root 推断）+ [#51](https://github.com/snowzhaozhj/rewriteInRust/issues/51)（VAL-05 性能实测：TS 路径 0%/-16%/-1% 无退化）已 CLOSED+COMPLETED；PLAN-M3 验收清单已全部回填 [x]。
- **阶段**: Sprint A ✅ → Sprint B ✅ → Sprint C ✅ → Sprint E ✅ → **Sprint D 端到端验收 ✅（M3-VAL-01~08 全达标，2026-06-29，PR [#49](https://github.com/snowzhaozhj/rewriteInRust/pull/49) 已合并——4 视角审查全跑、1 important（设计文档同步）+ 4 nit 全落实、just ci 532 绿）**
- **🟢 Sprint D 端到端验收 ✅**：2 真实 Python 项目各 ≥1 模块迁移到 done（按 §6 headless 规范）。
  - **VAL-02 jmespath**：2 模块全 done（coupled_batch 7 文件 + visitor.py），**902 黄金集 901 等价 + 1 豁免（D-10）**，端到端 `search()` 全链；独立复核 cargo test/clippy --all-targets 全绿。
  - **VAL-03 textdistance**：base.py 组（编辑距离算法）done，golden_edit_seq 70/70 等价；vector_based 草稿态忠实保留 unimplemented!()。
  - **VAL-04 差异测试**：golden 套件落地（源引擎录制→Rust 逐条断言），两项目实证。
  - **VAL-06 graduate**：jmespath 毕业成功 + textdistance 正确拒绝未完成。**VAL-08**：just ci 全绿。
  - **暴露并修复 4 项真实工具缺口**：① stats compare 支持 Python 源（补完 deferred M3）② scaffolder golden harness present-null 区分 ③ translator 加 Edit 工具防 Phase B Write 截断 ④ verify.sh done 门补全量集成测试 + --all-targets clippy。详见 `docs/sprint-d-acceptance.md`。
  - TODO 落账：ffi.rs 测试 deprecated（✅ 已修，PR #49）；analyzer source_root 推断加固 → [issue #50](https://github.com/snowzhaozhj/rewriteInRust/issues/50)；VAL-05 性能实测 → [issue #51](https://github.com/snowzhaozhj/rewriteInRust/issues/51)。
- **🟢 M3-DEC-02 轻量翻译路径 ✅**（PR [#46](https://github.com/snowzhaozhj/rewriteInRust/pull/46)，2026-06-28 已合并）：run.md 机械合批组轻量路径实现。
- **🟢 M3-DEC coupled_batch 分流修复 ✅**（PR [#48](https://github.com/snowzhaozhj/rewriteInRust/pull/48)，2026-06-28 已合并）：修复 populate 把非机械 batch 展开成独立模块、推翻 decompose 分组的接口断裂（与 MDR-011 §6 矛盾）。grilling + codex 双审收敛后实施：
  - **新增 `CompositeKind::CoupledBatch`**：`Batch` 收窄为全机械（轻量路径，编译即门禁）；`CoupledBatch`=含逻辑耦合簇（完整组路径：翻译→结构门→Phase B→行为测试→审查）。populate 保留 `classify_file` 按成员机械性分流（读失败保守落 CoupledBatch）。
  - Plugin 文档：run.md 新增「CoupledBatch 组完整路径」+ 形态/路由分支；translator.md 新增「CoupledBatch 组翻译」；workflow.md 修正「多文件=SCC」分派为按 `composite_kind` 分流（codex 标的真风险）；analyze.md 同步三类 composite 说明。
  - 测试：衔接测试改断言 coupled_batch + 组感知 `state deps`；新增 py-pkg-deps 混合簇保留为 1 个 coupled_batch 回归测试；orphan/active-progress 测试 pin `--no-decompose`（保留旧路径回归）。
  - 验证：`just ci` 全绿；jmespath 真实场景 8 文件→2 模块（1 coupled_batch[7]+1 single），符合预期。
  - 计划文档：`docs/plan-populate-batch-unify.md`（含 grilling 决策记录 + codex 8 条补充）。
  - 审查：4 视角全跑（主审/设计契约/专项 4 agent/异构交叉）。本次引入项全修：枚举头注释「两种→三种」、09-schema 补 `coupled_batch`、补全机械 Batch 回归测试（新增 `fixtures/ts-mechanical-batch` + `e2e_populate_all_mechanical_cluster_is_batch`）、MDR-011 §8 偏离回链、member_files/decomposition_frozen 注释更新、`all_mechanical` debug_assert、`--human` 覆盖回补、deps 断言强化。
  - TODO 落账（pre-existing，独立 PR）：① danger→RULE/定向测试注入（跨路径既有缺口）；② `graph topo-sort --members --reverse`；③ `read_failures` 缺阈值硬门禁——全/高比例读失败时静默产出退化 plan（PLG-06 既有，CoupledBatch 路由会放大影响）；④ `state transition` 不做非代表成员 key 组归一（与 `state deps` 不对称）；⑤ 默认 decompose 路径下「组缩小/整组消失」的孤儿清理无回归覆盖。
- **🟢 PLG-06 populate-modules 接入 decompose ✅**（PR [#47](https://github.com/snowzhaozhj/rewriteInRust/pull/47)，2026-06-28 已合并）：`populate-modules` 消费 `plan_decomposition` 产出，写 `migration-state.json`（`composite_kind` + `member_files` + `decomposition_frozen`）。新增 `--budget`/`--no-decompose` 参数。（注：原「含 non-mechanical 成员展开为独立模块」行为已由上方 M3-DEC coupled_batch 修复推翻。）
- **MDR-011 ✅ 已合并（PR [#45](https://github.com/snowzhaozhj/rewriteInRust/pull/45)，2026-06-28）**：目录优先两阶段凝聚合并。10 真实项目均值 ~76% 缩减。
- **Sprint E ✅ 全部完成**：DEC-01（PR #43）+ DEC-GATE（Python 分类器修复）+ DEC-02（PR #46）。
- **测试基线**: 552 测试 / clippy -D / deny / fmt / shellcheck + plugin validate 全绿
- **CI 覆盖率**: 待更新
- **最新合并 PR**: [#55](https://github.com/snowzhaozhj/rewriteInRust/pull/55)（danger→规则注入闭环 C1+C2，MDR-013）；[#54](https://github.com/snowzhaozhj/rewriteInRust/pull/54)（decompose 代表漂移孤儿回归）；[#53](https://github.com/snowzhaozhj/rewriteInRust/pull/53)（CLI 三连 + MDR-012）；[#48](https://github.com/snowzhaozhj/rewriteInRust/pull/48)（CoupledBatch 分流修复）；[#47](https://github.com/snowzhaozhj/rewriteInRust/pull/47)（PLG-06 populate 接入 decompose）；[#46](https://github.com/snowzhaozhj/rewriteInRust/pull/46)（DEC-02 轻量翻译路径）；[#45](https://github.com/snowzhaozhj/rewriteInRust/pull/45)（MDR-011 凝聚合并）；[#43](https://github.com/snowzhaozhj/rewriteInRust/pull/43)（DEC-01 拆解引擎）；[#42](https://github.com/snowzhaozhj/rewriteInRust/pull/42)（M3-VAL-07 §11.2 两文件契约同步）
- **开放 PR**: 无

### M3 遗留债清理（为 M4 打地基）✅ 完成（2026-06-30）

**目标（用户 2026-06-29 设定）**：完成 M3 全部任务并达到验收标准（✅），清理 pre-existing 工程债，为 M4 打好坚实地基（✅ 5 项全清）。

5 项 CoupledBatch pre-existing 工程债全部清理 + 审查 + 合并：

| 项 | 内容 | PR | 关键决策 |
|----|------|----|---------|
| ③ read_failures 硬门禁 | 占比 ≥50% 阻断全 0-size 退化 plan | [#53](https://github.com/snowzhaozhj/rewriteInRust/pull/53) | MDR-012 |
| ② topo-sort 参数 | **撤 --members**（违反「破环不在此命令」冻结契约）、新增 --reverse | [#53](https://github.com/snowzhaozhj/rewriteInRust/pull/53) | MDR-012：组感知顺序归 populate |
| ④ transition 组归一 | 复用 state deps 的 member_files 归一 | [#53](https://github.com/snowzhaozhj/rewriteInRust/pull/53) | — |
| ⑤ 孤儿清理回归 | 默认 decompose 代表漂移孤儿 e2e | [#54](https://github.com/snowzhaozhj/rewriteInRust/pull/54) | — |
| ① danger→规则注入 | CLI 落 state + plugin 消费闭环 | [#55](https://github.com/snowzhaozhj/rewriteInRust/pull/55) | MDR-013：state 只落原始类别，RULE 映射归 translator |

- **审查**：批次 A 4 视角、B 2 视角、C 4(C1)+2(C2) 视角，全部无 important，共识 nit 全修。
- **新增 MDR**：MDR-012（批次 A 三项偏离）、MDR-013（danger 落 state）。
- **后续 TODO**（MDR-013 登记，非阻塞，留待 M4）：io_side_effect 补专属 RULE；DangerCategory 上移 types 层恢复类型安全；RULE-6/12/15 porting-template 完整展开。

> M3 收尾 + 遗留债清理均已完成；PLAN-M3 验收清单回填 [x] + 头部完成横幅；本文件「当前位置」标记 M3 + 地基 ✅。

### 下一步：M4「完善」——规划已定稿（[PLAN-M4.md](PLAN-M4.md) v0.2，2026-06-30）

**主线决策（双主线）**：经 2 路调研（代码就绪度 + 技术可行性交叉验证）→ 分析 → R1 三路对抗审查（设计契约/主线决策/可执行性）→ 重定位 → R2 用户审查修正产出。

- **巩固线（真正的「完善」）~17d**：迁移质量度量框架（源行为覆盖率/degrade 率/人工修订率/final_score）+ Community 结构偏离度诊断（Tier 1）+ 既有 TS/Python 真实基线 + 循环健壮性（checkpoint 硬化/watchdog stall 恢复/额度韧性续跑）+ MDR-013 三项清债。
- **Go 扩语言线（roadmap 承诺）~31d**：复用 trait 架构接 Go；**关键 critical 修正**——Go 包系统需**扩 trait 暴露目录列举**（`resolve_import` 的 `exists`-only 签名无法探任意命名包代表文件，扩 trait 是 baseline 非 fallback）；Go 验收用质量度量框架设真实门槛（多模块，非单模块编译）。
- **明确推迟/砍**：C（无类型 IR 下语义难度+ROI）/ Kani（**推迟**，与 proptest 互补不替代，当前 ROI 不足）/ Community Tier 2/3（Tier 1 已纳入）/ Strangler Fig（降文档，离线场景下共存需求不强）/ 并行编排程序化调度器（当前 SKILL.md 编排满足需求，ROI 不足）/ index.json（YAGNI）。
- **Sprint 结构**：A 债务收口+Go前置 → B 质量度量+既有基线+Community诊断 ‖ C Go Adapter Core → D Plugin Go → E Go 端到端验收 → F 健壮性+编排收口。共 37 任务 ~48d，两线可独立分批交付。
- **配比决策（2026-06-30 用户拍板）**：双主线并行——Sprint A 完成后，B（巩固线）与 C（Go 线）可并行启动。

#### Sprint A：债务收口 + Go 接入前置 ✅ 完成（2026-06-30，分支 `feat/m4-sprint-a-debt-and-go-wiring`，待 PR 审查）

| 任务 | 状态 | 交付 |
|------|------|------|
| M4-DEBT-01 io RULE 归属 | ✅ | **裁定并入 RULE-10（标准库 IO 映射）**，不新开 RULE（保持 26 类）；translator.md 定向表 + TS/Python porting-template 补「标准库 IO 映射」节；concern() 文案加 RULE-10 引用 |
| M4-DEBT-02 DangerCategory 上移 | ✅ | 枚举从 `lang/mod.rs` 移到 `types/common.rs`，加 `Deserialize`+`#[serde(other)]` 兜底 `Unknown` 变体；`ModuleState.danger: Vec<String>` → `Vec<DangerCategory>`；lib.rs 去 `as_str()` 转换、`union_danger` 按 `as_str()` 重排保旧字典序；新增 4 个 serde 双向/兜底/旧版兼容测试 |
| M4-DEBT-03 RULE-6/12/15 展开 | ✅ | TS/Python porting-template 各补「并发模式/unsafe 使用策略/全局状态处理」三节（映射表+陷阱）；concern() 文案语言中立化（去 TS 口径硬编码）；各模板 frontmatter bump `rule_version`（+RULE-6/10/12/15）；translator.md 脚注同步 |
| M4-LANG-01 Go registry 接线 | ✅ | workspace 引 `tree-sitter-go=0.21`；`registry.rs` 加 Go 臂；`lang/go.rs` 骨架（language/can_handle/resolve_extensions/detect_source_root 实，余 `todo!()`）；新增 `create_go_adapter` 测试 |

- **验证**：559 测试全绿（基线 552 +4 serde +3 go 骨架测试）；`just ci` 全过（fmt+clippy -D+test+deny+shellcheck）。
- **审查（4 视角全跑，PR [#57](https://github.com/snowzhaozhj/rewriteInRust/pull/57)）**：主审/设计契约/专项/异构交叉。**1 important 必修 + 4 nit/文档同步全落实**：
  - **important（4 方一致）**：Go registry 接线后 `todo!()` 让 Go 项目 graph build/populate **panic 崩进程**（回归，违反 CLI 统一 JSON）。修：骨架方法非 panic 化——`analyze_file` 返 `Err(NotImplemented)`、`detect_tier` 返保守 `Full`、删 `classify_file` override 用 trait 默认 `conservative()`；新增 3 个 go 骨架回归测试。
  - **设计文档同步**：09-schema danger 字段（`Vec<String>`→`Vec<DangerCategory>` + unknown 兜底说明）；MDR-013 决策 2/3 标注被 DEBT-02 取代 + 后续 TODO 三项收口标注；translator.md 文末补 RULE-10。
  - **nit**：`detect_source_root` go.mod 返 `Some(".")` 而非 `None`（避免误导 fallback warning）；Unknown 有损单向性在类型层文档注明（PLAN 授权 + 不可触发理由：danger 恒为分类器 6 类、跨版本由 schema_version 管）。
  - **Unknown 有损往返研判**：异构定 HIGH、主审 MEDIUM、专项 nit。研判为**理论回归、单版本不可触发**（danger 只由分类器产 6 类，Unknown 仅手工编辑/跨版本时现）；PLAN-M4 DEBT-02 已授权 `#[serde(other)]`；保真方案（`Unknown(String)`）破 Copy + as_str 签名冲突 + 手写 serde 出错面，ROI 不足。采文档充分注明 + 测试锁边界。
- **待办**：等用户审阅拍板合并（不自行 merge）。

### 历史：Sprint D 端到端验收（M3-VAL-01~08）✅

- **M3-VAL-01 选型**：jmespath + textdistance（纯计算/数据处理，有 pytest 覆盖）
- M3-VAL-02/03：两项目各 ≥1 模块 done（cargo check+test+clippy 过）
- M3-VAL-04：差异测试框架（pytest 行为录制 JSON fixture → Rust 对比）
- M3-VAL-05/06：性能回归（TS 实测无退化）+ graduate Python 路径验证
- M3-VAL-07 ✅ PR #42（设计文档同步）
- M3-VAL-08：全量回归 + 覆盖率 ≥70%

### M2 遗留（Sprint A 已全部关闭）

| 项目 | 处理 |
|------|------|
| FFI 方向不匹配 | ✅ MDR-007：取消 FFI，degrade_skip 唯一路径 |
| TS 特有概念泛化 | ✅ LANG-05：constructor_bindings → instance_type_bindings |
| DEVIATION 4 项待 MDR | ✅ MDR-008：4 项偏差补录 |
| F2-FFI 验收缺口 | ✅ MDR-007 标记为"设计变更取消" |

### Sprint A 完成清单

| 任务 | 状态 | 说明 |
|------|------|------|
| LANG-01 adapter 工厂 | ✅ | `lang/registry.rs` + `create_adapter()` |
| LANG-02 resolve_import 下沉 | ✅ | trait 新增方法，build.rs 通过 adapter 调用 |
| LANG-03 build_graph 泛化 | ✅ | 4 个便捷函数改用工厂 + `build_graph_for_lang` |
| LANG-04 alias 漏边修复 | ✅ | 函数调用分支补 alias_to_original 查找 |
| LANG-05 instance_type_bindings | ✅ | constructor_bindings 改名 + 删 TODO(M3) |
| LANG-06 配置泛化 | ✅ | source_language: Option + default_excludes_for_lang |
| LANG-07 stats 泛化 | ✅ | collect_source_files(lang) + source_max_nesting |
| FFI-CLOSE | ✅ | ffi.rs deprecated + MDR-007 |
| DEV-01 DEVIATION MDR | ✅ | MDR-008 补录 4 项偏差 |

### 当前工作：Sprint B（Python Adapter Core）

**目标**：实现 `PythonAdapter`，可解析 Python 源码、构建依赖图、检测复杂度分档。

**PR 拆解（3 步走）**：

| PR | 任务 | 预估 | 并行策略 |
|----|------|------|---------|
| **PR-B1 Foundation** | PY-01 + PY-09 | ~1d | 串行，所有后续前置 |
| **PR-B2 Core Analysis** | PY-02 + PY-03 + PY-04 + PY-05 + PY-06 | ~5d | 内部双线并行：Track A (import→resolve) ∥ Track B (symbol→call+signature) |
| **PR-B3 Validation** | PY-07 + PY-08 | ~2.5d | 串行，验收层 |

**依赖图**：
```
PY-01 ─┬→ PY-02 → PY-03 ─────┐
        ├→ PY-04 → PY-05/06 ──┼→ PY-08
        └→ PY-09               │
                    PY-07 ─────┘
```

**进度**：
- [x] PR-B1：PY-01 adapter 骨架 + PY-09 注册/契约
- [x] PR-B2：PY-02 import 解析 + PY-03 resolve + PY-04 符号 + PY-05 调用 + PY-06 签名
- [x] PR-B3：PY-07 fixture（4 个）+ PY-08 集成测试（23 测试）+ CLI graph build 语言检测泛化

**PR-B3 交付**：
- 4 个 Python fixture：`py-linear-deps`（线性+`__all__`+async+构造调用）/ `py-diamond-deps`（菱形+继承 extends）/ `py-circular-deps`（环检测+shared 不在环）/ `py-pkg-deps`（`__init__.py` 包+re-export 透传偏序+`TYPE_CHECKING` StaticType）
- `python_ground_truth.rs`：24 测试，节点/边**双向严格校验**（含 sub_kind，防多余/缺失/标注错误漏检）+ 拓扑偏序 + Python 特有断言（extends 无 Implements、signature round-trip、StaticType import、构造 sub_kind、循环 SCC 精确同环）
- CLI `cmd_graph_build`：源语言优先取 config（避免热路径重复全树扫描），未配置才 `detect_language` 探测，失败显式告警回退 TS；非 TS 强制全量并提示降级；新增 `build_graph_full(root, lang, profile)`；TS 增量路径不回归
- `cli_e2e.rs` 新增 Python graph build 端到端用例（探测→降级→status=warning）
- `cargo run -- graph build --root fixtures/py-linear-deps` 输出 node=12/edge=15 ✓
- **审查**：4 视角全跑（主审/设计契约/专项/异构交叉）；6 项测试保真+CLI 健壮性问题已修，无遗留 important

### 当前工作：Sprint C（Plugin Python 适配）

**目标**：Plugin 层支持 Python 项目迁移分析和翻译（PLG-01~06）。

**PR 拆解（修正 PLAN-M3 偏离后）**：

| PR | 任务 | 说明 |
|----|------|------|
| **PR-C1** | PLG-01修正 + PLG-02 | Python adapter 资产：`analysis-tools.json` + `porting-template.md` |
| **PR-C2** | PLG-03 + PLG-04 | translator.md / analyzer.md / verifier.md 多语言分支 |
| **PR-C3** | PLG-05 + PLG-06 | degrade_skip 降级报告增强 + Plugin Python 端到端验证 |

> **PLG-01 偏离修正**：PLAN-M3 字面要求建 `adapter.json` + `detect.sh`，但实际架构中
> TS adapter 目录仅 `analysis-tools.json` + `porting-template.md`——语言检测在 `analyze.md`
> Step 2（读特征文件）、依赖分析由 CLI `graph build`（tree-sitter）完成，设计文档 06 §11.2
> 的 shell 脚本模式从未落地。Python adapter 对齐 TS 实际结构，不建 adapter.json/detect.sh。

**进度**：
- [x] PR-C1：Python adapter 资产（[#38](https://github.com/snowzhaozhj/rewriteInRust/pull/38)，审查必修全落实，待合并）
  - 审查：迁移规则正确性 + 设计契约 2 视角全跑；2+1 项 important 已修（regex 反向引用/环视、dict 插入顺序、PLG-01 偏离落 MDR-009）+ 多项 nit
  - MDR-009：适配器 shell 脚本模式取消，adapter 目录契约 = analysis-tools.json + porting-template.md
- [x] PR-C2：translator.md/analyzer.md/verifier.md 多语言分支（PLG-03 + PLG-04，待审查/合并）
  - translator.md（PLG-03）：核心规则节加「语言基线」——TS 内嵌表仅 source_language=typescript 套用，非 TS 以 `adapters/<lang>/porting-template.md` 为权威；RULE-2 表标 TS 基线；Phase A 加 Python 特化小节（`self` 参数转换 / `__init__.py` 包→mod 树 / 无 type-only import 区分）
  - analyzer.md（PLG-04）：R6 源语言特化分析——Python 框架识别（django/flask/fastapi 等）+ 动态特性扫描（getattr/eval/metaclass/monkeypatch）记入 `gaps.dynamic_features`（输出格式示例同步加键）
  - verifier.md（PLG-04）：9 维度表后加「源语言特化探测案例」——Python 替换 TS 案例（int 任意精度 / dict 插入序 / str 码点 vs UTF-8 / GIL·multiprocessing 进程隔离 / except pass·try-finally / Decimal 禁降级 f64）
  - 自检：改动区无死链；plugin validate 通过
  - **审查**：4 视角（主审/设计契约/专项全跑，异构 skip：34 行纯文档不涉算法/解析器）；1 important + 3 nit 已修
    - important（主审查证 python.rs StaticType，design-checker 漏判）：「Python 无 type-only import」表述错误 → 改为「无 `import type` 语法关键字，但 `TYPE_CHECKING` 块是惯用仅类型导入，图层已标 StaticType」（translator + analyzer）
    - nit：dynamic_features 条目格式点明为 `"file: 简述"` 字符串；translator 语言基线补「无适配器模板语言降级回退 TS + TODO(port)」
    - nit 未采纳：self 段指针化（保留结构映射防 run 阶段丢失，专项亦认可可接受）
- [x] PR-C3：degrade_skip 降级报告增强 + 端到端验证（PLG-05 ✅ + PLG-06 进行中）

> **遗留待办**：✅ 已由 PR [#42](https://github.com/snowzhaozhj/rewriteInRust/pull/42) 处理（M3-VAL-07）——① 设计文档 06 §11.2 按 MDR-009 改写为两文件契约；② verifier.md 第 58/87 行 `权威来源：05 §6.x` 死链已清理。待合并。

### M3 多语言扩展点（调研结论，2026-06-24）

**已就绪**：
- `LanguageAdapter` trait 6 方法已抽象（`language/can_handle/resolve_extensions/import_specifier_extensions/analyze_file/detect_tier`）
- `SourceLang` 枚举已预定义 TypeScript/Python/C/Go
- `profile/detect.rs` tokei 映射已含 Python/C
- Plugin 层 `SKILL.md` / `analyze.md` 已考虑多语言分发
- 设计文档 06 §11 有完整的语言扩展架构设计

**需泛化（TS 硬编码）**：
- `detect.rs`: 直接实例化 `TypeScriptAdapter`（需 adapter 工厂）
- `graph/build.rs`: `build_graph_ts()` 等 4 个便捷函数硬编码 TS adapter
- `stats/compare.rs`: `collect_ts_files()` / `ts_max_nesting()` / 独立创建 TS parser（绕过 adapter 抽象）
- `types/config.rs`: 默认 `source_language: TypeScript` / exclude 含 `node_modules`
- Plugin `translator.md`: 类型映射表以 TS 为基线
- Plugin `adapters/`: 仅有 `typescript/` 目录

## 历史归档

- **M1 详细记录**：[STATUS-M1-archive.md](STATUS-M1-archive.md)
- **M2 详细记录**：[STATUS-M2-archive.md](STATUS-M2-archive.md)（Sprint D/E/F 任务清单、PR 记录、审查修复、已知问题处理状态）
- **M2 计划**：[PLAN-M2.md](PLAN-M2.md)（55 项任务 + 5 项验收，Sprint A→F）
- **M2 Sprint F 验收**：[sprint-f-acceptance.md](sprint-f-acceptance.md)
