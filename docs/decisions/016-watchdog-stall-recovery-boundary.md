# MDR-016: watchdog stall 检测与恢复的职责边界（M4-ROB-01b）

- **状态**: 已决策
- **日期**: 2026-07-05
- **范围**: M4 Sprint F ROB-01b「watchdog stall 检测 + 恢复路径」——新增 `state recover` CLI 命令（core `recover_module` + `RecoverPolicy`/`RecoverOutcome`）、`[orchestration]` 扩 `stall_timeout_secs`/`stall_recovery_policy`；SKILL.md/run.md/workflow.md stall 编排 prose。改 `cli/`（core + CLI）、`plugin/`、`docs/`。延续 [MDR-015](015-reset-idempotent-retry-boundary.md)。

## 背景

M3 实战反复遇到 **watchdog stall**：后台长命令 stdout 静默超 600s 被判失败（见 memory `feedback_watchdog_stall`）。现状（2026-07-05）：

- 系统**只有 SubAgent 调用级总超时**（`subagent_timeout_secs=600`）与「产出物校验失败」两类失败——都能靠 `max_retries_per_step` 计数兜。**stdout 静默卡死**是第三类：agent 假死、外部命令 hang，**没有返回、没有报错、只是不再产出**，计数器兜不住。
- ROB-01a 已交付幂等回退原语 `state reset`，MDR-015:67 明确记账「ROB-01b 复用 state reset 做失败模块回退」；paused 的 `--force` 守护（MDR-015 决策 4）、`cleanup.skip` 幂等信号（MDR-015 审查加固）都是为本任务预埋。
- 「stall→标失败→跳过/重试→不阻塞无依赖模块」这条恢复链此前只散落在 run.md/workflow.md prose，未程序化——无人值守循环（ROB-01c）无法确定性、幂等地驱动。

## 决策

### 决策 1：检测归编排器，恢复归 CLI（延续 MDR-015 的引擎/编排分工）

- **检测（stdout 静默判定）归编排器/harness**：CLI 是短命子进程，**观测不到**被派发命令的 stdout 流。静默检测只能由编排器用 `run_in_background` + 轮询 `BashOutput` 完成（无人值守用主会话 background bash 驱动，勿让后台 Agent 跑长命令）。故 CLI **不做** stall 检测——不新增任何观测/轮询命令。
- **恢复（状态回退决策）归 CLI**：retry-vs-skip 后的状态变更是 `(from-state, policy)` 的确定性函数，编码进受测 core 优于 run.md 模糊 prose 分支；给交互式 run.md 与无人值守 ROB-01c 一个**共享的幂等入口** `state recover`。
- **产物 `.rs` 删除仍归编排器**：retry 时 CLI 只输出 `recovery.member_files` 源作用域（同 `reset`），不猜路径删文件（MDR-015 决策 1 立场不变）。

### 决策 2：新增 `state recover` 命令而非纯 prose 复用 reset/transition

选**命令**（延续 ROB-01a 的 `state reset` 先例、Sprint F 程序化可靠性主题）：

- **为何不纯 prose**：ROB-01b 的净价值正是把恢复**程序化 + 幂等 + 可测**。纯 prose 让编排器手工串 `record-subagent-call` + 分支 + `reset`/`transition`——不可测、崩溃中途不可安全重入、两个消费方（run.md 与 ROB-01c）易漂移。
- **命令非冗余**：`recover` **合成**既有原语（retry 委派 `reset_module`、skip 直设 paused），净增量是「stall 审计记录 + 策略执行 + 统一恢复计划输出（retry→cleanup / skip→advice+无依赖模块推进指引）」——后者是 `reset`/`transition` 都不产出的、任务明确要求的「输出失败原因和恢复建议」。

### 决策 3：skip 直接置 `paused` 且**绕过转换矩阵**

skip 策略把模块置 `paused`（决策点，headless 由既有编排 prose 自动 `degrade_skip`、交互态待人类抉择），而非直达 `degrade_skip`——不绕过 MDR-015 确立的「降级是人类决策」边界。

