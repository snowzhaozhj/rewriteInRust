# MDR-018: 迁移工作流保留并行翻译 + worktree 写隔离（否决串行化）

- **状态**: 已决策
- **日期**: 2026-07-14
- **范围**: 复盘 M4-ORCH-01 落地方式时，一版方案主张「砍并行、砍 worktree 隔离、迁移工作流永久回归串行」。经用户否决。本文件记录**保留并行**的决策 + 概念澄清 + 现状事实基线，并重申 [MDR-003](003-m2-parallel-write-isolation.md) 有效。关联 [MDR-006](006-scc-per-file-stub-first.md)、[MDR-019](019-post-translation-review-gate.md)。

## 背景

ORCH-01 原为「worktree 生命周期代码化」。落地调研时发现两套 worktree 机制并存矛盾（手动 `git worktree add` vs `Task` 工具 `isolation:"worktree"`）、且并行路径从未端到端跑通，遂有一版方案主张整体砍并行、回归串行。其**决定性理由**是：

> 迁移是给终端用户的 human-in-the-loop 过程，用户要逐模块 review 翻译是否行为等价；review 是人的认知活动、天然串行，一次只能专注一个模块；并行让多模块结果同时涌出堆成待审队列，人无法并行审查 → review 积压或退化成橡皮图章。

**用户否决此方案**：并行是本工作台的核心能力，砍并行则项目失去意义。

## 概念澄清（串行化论据的错误所在）

上述理由**偷换了「并行翻译」与「并行审批」两个概念**——二者可以且应当分离：

- **并行翻译**：多个 translator agent 同时翻译**拓扑上互相独立**（无依赖边）的模块。纯加速，不影响审查顺序。
- **串行人工审批**：人仍一次审一个模块，从待审队列里**按自己的节奏取**。

并行翻译只是让待审队列**填得更快/预先填好**，**不会逼人同时审多个**。队列由人自行排空即可。因此「并行破坏可审性」不成立——审批的串行性由 [MDR-019](019-post-translation-review-gate.md) 的译后签批门保证，与翻译是否并行正交。

## 决策

**迁移工作流保留并行能力**，三者共存：

1. **并行翻译**：编排器按拓扑分层（`parallel_groups`），同层内独立模块可并行派发多个 translator agent。
2. **worktree 写隔离**：并行安全由 git worktree 隔离提供——每 agent 一个 worktree，防多 agent 同时写主树；逐层合并 + 整组 `cargo check` 真门（沿用 [MDR-003](003-m2-parallel-write-isolation.md)）。
3. **串行人工审批**：译好的模块进入待审队列，人逐个 review 签批（[MDR-019](019-post-translation-review-gate.md) 译后签批门）。

**重申 MDR-003 有效**（不取代）。SCC 破环机制（[MDR-006](006-scc-per-file-stub-first.md)）保留。

## 现状事实基线（决定 ORCH-01 的性质）

调查确认：并行/worktree 基础设施当前是 **coded-but-dead + prose-only**，从未接进活路径、从未端到端跑通：

| 组件 | 现状 |
|------|------|
| `core/src/types/parallel.rs`（Dispatch/Result/PortingRules 等协议类型） | 死代码，仅自身序列化单测，无 `.rs` 消费者 |
| `MigrationSequence.parallel_groups`（`graph/topo.rs`） | 在活函数 `migration_sequence()` 里算出，但**无生产代码读取、从不落盘** |
| `StrategyConfig.max_concurrent_agents`（`types/config.rs`） | 死字段，无读取者（默认值 4，与文档「默认 3」还不符） |
| 两层 done（`is_agent_done`/`batch_transition_done`，`state/machine.rs`） | 死代码，仅单测 |
| CLI 命令层 | 零并行输出；`graph topo-sort` 只吐 `order`，不含 `parallel_groups` |
| Plugin `workflow.md` | 指示从 state 读 `migration_sequence.parallel_groups`——**该字段 CLI 从不写入**，指向不存在的落盘字段 |
| worktree 机制 | **两套并存且矛盾**：手动 `git worktree add`（run.md）vs Task `isolation:"worktree"`（workflow.md） |
| 端到端「多 agent 并行 + worktree + 逐层合并」测试 | **不存在** |

**含义**：「保留并行」**不等于「别删可用代码」**——没有可用的并行可保留。要有真正的并行，ORCH-01 须从「文档收口」**重定义为实现任务**：把 `parallel_groups` 接进 CLI/state 输出、在两套 worktree 机制中择一并删另一套、建逐层合并 + reconcile + 整组真门的编排、补端到端集成测试。工作量远超原「1d worktree 生命周期代码化」，**单独 Sprint 正式估时**（本文件不含落地清单，留待专门规划）。

## 真正存在的权衡（留待 ORCH-01 实现处理，非否决理由）

1. **审查经验前向传播**：严格串行时，模块 A 的 review 反馈能指导 B 的翻译；并行译好一批后再审则该批吃不到 A 的反馈。可分波次并行 / 按反馈重译缓解。
2. **共享文件写冲突**：独立模块也常同改 `Cargo.toml` / `lib.rs` mod 声明 / 共享 `Error` 枚举——需 worktree 逐层合并解决三路冲突 + 语义冲突（orphan rule / E0119 coherence，仅整组编译才暴露）。
3. **并行宽度受依赖限制**：只有拓扑独立模块能并行，依赖链本就串行；实际并行度取决于依赖图形态。

## 影响

- **不取代 MDR-003**，反而重申其并行写隔离方案有效。
- **不影响 MDR-006**：SCC 破环（折叠 composite + 契约 + stub + 逐文件填空）保留。
- **[MDR-019](019-post-translation-review-gate.md)** 译后签批门是让「并行翻译 + 串行审批」共存的关键机制——并行译好的模块靠它排队供人串行审。
- **ORCH-01 重定义**为「并行 + worktree 编排真正落地并跑通」，单独规划估时。
