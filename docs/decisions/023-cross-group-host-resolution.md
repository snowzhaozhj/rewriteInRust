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

这条同时**取消了 MDR-022 决策 2 的一半理由**：`Ambiguous` 引用不再是跨组破坏的「唯一检出通道」，故 `reset` 清整个 `blocked_by` 数组不再连带擦掉唯一诊断。repair 保留 `Ambiguous` 依旧正确，但理由收窄为「repair 只删无处归属的条目，而歧义引用的实体是存在的」。

仓内该失实声明共 **7 处**，已逐处如实改：`machine.rs` 的 `repair_ghost_blocked_by` doc / 测试注释 / 测试断言消息、`lib.rs:438` 的 `state repair` **clap long help**（用户可见输出）、`06` ×2、`run.md`；`09-appendix-schemas.md` 另有 2 处措辞不同的同义声明（「这一检出通道」+ 只描述形态⒜）亦已改。

**处数值本身踩过一次坑**：PR 正文与本 MDR 初版都写「5 处」，那是用单行 `rg 唯一检出通道` 得到的——而其中两处**跨行断开**（`…唯一检出` + 换行 + `通道）…`），单行 grep 扫不到；`09` 的两处则是**换了措辞**（「这一」而非「唯一」）。前者由编排器用 `rg -U` 多行模式补全，后者由设计契约视角实测指出。「文档里的具体处数」正是本仓反复出问题的那类值，此处如实留痕。

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

### 决策 6：告警对命令行为的断言**不做穷举**，只举例并明示不穷举

跨组告警要告诉编排器「这类 key 上什么会失败」。这条决策改过两轮，两轮都是被实证推翻的：

**初版（推断）**：按推断写「`state transition`/`reset`/`recover`/`repair` 会直接报错」。自查逐条真跑六个命令，发现**不带 `--module` 的全量 `state repair` 并不报错**（它不做 key 归一，照常 noop）——一条靠推断写下的失实声明。

**第二版（假守卫）**：把清单收成 `KEY_NORMALIZING_COMMANDS` 常量，告警插值它，e2e 断言「文案列举的每条都被真跑过」，并自称沿用 MDR-022 决策 7 的绑定手法。**设计契约视角实测推翻了这一版**：

- ⒜ 那 4 条清单**合并时就已失实**。`canonical_module_key` 有 7 个生产调用点，实测至少 8 条 CLI 命令命中同一行为——除清单里的 4 条外，`state record-metrics` / `review-gate` / `approve` / `update` 同样返 `E012` + 同一归因（编排器已独立复现全部 4 条）。
- ⒝ 那个 e2e 断言比的是「**手写 cases == 手写常量**」，两侧都是人工维护，**结构上无法检出「常量漏了代码里真实存在的命令」**。它与 `reset_force_reason` 那种真双向守卫（一侧遍历 `ModuleStatus::iter()` 过真实函数）不是同一强度，而第二版恰恰自称沿用的是那套手法。

**定版**：穷举「凡走某内部函数的命令」需要静态分析，**手写清单假装它是双向守卫比不写更糟**——它给下个维护者虚假的安全感。故：

- 常量改名为 `KEY_NORMALIZING_COMMAND_EXAMPLES`，doc 头一句就写「⚠️ 这不是穷举清单」；
- 告警与 `run.md` 的措辞从枚举改为**性质** + 举例 + 明写「不止这些」：「凡按 key 归一的单模块操作（带 `--module` 的命令，如 … 等，不止这些）」；
- 删掉 `covered == declared` 那条假守卫，e2e 改为**逐条真跑 8 条已知同型命令**并断言 `error_code == E012` + 归因指向 `member_files`（覆盖是真的，只是不声称穷举）；
- 真守卫的修法记入后续 TODO（从 clap 树取全部带 `--module` 的叶子命令、在坏 state 上逐个跑、只允许显式豁免——沿用 `design_command_table.rs` 的 `cli_leaf_commands()` 手法）。

补一条实测细节：`state update` 的 CAS 检查排在归一**之前**，`--cas-version` 不对会先返 `E007` 而根本走不到宿主判定（e2e 最初写 `1` 即撞上这点）。**命令内部的检查顺序决定哪个错误先报**，这也是「对命令行为下断言必须逐条真跑」的一个具体理由。

