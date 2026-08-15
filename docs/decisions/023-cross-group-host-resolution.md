# MDR-023: 跨组宿主解析统一——不再命中即返回（收口 MDR-022 待办 4/5）

- **状态**: 已落地（待用户拍板合并）
- **日期**: 2026-08-15
- **范围**: `member_files` 跨组互斥不变量的**判定**与**检出**。新增 `core::state::host_index`（`HostIndex` / `HostResolution`），删除 `validate::resolve_blocked_ref`（判定并入索引）、重写 `MigrationStateMachine::canonical_module_key` 的归一、`validate_state` 的跨组告警改扫全量划分、`batch_transition_done` 的 `skipped.code` 新增 `broken_partition`。改 `cli/`（core）、`docs/design/06`、`plugin/`（run.md / workflow.md）。

## 背景

[MDR-022](022-ghost-blocked-by-repair-boundary.md) 待办 4/5 是 PR #90 异构交叉审查抓出的两条 pre-existing 缺陷，当时判为独立 PR（塞进审查修复阶段的 PR 恰是「审查面过大」的复发，而那个功能被过大的审查面咬了四轮）。两条同源——都出自同一个函数的同一个形状——故合成本 PR。

同一个不变量（`member_files` 是文件节点的**划分**，每个文件至多属一个模块）此前有**两份判定**：

- `validate::resolve_blocked_ref`——处置是降级告警（校验命令不能硬错，旧文件须可读）；
- `MigrationStateMachine::canonical_module_key`——处置是硬错（MDR-015:55：静默取一个宿主会让破坏性的 reset 清空**错误模块**的进度字段）。

处置策略不同是对的，但**两份判定都写成了「先查 `modules`，命中就早返回」**。于是「被引 key 既是登记模块、又被别的组列为 `member_files` 成员」这一整类破坏，在两条路径上同时被判成合法。

## 独立复现（编排器用真实 CLI，非推断）

`/tmp` 造同形 state：`file:shared.ts` 登记为独立模块且 `done`；`g1` 组（`translating`）的 `member_files` 含 `file:shared.ts`；`holder` 是 `blocked`、`blocked_by=["file:shared.ts"]`。

| 命令 | 修复前实测 | 应有 |
|------|-----------|------|
| `validate state` | `valid:true`，**零跨组告警** | 报出宿主不唯一 |
| `validate state --check-blocked` | `ready_to_unblock:["holder"]`、`ghost_refs:[]` | holder 留在 `still_blocked` |
| `validate state --check-blocked --auto-unblock` | `unblocked:["holder"]`，**落盘**（`blocked → translating`、`blocked_by` 清空） | 一个都不动 |
| `state repair --module file:shared.ts` | 静默归一到它自己（`scope:"file:shared.ts"`） | 拒绝并指出划分被破坏 |

**后果是数据损坏，不止漏诊断**：`holder` 真的被解除，而它依赖的那份 `file:shared.ts` 还在 `g1` 组里翻译中。

## 决策

### 决策 1：判定收成唯一实现，且**不早返回**

新增 `state::host_index::HostIndex`。`resolve` 把两个来源（自己是登记模块 / 被某组列为成员）的宿主**全部收集完再判个数**：0 → `Missing`，1 → `Resolved`，≥2 → `Ambiguous`。

两侧共用它，各自保留自己的处置策略。这是「同一概念两份表示必然漂移」的又一次实证——上一次是 `BlockedCheckResult.missing` 与 `GhostReference` 并存致口径不一致（MDR-021 待办 1 第三点，最终删掉前者）；这一次更糟，两份实现不是漂移而是**同时错在同一处**，因为它们是互相照着写的。

`HostIndex` **不提供「查单条」的自由函数**：那正是让调用方在循环里退回二次复杂度的入口（见决策 5）。

### 决策 2：自引用不算破坏（本次最容易踩的反向坑）

建索引时跳过「组代表把自己列进 `member_files`」那一项。

前提是实测确认的，不是假设：`populate-modules` 落盘的 `member_files` 就是 `DecompUnit.members` 全体，而 `decompose.rs:38` 注明「成员文件（NodeId 字典序；**第一个作 module key 代表**）」——组代表**必然**在自己的 `member_files` 里。若把这份自引用也计为一个宿主，则每个正常 composite 组的代表都会被判成跨组破坏。