- **为何绕矩阵直设**：stall 可发生在 `translating`（Phase A），而 `translating → paused` **不在** `can_transition_to` 矩阵（仅 `compile_fixing`/`testing` → paused）。stall 是异常路径，仿 `reset_module` 的破坏性直设 status（不走 `transition_module` 的矩阵校验），否则 Phase A 卡死无法 skip。

### 决策 4：retry 复用 `reset_module(force=true)`；幂等语义与 reset 一致

- retry 委派 `reset_module(&canonical, true)`——stall 时模块常在 `paused`/进行态，须跨守护回退；复用其幂等（`was_noop`）、进度清理、`member_files` 作用域。非 noop 时**额外追加**一条 `stall-recover:retry` 审计（区别于 reset 自身的 `reset:` 记录，供区分「stall 触发的回退」vs「普通回退」）；noop 则整体 noop，保 `recover;recover == recover`。
- skip 幂等：已 `paused`/`degrade_*` → noop（已跳过/待决策，不重复置态/记录）。

### 决策 5：守护——仅放行「运行态 + paused」，其余（pending/done/blocked/degrade_*）拒绝；`graduate` 项目态拒绝

`recover`（两策略、无 `--force` 逃生口）用**全枚举显式 match** 先于策略判定，仅放行**可能 stall 的运行态**（`translating`/`compile_fixing`/`testing`/`reviewing`）**+ stall 落点 `paused`**（重试耗尽的常见落点，retry 委派 `reset(force)` 跨 reset 守护回退，skip 对 paused 幂等 noop）。其余一律拒绝：

- `pending`（尚未起步、无运行中 agent，非 stall 态）；`done`（唯一真终态，不会 stall）；`blocked`（等依赖、无运行中 agent，非 stall 态）——避免把未起步模块推进/误清终态/锚点。
- **`degrade_*`（人类降级决策终态语义）——recover 不得撤销**：这是 4 视角审查（codex）揪出的 **HIGH 真 bug**。初版守护只挡 done/blocked，`degrade_*` 会落入 retry 分支调 `reset_module(force=true)` 绕过 reset 对「`degrade_* → translating` 须 `--force` 人类确认」的守护；更糟 `retry;skip` 会把依赖侧已视终态的 `degrade_skip` 模块变回 `paused`（非终态）。如确需恢复已降级模块，走 `state reset --force`（人类显式）。
- `graduate` 项目态（同 `reset_module`，含逻辑拒绝）——防「项目终态 + 非终态模块」矛盾。

**与 `reset` 守护的差异**：`reset` 用 `--force` 放行 done/blocked/paused/degrade；`recover` **无 `--force`**——它是程序化 stall 恢复入口，非 stall 态是「不该被 recover 触达」的误用（编排器只对运行中/paused agent 调 recover），故直接拒绝而非留逃生口，把误用暴露为错误。全枚举 match 让未来新增 `ModuleStatus` 时编译器强制归类（放行/拒绝），不会静默漏判。

### 决策 5b：skip 清 `substatus`、保留进度字段；跨策略非幂等有意

skip 直设 `paused` 时**清 `substatus`**（活跃态瞬态阶段标记如 `phase_a_complete_awaiting_review`，挂到 paused 上语义不符），但**保留** `phase_a_version`/`test_pass_rate`/`coverage`/`known_differences`（供后续降级分析读取）——skip≠reset（reset 是完全回退重译才清全部进度）。stall 原因由 `attempts` 的 `stall-recover:skip` 承载。

**跨策略非幂等是有意**：同策略幂等（`retry;retry`/`skip;skip` 收敛）；不同策略互相撤销（`skip` 后 `retry`：paused→translating）是**显式重试决策**，非缺陷——编排器按解析出的单一 `--policy` 调用一次，不会自发交替。

### 决策 6：策略解析三方分工（config / 编排器 / CLI）