六条命令的实测行为（同一坏 state）：

| 命令 | 实测 |
|---|---|
| `state transition --to done` | `E012`，归因指向 `member_files` |
| `state reset --force` | `E012`，同上 |
| `state recover --policy retry` | `E012`，同上 |
| `state repair --clear-ghost-blocked-by --module <M>` | `E012`，同上 |
| `state record-metrics` / `review-gate` / `approve` / `update --cas-version <正确值>` | `E012`，同上（第二版清单漏掉的 4 条） |
| `state repair --clear-ghost-blocked-by`（全量） | **`status:ok`、`was_noop:true`**——不做 key 归一，故不受影响；但它也清不掉本问题 |
| `state batch-transition-done --module <M>` | 不硬错，`skipped.code=broken_partition` |

### 决策 7：单次 resolve 的 O(成员总数) 是正确性的代价，但**不得在循环里重复建表**

`master` 的 `canonical_module_key` 对登记模块走 `contains_key` **O(1) 早返回**——而那条快路径**正是本 PR 要消灭的缺陷**（它不反查 `member_files`）。故单次归一必然是 O(全部 `member_files` 条目)，无法再有 O(1) 快路径。单条命令调用一次，真实规模（百级模块）下微秒级，可接受。

但**设计契约与类型设计两个视角独立报出**：`batch_transition_done` 在 `for name in modules` 循环里调 `resolve_module_host`，每个模块白建一次索引，规模大时呈平方。设计契约的 A/B 实测 4000 模块 33s vs master 0.87s。

编排器独立复现并确认修复（debug profile，同一 state，`batch-transition-done` 传 N 个 `--module`，多轮取稳定值）：

| N | master | 修复前（变异复现） | 修复后 |
|---|---|---|---|
| 2000 | 0.45s | 5.54s | 0.38s |
| 4000 | 0.49s | **17.33s** | 0.48s |

修复前 2× 规模 → 3.1× 时间（平方），修复后与 master 持平。修法两处：① 归一提到循环外一次性完成（借用检查所致：循环持 `&mut self`，索引借自 `self.state_file`，故先在不可变块里把入参归一成 owned 结果）；② 新增私有 `approve_canonical`（`approve_module` = 归一 + 调它），否则 batch 已归一完毕却再走 `approve_module`，每个模块白建**第二次**索引。

**教训**：决策 5 的性能表只测了 `validate state --check-blocked`（那条路径各只建一次索引），而同一 PR 新改的 batch 路径**从未被测**，MDR-022 待办 5 却已标为「已收口」——这是「测试定义域窄于行为定义域」的又一次复发。同批订正 `host_index.rs` 那句「不提供查单条的自由函数」：它被同 PR 的 `resolve_module_host` 当场否证，已改为如实说明「每次调用重建索引、不得在循环里调」。

## 影响

- **行为变更（非破坏性 API，但改既有判定）**：「key 既是登记模块、又被别组列为成员」这类引用从「合法、可能判 ready」变为「一律非终态、不判 ready，且对该 key 的状态操作硬错」。5 个消费方（`check_blocked_modules` / `detect_blocked_cycles` / `scan_ghost_references` / `validate_state` 跨组告警 / CLI `cmd_state_deps`）连带受影响，逐个有测试。
- **`skipped.code` 新增 `broken_partition`**：机读契约的**增量**（旧值域全部保留）。已同步 `06` 值域声明与 `workflow.md` 分流列表。
- **新增 pub 类型** `HostIndex` / `HostResolution`、pub 方法 `resolve_module_host`、pub fn `broken_partition_message`；新增私有 `approve_canonical`；删除私有 `resolve_blocked_ref`（无 pub 移除）。
- 告警文本变化（`warnings` 是用户可见文本、非机读契约；`plugin/` 对旧措辞零依赖）。
- **`state deps` 在依赖闭包含坏划分文件时整命令硬错**——这是新增的失败路径。权衡：宿主不唯一意味着「该依赖属于哪个组」不确定，而组状态决定就绪；即便两个宿主当下终态性相同（选谁都判 ready），那也是**按当下状态推导**，正是 MDR-021/022 反复被推翻的形状。故宁停不错，并把损坏暴露出来促使修复。**不涉及该文件的模块不受影响**（只有入参归一或其依赖闭包命中坏划分才报错）。
- 测试 897 → 914。

