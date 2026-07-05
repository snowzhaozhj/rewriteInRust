# MDR-018: 迁移工作流回归串行——移除并行翻译与 worktree 隔离（M4-ORCH-01）

- **状态**: 已决策
- **日期**: 2026-07-05
- **范围**: 取代 [MDR-003](003-m2-parallel-write-isolation.md)（M2 跨模块并行写隔离）。ORCH-01 由「worktree 生命周期代码化」重定义为「工作流串行化收口」。本文件记决策 + 下次开工的落地清单；实际改 `cli/` + `plugin/` + `docs/design/` 分离到后续开工 PR。

## 背景

MDR-003 定「M2 跨模块并行翻译：每 agent 一个 git worktree（含约束包）+ 逐层合并 + 整组 `cargo check` 真门」。ORCH-01 调研这套编排的落地方式时发现：文档里「手动 `git worktree add .wt/{module}`」与 `Task` 工具 `isolation:"worktree"` 两套机制并存、互相矛盾，且从未真实端到端跑通。经实证与对齐，用户拍板**砍并行、砍 worktree 隔离、回归串行**。

## 决策

**移除并行翻译，移除 worktree 隔离，迁移工作流永久回归串行。** 编排器（SKILL.md 驱动的主 session）按拓扑序**逐模块**推进，每模块走完整 run.md 循环（翻译→cargo check→验证→呈现用户 review→确认→状态机推进）后再进入下一个。translator SubAgent **不隔离**、直接在主工作树写 Rust；上下文经济由 `Task` 的**上下文窗口**隔离提供（干重活、回传摘要），文件系统无需隔离。

## 理由

1. **产品可审性（决定性理由）**：迁移是给终端用户的 human-in-the-loop 过程，用户要**看到迁移一步步发生**、逐模块 review 翻译是否行为等价。review 是人的认知活动、天然串行，一次只能专注一个模块。并行让多模块结果同时涌出堆成待审队列，人无法并行审查 → review 积压或退化成橡皮图章，信任基础崩塌。**即便并行技术可行也砍。**
2. **并行是坏投资**：`isolation:"worktree"` 的成果默认不流回主 session（隔离 worktree 焚毁前不 merge 则丢失），要用需自实现成果回收；且并行还须克服共享文件（Cargo.toml/lib.rs/共享 Error）三路合并冲突 + 语义冲突（orphan rule/E0119 coherence/feature 统一，只有整组编译才暴露）。成本远高于收益。
3. **速度非本工作流卖点**：卖点是可审 + 可控 + 可跟随的渐进迁移。并行优化速度（非卖点），代价是牺牲可审性（真卖点）——亏本买卖。

> worktree 隔离仅为并行安全（防多 agent 同时写主树）服务；串行下任一时刻只有一个 agent 动主树、天然无写冲突，隔离失去存在理由，二者一并移除。

## 取代与影响

- **取代 MDR-003**：并行写隔离方案（worktree + 约束包 + 逐层合并 + 整组真门）整体废弃。
- **影响 [MDR-006](006-scc-per-file-stub-first.md)**：SCC 破环机制（折叠为 composite + 契约 + stub + 逐文件填空）串行下**保留**；仅其「6b 同 worktree N agent 并行填空」改为**逐文件顺序填空**（签名仍由契约冻结，机制不变）。不取代 MDR-006。
- **不影响代码并发能力**：loom/shuttle、Send/Sync、intent「并发模型」字段、danger=Concurrency 分类等「翻译并发源码」的能力与本决策正交，全部保留。

## 落地清单（下次开工 · 一个收口 PR）

> 调研已确认 serde 兼容安全（无 `deny_unknown_fields`、全 `#[serde(default)]`、`MigrationSequence` 不反序列化）——删字段不破坏老 state/config；唯一会编译失败的是引用被删项的**测试**，须同步删。

**CLI 死代码移除**（全无生产消费者，调研确认）：
- 删整文件 `cli/crates/core/src/types/parallel.rs` + `types/mod.rs` 的 `pub mod parallel;`
- 删 `graph/topo.rs` 的 `MigrationSequence.parallel_groups` 字段 + `compute_parallel_groups()` + 私有 `compute_level()`（`MigrationSequence`/`order`/`cycles`/`scc_groups`/`compute_scc_level` 保留）
- 删 `types/config.rs` 的 `StrategyConfig.max_concurrent_agents` + 默认值
- 删整文件 `cli/crates/core/tests/petgraph_isolation.rs`
- 删 `state/machine.rs` 两层 done 三件套（`SUBSTATUS_AGENT_DONE`/`is_agent_done`/`batch_transition_done` + 单测）
- 同步删引用被删项的测试：`topo.rs` 单测、`tests/proptest_graph.rs`、`tests/ground_truth.rs`

**Plugin 文档串行化**（改前读 `docs/learnings/agent-skill-prompt-guide.md`）：
- `plugin/skills/migrate/workflow.md`：整篇重写为「按拓扑序逐模块串行调 run.md」——删 `--max-concurrent`/worktree/逐层合并/reconcile/清理/两层 done；保留 sprint 批量入口/断点续跑/headless/错误汇总/依赖门禁
- `plugin/skills/migrate/run.md`：删「并行编排（M2-SCALE-02）」整节 + 修正若干指针/锁句
- `plugin/agents/translator.md`：删并行节，但保留其中通用约束（共享写面最小化 4 条 + SCC 零共享写/签名冻结）
- `plugin/skills/migrate/SKILL.md`：简化「全局锁」段 + 删「并行模式下的锁策略（M2-SCALE-LOCK）」+ description/路由去「并行」
- verifier.md / analyze.md / adapters 无需改动

**设计文档一致化**（唯一权威，有界修剪）：
- `03-execution-model.md` §4.2.1 模式表 + §4.10 并行段 → 串行
- `06-plugin-structure.md` §10.5 D3 约束包 + 若干 M2 并行预留说明 → 标注被本 MDR 取代
- `09-appendix-schemas.md` `lock_token` 注释 → 并行已取消恒 null

**计划/状态**：PLAN-M4 ORCH-01 重定义已随本 PR 落地；实际收口完成后回填 STATUS + 验收标准。
