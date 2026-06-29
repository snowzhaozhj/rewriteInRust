# MDR-012: M3 遗留债批次 A 的三项设计偏离决策

- **状态**: 已决策
- **日期**: 2026-06-29
- **范围**: M3 遗留债清理批次 A（read_failures 门禁 + topo-sort --reverse + transition 组归一），PR #53

## 背景

M3 验收完成后清理 CoupledBatch 引入时记录的 pre-existing 工程债。批次 A 触及三处对外契约/设计文档约定，经 4 视角 PR 审查（主审 / 设计契约 / 专项错误处理 / 异构交叉）暴露需 MDR 记录的偏离，逐项决策如下。

## 决策 1：`read_failures` 高占比硬门禁（decompose / populate）

`graph decompose` 与 `state populate-modules` 在源文件读取失败占比 **≥ 50%**（`READ_FAILURE_ABORT_RATIO = 0.5`）时返回 `MigrateError::Config` 中止；低于阈值仍 `warn` 放行。

- **旧行为**：恒 `warn` 放行，「自身源码规模按 0 保守处理」后照常产出 plan。
- **问题**：当 `--root` 与 graph build 时的源码根不一致（如 monorepo 双根只命中其一），几乎全部文件读失败，却静默产出一份全 `self-size=0` 的退化 plan，污染后续所有 Sprint 规划与预算装箱。
- **阈值取值**：半数不可读已基本不可能是「个别文件被删/改名」；边界（恰好 50%）向安全侧倾斜中止（专项审查 nit）。错误文案不再断言「几乎必是根不一致」，而是「最常见原因…；若文件确已批量删除/重命名，请重建 graph」，并附首个失败路径样例。
- **与 MDR-011 §4.2 的取向差异**：MDR-011 的 `U>800` 规模兜底是「慢≠错、仅 warning、不改变行为」；本门禁相反——读失败是**正确性**问题（产出垃圾 plan），非性能问题，故取「可中止」契约。两者场景正交，不矛盾。

## 决策 2：`graph topo-sort` 拒绝破环，仅新增 `--reverse` 纯排序变体

原 TODO 记为 `graph topo-sort --members --reverse`。实现时一度让 `--members` 走 `migration_sequence()` 做 SCC 缩点折叠、对有环图返回 0——**设计契约审查判定违反冻结契约**：

- [04 § 5.7.6](../design/04-toolchain.md#576-图查询能力清单)：「`topo-sort` 是纯排序原语…**这是它的契约，不变**」「E002 / topo-sort 语义分叉**有意为之**」。
- [06 §11.2](../design/06-plugin-structure.md)：「**破环不在此命令**」。
- [MDR-004](./004-scc-fold-break-cycle.md)：破环收口在 `state populate-modules`（SCC 缩点折叠为 composite 组）。

**决策**：撤掉 `--members`，不在 topo-sort 引入第二条破环路径。理由：
- 改设计冻结契约（9 轮对抗审查收敛、03/04/06 三处一致的「不变」措辞）风险远高于撤实现。
- `--members` 功能**冗余**：组感知迁移顺序（含 `member_files` + `sprint`）已由 `state populate-modules` 落盘的 `migration-state.json` 提供，run 编排直接读取即可，无需 topo-sort 复刻 populate 的缩点能力。

保留 `--reverse`：纯排序变体，仅反转 `order`，**不破环**——有环仍返回 E002 非零退出，契约不变。新增 flag 已登记到 06 §11.2。

## 决策 3：`transition_module` 成员 key 归一（登记）

`state transition` 对 composite 组的**非代表成员** key（如折叠组里的 `file:types.ts`）发起转换时，先反查 `member_files` 归一到组代表再应用转换，与 `cmd_state_deps` 的组感知逻辑对称。

- **不改动状态转换矩阵**：仅在应用转换**前**做 key 解析，`from→to` 合法性判定不变，故不违反 [02 §3.4](../design/02-architecture.md) / 09-appendix 的状态机定义，属合理的 key 解析便利（EXTENSION 而非 DEVIATION）。
- **依赖不变量**：`member_files` 是文件节点的**划分**（跨组互斥）。该不变量成立时反查命中唯一；代码以 `debug_assert!` 钉住，防未来数据回归时静默错配到「随机」组。

## 影响

- CLI 对外契约新增：`graph decompose` / `state populate-modules` 可因高占比读失败返回错误；`graph topo-sort --reverse` 新增。
- `graph topo-sort` 破环契约**保持不变**（撤回越界实现）。
- `state transition` 容忍组成员 key（与 `state deps` 对称）。
- 设计文档同步：06 §11.2 topo-sort 行补 `--reverse`；本 MDR 为三项偏离的权威记录。
