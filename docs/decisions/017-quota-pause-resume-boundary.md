# MDR-017: 额度耗尽优雅暂停 + 续跑的职责边界（M4-ROB-01c）

- **状态**: 已决策
- **日期**: 2026-07-05
- **范围**: M4 Sprint F ROB-01c「额度耗尽优雅暂停 + 断点续跑」——新增 `state resume` CLI 命令（core `resume_plan` + `ResumePlan`/`InterruptedModule`/`ResumeProgress`）；SKILL.md/run.md/workflow.md 续跑 prose。改 `cli/`（core + CLI）、`plugin/`、`docs/`。延续 [MDR-015](015-reset-idempotent-retry-boundary.md) / [MDR-016](016-watchdog-stall-recovery-boundary.md)。

## 背景

M3 实战反复遇到 **token 预算 / API 额度耗尽**导致长迁移循环中途中断（见 memory `feedback_quota_resilient_loops`）。现状（2026-07-05）：

- ROB-01a 已交付**逐步原子 checkpoint**（`migration-state.json` 每步 tmp→fsync→rename）+ 幂等回退 `state reset`；ROB-01b 交付 stall 恢复 `state recover`（retry 复用 reset / skip 置 paused）。MDR-016:82 已明确记账「ROB-01c 断点恢复时对进行中模块调 `state recover --policy retry` 保证幂等重入；额度检测同 stall 检测归编排器」。
- 缺口在**续跑的确定性入口**：额度刷新后重入时，「哪些模块被中断需幂等重入、哪些已完成不重跑、下一步做谁」此前只能靠编排器手工翻 state，无程序化、可测的断点计划——无人值守续跑（定时任务在额度刷新后自动接力）无法确定性驱动。

## 决策

### 决策 1：检测归编排器/harness，续跑计划归 CLI（延续 MDR-015/016 引擎/编排分工）

- **额度检测（逼近上限）归编排器/harness**：CLI 是短命子进程，**观测不到** token 预算 / API 额度。检测只能由 harness（`budget.remaining()`，无人值守）或人工判断完成。故 CLI **不做**额度检测、**不加**额度阈值 config。
- **优雅暂停 = 编排器在当前原子步后停止**：状态已由 ROB-01a 逐步原子持久化，**无需单独的「pause」写操作**——暂停的正确姿势是「当前正在写 `.rs`/改 state 的原子步收尾后停」，而非在半途硬停（半途中断由 load 的 backup 兜底 + resume 的中断分类处理）。
- **续跑（断点计划）归 CLI**：新增 `state resume`——**纯查询、无副作用、不加载 graph**，读已 checkpoint 的 state 分类模块并输出续跑计划。实际重入的**状态变更复用既有 `state recover --policy retry`**（不重复实现 mutation）。

### 决策 2：`state resume` 是纯查询，不是批量 mutation

选**纯查询**（区别于 `reset`/`recover` 的 mutation 命令）：

- **为何不批量 recover**：一个「resume 自动 recover 所有中断模块」的命令看似省事，但会引入 **paused 复活 bug**——见决策 3。查询式产出计划、把 mutation 留给编排器逐个调 `recover`，天然规避，且与 `state deps`（同为纯查询门禁）风格一致。
- **净价值**：把「断点位置 + 逐模块恢复命令 + 下一步指引 + 进度对账」程序化 + 可测。两个消费方（交互式 run.md 与无人值守续跑）共享同一确定性入口，不漂移。

### 决策 3：运行态与 `paused` 分列——`paused` 续跑**不复活**

按 `ModuleStatus` 归 5 桶（见下），关键是**运行态与 `paused` 必须分列**：

- **运行态**（`translating`/`compile_fixing`/`testing`/`reviewing`）→ `interrupted`：这是被额度打断的进行中工作，应 `recover --policy retry` 幂等重入。
- **`paused`** → `awaiting_decision`：`paused` 是前次 stall skip / 重试耗尽留下的**人类决策点**（MDR-015/016 确立「降级是人类决策」边界）。额度续跑若把 paused 也 retry，会**绕过降级抉择**把它复活重译——这是必须规避的正确性 bug。故 resume 把 paused 单列、**不给** `recover_command`。
- **终态**（`done`/`degrade_*`）→ 仅计入 `progress`，**不重跑**（对齐验收「已完成模块不重跑」）。
- **`pending`** → `next`（编排器用 `state deps <M>` 判就绪后推进）；**`blocked`** → `blocked`（等依赖）。