- **config**（`[orchestration].stall_recovery_policy`，默认 `retry_then_skip`）：声明策略意图。
- **编排器**：读 config + 自身 retry-round 计数（对齐 `max_retries_per_step`）→ 解析出本次 `--policy retry|skip`。与 `max_retries_per_step` 的消费方式一致（编排器计数，非 CLI）。
- **CLI**：**不读 config、不计数**——只按传入 `--policy` 确定性执行。保 CLI 无状态、可测；避免 CLI 与编排器双方都读 config 造成漂移。

`stall_timeout_secs`（默认 600）与 `subagent_timeout_secs`（默认 600）**正交**：前者是 stdout **静默**阈值（持续产出的长命令不算 stall），后者是单次调用**总**预算。默认值相同是巧合（都取 memory 记录的 600s 经验值），语义不同。

## 影响

- **新增**：core `recover_module` + `RecoverPolicy`/`RecoverOutcome`（导出）+ 私有 `push_recover_audit`；9 recover 单测。`state recover` CLI 命令 + `cmd_state_recover` + `RecoverPolicyArg`（1 cli_e2e：retry/skip/noop/done 守护）。`OrchestrationConfig` 扩 2 字段 + `StallRecoveryPolicy` 枚举；3 config 单测。
- **改**：SKILL.md 新增「Watchdog stall 检测与恢复：`state recover`」单点约定；run.md「两个独立计数器」补 stall 正交说明 + 指向 SKILL.md；workflow.md「失败不阻塞」补 worktree agent stall 分支。
- **不改 schema**：`ModuleState` 无新字段（复用 status/attempts/member_files）。
- **复用不重复**：retry 复用 `reset_module`、skip 复用 status 直设 + `canonical_module_key` 归一；无依赖模块推进复用既有 `state deps` + `blocked_by` 机制（recover 只在输出里指引，不做依赖传播）。

### 审查加固（4 视角审查后落实）

- **[important·codex HIGH] 守护补 `degrade_*`**（见决策 5）：初版守护漏 degrade，retry 可绕 `--force` 撤销人类降级 + `retry;skip` 把 degrade_skip 变回非终态。修：守护改全枚举 match，`degrade_*` 拒绝。
- **[important·专项] `unblock_next` 命令语法 + 语义**：`recovery.unblock_next` 原写 `state deps --module <M>`——但 `state deps` 的 module 是**位置参数**（非 `--module`），编排器照做会 `cli_parse` 失败；且 `state deps <M>` 查的是**该模块自身**依赖就绪、非「无依赖模块清单」。修：改为对**同层其他模块** `state deps <其他模块>` 判就绪后推进；SKILL.md 措辞同步厘清。
- **[Medium·三方共识] `pending` 纳入守护拒绝**（主审+设计契约+codex）：skip 会把未起步的 pending 直设 paused（与 reset 保持 pending 不对称）。修：守护拒绝 pending（非 stall 态）。
- **[Low·设计契约+codex] skip 清 `substatus`**（见决策 5b）：translating 瞬态标记挂 paused 语义不符；修：skip 清 substatus，保留其他进度字段。
- **[nit·设计契约·pre-existing] 09 转换矩阵例外脚注**：`state reset`/`state recover` 均绕矩阵（ROB-01a 起既有），在 09 §合法状态转换补一句指针，把散在 MDR 的例外在权威矩阵处留链。

## 后续 TODO（记账，非阻塞）

1. **ROB-01c（额度耗尽优雅暂停 + 续跑）**：断点恢复时对进行中模块调 `state recover --policy retry` 保证幂等重入；额度检测同 stall 检测归编排器。
2. **CAS 版本不递增**：`recover` 同 `reset`/`transition` 不递增 `metadata.version`（MDR-015 TODO 4 的 pre-existing 现状，未来并发写全面启用时统一接入）。
3. **stall 检测阈值自适应**：当前 `stall_timeout_secs` 固定；真实项目若暴露不同命令的静默特征差异大，再评估按命令类型分档（当前 YAGNI）。