负向实证（摘掉这条排除）：**11 个测试红**，其中 6 个是既有 e2e（`state repair` 全系、`stats quality`、`populate-modules` 的正常 composite 路径当场瘫掉）。这条反向坑比正向缺陷更能一击致命，故配了 5 个专门守卫。

### 决策 3：跨组检出改扫**全量划分**，不靠引用路径撞见

`validate_state` 此前是在遍历各模块 `blocked_by` 时顺带发现 `Ambiguous`。于是「破坏存在但没有任何 `blocked_by` 引用到它」时 `validate state` 一声不响，而下一步对该模块的 `transition`/`reset` 会硬错——**体检说健康、动手就报错**。

改为 `HostIndex::broken_partitions()`：扫全部 `member_files` 划分。判据取 `resolve` 的 `Ambiguous` 那一支（不另写一套「什么算破坏」，否则告警口径与判定口径各自漂移）。

这条同时**取消了 MDR-022 决策 2 的一半理由**：`Ambiguous` 引用不再是跨组破坏的「唯一检出通道」，故 `reset` 清整个 `blocked_by` 数组不再连带擦掉唯一诊断。repair 保留 `Ambiguous` 依旧正确，但理由收窄为「repair 只删无处归属的条目，而歧义引用的实体是存在的」。仓内 5 处「唯一检出通道」声明已按此如实改（`machine.rs` doc + 测试注释、`06` 两处、`run.md`）。

告警文本因此从「引用方 → 被引 key（同属 …）」改为「被引 key（同属 …）」——不再有「引用方」这一维。引用侧信息仍可从 `--check-blocked` 的 `unresolved` 得到。

### 决策 4：`canonical_module_key` 对两种形态一视同仁硬错

不因「硬错会挡住正常推进」而放宽：挡住正是目的（MDR-015 已确立「宁停不错」）。修 `member_files` 只能改 state 文件本身（CLI 没有改划分的命令，它是结构冻结字段），故硬错**不会堵死修复入口**——这与 MDR-022 决策 4 讨论 `graduate` 下放行 repair 时用的是同一条判据（会不会让损坏永远没有出口）。

顺带修一处归因错误：`batch_transition_done` 用 `let Ok(canonical) = … else` 把归一失败一律记成 `skipped.code=not_found`（「模块不在 migration-state.json」）。两类失败的处置方向相反（补登记 vs 修划分），合成一个码会把编排器指向错误的排查方向——同「一个判据替多个谓词说话」的失败模式。新增 `broken_partition` 码；错误文案由 `broken_partition_message` 单点提供，故 `canonical_module_key` 的报错与 `skipped.detail` 不会各说各话。

### 决策 5：索引化，顺带收口二次复杂度（MDR-022 待办 5）

旧实现每查一条引用就全表扫一遍 `modules` + `member_files`。合法引用走哈希命中直接返回，所以按合法数据测出来近线性（MDR-021 记的「10 万模块 1.11s」）；但**全未命中**的坏 state 走全表扫那一支，而面向坏 state 的 `state repair` 恰好总在这一侧。

实测（debug profile，同一 state 文件，`validate state --check-blocked`；该负载让三处扫描各跑一遍全表，故绝对值高于 MDR-022 待办 5 记的单函数数字）：

| 模块数（全部引用均未命中） | 修复前 | 修复后 |
|---|---|---|
| 1000 | 0.86–1.43s | 0.30–0.35s |
| 5000 | 4.05–4.11s | 0.15s |
| 20000 | **63.93–64.08s** | 0.46–0.52s |

修复前 5000 → 20000 是 4× 规模、**15.8× 时间**（平方）；修复后 3.1×（线性）。真实规模百级故不阻塞任何人，但 60 秒无输出在 CLI 上与挂死无法区分。

### 决策 6：告警对命令行为的断言由常量绑定，且逐条实测

跨组告警要告诉编排器「这类 key 上什么会失败」。初版按推断写了「`state transition`/`reset`/`recover`/`repair` 会直接报错」，自查时逐条真跑六个命令，发现**不带 `--module` 的全量 `state repair` 并不报错**（它不做 key 归一，照常 noop）——一条靠推断写下的失实声明。

