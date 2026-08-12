# MDR-022: `state repair --clear-ghost-blocked-by` 非破坏性清理边界（收口 MDR-021 待办 1 后半段）

- **状态**: 已落地（PR #90 于 2026-08-13 合并到 master `dac8950`）
- **日期**: 2026-08-12（决策）/ 2026-08-13（合并）
- **范围**: 幽灵 `blocked_by` 引用的**处置**入口——新增 `state repair` CLI 命令（core `repair_ghost_blocked_by` + `GhostRepairOutcome`/`ClearedGhostRef`/`RepairedModule`）、`validate state` 告警文案从「未提供处置命令」改为指向本命令。改 `cli/`（core + CLI）、`plugin/`（SKILL.md / run.md）、`docs/design/`（06 命令表 + 06 § 10.7 + 09 附录）。

## 背景

[MDR-021](021-no-json-schema-validation.md) 待办 1 的**检出层**已于 PR #88 交付（master `eb94959`）：`validate state` 会点名「哪个模块的 `blocked_by` 指向未登记 key」，`--check-blocked` 在 `data.ghost_refs` 给逐条机读明细。但**处置为零**——告警正文只能写「当前 CLI 未提供自动处置命令（后续版本将提供非破坏性清理入口）」。用户拿到精确诊断，却没有一条可执行的命令。

这个状态是 #88 第四轮审查主动选择的结果，不是遗漏：第三轮曾补过一层「处方」，按四个正交谓词（能否进 blocked / reset 是否需 `--force` / 项目是否 graduate / blocked 有无锚点）为每个 `ModuleStatus` 推导「该跑哪条命令」，被第四轮实证推翻——它的依据只是「看一条出边」，却对多步可达的状态下了「不可能」断言，而反例存在（`X→blocked→X` 两步往返零代价清引用；`degrade_*→translating→blocked` 两步可达）。既已知为假就不能合并，故整层移出、留待本 MDR 重做（详见 MDR-021 文末「第四轮四视角 + 拆 PR」）。

## 决策

### 决策 1：处置是**单一确定性动作**，不是按状态推导的处方

`state repair --clear-ghost-blocked-by` 对全部 11 个 `ModuleStatus` **一视同仁**：删除 `blocked_by` 里无处归属的条目，别的什么都不做。

**这是本 MDR 最重要的一条**，因为它从结构上消除了前一轮出错的可能：正因为动作单一、不随状态分叉，它**不需要任何可达性判断**——既不必判「这个状态能否再进 blocked」，也不必判「transition 能不能修这一类」。前一轮的全部错误断言都产生于「为不同状态选不同命令」这个需求，需求消失，断言也就无处可写。

推论（已落进代码注释与文档）：告警文案**不得**再按状态分叉给不同命令，也**不得**声称 repair 是唯一入口——`transition` 的两步往返仍然能清掉引用（见决策 5），声称唯一即又是一条假断言。

### 决策 2：只删幽灵条目，绝不清空整个 `blocked_by` 数组

`Resolved`（能归一到宿主模块的引用，与宿主是否终态无关）与 `Ambiguous`（宿主歧义）原样保留。

`Ambiguous` 的保留是硬要求而非洁癖：同一文件被多个组列为 `member_files` 时，`validate_state` 的跨组不变量告警**只经 `blocked_by` 里的这条引用**才能扫出来。第四轮实证 `reset` 清整个数组会把这条通道连带擦掉，损坏还在而诊断没了。

**限定（异构交叉审查实证，pre-existing，见下方 TODO 4）**：上面这句只对「被引 key **不是**登记模块、而被 ≥2 个组列为成员」这一形态成立。另一形态——**被引 key 既是登记模块、又被别的组列为成员**——目前**根本检不出**：`resolve_blocked_ref` 先查 `modules` 命中就早返回 `Resolved`，不再反查 `member_files`，于是跨组互斥已被破坏却判成合法引用。实测后果不止漏诊断：`shared.ts` 登记为 `done` 而 `g1`（`translating`）把它列为成员时，`validate state` 零跨组告警，`--auto-unblock` **真的解除**了引用它的模块（`unblocked:["holder"]`），而该文件实际还在翻译中。故「唯一检出通道」这个说法**不可无条件引用**；repair 保留 `Ambiguous` 仍然必要（它是那一半形态的唯一通道），但不足以覆盖全部跨组破坏。

