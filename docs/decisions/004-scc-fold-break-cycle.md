# MDR-004: 破环——SCC 折叠为 composite 翻译单元

> 状态：已定稿（2026-06-20，M2-SCALE-SCC 已实现并通过测试）。本 MDR 收口设计正文 04/03/02 原「环→拒绝+降级」规定与实现的分叉。
>
> **后续精化**：本 MDR 定「SCC 折叠为 composite 组」（保留），但其「整组一次翻译」一环已被 [MDR-006](./006-scc-per-file-stub-first.md) 精化为「契约+stub→逐文件填空→整组编译门」——**SCC 是编译门禁单元，不是翻译单元**。折叠机制不变，折叠后的翻译粒度改为单文件。

## 背景

原设计将源码循环依赖（强连通分量 SCC）视为迁移障碍，统一走「拒绝填充 + 降级」路径：

- **04 § 5.7.6**：`topo-sort` 检测到环则非零退出，`/migrate analyze` 暂停，提示二选一降级——(a) 源项目临时打破某条循环导入后重跑；(b) 跳过环内模块标 `requires_manual_review`，人工拆环后再迁移。
- **03 § 4.2**：同上两种降级 (a) 打破环 / (b) `requires_manual_review`，完整 SCC 检测推迟到 M2 `graph cycles`。
- **02 § 3.4**：拆分决策树把「循环依赖」与「拆分后仍 > 100K」并列为**触发降级路径**的条件。

这套设计的隐含前提是「源码环 = 翻译障碍，必须人工破环或 FFI/手动拆环才能继续」。该前提不成立（见下决策论据），导致对含环项目（含 re-export 间接环、event-bus ↔ handler ↔ emitter 类互引）一律阻塞，迁移无法自动推进。

## 决策

`populate-modules` 不再拒绝含环图。每个 SCC **缩点折叠为一个 composite 模块组**，作为单一迁移单位整组翻译：

- **module key** = 组内字典序最小成员的 NodeId（组代表）。
- **`ModuleState.member_files`** 列出组内全部互引源文件（单文件模块省略该字段）。
- 在**缩点 DAG**（SCC 缩点后无环）上排 sprint 层级（叶组 = sprint 1）。
- translator 把整组一次性翻译为一组 Rust `mod`。

**核心论据**：Rust **同一 crate 内 mod 之间允许互相 `use`**（mod 间循环引用合法，只有 crate 间禁环）。源码环落到同一 crate 的一组 mod 即可消解，**不是翻译障碍**——无需破环、shared-types 抽取或 FFI。原「打破环 import / 拆环」的人工前置工作被取消。

## 与旧设计的冲突点（逐条）

| 旧规定 | 冲突点 | 新语义 |
|--------|--------|--------|
| 04 § 5.7.6「topo-sort 有环降级」二选一 (a)/(b) | `populate-modules` 不再要求人工破环；环由折叠消解 | populate 折叠为 composite，不暂停 |
| 04 § 5.7（决策树注「循环依赖→FFI」） | 环不再是 FFI 触发条件 | FFI 退化为「单 SCC 超预算」兜底（TODO） |
| 03 § 4.2「循环依赖处理」(a) 打破环 / (b) `requires_manual_review` | 环内模块不再标 `requires_manual_review` | 折叠后正常排 sprint 迁移 |
| 02 § 3.4 决策树「循环依赖 或 拆分后仍 > 100K → 触发降级」 | 「循环依赖」从降级触发条件中移除 | 仅「单组超预算」保留为降级（FFI 切分，TODO） |

## 实现要点

1. **`member_files`**（`types/state.rs`）：`Option<Vec<String>>`，`None`=单文件模块，`Some([..])`=composite 组成员文件 NodeId 列表，`#[serde(skip_serializing_if = "Option::is_none")]` 故单文件模块序列化时省略。
2. **缩点 sprint**（`graph/topo.rs` `migration_sequence`）：Tarjan 求 SCC → 缩点为迁移单元（覆盖全部 File 节点，单文件 = 单成员组）→ 缩点 DAG 上计算每组层级（迭代式避免深链栈溢出）→ sprint = 层级 + 1。
3. **`state deps` 组感知门禁**（`cli/src/lib.rs`）：run 阶段依赖就绪门禁**不能**用 `graph deps`（纯图、文件级——折叠后组内成员不在 `modules` 表会落空），改用新增的 `state deps`：把 composite 组成员的文件级依赖**映射回组代表 key**、剔除组内自依赖、按终态判就绪，输出 `{dependencies:[{module,status,ready}], all_ready, blocking}`。
4. **E002 / topo-sort 语义分叉**（见下）：`graph topo-sort` 命令对有环图仍返回 E002，`populate-modules` 折叠不拒绝——破环收口在 populate，**有意为之**。

## E002 / topo-sort 语义分叉

`graph topo-sort` 是**纯排序原语**（Kahn 算法），对有环图仍返回 `E002 CyclicDependency` 并列出环路径——这是它的契约，不变。破环逻辑收口在 `populate-modules`（Tarjan SCC 缩点）而非 `topo-sort`，因此 `topo-sort` 拒绝、`populate-modules` 折叠的语义差异是**有意为之，不是 bug**。`dump_tiers` example 直接取 File 节点逐文件评估 tier、绕过 populate 的缩点分组——因为 tier 是 per-file 的、与模块如何分组无关，直接取 File 节点比走 populate 更直接。

## composite 与 token 预算 / Phase A/B 单模块模型的关系

Phase A/B 双阶段翻译与 token 预算检查（02 § 3.4）以「单模块」为单位建模。composite 组作为单一迁移单位整体进入翻译循环：意图摘要、预算预估、Phase A 忠实翻译 / Phase B 惯用化均以**整组**为粒度。

**超预算兜底（TODO，未实现触发路径）**：当单个 SCC 大到组内全部互引文件超上下文预算时，无法整组塞入一次翻译。此时退化为 FFI 切分——把组内部分成员切到 FFI 边界以拆分翻译单位。这是 FFI 在破环后唯一的残留用途（从「环的默认降级」退化为「超预算兜底」）。当前实现**未实现该触发路径**，留作 TODO。