修法沿用 MDR-022 决策 7 的手法：清单收成 `validate::KEY_NORMALIZING_COMMANDS` 常量，告警文案插值它，e2e 据它断言「文案列举的每条都被真跑过」（新增一条即须补一个 case，否则集合不等而报红）。判据本身也从枚举改为**性质**——「凡按 key 归一的单模块操作」，故将来新增的命令只要走归一就自然满足，声明不因新增而失实。

实测结果（同一坏 state，逐条）：

| 命令 | 实测 |
|---|---|
| `state transition --to done` | `E012`，归因指向 `member_files` |
| `state reset --force` | `E012`，同上 |
| `state recover --policy retry` | `E012`，同上 |
| `state repair --clear-ghost-blocked-by --module <M>` | `E012`，同上 |
| `state repair --clear-ghost-blocked-by`（全量） | **`status:ok`、`was_noop:true`**——不做 key 归一，故不受影响；但它也清不掉本问题 |
| `state batch-transition-done --module <M>` | 不硬错，`skipped.code=broken_partition` |

## 影响

- **行为变更（非破坏性 API，但改既有判定）**：「key 既是登记模块、又被别组列为成员」这类引用从「合法、可能判 ready」变为「一律非终态、不判 ready，且对该 key 的状态操作硬错」。4 个消费方（`check_blocked_modules` / `detect_blocked_cycles` / `scan_ghost_references` / `validate_state` 跨组告警）连带受影响，逐个有测试。
- **`skipped.code` 新增 `broken_partition`**：机读契约的**增量**（旧值域全部保留）。已同步 `06` 值域声明与 `workflow.md` 分流列表。
- **新增 pub 类型** `HostIndex` / `HostResolution` 与 pub 方法 `resolve_module_host`；删除私有 `resolve_blocked_ref`（无 pub 影响）。
- 告警文本变化（`warnings` 是用户可见文本、非机读契约；`plugin/` 对旧措辞零依赖）。
- 测试 897 → 912（8 host_index 单测 + 3 machine + 3 validate + 1 e2e）。

## 验证

- **负向实证四轮**（独立 worktree，全部报红且归因精确）：
  1. `resolve` 恢复「命中即返回」→ **5 层同时红**（e2e + host_index ×2 + machine + validate），即原始缺陷复现；
  2. 自引用计入宿主 → **11 红**（6 个既有 e2e + 5 个新反向守卫），证明决策 2 的排除是必须的、且守卫有区分力；
  3. 跨组告警退回「沿 `blocked_by` 撞见」→ 只有 `test_broken_partition_warns_without_any_blocked_by_reference` 红，证明它是该覆盖的唯一守卫；
  4. `broken_partition` 码退回 `not_found` → 分码测试红，报错里两条都是 `not_found`；
  5. 从 e2e 的 case 表里删掉 `state recover` → 覆盖完整性断言红并列出集合差集（证明「文案列举 == 真跑过」这条绑定不是摆设）。
- **端到端锁** `smoke_auto_unblock_refuses_when_dep_is_registered_and_owned_by_another_group`：按上表逐条断言（告警 / 不判就绪 / 不解除且不落盘 / 四条命令硬错 / 全量 repair 不报错 / batch 分码）。
- 测试 897 → 912，`just ci` 全绿。

## 后续 TODO（记账，非阻塞）

1. **`skipped.code` 值域无 CI 守卫**：真值源是 `machine.rs` 里散落的字符串字面量，`06`/`workflow.md` 的声明靠人工同步。本 PR 新增一个码即手工改了两处文档；下次新增仍会漂。修法需先把这些码收成枚举或常量（同 `ModuleStatus`/`--status` 的先例），再仿 `subagent_call_status_domain_is_consistent_across_docs` 加集合相等守卫。
2. **`broken_partitions()` 的宿主顺序缺确定性守卫**：`hosts.sort()` 摘掉后测试只是**概率性**变红（`HashMap` 迭代序偶然可能正好有序），故本次未把它列为负向实证的一轮。同型问题在 `scan_ghost_references` 的排序上已有专门测试（构造 ≥2 条比对序），此处可仿之——但需要构造足够多的宿主才能把假阴性概率压低。
3. **MDR-022 待办 1**（`transition_inner` 下沉「离开 blocked 需依赖全终态」）仍挂账，与本 PR 无关。