判据复用 `validate::scan_ghost_references`（内部 `resolve_blocked_ref` 是「什么算幽灵」的唯一实现），**不在 `machine.rs` 里重写第二套**。同一功能已经为「同一概念两份表示」付过账（MDR-021 待办 1 第三点：`BlockedCheckResult.missing` 与 `GhostReference` 并存导致口径不一致，最终删掉前者）。

### 决策 3：不改状态、不清进度字段、不发产物清理指令；恢复交给既有 `--auto-unblock`

与 `reset_module`（清 8 个进度字段 + 输出删 `.rs` 作用域，[MDR-015](015-reset-idempotent-retry-boundary.md)）正相反。

清完后若模块被 `check_blocked_modules` 判 `ready`（`ready = unresolved.is_empty()`——注意**已终态的剩余依赖不计入 `unresolved`**，故「剩余为空」只是就绪的充分条件之一，不可当代理），由既有 `validate state --check-blocked --auto-unblock` 按 `pre_blocked_status` 恢复（锚点缺失时以 `pending` 兜底）。**repair 不重复实现恢复逻辑**——那条路径已有测试与实战覆盖，重写一遍只会多一处会漂的判据。

### 决策 4：项目 `graduate` 态**放行**（与 `reset` 相反）

`reset_module` 在 `graduate` 下一律拒绝且 `--force` 不可绕，理由是它把 `done` 模块打回非终态、制造 MDR-015 禁止的「项目终态 + 非终态模块」矛盾。repair 不改状态，制造不出该矛盾；它删的是本就不该存在的引用。若此处也拒绝，`graduate` 项目里的这类损坏将**永远没有修复入口**。

### 决策 5：不收紧 `transition_inner`（MDR-021 待办 3 保留）

有了 repair 作为正当入口，将来可以把「离开 blocked 需依赖全终态」的不变量下沉进 `transition_inner`，让人工逃生统一走 repair 或显式 `--force`。**本 PR 不做**：那是改**既有**转换语义，牵动 `--auto-unblock` 流程、`run.md` 步骤 2 的 ② 号分流与既有测试，与「新增一条命令」合成一个 PR 会让审查面过大——而这个功能恰恰是被过大的审查面反复咬伤的（MDR-021 四轮）。

**代价须如实记住**：`X→blocked→X` 两步往返（两步都不需要 `--force`）仍然能清掉 `blocked_by` 并保留全部进度字段。它是待办 3 那处宽松的产物，也正因如此，文档不得声称 repair 是唯一入口。

### 决策 6：锚点问题只告警、不顺带修

清完后经 `check_blocked_modules` 判为**就绪**（剩余依赖全部终态，含清空）、而恢复锚点用不了的模块，CLI 层告警：

- `pre_blocked_status` 不在 `blocked` 的合法出边内（如 `done`）→ 按锚点恢复的路径会被转换矩阵拒；
- 锚点缺失 → `--auto-unblock` 以 `pending` 兜底，原状态不可复原而进度字段仍在。

**这两条是静态一步判定**（转换矩阵是静态的，`auto_unblock_modules` 恰好只走一条 `transition_module`），不是「任意步不可达」预言——后者正是被推翻过的那类断言，此处不重犯。判定放在 **CLI 层**，core 的 `RepairedModule` 只放字段不放谓词：把复合谓词塞进数据结构正是前一轮出错的形状（一个 bool 替多个谓词说话）。

**不动锚点**：改锚点会丢失恢复目标，属破坏性，与本命令的定位冲突。记账见下。

### 决策 7：告警文案与 e2e argv 共用一个常量

`validate::REPAIR_GHOST_COMMAND` 同时被告警插值与 e2e 的 argv 构造使用，故「文案给的命令」与「照做真能跑的命令」不可能漂移。

这是对两轮教训的正面回应：第二轮的 e2e 是「解析人读文案 + 跑手写 argv」，二者毫无绑定，被 4/4 视角各自变异穿透（`--keep-progress`/`--ghost-purge`/`--bogus-flag` 全部 PASS）；第三轮改成按机读 `remedy` 字段构造 argv，绑定成立了，但那个字段是按状态推导的、整层被推翻。**现在的处置是常量，所以绑定可以是常量比对**——这是决策 1 的直接红利。