**全枚举 match**：`resume_plan` 对 `ModuleStatus` 做穷举 match，未来新增变体时编译器强制归桶，不会静默漏判。

### 决策 4：不加额度阈值 config（YAGNI）

不新增 `[orchestration].quota_*` 字段。理由：额度检测归 harness（已持有 `budget.total`/`budget.remaining()`），CLI 永不读；resume 的分类策略是 `ModuleStatus` 的确定性函数、无 knob 可调。加一个只有 harness 读的 config 是投机——与本里程碑砍 index.json 的 YAGNI 立场一致。若未来出现「按模块类型分档暂停」等真实诉求再评估。

### 决策 5：`resume` 输出结构与既有命令对齐

- `interrupted[]` 每项含 `module` + `status` + `member_files`（源作用域，同 `RecoverOutcome::member_files`）+ `recover_command`（即 `state recover --module <M> --policy retry`，编排器可直接执行）。
- `progress` 六桶计数 + `total`，且 **total == 各桶之和**（含 awaiting_decision），供台账进度回填与续跑校验对账。
- `resume_point.sprint` 反映 `sprint.current`（仅上下文展示，分类不按 sprint 过滤——终态跨 sprint 均不重跑）。
- 复用统一 `{status,data,warnings}` 框架 + `load_state_with_warnings`（backup 恢复告警透传）。

## 影响

- **新增**：core `resume_plan` + `ResumePlan`/`InterruptedModule`/`ResumeProgress`（导出）；8 resume 单测。`state resume` CLI 命令 + `cmd_state_resume`（1 cli_e2e：混合态归桶 + recover_command + 终态不重跑 + progress 对账）。
- **改**：SKILL.md 新增「额度耗尽优雅暂停与续跑：`state resume`」单点；run.md 计数器段后补额度暂停/续跑正交说明；workflow.md「失败不阻塞」补 budget-aware 暂停分支。设计 06 CLI 表加 `state resume`。
- **不改 schema**：`ModuleState` 无新字段（复用 status/member_files）；无新 config。
- **复用不重复**：retry 重入复用 `recover_module`（→ `reset_module`）；`pending` 就绪复用 `state deps`；纯读复用 `resume_plan` 无 graph。

## 后续 TODO（记账，非阻塞）

1. **无人值守自动续跑**：定时任务在额度刷新后拉起 `state resume` → 批量 `recover retry` → 继续。属编排/harness 层（memory `feedback_quota_resilient_loops`），非 CLI。
2. **进行中细粒度 checkpoint 与 resume 对账**：run.md §6b 的 `{group}-progress.json` 成员级 checkpoint 与磁盘事实裁决已存在；resume 目前只到模块粒度，若真实项目暴露「组内部分成员完成」的续跑精度诉求，再评估把成员级进度纳入 `resume_plan`（当前由 recover retry + 磁盘事实裁决覆盖）。

## 审查加固（PR 6 视角）

6 视角全跑，结论汇总：

| 视角 | 结论 |
|------|------|
| 主审 `/code-review` | 无 findings |
| 主代码审查 | 无 important/nit，8 测通过、clippy 零告警 |
| 设计契约 `design-checker` | 6/6 PASS，1 nit（`resume_point` 精简视图与顶层 `interrupted[]` 语义重叠，无害 EXTENSION） |
| 测试覆盖 `pr-review-toolkit` | 2 important：`sprint` 字段无断言 / `resume_point` e2e 未断言 |
| 类型设计 `type-design-analyzer` | 1 important：`ResumeProgress` 双真相源 |
| 异构交叉 `codex` | 无 important，2 nit（文档计数漂移 / 注释「五个列表桶」应为四个） |

已修：
- 类型设计 important — `ResumeProgress` 由存储字段改为 `ResumePlan::progress()` 按需派生（单一真相源=列表长度 + `done`/`degraded`）。
- 测试 important — 新增 `test_resume_plan_sprint_reflects_current`；e2e 断言 `resume_point.interrupted`/`sprint`。
- codex nit — 本文档单测计数订正 7→8；`machine.rs` 注释「五个列表桶」订正为「四个列表桶」。
- 其余 nit — 空 `modules` 注释订正、`next` 桶逆序插入验证排序、reconcile 补派生等式断言、`Eq` derive 向 sibling 看齐。
