---
name: migrate
description: 把 TypeScript / Python / C 项目迁移到 Rust 的验证工作台。当用户想要分析待迁移项目、生成迁移规则、逐模块翻译为 Rust、批量翻译整个 sprint、或查看迁移进度时使用——只要用户提到"迁移到 Rust""rewrite in Rust""把这个项目改写成 Rust""迁移进度""批量翻译""并行迁移"或对一个 TS/Python/C 仓库表达改写为 Rust 的意图，即使没有显式输入命令也应考虑此 skill。子命令：analyze（分析+建规则+搭测试）、run（模块翻译）、workflow（sprint 级批量并行翻译）、review（验证+仪表板）。
argument-hint: "[analyze|run|workflow|review] [module]"
---

# /migrate — Rust 迁移验证工作台

把源项目（TS/Python/C）迁移到 Rust 的编排入口。确定性计算（建图、统计、状态校验）由 `rustmigrate` CLI 承担；翻译策略、等价性判断等非确定性工作由 SubAgent 完成。本文件是路由 + 所有子命令共享的约定；具体流程在子命令文件中按需 Read。

## 路由

读取调用参数的第一个词，Read 对应子命令文件并严格按其分步指令执行：

| 参数 | 子命令文件 | 作用 |
|------|-----------|------|
| `analyze`（默认） | [analyze.md](./analyze.md) | 分析源码、生成迁移规则、搭建测试基础设施（init+plan+test 合并） |
| `run` | [run.md](./run.md) | 翻译指定模块（Phase A 忠实翻译 / Phase B 惯用化） |
| `review` | [review.md](./review.md) | 完整验证管线 + 迁移进度仪表板 |
| `workflow` | [workflow.md](./workflow.md) | Sprint 级批量翻译——按拓扑层并行编排多模块（M2-SCALE-01） |
| `graduate` | — | 毕业评估 + unsafe 审计（M2，非 MVP） |

无参数时默认 `analyze`（迁移的起点）。参数为未知词时，提示用户可用子命令而非猜测。

## 共享约定（所有子命令遵守）

### CLI 调用与输出解析
- 通过 Bash 调用 `rustmigrate <子命令>`，工作目录为源项目根。所有 CLI 输出是统一 JSON：`{status, data, warnings}`。
- **定位 CLI**：裸调 `rustmigrate` 假设其在 `$PATH`。若不确定是否安装，先运行 `BIN=$(hooks/scripts/ensure-cli.sh)` 取二进制绝对路径（解析优先级 PATH > `$RUSTMIGRATE_BIN` > 本地构建产物），后续用 `"$BIN" <子命令>` 调用；脚本未找到二进制时退出非 0 并打印安装指引，应如实转达用户。
- **只解析 `data` 字段**取结构化结果；`status` 为 `error` 时按 `data` 中的错误码处理，不要从自然语言里猜成败。`warnings` 非空时如实转达用户，不要静默吞掉。
- 命令清单（M1 共 14 个 + M2 新增 `state deps`）：`init`、`profile --root [--adapter-tools]`、`graph build --root [--full]`、`graph topo-sort`、`graph deps <m>`、`graph interfaces <m> [--deps-of <t>]`、`graph stats`、`validate state`、`state get <m>`、`state transition [--module] --to [--substatus] [--reason] [--force]`、`state populate-modules`、`state deps <m>`（组感知依赖门禁，破环 M2-SCALE-SCC）、`stats loc`、`stats compare`、`scaffold workspace [--target] [--name]`。
- **`profile --adapter-tools` 路径自动解析**：analyze 流程步骤 3 按优先级定位 `analysis-tools.json`——①`.rustmigrate.toml` 的 `adapter_path` ② `$CLAUDE_PLUGIN_ROOT/skills/migrate/adapters/<lang>/` ③ `plugin/skills/migrate/adapters/<lang>/`（同仓相对路径）④ 全部未命中则省略参数（降级 warning）。详见 [analyze.md](./analyze.md) 步骤 3。