同批订正一条失实断言：`test_validate_warns_on_ghost_blocked_by_reference` 原有 `hit.contains("populate-modules")`，注释声称它保的是「告警须给出重新同步的处置」，而文案里 `populate-modules` 一直是**否定**用法（「不要用」），子串断言在两种相反语义下都过。同型空断言在本功能上被抓过一次（MDR-021 第二轮的 `!advice.contains("populate-modules 同步")` 恒真），此处一并改为绑定到 `REPAIR_GHOST_COMMAND`。

## 影响

- **无破坏性变更**：新增命令与新 pub 类型，不改既有命令的输入/输出契约。`validate state` 的告警**文本**变了（`warnings` 是用户可见文本、非机读契约；`plugin/` 侧对旧措辞零依赖）。
- **命令数 30 → 31**：`06:105` 表头计数、06 命令表行、SKILL.md 命令清单（分组由「断点续跑（ROB-01a/b/c）」改名为「断点续跑与数据修复（ROB-01a/b/c + MDR-022）」）+ `skill_md_command_list_groups_are_frozen` 期望值同步。三处都有 CI 守卫，漏一处即红——本次实测确实报红（35 vs 36 条）后才补齐。
- **`run.md` 步骤 2 的 ③ 号分流有了落点**：从「CLI 只检出、不处置，人工处置须守两条硬约束」改为「跑 repair → 重跑 `--check-blocked` 重新分流」，并保留三条「别拿来代替它的路」（transition / populate-modules / reset）。
- 测试 886 → 897（8 core + 3 e2e）。

## 后续 TODO（记账，非阻塞）

1. **MDR-021 待办 3**：把「离开 blocked 需依赖全终态」下沉进 `transition_inner`，使 transition / CAS / auto-unblock 三条路径共享同一不变量，人工逃生走 repair 或显式 `--force`。见决策 5。
2. **非法 / 缺失 `pre_blocked_status` 锚点的修复入口**：当前只告警（决策 6）。若将来要修，须先定「恢复目标由谁决定」——锚点丢了之后 CLI 无从推断原状态，可能得由人给 `--to`。
3. **`--module` 之外的批量收窄**：当前只有「全部」与「单模块」两档。若真实场景出现「只清某个 sprint / 某组」的需求再议（当下无证据，YAGNI）。
4. **⚠️ `resolve_blocked_ref` 的歧义判定漏一整类形态（pre-existing，本 PR 异构交叉审查实证）**——「被引 key 既是登记模块、又被别的组列为 `member_files` 成员」时，早返回让它判 `Resolved` 而非 `Ambiguous`。**后果是数据损坏而非仅漏诊断**：实测 `--auto-unblock` 会据错误宿主判就绪、真的解除引用方（`shared.ts` 登记 `done` 而 `g1` 是 `translating`，holder 被解除且落盘）。`machine.rs` 的 `canonical_module_key` 是同型早返回，同样漏。

   **判为独立 PR**：修法本身小（把「直接命中」也算一个 host、总数 >1 即 `Ambiguous`），但它改变 #88 检出层的**既有判定行为**（这类引用从「合法且可能 ready」变成「一律非终态、不判 ready」），4 个消费方（`check_blocked_modules` / `detect_blocked_cycles` / `scan_ghost_references` / `validate_state` 跨组告警）连带受影响，须自带测试与负向实证。本 PR 正处在审查修复阶段，再塞一个行为变更进来恰是「审查面过大」的复发——那是这个功能被咬四轮的根因。
5. **`resolve_blocked_ref` 在「全为幽灵」的坏 state 上呈二次复杂度（pre-existing，同上审查实测）**——每条未命中的引用都全表扫 `modules` + `member_files`。实测 1000 模块 0.72s / 5000 模块 1.38s / 10000 模块 4.55s / **20000 模块 14.20s**。与 MDR-021 记的「10 万模块 1.11s 近线性」不矛盾：那测的是**合法**引用（`contains_key` 哈希命中直接返回），这测的是全未命中路径。修法是一次性建 `key/member_file → 宿主` 索引 + 待删集合用 `HashSet`。真实规模百级故不阻塞，但 repair 恰是面向坏 state 的命令，坏 state 正是全未命中的那一侧。
