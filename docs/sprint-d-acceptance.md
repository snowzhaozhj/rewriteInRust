# Sprint D 验收记录（M3 端到端验收）

> 目标：在 2 个真实 Python 项目上完成端到端迁移（各 ≥1 模块到 `done`），质量门全过，沉淀真实场景暴露的工具缺口。
> 方法论权威：[agent-skill-prompt-guide.md §6](learnings/agent-skill-prompt-guide.md)（Plugin Live 验证）。

## 验收方法（严格按 §6）

- **不污染本会话**：用独立 headless 进程 `claude --plugin-dir <abs>/plugin --dangerously-skip-permissions -p "<prompt>"` 驱动 `/migrate` skill；本会话不加载插件，命名空间实测 `rust-migrate:migrate` + `rust-migrate:{analyzer,translator,scaffolder,verifier}`。
- **无人值守**：目标项目 `.rustmigrate.toml` 设 `headless=true`（3 轮失败自动 degrade_skip + safe-default TODO）+ `auto_confirm_intent=true`（跳意图确认门禁）。
- **结论靠核查产出物文件**，不靠 headless stdout；每个迁移模块的 `done` 由验收者**独立**重跑 `cargo test` + `cargo clippy --all-targets -D warnings` + 全量 golden 等价复核确认，不信 agent 自述。

## VAL-01 真实项目选型 ✅

| 项目 | 规模 | 性质 | 选取理由 |
|------|------|------|---------|
| **A: jmespath.py** | 1675 行 / 8 模块 | JSON 查询语言（lexer→parser→AST→visitor→functions），零外部依赖，MIT | 经典解析器，完美映射 Rust enum+match；自带 JSON compliance 套件可直接做差异测试 |
| **B: textdistance** | 2657 行 / 14 模块 | 30+ 文本/序列距离算法（Levenshtein/Hamming/Jaccard…），零核心依赖，MIT | 纯算法、无 I/O 无副作用、输出确定性，输入/输出对可直接录制验证 |

工作区：`/tmp/jmespath-check`、`/tmp/textdistance-check`（真实 git clone，绝不动本仓库）。

## VAL-02 jmespath 端到端迁移 ✅

**analyze**（headless，~18min，串行 spawn analyzer/translator/scaffolder）：`state=sprint_loop`，拆解为 2 个 sprint-1 模块：
- `file:__init__.py`：`coupled_batch`，7 成员（__init__/ast/compat/exceptions/functions/lexer/parser），解析器核心。
- `file:visitor.py`：单文件，树解释器（依赖前者）。
- 产出 **902 条黄金三元组**（752 value + 150 error，经源 jmespath 1.1.0 引擎离线复核 0 mismatch）+ `rust-src/` Cargo 骨架 + porting 规则（enum Ast / thiserror / getattr→match / metaclass→LazyLock）。

**run**（headless，CoupledBatch 完整路径 + 单文件全路径）：

| 模块 | 终态 | 证据（验收者独立复核） |
|------|------|----------------------|
| coupled_batch（7 文件） | ✅ done | 2666 行 Rust，93→114 内联单元测试全过，cargo check + clippy 0 warning，TODO(port)=0 |
| visitor.py | ✅ done | search() 全链打通（Parser→ParsedResult::search→TreeInterpreter::visit），getattr `visit_%s` 动态分派→静态 match |

**端到端强等价（差异测试，即 VAL-04 对 jmespath 的落地）**：
- `rust-src/tests/golden_compliance.rs` 移除 `golden_equivalence` 的 `#[ignore]`、接线 `jmespath::search(expr,&given,None)` 逐条断言 902 黄金集。
- **结果：901 等价 + 1 豁免（mismatch=0）**。豁免 `slice/0/27`（`foo[8:2:0]`）：源抛 ValueError（slice step==0），headless safe-default 返 `Ok([])` 不复刻——登记 KNOWN_DIFFERENCES D-10，且测试仍锁定该例 safe-default 行为断言（豁免不掩盖回归）。
- **验收者独立复核**：`cargo test` = 114 lib + 2 golden 集成全过、0 ignored；`cargo clippy --all-targets -D warnings` 清零。

> 等价深度：经源引擎实测的 902 条 JMESPath compliance 用例端到端比对（Python 录制行为 → Rust 迁移后逐条断言），是真实场景强等价，非工具自测。

## VAL-03 textdistance 端到端迁移 🔄

**analyze** ✅：`state=sprint_loop`，14 文件经 decompose 凝聚成 3 个 coupled_batch 组；`benchmark.py`（timeit 开发工具）+ `libraries.py`（importlib 动态加载外部加速库）判 degrade_skip（外部库仅同算法加速，原生 Rust 即权威实现）。

**run**（进行中）：目标 `file:algorithms/base.py` 组（base/edit_based/types/vector_based，纯距离算法）→ done + golden 差异等价。

## VAL-04 统一差异测试框架 ✅（以 golden 套件落地）

差异测试 = Python 原始行为录制（源引擎实测 → JSON golden 三元组）→ Rust 迁移后消费同一 fixture 逐条断言。jmespath 已实证 902 用例端到端通过；textdistance 同构（golden_all.json 按算法分类录制）。框架即 scaffolder 产出的 `test-fixtures/golden/` + `golden_compliance.rs` harness。

## 真实场景暴露的工具缺口与修复

| # | 缺口 | 性质 | 处理 |
|---|------|------|------|
| 1 | `stats compare` 结构门硬编码 TypeScript，非 TS 源直接报错「M3 实现」→ Python run.md 步骤 8 结构门只能 degrade | CLI 缺口（deferred M3 项未补完） | ✅ 修：`compare_structure` 增 `source_lang` 参数 + `source_max_nesting` 补 Python arm（tree-sitter-python + `is_py_control_flow`）+ CLI 传 config 语言；新增单元 + e2e 测试。提交 942da23 |
| 2 | scaffolder 生成的 golden harness 用 `Option<T>`+`#[serde(default)]` 承接期望值，`"result": null`（present-null）被误判「缺 result」 | Plugin 缺口（golden 一致性误报） | ✅ 修：scaffolder.md R2 增 present-null 区分约束（`deserialize_with`）。提交 942da23 |
| 3 | analyze 设 `source_root` 不可靠：jmespath 修正 `src→jmespath`，textdistance 漏修留 `src`（实际包在 `textdistance/`） | 流程缺口（src-layout vs flat-package 推断） | 🟡 验收中人工修正 config；analyzer source_root 推断待加固（记 TODO） |
| 4 | translator Phase B 用 `Write` 截断既有 `.rs` 后凭记忆重建（险情，靠下游全量 golden 兜住） | Plugin 缺口（无 Edit 工具被迫整文件重写） | ✅ 修：translator 加 `Edit` 工具 + Phase B 强制「改既有文件用 Edit、禁 Write 重建」 |
| 5 | ffi.rs 测试用 deprecated `generate_ffi_binding` 无 `#[allow(deprecated)]`，`clippy --all-targets` 报错 | pre-existing 潜伏（`just lint` 不带 `--all-targets` 故门禁未覆盖） | 🟡 记 TODO（非本验收引入） |

## 待完成

- [ ] VAL-03 textdistance base.py 组 → done（run 进行中）
- [ ] VAL-06 `/migrate graduate` 对 jmespath（2 模块全 done）验证 Python 路径
- [ ] VAL-08 `just ci` 全量回归 + 覆盖率
- [ ] STATUS.md 更新 + PR