### 全局锁（跑 `/migrate` 命令的进程开始时取，结束或异常退出时释放）
同一项目同一时刻只允许一个 `/migrate` 命令运行。**持锁的始终是跑该子命令的进程**（串行 run 中它就是翻译执行者；并行 run/workflow 中它是编排器）；它**派发的 SubAgent 从不取锁**（理由见下「并行模式下的锁策略」）。锁文件 `.rust-migration/.migration-lock`，内容为单行 JSON `{session_pid, started_at, hostname}`，`session_pid` 取 `$PPID`（Claude Code 宿主进程，生命周期覆盖整个会话）。
- **取锁**：写临时文件后 `link` 到锁文件（原子，等效 `O_EXCL` 且保证有内容），link 成功即获锁。
- **link 失败 → 判陈旧**：同机且 `session_pid` 进程已死、或 `session_pid == 当前 $PPID`（同会话串行残留）→ 删锁后重试一次；不同会话的活进程 → 真实并发，报错退出；跨机或 PID 不可判定且 `now - started_at > lock_timeout_secs`（默认 300）→ 视为陈旧。
- **释放**：命令结束或异常退出时删除锁文件。
- **逃生口**：卡死时用户可手动删除 `.rust-migration/.migration-lock`；报错信息须包含这一提示。

#### 并行模式下的锁策略（M2-SCALE-LOCK）
并行翻译模式（`/migrate workflow` 或 `/migrate run` 并行派发）下，编排器持锁、SubAgent 不取锁的理由：
- **编排器**全程持有主 tree 的 `.rust-migration/.migration-lock`，直到结束或异常退出才释放（即上文「持锁的始终是跑命令的进程」在并行下的具体形态）。
- **SubAgent** 在独立 worktree（`.wt/{module}/`）中工作，worktree 不共享主 tree 的 `.rust-migration/` 路径，天然不碰锁文件——所以不取锁不是特例放行，而是它根本不触达锁与共享状态。
- **状态写入集中化**：只有编排器可写主 tree 的 `migration-state.json`（集中 writer）。SubAgent 完成后回传 `TranslationResult`（含 touched-list），编排器负责合并代码（git merge）并更新状态。
- **串行模式**：单模块 `/migrate run <module>` 不派发 SubAgent 翻译时，持锁者即翻译执行者本身，取锁/释放与 M1 一致。

### SubAgent 编排
- 用 **Agent tool** 调用 SubAgent，参数 `subagent_type` 取带插件命名空间前缀的 agent 名：`rust-migrate:analyzer` / `rust-migrate:translator` / `rust-migrate:scaffolder` / `rust-migrate:verifier`。MVP 阶段 SubAgent **串行执行**，通过 `.rust-migration/` 下的文件通信，不直接对话。
- **调用前后记台账**（每次 Agent 调用都做，含重试；否则 `subagent_calls` 恒空、卡死/重试无法诊断）：
  - 调用**前**：`rustmigrate state record-subagent-call --step-index <子命令步骤号> --subagent-name <analyzer|translator|verifier|scaffolder> --status started`。
  - 调用**后**：按结果再记一条 `--status ok`（产出物校验通过）或 `--status error --error-message "<原因>"`（校验失败 / 超时）。`--step-index` 与 `--subagent-name` 同上一条对齐。
  - 子命令各调用点已标注本步的 `step_index`；统一用该步整数号，便于按步聚合统计。
- **不解析 SubAgent 的返回文本判断成功**。每次调用后只做产出物的确定性校验：
  - **L1 存在性**：文件存在、非空、含关键标题（Markdown / 代码 / 配置产出物）。
  - **L2 结构校验**：JSON 产出物（`migration-state.json`、测试结果）格式合法、关键字段非空；`source-graph.db` 必要表存在。
- 校验失败时按失败恢复三步处理：①记录调用到 `migration-state.json.subagent_calls` ②诊断+重试（`max_retries_per_step` 默认 2）③重试耗尽则提示用户三选项「重试 / 部分跳过(降级) / 完整回滚」。中间产物 `intermediate/attempts/*` 始终保留。

### 失败/中途模块回滚：`state reset`（统一命令，勿手工改 state）
需把某模块回退到干净重译入口重跑时（编译/测试失败复位、`degrade_*`/`done` 重做、断点脏状态清理），**统一调 `rustmigrate state reset --module <M> [--force]`**，不要手工编辑 `migration-state.json`：
- **确定性状态回退**：status→`translating`、清全部进度字段（substatus / phase_a_version / audit / 通过率 / coverage / known_differences / blocked 锚点）；**保留** `attempts`（追加一条 `reset` 审计）与结构冻结字段（tier / member_files / composite_kind / decomposition_* / danger）。`pending` 保持 `pending`。
- **幂等**：模块已在干净入口（`translating`/`pending` 且进度字段皆空）时为空操作（`was_noop=true`，不追加审计、免落盘，`cleanup` 返回 `{skip:true}`）——`reset` 可安全重复调用。
- **守护**：`done`（终态）/ `blocked`（依赖锚点）/ `paused`（自动重试耗尽待人类抉择）/ `degrade_*`（降级须人类确认）须显式 `--force`，否则报错；项目 `graduate` 态下一律拒绝（含 `--force`）。
- **产物清理归编排器**：CLI 不删 `rust_root` 下的 `.rs`（不猜路径）。**先看 `data.was_noop`**：`true`（`cleanup.skip=true`）→ 无产物需清理、直接 `run <M> --retry`；`false` → 据 `data.cleanup.member_files`（模块源作用域）删这些源文件对应的部分 `.rs` 产物，**删后回读核对目标 `.rs` 已不存在**（把「漏删」从静默腐蚀变可观测），再 `run <M> --retry`；`intermediate/attempts/*` 与审计产物始终保留。