## 验证

- **负向实证六轮**（独立 worktree / 变异，全部报红且归因精确）：
  1. `resolve` 恢复「命中即返回」→ **5 层同时红**（e2e + host_index ×2 + machine + validate），即原始缺陷复现；
  2. 自引用计入宿主 → **11 红**（6 个既有 e2e + 5 个新反向守卫），证明决策 2 的排除是必须的、且守卫有区分力；
  3. 跨组告警退回「沿 `blocked_by` 撞见」→ 只有 `test_broken_partition_warns_without_any_blocked_by_reference` 红，证明它是该覆盖的唯一守卫；
  4. `broken_partition` 码退回 `not_found` → 分码测试红，报错里两条都是 `not_found`；
  5. 从 e2e 的 case 表里删掉 `state recover` → 覆盖完整性断言红并列出集合差集（**该断言后按设计契约结论整体删除**，见决策 6——它比的是两份手写清单，不是真守卫）；
  6. 归一退回循环内 → **性能回归复现**（N=4000 17.33s，master 0.49s，修复后 0.48s）。
- **端到端锁** `smoke_auto_unblock_refuses_when_dep_is_registered_and_owned_by_another_group`：按决策 6 的表逐条断言（告警 / 不判就绪 / 不解除且不落盘 / **8 条命令**硬错且 `error_code==E012` / 全量 repair 不报错 / batch 分码）。
- **「唯一实现」已用跨行 grep 核实**：`rg -Un --multiline "member_files[\s\S]{0,200}?(iter\(\)\.any|contains\(|find\(|\.get\()" crates/` 在**生产代码里零命中**（全部命中都在测试文件），即三份反查判定已确实收敛（`HostIndex::build` 用 `for file in members` 建表，不属该模式）。这条核实是必要的：编排器最初用单行 grep 只找到两份判定，漏掉了 `cmd_state_deps` 那一处。
- 测试 897 → **914**，本地 `just ci` 全绿；**远端 CI 针对 `e40af62` 5 项全过**。

## 后续 TODO（记账，非阻塞）

1. **`skipped.code` 值域无 CI 守卫**：真值源是 `machine.rs` 里散落的字符串字面量，`06`/`workflow.md` 的声明靠人工同步。本 PR 新增一个码即手工改了两处文档；下次新增仍会漂。修法需先把这些码收成枚举或常量（同 `ModuleStatus`/`--status` 的先例），再仿 `subagent_call_status_domain_is_consistent_across_docs` 加集合相等守卫。**同批发现**（设计契约）：`BatchDoneOutcome::skipped` 的 doc 里列的 `policy_rejected` / `transition_rejected` 在实现中**不存在**（真实码集来自 `approval_error_code()` 的 9 个前缀码 + `unexpected_rejection` 兜底）——pre-existing 漂移，本 PR 恰好在这行加了 `broken_partition` 而未顺手订正，一并留给这条待办。
2. **「凡按 key 归一的单模块操作」缺真守卫**：见决策 6。修法是从 clap 树取全部带 `--module` 的叶子命令（沿用 `design_command_table.rs` 的 `cli_leaf_commands()`），在坏 state 上逐个跑，断言「要么 `E012` 且归因指向 `member_files`，要么在显式豁免列表里」。这样新增命令会被自动纳入检查，而不是靠人记得改常量。
3. **`broken_partitions()` 的宿主顺序缺确定性守卫**：`hosts.sort()` 摘掉后测试只是**概率性**变红（`HashMap` 迭代序偶然可能正好有序），故本次未把它列为负向实证的一轮。同型问题在 `scan_ghost_references` 的排序上已有专门测试，此处可仿之——但需构造足够多的宿主才能把假阴性概率压低。
4. **`E012` 的 `suggestion` 对本错误误导**（设计契约，pre-existing）：实测返回「配置错误，请检查配置文件」，而本错误的处置是改 `migration-state.json` 的 `member_files`，与配置文件无关。根因是 `MigrateError::Config` 的通用映射；`06 § 10.7` 的 `E012` 行也未列入这条新的硬错来源（该行以「等」收尾，勉强可覆盖）。
5. **MDR-022 待办 1**（`transition_inner` 下沉「离开 blocked 需依赖全终态」）仍挂账，与本 PR 无关。