### Watchdog stall 检测与恢复：`state recover`（M4-ROB-01b）
SubAgent/后台长命令可能 **stdout 静默卡死**（agent 假死、外部命令 hang）——这与「产出物校验失败」不同：没有返回、没有报错，只是不再产出。编排器负责**检测**（CLI 观测不到子进程 stdout），检测到后统一走 `state recover` 做确定性、幂等恢复：

- **检测**：长命令用 `run_in_background` 派发（无人值守用主会话 background bash 驱动，勿让后台 Agent 跑长命令——静默超 600s 会被判失败），轮询 `BashOutput`；**stdout 静默超 `[orchestration].stall_timeout_secs`（默认 600s）** 判 stall。这是与 `subagent_timeout_secs`（单次调用**总**超时）正交的**静默**阈值——持续产出的长命令不算 stall。
- **策略解析**：据 `[orchestration].stall_recovery_policy`（默认 `retry_then_skip`）+ 本模块已重试轮次（对齐 `max_retries_per_step`）解析出 `--policy`：未超重试上限 → `retry`；超出 / 策略为 `always_skip` → `skip`。
- **恢复**：`rustmigrate state recover --module <M> --policy <retry|skip> --reason "stall: stdout 静默 <N>s"`：
  - `retry` → 复用 `reset` 语义回退干净重译入口，据 `data.recovery.member_files` 删部分 `.rs`（同上「失败/中途模块回滚」，删后回读核对），再 `run <M> --retry`。
  - `skip` → 置 `paused`（决策点，headless 由既有编排自动 `degrade_skip`）；据 `data.recovery.unblock_next` 对**同层其他模块**用 `state deps <模块>`（位置参数）判依赖就绪后继续推进（不被卡死模块阻塞），依赖本模块的下游沿 `blocked_by` 机制自动阻塞。
  - **幂等**：`data.was_noop=true`（`recovery.skip=true`）→ 已在目标恢复态、无副作用，不重复删+重跑（无人值守/断点续跑安全）。
- **守护**：`done`/`blocked` 非 stall 态、`graduate` 项目态 → `recover` 报错（不误清终态/锚点）。

### 额度耗尽优雅暂停与续跑：`state resume`（M4-ROB-01c）
token 预算 / API 额度可能在长循环中途逼近上限。检测归编排器/harness（CLI 观测不到额度——用 harness 的 budget.remaining() 或人工判断）；状态已由每步原子写持续 checkpoint（见「产出物根目录」），故**优雅暂停 = 当前原子步收尾后停止**（勿在写 `.rs`/改 state 半途中断），随后 commit + 更新台账（断点续跑安全，呼应额度韧性长循环）。

续跑时先 `rustmigrate state resume` 拿断点计划（纯查询、不改 state），据输出推进：
- **`data.interrupted`**（运行态被中断的模块）：**逐个执行其 `recover_command`**（`state recover --module <M> --policy retry`）幂等重入——回退干净重译入口后 `run <M> --retry`。这是「进行中模块中间状态」的确定性恢复入口。
- **`data.next`**（pending 候选）：对每个用 `state deps <M>`（位置参数）判依赖就绪后推进。
- **`data.awaiting_decision`**（paused 决策点）：**续跑不复活**——待人类/降级抉择（headless 由既有 stall/编排 prose 处置），resume 不给 retry 命令。
- **已完成/降级模块不重跑**：done/degrade_* 仅计入 `data.progress`，不出现在 interrupted/next。
- **`data.progress`**：done/degraded/in_progress/pending/blocked/awaiting_decision/total 计数，用于台账进度回填与续跑校验。

### 产出物根目录
所有产出物在源项目下的 `.rust-migration/` 目录（`init` 创建）。关键文件：`migration-state.json`、`source-graph.db`、`porting/`（迁移规则）、`PARITY.md`、`AGENTS.md`、`test-fixtures/golden/`。写 `migration-state.json` 统一走 CLI（原子写：tmp→fsync→rename）。
