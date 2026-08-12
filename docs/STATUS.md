# 项目状态快照

> 每次会话结束前更新。新会话读此文件 → 找到 PLAN.md 对应任务 → 继续执行。

## 当前位置

- **位置（2026-08-12）**：master 在 `2b86691`，**开放 PR 仅 [#88](https://github.com/snowzhaozhj/rewriteInRust/pull/88)**、开放 issue 0。**#88 已按第四轮结论收窄为「幽灵引用检出层」**——第三轮补的**处方层**（`GhostRemedy`/`remedy`/状态网格/四类处置文案）被第四轮证明仍把「不可能」写进用户可见文案（依据只是「看出边」，而 `X→blocked→X` 两步往返零代价清 `blocked_by` 且保留全部进度、`degrade_*` 多步可达等反例存在），故**整体移出 #88、拆为后续 PR**（届时设计非破坏性入口 `state repair --clear-ghost-blocked-by`）。详见 [MDR-021](decisions/021-no-json-schema-validation.md)「第四轮 + 拆 PR」段。
  - **#88 现内容（本轮已完成）**：JSON Schema 如实化 + 摘依赖、错误码守卫、幽灵引用**检出**（`ghost_refs` = 纯事实 `{module, missing, status}`，告警只点名 + 区分 blocked/残留、不给动作、不作可达性预言）、`reset_force_reason` 唯一真值源 + 文档↔代码集合守卫（本轮改扫**全部**锚点出现处）。顺带修：`state reset --help` 两处补 `paused`、`grep -c` 获取命令纠正为读 rustdoc 自报计数。`just ci` 全绿 **886 测试**（拆掉 4 个处方测试：890 → 886），`cargo doc` 警告仍 10（全 pre-existing）。
  - **审查设界（用户 2026-08-12 拍板）**：第四轮只跑专项三子视角 + codex，**结束即收口、不起第五轮**，剩余问题记账 MDR-021。处方层遗留约束（两步往返依赖待办 3、reset 会擦 `Ambiguous` 检出通道、多步可达须按可达性处理）留给后续 PR。
  - **待拍板**：<推送 sha + 远端 CI 状态，push 后回填>。**绝不自行 merge**。
- **位置（2026-08-11，已被上条取代，保留作历史）**：master 在 `e9220bf`（#89 已合并），**开放 PR 仅 [#88](https://github.com/snowzhaozhj/rewriteInRust/pull/88)**、开放 issue 0。master 上复跑 `just ci` 全绿 **856 测试**（849 → 856，+7 为 #89 的守卫），远端 CI 针对 `e9220bf` 5 项全过。
  - **#88 的剩余工作两项**（PR 内容见下方 #88 两条专条）：
    - ① **第三轮四视角审查已完成，8 类 important，修复已落地**（详见下方 #88 专条与 [MDR-021](decisions/021-no-json-schema-validation.md)「第三轮四视角」段）。结论是**上两轮的修法方向就是错的**——不是文案写错几个字，而是 `module_blocked` 一个 bool 扛了四个正交谓词。已按根因重做：`reset_force_reason` 抽为唯一真值源、`GhostReference` 改带 `status` + `remedy`（四类、已替换好实际值）、新增文档↔代码集合相等守卫、e2e 改按机读 `remedy` 真跑并参数化成状态网格。
    - ② **冲突已解**：2026-08-11 merge `origin/master`（非 rebase——分支 23 个提交里 6 个是 STATUS 更新，逐个 rebase 会在同一段反复冲突；本仓 PR 一律 squash，分支内 merge commit 不落 master）。合并后远端 CI 针对 `60744c6` 5 项全过。**顺带丢弃本地一个未推送的残留提交 `81fb60d`**——它是 #89 的初版草稿（守卫 +239 行），而 master 上已合并的是过审终版（+588 行），不发现就会把被取代的旧实现重新带回分支（已备份到本地分支 `backup/stray-89-draft`）。
    - **审查轮次设界（用户 2026-08-11 拍板）**：第三轮范围严格限定 `94b53aa` 的 diff。第三轮的修复量（+796/−208、9 文件）远超豁免阈值，故仍走**一轮**审查，但**范围收窄为「专项三子视角（测试覆盖 / 静默失败 / 注释准确性）+ 异构交叉 codex」，不跑主审与设计契约**——第三轮实测这两个视角的结论 100% 被专项覆盖（主审 5 条全部重合），而最有产出的三条恰来自专项与静默失败。**这一轮结束后无论结果如何都收口**：剩余问题记账进 MDR-021，不起第五轮。「审查修复本身要再过一轮」这条规则自身没有终止条件，必须显式设界。
  - **一条方法论教训（已入记忆）**：第三轮里**多视角互相矛盾**——设计契约称 MDR-021 记的「仓内仍有 5 处断链」准确，主审与注释视角称实测 13 条。编排器自己跑 `cargo doc` 定论：**13 条**（4 unresolved + 1 unclosed HTML + 8 links-to-private，其中 3 条是 94b53aa 自己造的）。**视角冲突时须自己跑一遍，不能按视角数量投票。**
- **`blocked_by` 幽灵引用检出已交付（PR [#88](https://github.com/snowzhaozhj/rewriteInRust/pull/88) 第二部分）**（⚠️ **本节留作历史**：以下第三轮记录的**处方层**已按第四轮结论整体移出 #88，见顶部「位置 2026-08-12」与 MDR-021「第四轮 + 拆 PR」段；留在 #88 的只有**检出层** + `reset_force_reason` 守卫）——收口 [MDR-021](decisions/021-no-json-schema-validation.md) 待办 1，是该批待办里唯一的**真实功能缺口**（其余为文档/守卫类）。**审查状态：四视角已跑四轮**——第一轮针对功能主体，第二~三轮针对前轮修复产物，第四轮（专项三子视角 + codex）推翻了第三轮的处方层。
  - **第三轮（2026-08-11，四视角 + 专项拆三子视角，共 6 agent 并行、各自独立 worktree）：8 类 important，4 类由 4/4 视角独立复现，修复已落地**。完整缺陷清单、修法与七轮负向实证见 [MDR-021](decisions/021-no-json-schema-validation.md)「第三轮四视角」段；此处只留结论与教训：
    - **根因：`module_blocked` 一个 bool 扛四个正交谓词**（能否进 blocked / reset 是否需 `--force` / 项目是否 graduate / blocked 有无锚点），于是残留支一条命令要通吃 11 个 status。**这是本 PR 内第二次同型错误**（第一次是 `data.retryable` 兼任错误归属，被 codex 双向反例推翻）。教训：**一个 bool 要分述两类以上后果时，先问它是不是在替多个谓词说话。**
    - 四条实测缺陷（编排器逐条独立复核后才动手）：⒜ `--force` 清单漏 `paused`（真值是 `Done|Blocked|Paused|degrade_*`），paused 残留模块照做即 `E012`，且**不只手工编辑可达**——`state recover --policy skip` 就会置 paused 并保留 `blocked_by`；讽刺的是 `06:123` 本来就写着正确口径，06 自己两行矛盾。⒝「transition 一律报非法转换」是假的——`blocked_by=None` 只在 `from==Blocked` 分支执行，合法边**返回成功而引用原样留着**，`status:ok` 会被读成已处置，比 `E004` 危险。⒞ `done`/`degrade_*` **永不可能再进 blocked**，告警对它们的前提不可达，却据此建议破坏性 reset（会把已完成模块打回 `translating` + 清进度 + 发「删 .rs」指令，而 `validate` 转 `ok` 让破坏看起来像成功）。⒟ `graduate` 下 reset 一律拒、`--force` 不可绕，CLI 层一条可行命令都没有。
    - **修法是根治而非再补一处文案**：`reset_force_reason` 抽为唯一真值源（变体逐个列出，新增状态即**编译失败**，不落 `_ => None`）；`module_blocked: bool` → `status: ModuleStatus` + `remedy: GhostRemedy`（`leave_blocked{to,anchored}`/`reset{force}`/`inert`/`reopen_migration`，**已替换好实际值**，编排器不再回读 state 推断——旧占位符在锚点为 null 时会替换出 `--to null`，实测 `E012`）；新增 `reset_force_states_are_consistent_across_docs` 守卫把 `06`/`09`/`run.md` 三处声明与代码做**集合相等**比对（不是包含——只查缺失挡不住多写）。
    - **e2e 换掉了「解析人读文案」这条路**：上一轮的 `parse_advice_command` 被 4/4 视角各自变异穿透（`--keep-progress`/`--ghost-purge`/`--bogus-flag` 全 PASS），因为它解析完之后跑的仍是**手写 argv**；而按反引号奇偶位解析本身也不稳（module key 经 `serde_json` 转义但**反引号不在转义集里**，key 含一个反引号即整体错位，报成「告警没给命令」的假归因）。现改为按机读 `remedy` 构造 argv 真跑，并参数化成**状态网格**（blocked+锚点 / blocked+无锚点 / translating / reviewing / paused / done / degrade_skip）。
    - **rustdoc 那条上一轮声称修好、实际一条没少**：`resolve_blocked_ref` 是私有 fn，改名只把 `unresolved link` 换成 `links to private item`。本轮改纯反引号，实测 `cargo doc --no-deps -p rustmigrate-core` 警告 **13 → 10**（余 10 条全 pre-existing）。
    - `just ci` 全绿 **890** 测试（合并基线 886 → 890）。**负向实证七轮**（独立 worktree，全部报红且归因精确）：摘 `paused` → core 穷举 + machine 既有守卫 + e2e 网格**三层同时红**；摘可否进 blocked 判据 → 归因到 `status=done`；文案改「可自行补上」→ 网格红；argv 漏 `--force` → 网格红且报错里是 `E012`；文档漏写 `paused`（**复现原始缺陷**）→ 守卫红并列出集合差异；文档多写 `translating` → 守卫红；锚点句被润色 → 报「找不到锚点」而非静默跳过。
  - **⚠️ 教训：交付曾滞留本地 12 个提交未推送，远端 PR 上的绿 CI 是旧提交跑的**。本条此前写「已交付待拍板」而未记推送状态，下一会话据此会误判为「可以合并」，而当时合并的是**不含本功能**的旧提交。**此后凡记「待拍板」，必须同时记明：已推送的 commit sha + 远端 CI 是否针对该 sha 复跑通过**——「本地 `just ci` 全绿」不等于「PR 上的东西是绿的」。已在推送后校验 5 项远端检查针对 `47a8b3c` 复跑。
  - **⚠️ 第二个教训：「四视角闭环」曾是对审查产物自身未复审的状态下写的**。上一轮 STATUS 记「闭环」时，`8ebb801`（主审 imp 修复）与 `c0f7898`（专项 imp 修复，删 pub 字段 + 重写 run.md 步骤 2）**都是审查结论的产物、自身从未过审查**，合计 174 行。**补跑四视角后，三个视角各自独立复现了同一个真实缺陷**（详见下条）——证明「修完就记闭环」不成立。**此后审查修复本身若超出豁免阈值（本项目阈值见 CLAUDE.md 审查表），必须再过一轮**；「审查跑完」≠「修复已验证」。
  - **补跑四视角抓出的缺陷（2026-08-08，均已修 + 负向实证）**：
    - **imp【三视角独立复现，最严重】残留类幽灵引用的处置建议照做确定性失败**——`scan_ghost_references` 有意扫**全部模块**，故告警覆盖 `module_blocked=true`（正 blocked）与 `false`（残留引用）两类，但处置文本是**两类共用一段**，只对前者成立。残留类模块不在 blocked 态、`pre_blocked_status` **恒为 null**（该字段只在进入 blocked 时写入）。**编排器独立复现**：`translating` 模块照做，`--to pending` 与 `--to translating` 均报 `E004 非法状态转换`，幽灵引用原样留着、`status` 仍 `warning`；而告警**明确否掉**的 `state reset` 一步成功（`blocked_by → None`，`validate` 转 `ok`）。这与本轮刚修的「告警建议照做会失败」是**同一失败模式的另一半入口**，只修了一半。修：告警按 `module_blocked` 分述处置（blocked → `transition` 回锚点、不丢进度；残留 → `reset`，并**如实标注代价**——实测 `reviewing` 会回退到 `translating` 并清进度字段、`done`/`degrade_*` 需 `--force`、`pending`/`translating` 原状态不变，旧文案「不丢迁移进度」通吃是过度承诺）；`06`/`09`/`run.md` 三处同源文案同步。
    - **imp【专项抓出，守卫无区分力】上一轮为该 imp 加的断言是空的**——`!advice.contains("populate-modules 同步")` 恒真（旧文案实为 `` `state populate-modules` 同步 ``，反引号在中间故不含该子串），且后面「照做」的代码用的是**硬编码** `--to pending`、与文案无任何绑定。审查变异实证：把死路建议换措辞塞回、甚至改成确定性失败的 `--to done`，该测试**照样 PASS**。修：新增 `parse_advice_command` 从 advice 里**解析**命令再执行（按反引号奇偶位取，不用内容启发式——后者会被散文里的同名词误触发），文案给错即报红。
    - **imp【主审 + codex 共同必修】`--auto-unblock` 遇环整体拒绝，而 run.md 分流表把 `ready_to_unblock` 与 `cycles` 列成独立行**——两者可同时非空，编排器若按 ready 行选 `--auto-unblock` 会拿到 `E012` + 退出码 1 且**一个模块都不解除**。**编排器独立复现**（造 `READY` 可解除 + `C1↔C2` 互锁）：`ready_to_unblock=["READY"]`、`cycles` 非空，`--auto-unblock` 返 E012，`READY` 状态仍 `blocked`。修：分流表改为**带优先级五行**（① cycles 先判并明示此时不可调 auto-unblock → ② ready → ③ ghost_refs → ④ warnings 类数据完整性问题**不可当正常等待跳过** → ⑤ 其余 still_blocked），并补 e2e 钉住该前提行为。
    - **nit 全修**：三处 rustdoc 内链指向已改名的 `canonicalize_blocked_ref`（实际是 `resolve_blocked_ref`，`cargo doc` 报 unresolved link，而 `just ci` 不跑 `cargo doc` 故 CI 全绿掩盖了它）；第五处 `missing ⊆ unresolved` 残留注释（上轮声称修完四处、实漏此处）；`cmd_validate_state` doc 称 `ghost_refs` 条目「同时在 `still_blocked` 里」——改用全模块扫描后**不成立**（残留类不进 `still_blocked`），且「处置须重新同步」与新告警互相矛盾。
    - **负向实证三轮**（独立 worktree，全部报红且归因准确）：① 残留类处置退回共用 transition 建议 → `smoke_residual_ghost_reference_advice_actually_works` 报红，报错文本正是原始缺陷形态；② blocked 类建议改成 `--to done`（**审查已证旧断言挡不住这个**）→ `smoke_ghost_reference_advice_actually_works` 报红；③ 摘掉 `--auto-unblock` 的环守卫 → `smoke_auto_unblock_refuses_entirely_when_cycles_present` 报红，输出显示 `READY` 被真的解除（`unblocked:["READY"]`）。
    - **教训（已入记忆）**：**测试的场景定义域必须覆盖被测行为的定义域**。告警分述两类而 e2e 只覆盖一类，另一类的同型缺陷就原样留下——这是本轮三个视角都独立指向的同一根因。
  - **缺口实证**：`blocked_by` 引用无处归属的 key 时，`validate state` 返 `valid:true` **零告警**、`--check-blocked` 只把它列进 `still_blocked`——与「依赖还在翻译中」完全无法区分，而该依赖永远不会进终态，模块**永久阻塞**。根因是 `check_blocked_modules` 用 `.unwrap_or(false)` 把「依赖不存在」与「依赖未终态」抹平进同一个 `unresolved`，而两者处置动作**相反**（重新同步 state vs 等待）。同一语义在 `state deps` 侧早已做对（幽灵依赖单列 + warning，`run.md:64` 明令不得填进 `blocked_by`），读侧漏了——本次补上这处不对称。
  - **交付**：新增 `scan_ghost_references`（幽灵引用的唯一表示，`validate_state` 告警与 CLI `data.ghost_refs` 共用它）；`check_blocked_modules` 把幽灵引用与宿主歧义都按未终态处理（不判 ready，故 `--auto-unblock` 不会在损坏数据上改状态）；告警扫描覆盖**全部模块**而非仅 blocked（正常路径离开 blocked 会清空 `blocked_by`，但手工编辑/旧文件可能残留，该模块再被标 blocked 即踩坑）；`run.md` 步骤 2 改为先调 `validate state --check-blocked` 再按 `data` 分流。**无破坏性变更**（中途曾加 `BlockedCheckResult.missing`，经专项审查确认无生产消费方后移除——它与 `GhostReference` 是同一概念的两份表示，正是「两处各算一遍致口径不一致」的同类风险源）。
  - **自查抓到的真实缺陷（本 PR 引入，已修）**：初版判据「不在 `modules` 即幽灵」会把 **composite 组的非代表成员 key 误报**——decompose 折叠后只登记组代表，成员不进 `modules` 是既定设计，而告警给的「重跑 `graph build` + `populate-modules`」对该场景是**无效动作**。危害不止误报：归一后语义完全不同（成员属哪个组、该组是否终态决定它是否就绪），不归一则组进 `done` 后引用方**永远解除不了**。修法是新增归一函数（先查 `modules`，再反查 `member_files`；后续因坏划分问题重构为三态 `resolve_blocked_ref`），与 `state deps` 的归一同源、两处共用同一判据。设计契约视角独立复现了同一缺陷。
  - **四视角审查全部闭环**（设计契约 / 异构交叉 codex / 主审 / 专项，累计 **8 imp + 8 nit 全处置**，每条编排器独立复现后才修）。**三个视角各自独立复现了同一个 composite 误报**，与我自查结论一致。
    - **imp【专项抓出，可达性，已修】检出在 plugin 里没有任何调用点**：`rg "validate state" plugin/` 只命中命令清单与我自己写的说明段，`run.md` 步骤 2 仍让编排器「手工遍历 blocked 模块 + 自己做 DFS 环检测」——**检出建好却无人调用 = 未落地**，正是记忆里「守卫要证可达性而非仅存在性」那条。改步骤 2 为先调 `validate state --check-blocked`（一条命令同时完成就绪判定 + 环检测 + 幽灵检出）再按 `data` 分流。这与 09:482 既定设计一致（那里早写明该 CLI 化推迟到 M2 抽取为本命令，CLI 侧已落地而 run.md 还停在 MVP 指令跟随写法），09 补现状标注。
    - **imp【归一修复引入，主审抓出，已修】环检测未跟随归一 → 经成员 key 表达的互锁完全静默**：`check_blocked_modules` 归一了而 `detect_blocked_cycles` 仍按原始字符串建边，成员 key 不在 `blocked_set` 里 → 边被丢弃。实测 `shared blocked_by handler`（emitter 组成员）+ `emitter blocked_by shared` 是真实死锁，却输出 `cycles:[]` + `ghost_refs:[]` + `status:ok` **零诊断**——归一前这至少还会报幽灵告警，属实打实的回归。**教训：给判据加归一时，所有共用该判据的路径必须同步，否则制造比原问题更隐蔽的盲区。**
    - **imp【归一修复引入，主审抓出，已修】坏划分让 `--auto-unblock` 真的改状态**：同一文件被多组列为成员时初版静默取字典序最小宿主。实测 X 同属 done 组与 translating 组 → 取到 done 那组 → 判 `ready` → 模块被解除**并落盘**（`blocked→pending`、`blocked_by` 清空），全程零告警——与本次守卫声称的「不在损坏数据上改状态」直接相反，只是入口换成坏划分。`machine.rs` 的 `canonical_module_key` 对同一不变量是 **release 硬错**（MDR-015:55），此处策略相反。修：归一改三态（`Resolved`/`Ambiguous`/`Missing`），歧义按非终态处理且不当幽灵（处置是修组划分），`validate_state` 补跨组不变量告警。
    - **imp【本 PR 引入，主审抓出，已修】告警建议照做会失败**：初版建议「`graph build` + `populate-modules`」，但 populate 对**非 pending** 模块一律拒绝重填（断点续传保护），而幽灵引用按定义就发生在 `blocked` 等非 pending 态 → 照做**确定性失败**；补的 `state reset` 前置同样无效（置为 `translating`，仍非 pending）。同 MDR-020 finding 5 的失败模式。改为 `state transition --to <pre_blocked_status>`（清空 `blocked_by` 且不丢进度，实测一步到位）。**根因是 e2e 只验告警文本、没验建议照做能成功** → 补端到端测试钉住「照做后问题真的消失」，并在幽灵态下反证 populate 确实被拒（反证须在处置前做：处置后模块已回 pending，populate 反而成功，证明不了任何事）。
    - **imp【本 PR 引入，codex 抓出，已修】`ghost_refs` 与告警覆盖口径不一致**：告警扫全部模块、`ghost_refs` 却只从 `check_blocked_modules`（仅 blocked 模块）提取 → 非 blocked 模块残留幽灵引用时输出「`warnings` 有、`ghost_refs` 是空数组」，只消费机读字段的编排器**直接漏掉**。改为两侧共用 `scan_ghost_references`。同批修告警的失实断言：原文对 `pending` 模块也说「引用方将永久阻塞」，而它此刻并未阻塞 → 新增 `module_blocked` 区分「现在就阻塞」与「残留、将来才阻塞」。
    - **imp【pre-existing，收窄承诺不改行为】`transition_inner` 不校验依赖终态**：codex 实证 `state transition --to <pre_blocked_status>` 与 `state update --cas-version` 共用该路径，**不带 `--force` 即可**解除幽灵阻塞并清空 `blocked_by`。故「不在损坏数据上改状态」只由 `--auto-unblock` 保证。该行为对所有 blocked 模块一视同仁、本 PR 之前即如此，收窄它会改变既有转换语义 → 测试注释与 MDR 承诺**如实限定**在 `--auto-unblock` 一条路径（同 STATUS 记过的「守卫可达性承诺过强」失败模式），修法方向记入 MDR-021 待办 3。**有意思的是同一行为在另一场景下反而是恰当逃生口**——它正是上面「告警建议」那条里唯一可行的处置手段。
    - **nit 全修**：告警里的 key 改 JSON 字面量转义（key 可手工编辑，含反引号/换行时裸包裹会让条目边界被**伪造**）；补排序/去重回归锁（排序是告警文本可复现的唯一手段，此前摘掉全绿、症状是低频 flaky，最难查的一类）；`valid` 恒真断言加注释点明它锁的是文档承诺而非实现分支；生命周期过度约束随三态重构一并修掉。
    - **未采纳**：`resolve_blocked_ref` 改建反向索引（主审 nit）——codex 实测 10 万模块 1.11s 近线性（`contains_key` 是哈希查找，非 O(n²)），真实规模百级，引索引多一份需与 `member_files` 同步的状态，收益不抵复杂度。
    - **codex 实证确认无问题的项**：`state recover`/`reset`/`resume`/`batch-transition-done` 遇幽灵引用行为均安全（各自拒绝或不推进）；正常离开 blocked 会清字段、无噪声告警；性能近线性；`blocked_by` 唯一正常写入路径是 `run.md` 回填 `state deps` 的 `blocking`，该输出已过归一且只放真实 key。
  - **负向实证八轮**（独立 worktree，非推断，全部报红且归因准确；①③ 针对中途存在过、后被移除的 `BlockedCheckResult.missing` 字段，保留记录以说明当时的判据确有区分力）：① `missing` 恒空（退回二分）→ 2 core + 1 e2e 报红；② 摘掉告警扫描 → 2 core 报红；③ `missing` 不计入 `unresolved`（幽灵变可就绪）→ 3 core 报红，归因精确到「带幽灵引用的模块不得判为就绪」；④ 判据反转（合法引用当幽灵）→ 3 core 报红，「反向不误报」由独立测试钉住；⑤ 摘掉 `member_files` 归一 → 2 个 composite 测试报红；⑥ 环检测退回不归一 → 互锁测试报红；⑦ 歧义退回静默择一 → 坏划分测试报红；⑧ 摘掉排序与去重 → 3 个测试报红。
  - **自查另修 4 处**：`lib.rs` 的 `state deps` 注释引用了本 PR 已改掉的 `unwrap_or(false)`（引用即失实）；e2e 用 `.keys().next()` 取模块，其确定性依赖「serde_json 未启用 `preserve_order`」这一不显眼前提 → 改 `.min()`；**两处否定断言按「幽灵引用」措辞匹配**，文案一改即恒真假绿，其中一处按模块名 `dep` 匹配还会被签批告警文本的子串误触发 → 改按扫描结果判定。
- **PR [#88](https://github.com/snowzhaozhj/rewriteInRust/pull/88)「取消 JSON Schema 空头承诺」待用户拍板**（收口记账 TODO ③，7 commits，**4 视角审查已全部闭环**，CI 5 项全绿）。方向由实证定为 **⒜ 如实化 + 摘依赖**，非 ⒝ 补实现——决策与全部实证过程见 [MDR-021](decisions/021-no-json-schema-validation.md)。
  - **方向判据（实测 6 类 state 损坏，非推断）**：serde 的类型化反序列化在**加载期**已覆盖 JSON Schema 能管的字段类型 / required / 值域 / 枚举四项，诊断带行列号；schema 的增量仅「未知字段」「minLength」两处，各有更轻解法（`deny_unknown_fields` / 手写判据）。而 `jsonschema` 0.18 默认启用远程 `$ref` 解析，摘除**连带移出 80 个传递 crate**（含 `tokio`/`hyper`/`reqwest` 整套异步 HTTP 栈，`Cargo.lock` −855 行，依赖数 310 → 230）——纯离线 CLI 挂 HTTP 客户端栈是无谓的构建开销与供应链面。schema 也表达不了本项目真正在意的约束（`state_history` 链完整性、状态转换合法性、前置条件），那些只能手写且 `validate_state` 已实现。
  - **排查中撞出的连带失实比原记账严重得多**，全部是**编排器会照抄的具体值**（与 #86 修的 `--status` 值域漂移同一失败模式）：① 三个 `VALIDATION_*` error_code 在 CLI 中**零命中**，而 06 §10.7 要求编排器按它们分流「工具故障 vs 产出物失效」——照做**恒为假**，一切工具故障被误判成产出物失效而进入无意义重试；② 错误码表 11 个码只有 3 个真实存在且形态各异，§10.7 示例把 `error_code` 画在 JSON 顶层（实际在 `data` 内）、`error_context` 字段整体不存在；③ `BLOCKED_BY_VALIDATION_FAILED` 不只未落地，**该校验本身缺失**（实测幽灵 `blocked_by` 引用返 `valid:true`，模块永久 `still_blocked` 无告警——真实功能缺口，已记待办）；④ `E010` 对 state 损坏给出「源码解析失败，请检查文件语法」，把用户指去查源码语法。
  - **主审 5 important 全修——核心结论是我新增的表自身重演了要消灭的模式**：表头写「编排器分流以此为准」，却把 **`E002`/`E003`/`E006` 三个当前不可达的码**列为普通可分流值（`E002` 唯一构造点被调用点就地消费后改用 `ErrorData::new` 重构造 → 输出无 `error_code`、退出码 2；`E003` 在 `From<&MigrateError>` 中无映射分支；`E006` 源变体零构造点，后两者实测均返 `E012`）。另修 `E010` 来源清单**一半失实一半漏项**：「源码语法错误」失实（`MigrateError::Parse` 三个调用点都在 `graph/build.rs` 降级为 warning，绝不上抛，这句是用户看到的第一条建议、恒指错方向）；漏了 `validate rules --registry` 的 `rule-registry.json` 损坏这条真实来源，且原建议「重新执行 `init`」对该路径是**无效动作**（`init` 不生成该文件）。
  - **异构交叉（codex）双向实证推翻两处，比原设计更深**：⒜ **`data.retryable` 承担不了「错误归属」判定**——反例 A：坏 SQLite 两次均返 `E011` + `retryable:true`，但那是产出物**持久**损坏、重试永无意义；反例 B：`--cas-version 99` 返 `E007` + `retryable:false`，成因只是调用方版本号陈旧、产出物完好，判据却会走 `state reset` 破坏性重做。根因是 `retryable`（处置建议）与错误归属正交，一个 bool 承担不了 → 改为按 `error_code` 归**四类**，其中「**调用方错误**」是初版二分完全缺失、却是编排器最常遇到的一类。⒝ **守卫的可达性承诺过强**——断言 7 只解析 `From` 映射文本，证明不了源变体有可上抛构造点，「有映射但零构造」型死码（`E006`）能让 7 条全绿（编排器复现确认）；已收回承诺并把「可达」侧改为**运行时实证**（新增 e2e 用真实命令逐个证明 10 个码确实出现，`E013`/`E015` 需 60s 超时/C 语言路径，codex 已实证但不进 CI，注释如实记录不假装全覆盖）。codex 2 nit 亦成立已修：`E001` 覆盖图**操作**错误（查询期也返本码，原建议「重试 graph build」指错方向）、`E007` 的「另有迁移进程运行」是过度归因（唯一构造点是 CAS 检查）。
  - **守卫新增 `design_error_codes.rs`（7 测试）+ e2e 4 个，测试 849 → 861**。真值域取自 `ErrorCode::iter()`（加 `strum::EnumIter`，`strum` 本就是 core 生产依赖）不写死清单。**断言落地即抓到真实缺口**：`E002`–`E013` 中 **9 个真实码在 §10.7 完全没登记**，这是补「实际码全表」的直接动因。
  - **负向实证累计 13 轮**（每条断言逐一确认有区分力，全部报红且归因准确）：改 retryable 值 / 写错码号 / 废弃码复活 / 新增码不登记 / 表被 HTML 注释包裹 / 含义列塞 true 字样 / 删表行但散文提及 / 奇数反引号 / 节标题降级 / 可达性标注反转 / 常量与表不一致 / **表与常量同时改成自洽的错误状态**（前 6 条全绿、仅类型级断言 7 报红，证明它提供独立于人工维护的防线）/ E010 文案还原旧口径。
  - **自查修正了自己的 5 处失误**：06 §10.7 段落重复；守卫 retryable 判定曾用内容启发式 `contains("true")`（#86 主审推翻过同类判据）；断言 2 曾用全文匹配（`E008` 在节内出现 4 次 → 删表行仍假绿）；`extract_e_codes` 曾对整节 `split('`')`（奇数反引号致错位假红）；写可达性测试时误以为前置条件由 `transition` 检查（实际在 `validate state`）。
  - **设计契约自查找到 1 gap**：新写的 error JSON 示例只列 5 字段，`ErrorData` 实为 7 个——漏了 `context`，已补并说明三个键在空值时整体省略（编排器不能假设键必然存在）。**变更性质登记**：本 MDR **无破坏性变更**（摘依赖零 `pub` API 影响；`suggestion` 是用户可见文本非机读契约，`plugin/` 对该键零命中；`EnumIter` 不改 serde 表示）。
  - **文档如实化 10 处**（`03`/`04`/`05`/`06`/`09`/`README`）；保留 `06:889/896`（MDR-009 沿革叙述，正确）与 `08:123`（未落地路线图估算，非现状声称）。**待办记入 MDR-021**：幽灵 `blocked_by` 无检出（真实缺口，**✅ 已由上方交付收口**）/ `E002` 定义了但不可达 / `transition_inner` 不校验依赖终态（收口时新发现，pre-existing）/ 字符串空值无校验 / `[validation]` 配置段未落地。
- **#86 批记账 TODO 清零**：③ JSON Schema 空头承诺由本 PR（#88）收口；① SKILL.md ↔ CLI 命令清单守卫 ✅ 已由 [#89](https://github.com/snowzhaozhj/rewriteInRust/pull/89) 收口（2026-08-10 合并到 master `e9220bf`）。#87 专项另记的 ⒜⒝⒞ 三项仍挂账，见下方「开放 PR」条。
- **PR [#87](https://github.com/snowzhaozhj/rewriteInRust/pull/87)「scaffold 检测目标成为外层 workspace 成员」已合并**（收口 #86 记账 TODO ②，2026-08-02 squash 到 master `26a4f78`，4 视角审查真闭环后合并；合并后在 master 上复跑 `just ci` 全绿 849 测试、远端 CI 5 项全过）：`scaffold workspace` 落在已有 workspace 的仓库内时，目标 crate 会被纳入该 workspace，而 CLI 此前返回 `status:ok` **零 warning**——此后用户仓库的 `cargo build --workspace`/`test --workspace` 会连带编译迁移产物（迁移中的 crate 常是 `unimplemented!()` 等不可编译中间态，足以把原本绿的构建搞红）。用户典型场景恰是「已有 Rust workspace 的仓库里迁模块进来」，而此前该路径零覆盖零告警（全部 scaffold 测试都在裸 tempdir 跑）。
  - **判据经两轮审查各自推翻，最终定为「问状态」而非「比变化」**——这是本 PR 最主要的收获，理由须留存：
    - ⒜ 不匹配 `cargo init` 的 stderr 文案（`Adding ... as member of workspace`）：随 cargo 版本变动、可被本地化。
    - ⒝ **初版用「改动前后比对父 manifest 内容」，被主审实证推翻**：父 workspace 写 `members = ["crates/*"]`（glob）时 cargo **不改** manifest，新 crate 却自动成为成员——比对判据在这类仓库里**结构上永不触发**，而危害照旧。编排器独立复现确认：CLI 报 `ok` 零 warning，往新 crate 塞 `compile_error!` 后父仓 `cargo build` 立即变红。glob 不罕见（`~/workspace/explore` 下的 oxc 在用）。
    - ⒞ **最终判据：调 `cargo metadata --no-deps` 问「目标是否在 `workspace_members` 里」**。它自己解析 members/glob/`exclude`/`default-members`，是成员关系的权威真值源；一个判据同时覆盖显式 members、glob、以及重跑场景。路径比对两侧过 `canonicalize`（metadata 返回符号链接解析后的真实路径，`/var` vs `/private/var` 曾让检测静默失效）。
  - **异构交叉（codex）4 important + 1 nit，全部处置**：① 词法匹配 `[workspace]` 漏掉 5 种等价 TOML 写法（`[workspace.package]`/`[workspace.dependencies]`/`[ workspace ]`/dotted/inline table）——编排器逐一复现确认「父被改而报 ok」，改用 metadata 后一并解决；② 相对 `--target` 从子目录执行漏报（`Path::ancestors()` 对相对路径只走到 `""`，到不了 `..` 之上；`--target` 默认值就是相对的 `rust`）——我在收到该结论前已独立发现并修（`absolutize` 拼 cwd）；③ **`cargo init` 成功但 `.gitignore` 写失败时告警永久丢失**——首次报 IO 错误、重跑走早返回报 `ok` 零 warning，两次都不知道；改为先算告警再写 `.gitignore`，且早返回路径也按状态重新判定；④ pub 签名 `Result<()>` → `Result<Vec<String>>` 属源码破坏性变更（仓内调用点已全部更新、`with_bin` 除测试零调用点）。nit：告警文案改因果中立（受控实验证明旧文案会在「变了但没加 member」时给出无法执行的建议）。
  - **主审 7 项，其中 finding 1 为阻断级（即上述 ⒝）**：另修 finding 5——旧文案只说「从 `members` 移除」，主审实测照做后 cargo 报 `current package believes it's in a workspace when it's not`、产出编译不了的 crate；而 `scaffolder.md` 又禁止 agent 加 `exclude`，文案把用户领进死路 → 告警补 `exclude` 半句，`scaffolder.md` 改为「默认不动用户文件；用户明确要求时须 members + exclude 双改」。finding 6（注释/文档把词法判据说成「宁可多报不漏报」，实际方向相反）随判据重写一并订正。
  - **测试合计 +21（828 → 849）**——判据重写那一轮 +14（828 → 842），metadata 失败分支 +2（842 → 844，见下），专项视角补跑 +5（844 → 849，见下）：core 11（显式 members / **glob 覆盖** / `[workspace.package]` / 裸目录不报 / 父为普通 package 不报 / 目标即自身 workspace 根不报 / `with_bin` 同样报 / 重跑仍报 / `.gitignore` 失败后重跑仍报 / 告警含 `exclude` 建议 / 告警路径绝对 + `absolutize` 单测）+ CLI e2e 2（warnings 接进统一 JSON 并降级 status；glob 场景端到端）+ 1 `with_cwd` helper（core 侧此前没有，改 cwd 的测试须串行化）。多数用例带**前置假设断言**（先证 cargo 的实际行为符合前提，cargo 行为变化时会红而不是让断言静默失去意义）。
  - **负向实证三轮**（独立 worktree，非推断）：① 检测恒返回空 → 3 测试红；② 只摘 `with_bin` 一处接线 → 对应测试独立红（证两函数各有守卫）；③ 摘掉 `absolutize` → 相对路径两测试红，且报错信息复现了原始症状（告警里出现 `../Cargo.toml`）。
  - **端到端回归矩阵 8 场景全对**：显式 members / `[workspace.package]` / `[ workspace ]` / inline table / 子目录+短相对路径 → 均 `warning`；裸目录 / 父为普通 package / **目标在 `exclude` 里** → 均 `ok`（`exclude` 语义由 `cargo metadata` 天然处理，手写判据很难做对）。
  - **文档同步**：06 命令表该行重写（判据/处置/两种被推翻的朴素方案/`exclude` 提示/重跑语义）；`scaffolder.md` R1 护栏改写。`just ci` 全绿（842 测试 + fmt + clippy + deny + shellcheck）。
  - **`cargo metadata` 失败不再静默**（编排器自查补的缺口）：实测无 manifest（`could not find Cargo.toml`）与坏 manifest（`unclosed table`）**都是 exit 101**，故**不靠 stderr 文案**区分（那正是本 PR 反复排除的脆弱判据），改按「上溯路径是否存在任何 `Cargo.toml`」：无 → 不在任何 Cargo 项目内，正常，不告警；有 → 检测确实没能进行，如实报「无法判定」+ 提示手工确认，不让调用方以为已确认无事。**注意该判据的执行时点**（专项视角订正）：本函数在 `cargo init` **之后**执行，故常见的裸目录 scaffold 走不到失败分支——产出的 crate 合法可解析，metadata 在其中 **exit 0** 并由「目标即自己的 workspace 根」短路返回；失败分支只在残缺 manifest（幂等重跑）或外层 workspace 本身坏了时命中。另实测 `--no-deps` 确实不解析依赖图（`[dependencies]` 写一个不存在的 crate，metadata 仍 exit 0），故不受网络/registry 影响。顺带修一处 fixture 不真实——「父为普通 package」测试造的 package 缺 `src/`，cargo 报 `no targets specified` 使 metadata 失败，会被误判为回归（真实项目的 package 都有源文件）。测试 +2（842 → 844）。文案限定 `--workspace` 的断言加进既有测试、未新增测试函数，故计数不变。
  - **设计契约审查 3 important + nit 全处置**（2026-08-02）：① 06 表行漏了实现的**第二类 warning**（「无法判定」）、且 `lib.rs` 的同源注释漏改（core 侧已订正、CLI 侧失实）→ 两处补齐；② **`pub` 签名破坏性变更未按先例登记 MDR** → 补 [MDR-020](decisions/020-scaffold-workspace-membership-detection.md)（同时承载判据决策，使 06 表行可瘦身为指针——原本 ~1100 字符混入过程叙事，而 06 是契约文档非决策记录）；③ **用户可见文案过度承诺**——审查实测配了 `default-members` 时裸 `cargo build`/`test` **不**编译迁移产物、只有 `--workspace` 才会，编排器复现确认 → 告警与 06/scaffolder.md 三处限定为 `--workspace`（未配 `default-members` 时裸 build 亦会），并加断言钉住。另处置：`scaffolder.md` 补「无法判定」类告警的指引（须说明是「检测没能进行」而非「已确认无事」）；提示词内的 docs 链接按规范去掉（改内联，同 PR-4 记账的死链教训）。**设计契约 PASS 项**：CLI 输出契约（warnings 非空即降级，由 `ok_with_warnings` + `skip_serializing_if` 双保证）、R1 护栏无自相矛盾（文案受话人是用户、护栏约束 agent，授权后改法一致）、未触及 state schema/状态机/枚举/types。
  - **专项视角 2026-08-02 已补跑并全部处置**（前两次未取得结论：首轮 watchdog stall、重跑撞 Team API 日限）。拆 4 子视角并行（静默失败 / 类型设计 / 测试覆盖 / 注释准确性），**5 important + 4 nit 全部处置，每条编排器独立复现后才修**。测试 844 → **849**。
    - **imp【最严重，本 PR 引入】`cargo metadata` 大输出触发 pipe 死锁 → 大 workspace 上告警永不出现且归因错误**：`process.rs` 原实现先 `wait_timeout` 再读 pipe，注释里把「输出远小于 64KB」写成前提（当时只接 `cargo check/init` 与 `--version`）。本 PR 新增的 `cargo metadata --no-deps` 输出约 750B/成员，打破该前提。**编排器独立复现**（造 90/150/400 成员的健康 workspace）：68KB → 0.4s 正确告警；**114KB（150 成员）与 305KB 均 60 秒超时**并报「无法判定 + 成员 `Cargo.toml` 语法有误」——而 workspace 完全健康。危害三重：本 PR 的全部价值（精确告警 + `exclude` 指引）在大 workspace 上**永不出现**；归因**错误**会让用户去排查不存在的语法错误；每次 scaffold 白挂 60s。实测 `~/workspace/explore/oxc` 就是 55 成员 **246KB**（触发区间内），pingora 96KB / codegraph-rust 101KB 贴近边界。修：spawn 两个读线程在 `wait_timeout` **之前**开始抽 pipe，修后 305KB 场景 1.6s 完成 + 正确告警。**同一机理踩了两轮，两轮都是「只防了一半」**（`kill` 只终止直接子进程，孙进程继承 pipe 写端后 EOF 迟到甚至永不到来）：⑴ 初版超时路径 join 读线程，把「2 秒超时」拖成「等满孙进程 60 秒」——比原 bug 更糟且**断言全绿看不出来**（实测 60.8s 通过）；⑵ **合并前自查发现正常退出路径仍无条件 join、且那里没有任何时限**——实测 `sh -c 'sleep 30 & echo done'` 子进程 4.7ms 退出而 join 硬等 30s，孙进程若是守护进程即**永久挂死**，恰好废掉本函数「避免子进程挂死导致 CLI 永久卡住」的全部意义，比原 bug 隐蔽（原 bug 至少 60s 后会返回）。最终实现：两条路径都不无条件等，读线程**边读边并入共享缓冲区**（读完才整块交付会让超期只能返回空、丢掉早已读到的内容），调用方按**两流共享的截止时刻**取结果（各给一份宽限期会让最坏耗时翻倍，实测正好 2 倍）；宽限期取 **200ms** 而非 5s——子进程退出时数据已在内核缓冲区里、只差那个读不到的 EOF，给大值只会让「后台任务持有 pipe」这个正常场景每次白等且不会让输出更完整。+4 测试（大输出不死锁 / 超时及时返回 / 孙进程持 pipe 时正常路径不挂且已读输出不丢）。**改动面超出 scaffold**：`run_with_timeout` 是全仓子进程封装，`validate/tiers.rs` 的 Tier0 `cargo check`/`clippy --  -D warnings` 同受此前的前提约束——错误多的项目 stderr 轻易超 64KB，此前会被**误报成「超时」**（门禁失败且归因错，掩盖真实编译错误）。这是 pre-existing 缺陷，本修复顺带解决；已复跑 `validate::` 55 测试 + validate 相关 e2e 65 测试确认无回归。
    - **imp【本 PR 引入】符号链接下「无法判定」分支静默漏报**：成功分支比对路径时两侧过了 `canonicalize_or_self`，**metadata 失败分支的祖先链上溯却走词法路径**。`--target` 含符号链接段时词法祖先链指向不存在的目录树，真实位置却在 workspace 内 → 判成「裸目录」返回空告警。**编排器独立复现**：同一坏 workspace，直路径 `warning`、经符号链接 `ok` 零 warning。正是 MDR-020 通篇要消灭的失效（第 45 行自己记过同一个坑，成功分支修了、失败分支漏修）。修 1 行 + 补回归测试，**负向实证**摘掉修复后仅该测试红（45 passed / 1 failed，告警为空即漏报症状），证此前该路径零覆盖。
    - **imp【本 PR 引入】metadata JSON schema 漂移会静默放行**：`id` / `manifest_path` 在 `filter`/`filter_map` 内部被吞（不像 `workspace_root`/`packages` 用 `?` 上抛），任一改名只让 `member_paths` 静默变空 → `is_member` 恒 false → `status:ok` 零告警。而 `id` 格式**确有先例会变**（cargo 1.77 换过 PackageId 格式）。修：加不变量「`members` 非空则 `member_paths` 必非空，否则返 `None` 走「无法判定」」；把纯解析抽成 `parse_workspace_metadata` 以便喂合成 JSON（沿用本 PR「不改真实文件」惯例）。**这条守卫初版漏配测试**——变异摘掉它 46 测试全绿，补测试后才报红。
    - **imp【本 PR 引入】`inside_cargo_project` 守卫零覆盖 + 我写的注释失实**：两个「裸目录」测试函数体逐字重复，且**都进不到 metadata 失败分支**——`cargo init` 产物是合法 crate，metadata 在其中 **exit 0**（编排器实测确认），走的是成功分支「目标即自己的 workspace 根」短路。故我先前写的「裸目录 metadata 同样失败/同为 exit 101」是**把退出码套错了执行时点**（检测发生在 `cargo init` 之后）。变异摘掉该守卫 23 测试全绿。修：改成预置残缺 manifest 真正逼进该分支的用例 + 订正 doc comment 点明本函数在 `cargo init` 之后执行。
    - **imp【本 PR 引入】`with_bin` 早返回路径无守卫**：四个接线点（两函数 × 主路径/早返回）只有 3 处有守卫，变异摘掉第 4 处 23 测试全绿。修：既有 `with_bin` 测试末尾追加重跑断言。
    - **nit【本 PR 引入，已修】告警文案对根 package 型 workspace 过度承诺**：上一轮设计契约审查为消除过度承诺加了「未配 `default-members` 时裸 build 同样会」，但 `default-members` 缺省值**不是**「全部成员」——cargo 文档「When unspecified, the root package will be used. In the case of a virtual workspace, all members will be used」。**编排器实测**：未配 `default-members` 时虚拟 manifest 裸 build exit 101（编译了迁移产物），**根 package 型**（`[package]` + `[workspace]`）裸 build **exit 0 不编译**。故限定语收得不够、又成了较弱的过度承诺。四处同步为「若该 workspace 无根 package 且未配 `default-members`，在 workspace 根执行的裸 build 同样会」（告警正文 / 06 / scaffolder.md / MDR-020），另修 `cli_e2e.rs` 两处漏改的同源措辞（上轮只改了「三处」，e2e 不在其中）。
    - **nit【本 PR 引入，已修】三处注释共同承诺了代码没做到的事**：`canonicalize_or_self` 与 `absolutize` 的 doc 及 MDR-020:45 都称「告警里给的是用户输入的形态、不换成用户不认识的真实路径」，实际告警插值的是 `metadata.root`（cargo 已 canonicalize）。**编排器实测**：输入 `crates/probe` 时告警显示 `/private/tmp/...`，恰是注释声称要避免的形态。功能无害但会误导下个改动者。三处订正为如实描述。（另注：`test_scaffold_warning_path_is_absolute` 的 `contains` 断言在 macOS 上因 `/private/var/x` 含 `/var/x` 子串而恰好为真，无区分力——记为已知局限。）
    - **nit【本 PR 引入，已修】`with_cwd` doc 把 nextest 执行模型说反**：写「nextest 默认多线程跑同一进程内的测试」，实际 nextest 是 process-per-test；真正需防的是 `cargo test`（线程模型）。串行化本身必要，理由写反了。
    - **nit【本 PR 引入，已修】源码留不可解析的过程编号**：5 处 `imp3` 是审查过程内部编号，仓内查会命中 STATUS 里 PR-3 时期一个**完全无关**的 `imp3`，比不写更糟。事实陈述保留、编号删除。
    - **类型设计核心结论：`Vec<String>` 采纳不改**。判据是**消费方性质**而非「与惯例一致」——本命令 warnings 只被 LLM 读，全仓无机读消费方解析它（`plugin/hooks/` 下 "warnings" 命中全是 `cargo clippy -D warnings`），两类告警的 agent 处置动作相同（转达 + 不动用户 manifest），文案自带区分词。反例：`validate rules` 的 `data.checks[].issues` 带 `kind` 是**因为有 CI 机读需求**。MDR-020:80 原按「与惯例一致」记理由，层次错了（照抄会让有机读需求的场景也选字符串），已改为按消费方性质表述。
    - **`WorkspaceMetadata` 评级**：封装 8/10（module-private、单构造点单消费点、15 行跨度）、不变量表达 6/10、强制 5/10。不建议引入 `AbsDir` newtype（成本不划算）；已把「metadata 侧路径已 canonicalize」这条隐式不变量升为注释显式——它正是符号链接漏报的成因。
    - **未采纳**：newtype + `#[must_use]` 阻断「丢弃告警」（实测确认 `#[must_use]` 在 fn 上被 `?` 消耗、在类型上才触发，编排器独立验证过）——收益是防未来回归、成本是 12 处测试调用点改写，当前唯一非测试调用点已正确接线，判为超本 PR 范围，记账见下。
    - **该视角实证确认无问题的项**：`run_with_timeout` 各 `.ok()?` 分支归并正确（cargo 不存在 / 超时均出「无法判定」而非静默）；`workspace_root`/`workspace_members`/`packages` 三字段漂移均正确报「无法判定」；`default-members` **不**造成检测漏报（`workspace_members` 仍含全部成员，`default_members` 是独立字段）；`with_cwd` 串行化无 flaky（16 轮压测 + 注入 300ms 放大竞争窗口）；「前置假设断言」说法属实（9 处）；`exclude` 处置建议**照做真能解决**（两种形态实测 exit 0）；`absolutize` 的 `..` 处理、CLI warnings 接线、告警文案 `exclude`/`--workspace` 断言均有守卫。
    - **仍留的记账**：① `absolutize` 的 `current_dir()` 失败 fallback 会静默退化成相对路径（`chmod 000` 可复现，但主路径上 cargo 自己会先报错，仅早返回路径静默；EACCES-on-cwd 罕见）；② newtype `#[must_use]` 防丢弃告警（见上）；③ `workspace_members` 交叉过滤是等价变异、不可测（`--no-deps` 下无法构造区分场景，硬造只会产出脆弱用例）。
  - **实证留痕说明**（设计契约指出的 important）：负向实证与 8 场景回归矩阵在临时 worktree 中进行、worktree 已销毁，仓库内无对应 commit 或产物。可复核的是「存在能被这些实证钉住的测试」（测试名与断言方向均可对照），实证过程本身需重跑才能再次确认。已连同命令与结果表落入 MDR-020「验证」段，避免只留在会话里。专项视角补跑那轮的 4 次变异实证同样落入该段（含首轮「变异全绿」的两条——它们正是补测试的理由，留着比只记结论有用）。
  - **4 视角审查已全部闭环**（主审 / 设计契约 / 异构交叉 / 专项），CI 5 项待新提交后复跑。
- **开放 PR：[#88](https://github.com/snowzhaozhj/rewriteInRust/pull/88)（见顶部「位置」条，**四视角第三轮审查进行中**）。开放 issue：无。** 记账 TODO ③（JSON Schema 空头承诺）**已由 #88 收口**——方向定为如实化 + 摘依赖，见 [MDR-021](decisions/021-no-json-schema-validation.md)。① SKILL.md ↔ CLI 命令清单守卫 ✅ 已由 [#89](https://github.com/snowzhaozhj/rewriteInRust/pull/89) 收口（2026-08-10 合并）。**#87 专项视角新增记账三项**（均判超范围）：⒜ newtype + `#[must_use]` 防「丢弃 scaffold 告警」——实测 `#[must_use]` 加 fn 上会被 `?` 消耗、加**类型**上才触发 `unused_must_use`，配合本仓 clippy `-D warnings` 可让忽略告警直接阻断 CI，载荷仍是 `Vec<String>` 故外部 JSON 契约零影响；成本是 12 处测试调用点加 `let _ =`。当前唯一非测试调用点已正确接线，属防未来回归；⒝ `absolutize` 的 `current_dir()` 失败 fallback 会静默退化成相对路径（`chmod 000` cwd 父目录可复现，但主路径上 cargo 自己先报错，仅幂等早返回路径静默；EACCES-on-cwd 罕见）；⒞ `workspace_members` 交叉过滤是等价变异、`--no-deps` 语义下无法构造区分场景，硬造只会产出测实现细节的脆弱用例，故不补测试（注释已如实说明动机为「显式过滤更稳」而非必要）。
- **PR [#86](https://github.com/snowzhaozhj/rewriteInRust/pull/86) 已合并**（收口 #85 三项记账 TODO，2026-08-01 merge，squash 到 master `f4f341c`）：三项均为「说法与实现不符」，其中 ② 含破坏性变更。**4 视角审查真闭环后合并**；合并后在 master 上复跑 `just ci` 全绿（基线 809 → **828**）；远端 CI 5 项全过。
  - **① 06 命令表 ↔ CLI 一致性守卫**（闭 #85 记账 ③，也是 #85 漂移的**根因**——该表是 CLAUDE.md 定的唯一权威却靠人工维护）：新增 `cli/crates/cli/tests/design_command_table.rs`（**13 测试**——初版 3 个，主审/专项/异构交叉三轮加强至 13）——`CommandFactory` 遍历真实 clap 树取叶子命令，解析 06 两段表格首列，双向断言 + 表头计数一致 + 「原 M2 推迟」段成员冻结。**三向负向实证**（非推断）：删表行 → 报 missing；加幽灵行 → 报 ghost；两者均使表头「30 个」vs 实际 29/31 行独立报错。另有解析器自守卫测试（阈值取 `cli_leaf_commands().len()` 而非写死 30 + **跨两段**四锚点命令）——**主审实证订正**：其价值不是防「空集比空集」假绿（表侧空集时 `missing` = 全部 35 条命令、前两个测试必然失败），而是**改善归因**（把「35 条全未登记」的噪声换成「解析器与表格式脱节」）。
  - **② `record-subagent-call --status` 值域统一 + 强校验**（闭 #85 记账 ②）：原自由字符串、CLI 不校验，三方口径并存（clap 帮助 `success/timeout/failed` × 附录 A 示例 `success` × SKILL.md `started`/`ok`/`error`），拼错静默入库致无法按状态聚合。收敛到 **`started`/`ok`/`error`/`timeout`**（clap `ValueEnum`，解析期即拒）。取值理由：以 SKILL.md 实际在用口径为基准——**`started` 是卡死判定锚点**（有 `started` 无终态即卡死信号），而旧三值口径缺它，可记台账的全部目的就是诊断卡死；`timeout` 与 `error` 分列便于 watchdog stall（MDR-016）单独统计。**破坏性变更**：旧值 `success`/`failed` 现被拒（既有 e2e 已改 `ok`；plugin 侧本就用新值域，无其它调用点）。新增 2 e2e：四值全接受且**落盘字面值与命令行取值同形**（文档照抄的前提——clap kebab 化若与文档不一致，编排器照抄即解析失败）、非法值（含废弃的 `success`/`failed`、拼错 `sucess`、空串）解析期拒且**一条不落盘**。四处文档同步：09 附录 A 示例 + 值域注、06 命令表行、SKILL.md 清单 + 台账段、workflow.md 回传台账。
  - **③ `scaffold workspace` 语义如实化**（闭 #85 记账 ①，**改描述不改名**）：实测产出 `[package]` + `src/lib.rs` + `.gitignore` **单 crate**、无 `[workspace]` 段。查证 06 § M2 写隔离约束已明确「worktree 是并行机制，与输出 crate 结构正交，**M2 沿用单 crate 输出**」——单 crate 是**既定设计**，产出物没错，错的只是命令名与描述。故保留命令名（改名会破坏 `scaffolder.md`/`workflow.md`/`SKILL.md` 既有调用），改如实描述 + 注明沿称由来 + 真需多 crate 时另开命令；`scaffolder.md` R1 补「勿据名手加 `[workspace]` 段或拆子 crate」防 agent 误解；单 crate 产出加断言钉进 `smoke_scaffold_workspace` 防描述再漂。顺带修 06:198 失实项（称本命令「用 toml_edit 生成 TOML」，实为委托 `cargo init`，且 `toml_edit` 不在依赖清单）。
  - **撤回一项初判**：调查中曾疑 `WorkspaceConfig{}` 空结构与 06:841-843 的 `cargo_workspace`/`crate_naming` 不符，核实为**非偏差**——与 `ToolsConfig`/`ParserConfig`/`AnalysisConfig`/`ReproducibilityConfig`/`ValidationConfig` 同属既有「M2 预留」空结构模式（设计契约审查复核确认撤回正确）。
  - **审查闭环（4 视角全跑；主审 + 设计契约 + 专项已回传并全部处置，**异构交叉 2026-08-01 已补跑并全部处置**——首次转后台后未回传结论，分支提交历史可证当时从未落过 codex 修复）**：
    - **主审**：5 important + 2 建议修 + nit 已处置（提交 `5437a17`）。① STATUS「唯一开放 PR = #85」失实（**第三次复发**，#85 已合并、真正开放的是 #86）；② 守卫注释理由不成立（「防空集假绿」→ 实为「改善归因」，见上）；③ 测试计数与守卫测试数失实；④ 残余两处反向断言（`workflow.md:19` 前置条件 + `06:165` 目录树注释仍写「workspace 骨架」）；⑤ `06:198` 依赖表述失实——原称「`toml_edit` 不在依赖清单」，`cargo tree -i` 证它由本仓直接依赖的 `toml 0.8` 拉入 lock，改为精确表述「非直接依赖、在 lock 中作为传递依赖、源码零处 `use`」。建议修 a（help 排除分支恒不触发）**不删**：clap 何时注入 help 属实现细节，升级或改用 `command().build()` 都可能变，而 help 混入即是永远登记不进 06 表的幽灵命令——注释如实改为「防御性」并记明实证结论。
    - **设计契约**：6 项逐条核对，**0 MISMATCH** + 2 DEVIATION 已修。① **06:736 第五处口径**（pre-existing，被本 PR 升级为**互斥**）——R2-D5-04 注要求把校验工具故障「记入 `subagent_calls` 的 substatus」，但 `SubAgentCall` **从无 substatus 字段**，值域收窄后 `validation_tool_error_*` 更无处安放 → 改由 `--error-message` 承载类型前缀、`--status` 取 `error`（工具故障非 agent 卡死，不占 `timeout`）；② core 侧 `status: String` 补值域 doc comment，如实点明两处敞口（`push_subagent_call` 是 `pub` 可绕过 CLI；旧文件可能含废弃值）。破坏性变更登记方式判 PASS（同 MDR-019 先例：MDR/STATUS 双处记「破坏性变更」，0.x 阶段不走 deprecation 期）。
    - **专项**（5 子视角，全部本地实证）：**4 important 全修**。⒜ **用户可见 `--help` 文案漏改**——本 PR 第 ③ 项全部论点是「产出单 crate、命令名是沿称」，却漏了唯一用户直接看到的 `ScaffoldCommands::Workspace` 帮助文案（与 #85 判为必修的 Leiden 文案同一位置、同一类失实）；⒝ **大小写敏感性零保护**——实测 clap 默认 `ignore_case=false` 故当前不失实，但变异证明只加一行 `ignore_case=true`，4 个相关测试**全部仍 PASS** 而行为已变 → 非法值数组扩入大小写变体，**负向实证加该 attribute 即在 `"OK"` 上失败**；⒞ **`as_str()` 与 clap 派生名双真值源**——`match` 穷尽性不检查返回值与取值名一致，对**新增第五个变体**无保护 → 仿 `as_str_matches_serde_serialize` 先例补等价性守卫（遍历 `value_variants()` + 钉住变体数=4），**负向实证改 `Timeout` 的 as_str 即失败**；⒟ **强校验只在参数层、读侧完全无约束**——审查实测手工改 state 为 `success` 后 `validate state` 返回 `valid:true`/exit 0/零 warning，等于把本 PR 要消灭的失败模式换到读侧 → `validate_state` 补值域扫描告警（沿用同函数 MDR-019「防御性可观测」惯例、去重+字典序，不硬判损坏因旧文件须可读），**端到端实证同一篡改场景现输出 `status=warning` + 具体非法值**。
    - **专项 nit 已修**：`machine.rs` round-trip fixture 的 `status:"success"` → `"ok"`（本 PR 刚定 success 为废弃，留着会让后人以为仍合法）；`scaffolder.md` frontmatter/角色段 + 06:335 职责表三处「Cargo workspace 骨架生成」→「Rust 项目骨架生成」（原先与本 PR 刚加的 R1 护栏在**同文件内互相抵消**）；非法值测试的「一条都不落盘」改为先落合法基线再验不被污染（原写法删掉整块仍绿）；`init` 改 `assert_eq` 硬校验（原 `let _ =` 时 init 失败会让四个非法值因「文件不存在」而退出码非 0，命题假绿）。
    - **编排器独立实证补的守卫缺口（2026-08-01）**：本 PR 加的命令表守卫（发现时为 9 个，现 13）**只钉首列命令名，说明列漂移零保护**——独立 worktree 负向实证：把 `init` 的说明改成明显胡话，9 测试全 PASS。这本身是设计权衡（说明列是自由散文，锁死会让正常改文案就红），但有一处例外必须钉：**`--status` 四值域声明**。实证把 06 表的值域改回本 PR 刚废弃的 `success`/`failed`，**822 测试全绿**；09 附录 A 值域注、SKILL.md 命令清单同样篡改亦全绿——三处都是编排器照抄的权威声明，漂回废弃口径即触发本 PR 要消灭的那个失败模式（照抄 → CLI 解析期拒）。已补 `subagent_call_status_domain_is_consistent_across_docs`（`cli_e2e.rs`）：从 `value_variants()` 取真值域，在三处文件的值域声明段内断言「四合法值齐备 + 废弃值不以值域条目形态出现」。**三向负向实证全部报红且归因准确**（06/09 篡改报「把废弃的 `success` 当合法值列出」、SKILL.md 报「缺合法值 `started`」）；**反向亦验不误报**——现有 06 段落本就含 `success/timeout/failed` 的沿革说明，守卫只拒 `` `success`（<释义>) `` 这种条目形态，故沿革说明可与守卫共存。基线 822 → 823（异构交叉修复后终为 **828**）。
    - **守卫测试经审查实证两轮加强**：① 归因判据从**按内容**（首列 `contains("rustmigrate")`）改**按位置定性**——审查实证把 `` | `rustmigrate state deps <module>` | `` 写成 `` | `state deps <module>` | ``（丢命令名前缀）时，该行既解析不出、又不满足内容启发式，被静默跳过后仍误报「命令未登记」，**任何依赖行内容的判据都会被格式变体绕过**；② 补「两段表头声明须齐备」后置断言——概览章节内新增 `###` 子标题会让扫描提前 break、后半段静默截断，而前半段「计数 vs 行数」仍自洽（假绿）；③ **验证方式从「临时改 06 文档」改为「喂合成字符串」**（新增 5 个解析器单元测试，覆盖双反引号/丢前缀/无反引号/加粗包裹/仅占位符/前导反引号被替换等变体 + 表格结构行 + 截断场景）——此前改文档实证在多审查视角并发的共享工作区里会互相冲掉改动（本 PR 审查期间**真实发生多次**），改合成字符串后边界可回归且无人需抢文件。
    - **异构交叉（codex，2026-08-01 补跑，首次转后台未回传故重跑）**：**2 important + 2 nit，全部处置**。
      - **imp1【本 PR 引入，已修】守卫不理解 Markdown 渲染语义 → 9/9 假绿**：扫描器逐行读原始文本，不排除 HTML 注释 / 代码块 / 引用块，且表头声明重复出现时后值覆盖前值。codex 实证把 35 条表行整体包进 `<!-- -->`（读者渲染后看不到任何表格）+ 可见处放引用表混入 `stats community` + 可见计数改 999 而注释里藏正确的 30/5 → **9 测试全 PASS**；**编排器独立复现确认**（仅注释掉第一张表即全绿）。这直接击穿守卫的命题（「**读者看到的**表 == CLI」）。修：`scan_table` 加四道渲染语义处理——HTML 注释（跨行 + 同行开闭）/ 代码块围栏 / 引用块与缩进块一律跳过，表头声明重复出现直接 panic。**负向实证修复生效**：同一注释变异现报红且归因正确（列出 29 条未登记命令）；段间搬迁变异被冻结守卫抓住。新增 4 个合成字符串单元测试（沿用本 PR「不改 06 文档」惯例）覆盖这四类不可见内容，守卫 **9 → 13**。
      - **imp2【pre-existing，判定不改签名 + 补回归锁】pub core API 绕过值域**：`push_subagent_call` 是 `pub` 且仍收 `String`，外部 Rust 调用者可持久化任意值（codex 用合成集成测试实证 `"totally-invalid"` 可写入并读回）。**判定**：该分层是既定惯例（同 `ModuleState::substatus`——core 无分支消费、值域约束落 CLI 层），设计契约审查已就此判 PASS，且 `status` 字段 doc comment 本就点明这两处敞口，故**不改签名**（下沉枚举到 core 会牵动 serde 兼容面与 `substatus` 惯例，超本 PR 范围）。但**兜底缺回归锁**——既有读侧告警测试直接构造 `SubAgentCall` 结构体，没走这条真实绕过路径、也没验往返：补 `test_validate_warns_on_status_written_via_pub_api_after_roundtrip`（pub API 写非法值 → `save` → `load` → 断言仍可读回**且**读侧告警命中）。
      - **nit1【本 PR 引入，已修】新增 rustdoc 自相矛盾**：`types/state.rs` 的 `status` 注释写「`validate state` 目前亦不校验本字段」，而本 PR 已在同一轮加了告警扫描——同句话先说不校验、又让读者去看告警。改为「反序列化仍兼容不报错（旧文件必须可读）；`validate state` 会扫出非法值并告警，但不硬判损坏」。
      - **nit2【pre-existing，本 PR 文案收口漏点，已修】`scaffold/mod.rs:1` 模块头仍写「Cargo workspace 骨架生成」**——与本 PR 第 ③ 项论点（单 crate、无 `[workspace]` 段）直接相反。本 PR 已改 6 处（`--help` / 06 表 / 06:165 / 06:335 / `scaffolder.md` ×2 / `workflow.md`），唯独漏了 core 模块头。改为「迁移目标 Rust 项目骨架生成（**单 crate**，不含 `[workspace]` 段）」；改后全仓 `Cargo workspace 骨架` 残留清零。
      - **codex 独立复核通过的项**：旧值调用点无遗漏（plugin 实际调用只用四值，残余 `success` 均为 `attempts[].result` / 进程退出状态 / 历史说明，非本参数）；`validate state` 读侧行为正确（旧文件可读、去重、稳定排序、合法四值无误报）；scaffold 用户可见文案正确。
  - **本 PR 记账 TODO（审查提出、确属超范围）**：① **`SKILL.md` ↔ CLI 命令清单无守卫**——06:105 表头同时要求同步 SKILL.md 清单、`SKILL.md:31` 自称「已穷举顶层子命令」，但零自动化检查（#85 那次是一次性人工验证）。本 PR 钉死了 06 ↔ CLI 一边，SKILL.md 那边下次新增命令仍会漂；守卫可复用 `design_command_table.rs` 的 `cli_leaf_commands()`。**2026-08-01 编排器摸清可行性**：SKILL.md 清单是 `SKILL.md:32-39` 的**行内反引号分组列表**（「建图/查图」「状态推进」「签批门」「度量/台账」「断点续跑」「校验」「统计/度量」「其他」8 组），格式与 06 的 Markdown 表格**不同**——不能直接复用 `parse_design_table`，需另写一个抽取器（从这 8 行里取每个 `` `cmd [args]` `` 的命令名段，剥参数占位符同现有 `parse_row` 逻辑）。守卫判据建议与 06 侧对称：双向断言（缺失 + 幽灵）+ 分组行数守卫（防某组整行被删后静默）。**编排器同时实测了当下一致性：SKILL.md 清单现为 35/35 准确**（TODO ① 是未来漂移风险，不是当下失实）。实测撞到两个抽取器必须处理的坑，记下省得下个 PR 重踩：⒜ **带管道的占位符**——`graph export [--format json|dot|mermaid]` 剥参数时若只按 `<`/`[`/`--` 前缀判定，`json|dot|mermaid]` 会残留成命令名的一部分，误报 `graph export` 缺失；⒝ **必须限定只扫命令项**——这 8 行里还散落着值域与状态名的反引号（`started`/`ok`/`error`/`timeout`/`agent_done`/`advanced:false`/`reviewing → done`/`rule_version`），若无脑抽取行内所有 `` ` ` `` 会产出 9 条伪幽灵。② ~~**`scaffold workspace` 会静默改用户仓库的父 `Cargo.toml`**~~ **✅ 已由上方「scaffold 父 workspace 告警」PR 收口**（2026-08-01）。原始记录：——审查实测在带 `[workspace]` 的父目录下执行，cargo 把新 crate 追加进父 `members`（输出 `Adding ... as member of workspace`），而用户典型场景恰是「已有 Rust workspace 的仓库里迁模块进来」，此后父仓 `cargo build` 会开始编译迁移产物；06 表与 `template.rs` 均未提及、无检测无 warning、全部测试都在裸 tempdir 跑（无覆盖）。**2026-08-01 编排器独立复现确认**（非推断）：`/tmp` 造含 `[workspace] members=["crates/existing"]` 的父仓 → 在其中执行 `rustmigrate scaffold workspace --target crates/migrated --name migrated_mod` → CLI 返回 `{"status":"ok"}` **零 warning**，而父 `Cargo.toml` 的 `members` 已被静默改成 `["crates/existing", "crates/migrated"]`（`git status` 显示 `M Cargo.toml`）。修法方向：`scaffold::template` 在 `cargo init` 后比对父 `Cargo.toml` 是否被改动，有则汇入 `warnings` 并降级 `status=warning`（沿用 CLI 输出契约），测试补一个「父目录带 `[workspace]`」的用例。③ 06:119 表行声称 `validate state` 做「JSON Schema」校验，实际 `jsonschema` 依赖在册但源码零引用、`docs/design/schemas/` 下只有 `*.example.json`。**2026-08-01 编排器复核确认并扩大范围**：`rg jsonschema --type rust cli/` **零命中**，而依赖在 `cli/Cargo.toml:36` + `cli/crates/core/Cargo.toml:21` **两处在册**；`validate_state` 实为手写的版本兼容 + `state_history` 链完整性 + 状态机约束校验（`validate/mod.rs:23`），无任何 schema 驱动校验。06 里的同类空头承诺**不止表行一处**：`06:118`（表行本体，真实行号是 118 非 119）、`06:201` 依赖表「jsonschema | JSON Schema 校验 | `validate state`, `validate config`」、`06:378`/`06:571`/`06:575` 的 L2 分级定义均以「JSON Schema 校验」为措辞。修法二选一，需先定方向：⒜ 如实化——把上述各处改为「结构与状态机约束校验（手写判据）」并从 `Cargo.toml` 摘除未用依赖（`cargo-deny` 不报未用依赖，故不会自动暴露）；⒝ 补实现——落地真 schema 文件 + `jsonschema` 校验，但需先确认 L2 分级是否真需要 schema 驱动（手写判据已覆盖状态机语义，schema 主要补字段类型层）。④ ~~专项另指出 `design_command_table.rs` 的 `>=30` 阈值 + 三锚点对「整段丢失」无效、且无断言检查命令的**段归属**~~ **✅ 已在本 PR 内收口**（提交 `5437a17`）：阈值改取 `cli_leaf_commands().len()`（不再被「第一段恰 30 条」卡住）+ 加跨段锚点 `graph rdeps`；新增 `design_06_deferred_section_membership_is_frozen` 钉死该段 5 条成员，**负向实证**模拟把 `stats community` 挪过去 + 同步两个计数 → 立即失败。
- **PR [#85](https://github.com/snowzhaozhj/rewriteInRust/pull/85) 已合并**（文档/提示词与实现对齐，2026-07-28）：收口三项 STATUS 已记账 TODO，均为「说法与实现不符」，不改行为。
  - **① Leiden 术语残留（代码侧 2 处）**：#81 术语统一只清了 `docs/`。`lib.rs` 的 `stats community --help` **用户可见文案**仍写 Leiden（实现自 #58 起为自实现 Louvain）；`types/graph.rs` 的 `NodeType::Community` 注释称「Leiden 算法产出」，实测该枚举**无任何产出点**（`stats community` 只读 `NodeType::File` 算诊断分数、不落社区节点）——改为如实描述。
  - **② SKILL.md 命令清单**（闭 ORCH-01 PR-2 记账「SKILL.md 命令清单完整审计」）：原缺 11 条命令，改为按用途分组补齐；**程序化验证** CLI 全部 35 个叶子命令逐条命中，断言改「已穷举顶层子命令」。
  - **③ workflow.md 2c 冲突指引**（闭 ORCH-01 PR-5 记账 TODO #3）：原文只有「abort + 重译」一条路，而 PR-5 实测 `lib.rs` 的 `pub mod` append 冲突**重译消除不了**。补分型；**但初版判据「文本不重叠即可 union」过宽、反而弱化了 MDR-003**，经异构交叉审查已收紧（见下）。
  - **同步 `docs/design/06` 命令表**（CLAUDE.md 定其为 CLI 命令列表唯一权威，本 PR 一度让 SKILL.md 反超权威文档）：补 4 条已实现却缺表行的命令（`graduate`/`state deps`/`state advance-sprint`/`state record-subagent-call`）；修表头「MVP（M1）— 14 个命令」失实（实际 30 行、混含 M2/M3/M4 命令）；「M2 扩展 5 个命令」5 条**全已实现**却仍挂「推迟理由」列 → 改标「已实现 + 沿革」；`graph decompose` 行算法描述过时（MDR-011 已推翻 first-fit 装箱）→ 订正。**校验：06 表 ↔ CLI 双向一致 35/35，无缺失无幽灵命令**（该校验已由 #86 固化为 CI 守卫测试）。
  - **异构交叉审查 4 important 全修（每条均本地实证，非推断）**：⒜ 判据改按**实体身份**——实证 `use a::Foo`+`use b::Foo`→`E0252`、同名 `pub mod` 两侧布局不同→`E0761`、同名 helper→`E0428`，这些都文本不重叠却是语义冲突（MDR-003 本就要求「均当冲突处理」）；⒝ 删「Cargo.toml 同理」泛化——实证同一 dependency key 重复直接 TOML 解析失败，且 MDR-003 约束 4 要求 default-feature 校验 + Cargo.lock 重解析；⒞ 补合并后置条件——本机 gitconfig 实为 `zdiff3`、实测冲突文本含 `|||||||` **基线段**（原指引「删标记保留双方」会把 base 当第三份声明），改用 `git show :2:/:3:` 取两侧原文 + 提交前查 `git ls-files -u`（`git add` 后残留标记不报错）；⒟ 补活锁防护——合并失败原先不计入任何计数器，与 MDR-003 约束 7 冲突，改为每文件只试一次、失败即升级并计入 `max_reconcile_rounds`。另修 `run.md:270` 与 workflow.md 同一套 reconcile 协议说法相反（run.md 原写「非 LLM 手解冲突块」）→ 按「单点收敛」归口；`validate rules` 两参数实为必填却写成 `[方括号]`（编排器照抄会 clap 报错）。
  - **设计契约审查 3 项已修**：① 06 的 `state deps` 输出漏第 5 个字段 `unresolved`——它单列「依赖未登记为模块」且**不计入 `blocking`**（否则被 run 填进 `blocked_by` 造成永久 blocked 死锁），是门禁消费方必须知道的语义；② 同行「亦被 `state resume` 的 `next` 桶复用」失实——`cmd_state_resume` 不调用 deps 逻辑，只在 `advice` 字符串里指引编排器去调；③ `record-subagent-call` 的 `--status` 我写成「(started/ok/error)」像是枚举，实际 **CLI 不做枚举校验**（自由字符串），且附录 A 示例是 `success`、SKILL.md 台账约定用 `started`/`ok`/`error`——两套口径并存，改为如实记录并标注待统一（**已由 #86 定为四值枚举 + 强校验**）。另修 `graduate` 的「幂等守护」用词（实为拒绝重入报错，非幂等）。
  - **#85 记账 TODO ①②③ 已全部由 [#86](https://github.com/snowzhaozhj/rewriteInRust/pull/86) 收口**（见上）。
- **Milestone**: M1 ✅ → M2 ✅ → **M3 ✅** → **M3 遗留债清理 ✅** → **M4「完善」✅ 全部收官**——巩固线 Sprint A ✅（#57）+ B ✅（#58）+ F「健壮性+编排收口」✅（GOV-01/ROB-01a/b/c #64/#65/#66+#67/#68 + ORCH-01 全 5 PR #71/#73/#74/#75/#76）→ Go 线 Sprint C ✅（#59/#60/#61）+ D ✅（#62/#63）+ **E「Go 端到端验收」✅ M4-VAL-01~08 全达标（#79）** → 质量度量数据流 QUAL-05 ✅（#77）→ 验收尾巴 issue 闭环：#78 loc_ratio 排除测试 ✅（#82）+ #80 术语统一/回填 ✅（#81）→ **QUAL-02 质量基线报告 ✅（#83，2026-07-24）** → **MDR-019 译后签批门 ✅（#84，2026-07-28）**。**M4 全部合并，开放 PR 与开放 issue 均清零**（#85 于 2026-07-28 合并、#86 于 2026-08-01 合并）。**唯一挂账：QUAL-02 字面「1 TS 真实项目基线」未落地（PLAN-M4 标 `[~]`）**——横比起点已由 Go CLI 实测 + Python 强等价实测两档建立，TS 真实基线是第三档样本，留待真实 TS 迁移需求时补（不阻断 M4 收官）。
- **MDR-019 译后签批门已合并（2026-07-28，PR [#84](https://github.com/snowzhaozhj/rewriteInRust/pull/84) 已 merge）**：M4 收官后唯一「已决策、未落地」的设计偏离——`reviewing`「最终签批门」被实现成编排器自动 `--to done`。落地把门重建在 `reviewing → done` 这条边**本身**：
  - **CLI 硬门**：裸 `state transition --to done` 一律被拒（`--force` 不是凭据；`state update --cas-version` 委派同一 `transition_inner` 也绕不过，有测试钉住）。唯一入口 `state approve`（人签批，须先停 `awaiting_final_review`）/ `state approve --by-policy <id> --attest …` / `state batch-transition-done --by-policy …`（**破坏性变更**：不带凭据时一个都不升 done，全落 `skipped.code=approval_required`）。
  - **新命令**：`state review-gate --module <M>`（纯查询：`decision` 三态 + `mandatory_reasons` 红线码 + `policies[].required_attestations` + `state_facts` + `evidence` 磁盘实存产物索引 + `evidence_commands` + `orchestrator_must_check` + 回显 `coverage_threshold`/`enabled_policies`）；`state approve`；`state record-metrics` 扩 `--coverage`/`--phase-a-audit-passed`。
  - **两条返工边**：`Reviewing => Done | Blocked | Translating | CompileFixing`（签批打回 / 整组验证失败，闭 ORCH-01 PR-3 遗留 TODO ②③），且**作废上一轮签批证据**（清通过率/覆盖率/结构门结论），防重译一轮后带旧证据够到策略放行。
  - **`DangerProvenance` 三态**（`unclassified`/`classified`/`partially_classified`，默认+旧 state 反序列化 = `unclassified`）消解 MDR-013 的 `danger=[]` 语义重载；顺带修一个静默失败：`FileClassification` 加 `classified: bool`——此前**语法解析失败的文件被当作「已分类且无危险」**，可被自动放行。
  - **`[review_gate].auto_approve_policies` 默认空 = 全停门**；内置 `batch_mechanical` / `headless_default`（后者超出 MDR-019 原决策 3，经用户显式授权，已在 MDR-019「落地细化」记明与被打穿的「风险分级自动放行」的三点区别）。签批门三命令用 **fail-closed** 配置加载（配置解析失败不回退默认，否则用户配的 `coverage_threshold=95` 会静默变 80）。
  - **`state resume` 新增 `awaiting_approval` 桶**：停门待签批模块不进 `interrupted`（否则续跑 `recover retry` 会清掉刚落的证据）；`validate state` 对「`done` 但无签批审计」告警（手工改 JSON 旁路可观测）。
  - **Plugin 同 PR 改**（不能拆，否则合并后 `/migrate run` 的裸 `--to done` 全被拒）：run.md 步骤 11 重写为 11a–11f + 断点路由细分、batch 6.5 / CoupledBatch 快进改走门；workflow.md 步骤 2d 三段式（推 reviewing → 逐模块判定分流 → 同策略批量放行 + 待签批汇总）；SKILL.md 新增「译后签批门」单点约定 + 命令清单。
  - **设计文档同步**：06 命令表（3 行新增/重写 + resume/reset/record-metrics 行订正）、09（substatus 保留值口径 + 转换矩阵凭据门 + `danger_provenance` + `[review_gate]` + `orchestrator_must_check` 值域表 + 附录 B Step 0.3/5/6）；MDR-019 状态改「已落地」+ 6 项落地细化。
  - **测试**：809 全绿（基线 757 → +52），含硬门/CAS 旁路/人签批链/策略拒因逐条/返工边证据作废/provenance 三态与解析失败降级/覆盖率阈值取自 config/纯查询无写盘/fail-closed/幽灵模块/`--attest` 无 policy 硬拒。
- **QUAL-02 质量基线报告收尾（2026-07-24，PR #83 已合并）**：新增 [m4-quality-baseline.md](m4-quality-baseline.md)——**汇编已验证真实迁移数据为横比起点，不重迁不造数**（M3 产物 + /tmp 工作区已清空）。三档口径如实分层：① Go（semver/go-humanize）`stats quality` CLI 实测 final_score=100 / test_pass_rate=1.0（276/276+87/87）/ degrade_rate=0；② Python（jmespath 901/902+豁免 D-10、textdistance 70/70）M3 强等价实测换算 test_pass_rate（**无 CLI 数值**——迁移早于 QUAL-01/05 框架落地）；③ **TS 真实项目基线缺口**（M3/M4 真实迁移全用 Python/Go，TS 仅 fixture 级）如实披露。**主审数据真实性核对全过**（无编造、可追溯、口径诚实、TS 缺口如实标注、无死链）；important（PLAN-M4 checklist `[x]` 略读误判 TS 已完成 → 改 `[~]` 部分完成 + 文案解耦）+ nit（回归机制章节引用订正到 §4.9「前 3 Sprint 均值 −10%」）全修。其余三视角按纯文档豁免。
- **M4 收尾审查（2026-07-24，PR #82/#81 已合并）**：
  - **#82（loc_ratio 排除测试，4 视角全跑）**：新增 `count_loc_excluding_tests` + `TEST_FILE_PATTERNS`（各语言测试文件命名 glob），`compute_project_loc_ratio` 改用它排除源侧测试（issue #78：`_test.go` 稀释 LOC 比）。**主审 1 imp + 设计契约 1 DEVIATION + 专项 2 imp + nit 全修**：① 注释订正——删失实的「Rust 侧 `*_test.rs` 排除」表述（patterns 无 Rust 模式、仓库无此命名文件、Rust 惯用内联 `#[cfg(test)]` 文件级 glob 排不掉），如实标注比率对 Rust 测试体量敏感；② 同步设计 03:649+06:130 口径（`count_loc`→`count_loc_excluding_tests`）消除 DEVIATION；③ 补 CLI e2e `e2e_stats_quality_loc_ratio_excludes_source_test_files`（A/B 对比法，锁定调用点不回退、不依赖 tokei 绝对计数）；nit：补 C 约定/conftest.py + 嵌套目录 `**` 递归语义测试（实证 tokei 按 gitignore glob 处理深层路径）。异构交叉（codex）用户叫停未等，其独特点（嵌套 `**`）已由新测覆盖不留缺口。
  - **#81（Leiden→Louvain 术语统一 + PLAN-M4 checklist 回填，纯文档→主审 1 视角，其余豁免）**：主审 0 imp，2 术语残留 nit 已修（04-toolchain.md:148 TRIAL 风险表补 MDR-011/自实现 Louvain 指针、PLAN-M4.md:225 QUAL-04 描述对齐 Louvain）；核实 checklist 回填真实（PR #57/#58/#77/#79 证据可对）、`m4-quality-baseline.md` 诚实留 [ ]。
- **Sprint F GOV-01 交付**（2026-07-05）：`validate rules` CLI 命令——校验各适配器 `porting-template.md` 的 `rule_version` vs 权威清单 `plugin/skills/migrate/references/rule-registry.json`。新增核心模块 `core/src/validate/rules.rs`（模块 `validate::rules`，命令↔模块同名；load_rule_registry/parse_template_rule_version/check_template_consistency/check_adapters_dir，三类 issue：missing_in_template/version_mismatch/unknown_rule）；`RulesConfig` 落地 `enforce_rule_version_consistency`（默认 true）——不一致时 enforce=true→`status=error` 退出码 1（非静默）、false→降级 warning。18 新测（13 单测 + 5 cli_e2e，含**真实模板一致回归守卫**）；`just ci` 全绿。设计同步：06 §10.0.1 命令清单 + §11.1 `[rules]` 注释；MDR-014。**砍 index.json**（YAGNI，对齐 PLAN）。**4 视角审查（主审/设计契约/专项/异构交叉）全跑 → 2 important + N nit 修复**：(A) `parse_template_rule_version` 只匹配顶层 `rule_version:`（去 `trim_start`），缩进 nested 字段不误采；(B) enforce=true 报错时结构化 `checks` 经 `ErrorData.details` flatten 提升到 `data` 顶层（对齐 `cycle_path` 先例），CI 机读可拿逐条不一致清单、并保留读配置 warnings；nit：CRLF/空值/空清单/nested-only 回归测试、cli_e2e 聚合全模板 issue（弃 `checks[0]`）。
- **Sprint D 全达标**（2026-07-05，见 [m4-sprint-d-acceptance.md](m4-sprint-d-acceptance.md)）：PLG-03/04 translator/analyzer/verifier Go 分支（4 视角审查全跑，2 important+5 nit 已修）；**PLG-05** Go `classify_file` danger 分类（goroutine/select/channel→Concurrency、reflect→DynamicReflection、cgo/unsafe→Ffi）端到端落 state + translator degrade crate 推荐；**PLG-06** 单文件 Go 模块 headless 全链路推进到 `translating` + 真实 Phase A 翻译 cargo check 绿；**修复 pre-existing bug**：populate tier 硬编码 TS adapter（非 TS 文件恒判 Full）→ 改按语言选 adapter。go 单测 51 + cli_e2e 81 全绿，`just ci` 通过。
- **M3 收尾（2026-06-29）**：Sprint A/B/C/D/E 全部合并，验收 M3-VAL-01~08 全达标；PR [#49](https://github.com/snowzhaozhj/rewriteInRust/pull/49)（ffi 测试修复）+ [#52](https://github.com/snowzhaozhj/rewriteInRust/pull/52)（source_root 探测加固）已合并；遗留 issue [#50](https://github.com/snowzhaozhj/rewriteInRust/issues/50)（source_root 推断）+ [#51](https://github.com/snowzhaozhj/rewriteInRust/issues/51)（VAL-05 性能实测：TS 路径 0%/-16%/-1% 无退化）已 CLOSED+COMPLETED；PLAN-M3 验收清单已全部回填 [x]。
- **阶段**: Sprint A ✅ → Sprint B ✅ → Sprint C ✅ → Sprint E ✅ → **Sprint D 端到端验收 ✅（M3-VAL-01~08 全达标，2026-06-29，PR [#49](https://github.com/snowzhaozhj/rewriteInRust/pull/49) 已合并——4 视角审查全跑、1 important（设计文档同步）+ 4 nit 全落实、just ci 532 绿）**
- **🟢 Sprint D 端到端验收 ✅**：2 真实 Python 项目各 ≥1 模块迁移到 done（按 §6 headless 规范）。
  - **VAL-02 jmespath**：2 模块全 done（coupled_batch 7 文件 + visitor.py），**902 黄金集 901 等价 + 1 豁免（D-10）**，端到端 `search()` 全链；独立复核 cargo test/clippy --all-targets 全绿。
  - **VAL-03 textdistance**：base.py 组（编辑距离算法）done，golden_edit_seq 70/70 等价；vector_based 草稿态忠实保留 unimplemented!()。
  - **VAL-04 差异测试**：golden 套件落地（源引擎录制→Rust 逐条断言），两项目实证。
  - **VAL-06 graduate**：jmespath 毕业成功 + textdistance 正确拒绝未完成。**VAL-08**：just ci 全绿。
  - **暴露并修复 4 项真实工具缺口**：① stats compare 支持 Python 源（补完 deferred M3）② scaffolder golden harness present-null 区分 ③ translator 加 Edit 工具防 Phase B Write 截断 ④ verify.sh done 门补全量集成测试 + --all-targets clippy。详见 `docs/sprint-d-acceptance.md`。
  - TODO 落账：ffi.rs 测试 deprecated（✅ 已修，PR #49）；analyzer source_root 推断加固 → [issue #50](https://github.com/snowzhaozhj/rewriteInRust/issues/50)；VAL-05 性能实测 → [issue #51](https://github.com/snowzhaozhj/rewriteInRust/issues/51)。
- **🟢 M3-DEC-02 轻量翻译路径 ✅**（PR [#46](https://github.com/snowzhaozhj/rewriteInRust/pull/46)，2026-06-28 已合并）：run.md 机械合批组轻量路径实现。
- **🟢 M3-DEC coupled_batch 分流修复 ✅**（PR [#48](https://github.com/snowzhaozhj/rewriteInRust/pull/48)，2026-06-28 已合并）：修复 populate 把非机械 batch 展开成独立模块、推翻 decompose 分组的接口断裂（与 MDR-011 §6 矛盾）。grilling + codex 双审收敛后实施：
  - **新增 `CompositeKind::CoupledBatch`**：`Batch` 收窄为全机械（轻量路径，编译即门禁）；`CoupledBatch`=含逻辑耦合簇（完整组路径：翻译→结构门→Phase B→行为测试→审查）。populate 保留 `classify_file` 按成员机械性分流（读失败保守落 CoupledBatch）。
  - Plugin 文档：run.md 新增「CoupledBatch 组完整路径」+ 形态/路由分支；translator.md 新增「CoupledBatch 组翻译」；workflow.md 修正「多文件=SCC」分派为按 `composite_kind` 分流（codex 标的真风险）；analyze.md 同步三类 composite 说明。
  - 测试：衔接测试改断言 coupled_batch + 组感知 `state deps`；新增 py-pkg-deps 混合簇保留为 1 个 coupled_batch 回归测试；orphan/active-progress 测试 pin `--no-decompose`（保留旧路径回归）。
  - 验证：`just ci` 全绿；jmespath 真实场景 8 文件→2 模块（1 coupled_batch[7]+1 single），符合预期。
  - 计划文档：`docs/plan-populate-batch-unify.md`（含 grilling 决策记录 + codex 8 条补充）。
  - 审查：4 视角全跑（主审/设计契约/专项 4 agent/异构交叉）。本次引入项全修：枚举头注释「两种→三种」、09-schema 补 `coupled_batch`、补全机械 Batch 回归测试（新增 `fixtures/ts-mechanical-batch` + `e2e_populate_all_mechanical_cluster_is_batch`）、MDR-011 §8 偏离回链、member_files/decomposition_frozen 注释更新、`all_mechanical` debug_assert、`--human` 覆盖回补、deps 断言强化。
  - TODO 落账（pre-existing，独立 PR）：① danger→RULE/定向测试注入（跨路径既有缺口）；② `graph topo-sort --members --reverse`；③ `read_failures` 缺阈值硬门禁——全/高比例读失败时静默产出退化 plan（PLG-06 既有，CoupledBatch 路由会放大影响）；④ `state transition` 不做非代表成员 key 组归一（与 `state deps` 不对称）；⑤ 默认 decompose 路径下「组缩小/整组消失」的孤儿清理无回归覆盖。
- **🟢 PLG-06 populate-modules 接入 decompose ✅**（PR [#47](https://github.com/snowzhaozhj/rewriteInRust/pull/47)，2026-06-28 已合并）：`populate-modules` 消费 `plan_decomposition` 产出，写 `migration-state.json`（`composite_kind` + `member_files` + `decomposition_frozen`）。新增 `--budget`/`--no-decompose` 参数。（注：原「含 non-mechanical 成员展开为独立模块」行为已由上方 M3-DEC coupled_batch 修复推翻。）
- **MDR-011 ✅ 已合并（PR [#45](https://github.com/snowzhaozhj/rewriteInRust/pull/45)，2026-06-28）**：目录优先两阶段凝聚合并。10 真实项目均值 ~76% 缩减。
- **Sprint E ✅ 全部完成**：DEC-01（PR #43）+ DEC-GATE（Python 分类器修复）+ DEC-02（PR #46）。
- **测试基线**: 600 测试 / clippy -D / deny / fmt / shellcheck + plugin validate 全绿
- **CI 覆盖率**: 待更新
- **Sprint F ROB-01a 交付**（2026-07-05，待审查）：**checkpoint 硬化 + 幂等重试**。现状调查确认**原子写已达标**（`atomic_write` tmp+fsync+rename+dir-sync+backup），缺口在幂等重试（`transition_module` 同态报错、回滚全靠 run.md 文字约定）。交付：① core `MigrationStateMachine::reset_module` + `ResetOutcome`——确定性状态回退（→translating、清全部进度字段、保留 attempts+审计、结构冻结字段不动）、`done`/`blocked`/`degrade_*` 须 `--force`、已在干净入口时**幂等空操作**（`reset;reset`==`reset`、免落盘）；② CLI `state reset --module <M> [--force]`（`cmd_state_reset`，输出 `cleanup.member_files` 源作用域驱动编排器删部分 `.rs`——CLI 不猜路径删文件）；③ 全字段 round-trip 完整性测试钉「不丢字段」。**边界决策 MDR-015**（收窄版方案 A：CLI 做状态回退+输出清单、产物 `.rs` 删除归编排器，不动 schema、YAGNI 同 index.json）。9 新测（8 核心 + 1 cli_e2e）；SKILL.md 单点收敛「失败/中途模块回滚」+ run.md 两处回滚约定改引用 `state reset`；设计 06 命令清单同步。`just ci` 全绿（707 测试）。**下游**：ROB-01b（watchdog）/ROB-01c（额度续跑）将复用 `state reset`。
- **最新合并 PR**: [#75](https://github.com/snowzhaozhj/rewriteInRust/pull/75)（ORCH-01 PR-4 编排集成测试）；[#74](https://github.com/snowzhaozhj/rewriteInRust/pull/74)（ORCH-01 PR-3 两层 done 接活）；[#73](https://github.com/snowzhaozhj/rewriteInRust/pull/73)（ORCH-01 PR-2 worktree 统一）；[#71](https://github.com/snowzhaozhj/rewriteInRust/pull/71)（ORCH-01 PR-1 并行分层落 CLI）；[#68](https://github.com/snowzhaozhj/rewriteInRust/pull/68)（ROB-01c 额度续跑）
- **ROB-01a 已合并**（2026-07-05，PR #65）：**4 视角审查全跑 → 2 important + 共识守护 + 2 MEDIUM + nit 全修**：① graduate 项目态守护（codex important，防矛盾终态）；② paused 纳入 `--force` 守护（专项 HIGH + 主审 + 设计契约共识，防绕过降级抉择）；③ canonical_module_key 不变量破坏 debug_assert→release 硬错（专项）；④ was_noop 时 cleanup 给 `skip:true`（专项，编排层幂等）；⑤ was_noop+backup 自愈、attempts 语义 MDR 点明、pre-existing len_zero 顺手修。CAS version 不递增判定为 pre-existing（transition 亦不递增）→ MDR-015 记 TODO。`just ci` 全绿（710 测试）。
- **Sprint F ROB-01b 交付**（2026-07-05，待审查）：**watchdog stall 检测 + 恢复路径**。现状缺口：系统只有「调用级总超时 + 产出物校验失败」两类可计数失败，**stdout 静默卡死**（agent 假死/外部命令 hang，无返回无报错）计数器兜不住。交付 **MDR-016 分工**（延续 MDR-015）：**检测归编排器**（CLI 是短命子进程、观测不到子进程 stdout，靠主会话 background bash + `BashOutput` 轮询 stdout 静默超 `stall_timeout_secs`）；**恢复归 CLI** `state recover --module <M> --policy retry|skip [--reason]`——① retry 委派 `reset_module(force)` 回退干净重译入口（复用幂等 + `member_files` 作用域）+ 追加 `stall-recover:retry` 审计；② skip **直设 `paused`**（决策点，headless 自动 degrade_skip）——**绕 `can_transition_to` 矩阵**（stall 可发生在 `translating`，而 `translating→paused` 不在矩阵，仿 reset 破坏性直设）；幂等（retry 已净/skip 已 paused|degrade → `was_noop`）；守护 `done`/`blocked`（非 stall 态）+ `graduate` 拒绝（无 `--force` 逃生口——recover 是程序化入口，误用暴露为错误）。core `recover_module`+`RecoverPolicy`/`RecoverOutcome`；CLI `cmd_state_recover`+`RecoverPolicyArg`；`OrchestrationConfig` 扩 `stall_timeout_secs`(600，与总超时正交)+`stall_recovery_policy`(RetryThenSkip)。**策略解析三方分工**：config 声明→编排器读 config+retry-round 解析 `--policy`→CLI 无状态确定性执行。Plugin：SKILL.md 新增「Watchdog stall 检测与恢复」单点 + run.md 计数器段补正交说明 + workflow.md 失败不阻塞补 worktree stall 分支。设计同步：06 CLI 表 + 两处 `[orchestration]`。12 新测（8 core + 1 cli_e2e + 3 config）。**下游**：ROB-01c（额度续跑）复用 `state recover --policy retry` 幂等重入。
- **ROB-01b 4 视角审查全跑 → 2 important + 共识 Medium + Low + nit 全修**（PR [#66](https://github.com/snowzhaozhj/rewriteInRust/pull/66)，待用户验收）：① **[codex HIGH]** recover 守护漏 `degrade_*`——retry 委派 `reset_module(force)` 会绕过「degrade→translating 须 --force 人类确认」边界、`retry;skip` 把依赖侧已视终态的 degrade_skip 变回非终态 → 守护改**全枚举显式 match**，degrade_* 拒绝；② **[专项]** `recovery.unblock_next` 命令语法错（`state deps` 是位置参数非 `--module`，编排器照做 cli_parse 失败）+ 语义偏（查的是该模块自身依赖就绪非「无依赖模块清单」）→ 改对；③ **[三方共识 Medium]** pending 纳入守护拒绝（skip 把未起步模块直设 paused、与 reset 不对称）；④ **[设计契约+codex Low]** skip 清 `substatus`（translating 瞬态标记挂 paused 语义不符），保留其他进度字段供降级分析；⑤ **[nit]** 09 转换矩阵补 reset/recover 绕矩阵例外脚注。设计契约 0 important（CLI JSON/状态机/枚举/config 六项逐条 PASS）。守护现为「仅放行运行态+paused，拒绝 pending/done/blocked/degrade_*」。`just ci` 全绿。
- **⚠️ ROB-01b 审查修复独立 PR（合并后补交）**：#66 初版在 4 视角审查跑完前即被合并（merge commit `04611e8`，parent = 初版 `0665ccd`），**2 个 important 修复未进 master**（degrade 守护 bug + unblock_next 命令 bug）。审查修复（含上条 5 项）已 cherry-pick 到新分支 `fix/m4-rob-01b-post-merge-review-fixes` 独立提 PR，**已合并（PR #67，merge commit `0a5ac5d`）**——master 的 ROB-01b 现已完整。
- **Sprint F ROB-01c 交付**（2026-07-05，待审查，分支 `feat/m4-sprint-f-rob-01c-quota-resume`）：**额度耗尽优雅暂停 + 断点续跑**。现状缺口：ROB-01a 已逐步原子 checkpoint、ROB-01b 已 stall 恢复，但**额度刷新后续跑的确定性入口**缺失（「哪些中断需幂等重入、哪些已完成不重跑、下一步做谁」只能手工翻 state）。交付 **MDR-017 分工**（延续 MDR-015/016）：**检测归编排器/harness**（CLI 观测不到 token 预算/API 额度，用 harness budget.remaining() 或人工判断）；**优雅暂停 = 当前原子步收尾后停止**（状态已 checkpoint，无需单独 pause 写）；**续跑计划归 CLI** `state resume`——**纯查询、无副作用、不加载 graph**，按 `ModuleStatus` 全枚举 match 归 5 桶：运行态（translating/compile_fixing/testing/reviewing）→ `interrupted`（各带 `recover_command`=`state recover --policy retry` 幂等重入）；`paused` → `awaiting_decision`（**续跑不复活**——规避绕过降级抉择的正确性 bug）；`pending` → `next`（用 `state deps` 判就绪）；`blocked` → `blocked`；终态（done/degrade_*）**不重跑**仅计入 `progress`（六桶计数，total==各桶之和）。实际重入**复用** `state recover`（不重复 mutation）；`pending` 就绪**复用** `state deps`。**不加额度阈值 config**（YAGNI，harness 已持 budget）。core `resume_plan`+`ResumePlan`/`InterruptedModule`/`ResumeProgress`；CLI `cmd_state_resume`。8 新测（7 core + 1 cli_e2e）。Plugin：SKILL.md 新增「额度耗尽优雅暂停与续跑」单点 + run.md 计数器段补正交说明 + workflow.md「失败不阻塞」补 budget-aware 暂停。设计同步：06 CLI 表 + MDR-017。**已合并 PR [#68](https://github.com/snowzhaozhj/rewriteInRust/pull/68)（2026-07-05，含 6 视角审查修复）。**
- **Sprint F ORCH-01 实现进行中**（2026-07-19）：决策反转已合并（PR #70），ORCH-01 按 5-PR 规划落地：**PR-1 CLI/state 并行分层落盘 ✅ 已合并（[#71](https://github.com/snowzhaozhj/rewriteInRust/pull/71)）→ PR-2 worktree 机制统一 ✅ 已合并（[#73](https://github.com/snowzhaozhj/rewriteInRust/pull/73)）→ PR-3 两层 done 接活 ✅ 已合并（[#74](https://github.com/snowzhaozhj/rewriteInRust/pull/74)）→ PR-4 编排集成测试 ✅ 已合并（[#75](https://github.com/snowzhaozhj/rewriteInRust/pull/75)）→ PR-5 真实项目并行演练 ✅ 跑通（分支 `feat/m4-orch-01-pr5-real-demo`，待审查）**。**ORCH-01 全 5 PR 落地完毕。**
  - **三项方向决策（用户拍板）**：① worktree 保留手动 `git worktree add`（删 isolation:"worktree"，因其从 origin 建会丢本地 done 代码 + 编排器拿不到路径无法统一合并）；② `parallel_groups` 收口到 `scc_groups`（已确认 `SccGroup.sprint` = 同层拓扑独立可并行）；③ 验收 = mock 集成测试进 CI + 一次真实项目并行演练。
  - **PR-1（#71）交付**：删死字段 `MigrationSequence.parallel_groups` + `compute_parallel_groups`/`compute_level` 死函数；新增 CLI `graph parallel-groups`（按 sprint 聚合并行层，有环折叠为 is_cycle 组不报错，与 topo-sort 相反）；核实 `populate-modules` 已写 `ModuleState.sprint`，无需新增 schema（YAGNI）；深链栈溢出守护测试迁到 `compute_scc_level`；proptest/ground_truth 不变量改按 scc_groups 聚合验证。**4 视角审查全闭环**：设计契约 PASS；主审+专项 1 important（误删 `smoke_init` 的 `#[test]`）已修；异构 codex 确认收口在有环图下正确（前提：编排器把 group 当原子调度单位——已写进 CLI doc + 设计 06），并抓 1 important（有环并行不变量未覆盖 → 新增 `arb_multi_scc_graph` proptest）已修；nit 全修。
  - **PR-2（#73）交付**（纯 plugin 文档）：删 workflow.md 的 `isolation:"worktree"`（消除文件内两套 worktree 机制矛盾，统一手动 `git worktree add`）；workflow.md 改读不存在的 `migration_sequence.parallel_groups` → 按 `ModuleState.sprint` 筛并行层 + `graph parallel-groups` 命令；SKILL.md 消除「MVP 串行执行」矛盾 + 补命令清单。主审无 important + 设计契约 PASS（4 项），专项/异构豁免。
  - **TODO（记账）**：SKILL.md 命令清单完整审计——现缺 quality/community/rules/graduate/advance-sprint 等十余命令，PR-2 只补 parallel-groups + reset/recover/resume，全量审计独立维护。**✅ 已由 [#85](https://github.com/snowzhaozhj/rewriteInRust/pull/85) 收口**（补 11 条 + 程序化验证 35/35 穷举 + 同步设计 06 命令表）。
  - **PR-3 交付**（两层 done 接活，非删除——[MDR-019](decisions/019-post-translation-review-gate.md):50 已定「随 ORCH-01 接活路径」）：新增 CLI `state batch-transition-done --module <M>...`（可重复，入口去重），委托已存在但死的 `batch_transition_done`（machine.rs:1005），把整组 `agent_done` 模块批量升终态 `done`；逐模块独立转换、非 `agent_done`/矩阵拒绝/模块不存在跳过、`skipped`/`duplicates` 非空降级 warning。workflow.md 步骤 2d 从「逐模块 `state transition --to done`」（无 agent_done 守卫）改为一条 batch 命令；设计 06 命令表新增行。4 个 e2e（全成功 / substatus 非 agent_done 跳过 / status 非 reviewing + 模块不存在跳过 / 重复去重）。
  - **PR-3 4 视角审查闭环**：主审 0 发现；专项 0 important + 3 nit（全修）；设计契约 PASS + 1 文案 DEVIATION（已修）；异构 codex 3 important + 3 nit。裁决：**nit 全修**（入口去重、warning 文案去掉虚称「已记入 attempts」、补 2 个 skip 路径测试）；**codex 2 个 important（imp2 并行未推进到 reviewing、imp3 `reviewing→compile_fixing` 被矩阵拒）经核实是 master(PR-2) 既有、PR-3 未触碰**——是 MDR-018 指出的「编排从未端到端跑通」缺口，归 **PR-4 集成测试**暴露修复（记 TODO）；**codex imp1 + 设计契约 DEVIATION（batch 绕过 MDR-019 签批门）** 的实质是：签批门坐落在 `reviewing → done` 这条边**本身**（不是 done 之后），MDR-019 未实现故当前无门可越，但 MDR-019 落地 PR 必须确保编排器不对「需人签批」模块调 `batch-transition-done`，否则绕过签批——已在 06 命令表 + MDR-019 落地清单加护栏注记。
  - **PR-3 遗留 TODO（记账，归 PR-4/MDR-019）**：① 并行 workflow 回传后须由编排器把主状态推进到 `reviewing`（现只设 `agent_done` substatus，batch 的 `→done` 会全被拒）——PR-4 mock 集成测试锁；② 整组验证失败的 `reviewing → compile_fixing` 回退边在矩阵不存在——与 MDR-019 的 `Reviewing → Translating` 返工路径统一定义；③ batch 命令不得对 `awaiting_final_review` 模块放行——MDR-019 落地 PR 加编排器守卫。**②③ 已于 MDR-019 落地 PR #84 结清**：② 两条返工边一并加入矩阵（`Reviewing => Done | Blocked | Translating | CompileFixing`）；③ 不止「编排器守卫」——`batch_transition_done` 在 **CLI 层**对 `awaiting_final_review` 模块一律拒（`skipped.code=awaiting_final_review`），不依赖提示词纪律。① 已由 workflow.md 步骤 2d 的 `testing→reviewing` 两步转换 + PR-4 集成测试覆盖。
  - **PR-3 已合并**（PR [#74](https://github.com/snowzhaozhj/rewriteInRust/pull/74)，squash 到 master `74d20da`）。
  - **PR-4 交付**（编排集成测试，补 MDR-018:47「端到端集成测试完全不存在」硬门）：新增 `cli/crates/cli/tests/orchestration_e2e.rs`——**Rust harness 扮演编排器**，确定性驱动真实 CLI（`run_with_args`）+ 真实 git（worktree/merge）+ 真实 cargo（整组 check），mock 产物冒充 SubAgent 回传，不跑真 LLM。3 用例：① happy path（2 模块并行 worktree→逐层 merge→整组 check 绿→两层 done 全升 done）；② merge 冲突（2 模块同改 error.rs→第二个真 merge 冲突→`git merge --abort`+标 `compile_fixing` 重译，MDR-003 约束7）；③ 整组 check 真门拦截（模块引用未定义符号→整组 `cargo check` 真失败→batch 全被拒、无一升 done）。`TranslationDispatch`/`TranslationResult`/`AgentStatus` 在集成测试中被消费（编排器据 `result.status` 驱动 agent_done 标记、据 `result` 分支名驱动合并）——不再是仅 roundtrip 单测覆盖的纯数据（真实非测试消费仍待 PR-5 真实演练 / 未来编排落地）。**顺带修 PR-3 遗留 TODO ①**：workflow.md 步骤 2d 补「整组 check 过→编排器对每模块 `--to testing`→`--to reviewing`→再 batch」（agent_done substatus 在这两步保留），并加译后签批门护栏注记（inline，不留 docs 死链）。
  - **PR-4 pre-existing 记账**：run.md:93 `[MDR-011](../../docs/...)` 相对路径少一级（应 `../../../docs/`），是运行时死链——与本 PR 无关，独立修（提示词按规范本就应内联、不留 docs 链接）。
  - **PR-4 4 视角审查闭环**：主审 0；专项「测试有效可合并」0 important + 4 nit；设计契约 PASS + 1 DEVIATION（compile_fixing 借用表意 reconcile）+ 2 覆盖说明；异构 codex 4 important + 2 nit（codex 实证用户最担心的跨 binary cwd 竞态/冲突真触发/agent_done 保留三点**均无问题**）。**已修**：codex I4（workflow 新循环误推进 paused 模块——改为只遍历「成功 agent_done 模块」+ 注明失败模块停 paused）；codex N1（步骤 2-11 含 done 与 2d「不执行 done 步」矛盾——单文件模块改述为「只产码+自检、不改主 state 终态」）；codex I3+专项 nit-1（真门只看退出码——`cargo_check` 返回诊断文本，断言命中 E0425/nonexistent_symbol + 隔离 `CARGO_TARGET_DIR`）；codex I2（协议类型摆样子——happy path 改为据 `result.status`/`result.module_key` 驱动 agent_done 标记与合并，STATUS 措辞据实改「集成测试消费」不再称「首获非测试消费者」）；设计 DEVIATION（compile_fixing 语义——注释改引 2d 归因表「跨模块冲突回 compile_fixing」而非混称 reconcile）。**记范围边界（非缺陷，harness 固有）**：codex I1 并行链是「串行模拟」——真并发 worktree/target 锁争用 + worktree 内真自检（MDR-003 约束1）+ reconcile 轮次上限降级串行，均属确定性 CI harness 覆盖不到、归 **PR-5 真实演练**；专项 nit-2（真门与 batch 拒绝是独立断言、CLI 不强制 check 门）注释已点明。
  - **PR-4 已合并**（PR [#75](https://github.com/snowzhaozhj/rewriteInRust/pull/75)，squash 到 master `cdd1701`）。
  - **PR-5 交付**（真实项目并行翻译演练，产出 [m4-orch-01-acceptance.md](m4-orch-01-acceptance.md)）：真实开源项目 **textdistance**（MIT，30+ 距离算法）的 sprint-3 拓扑层 **3 路真并发翻译**端到端跑通——`no-decompose` 分层（14 模块 6 层，默认 MDR-011 凝聚会把 `algorithms/` 压成 3 coupled_batch、并行度被吃掉）→ 前置串行译 types/utils/base 到 done → libraries.py degrade 解门禁 → sprint3 三路 translator 真并发（各独立 worktree + 隔离 `CARGO_TARGET_DIR`）→ 逐层 `git merge`（**2 次真实 lib.rs append 冲突，结构化合并解决**，非重译）→ 整组 `cargo check`/`clippy -D warnings`/`test` 真门全绿 → 推进 `reviewing` → `batch-transition-done` 两层 done → worktree 清理。**3 模块（simple/sequence_based/phonetic）真实迁到 `done`**（1536 行 Rust ← 719 行 Python）。补齐 PR-4 mock harness 覆盖不到的真并发/worktree 真自检/真 merge 冲突（codex I1 边界）；`batch-transition-done` 首次真实项目运用。
  - **PR-5 撞出真实工具缺口（已修 #1，记 TODO #2/#3）**：**① scaffold 不生成 `.gitignore`**（`cargo init --vcs none` 显式禁 VCS 文件）→ worktree 自检 `target/` 被 `git add -A` 吞入提交污染合并——**已修**：`scaffold::template` 两个 scaffold 函数补 `write_gitignore`（`/target`，幂等）+ 2 单测；**② 图静态传递依赖 vs 翻译期 safe-default 裁剪偏差**（base 裁剪 libraries 后下游门禁仍卡 libraries）——记 TODO；**③ workflow.md 步骤 2c 对聚合文件（lib.rs/mod.rs）append 冲突指引不足**（应结构化合并、非 abort 重译）——**✅ 已由 [#85](https://github.com/snowzhaozhj/rewriteInRust/pull/85) 收口**（补冲突分型；判据按实体身份而非行文本，避开 E0252/E0428/E0761 类撞名误判）。排除 1 个伪缺口（疑 substatus 不持久化，实为脚本假象，CLI 正确）。
  - **PR-5 范围边界（非缺陷）**：Phase A 忠实翻译未产单测（整组 test 0 tests，等价测试深度是 Sprint D 已验证能力）；按「证明性:1 层并行到 done」只跑 sprint-3 层未全量 graduate；headless safe-default 裁剪 base 的 libraries 外部库加速路径（等价 external=false）。
  - **PR-5 4 视角审查闭环**：主审 1 nit（幂等早返回不补 .gitignore）；专项 0 important + 1 参考（同上，判设计权衡可合并）；设计契约豁免（scaffold .gitignore 不涉 types/CLI JSON/state schema/状态机/枚举/pub 字段）；异构 codex **2 important + 4 nit**。**codex 2 个 important 均采纳修复**：① 已有 `.gitignore` 但无 `/target` 时跳过、安全目标不成立 → `write_gitignore` 重构为**后置条件式幂等**（缺有效 `/target` 规则则追加、保留用户内容，非「文件存在即跳过」）；② `Cargo.toml` 已存在的早返回破坏失败重试语义（init 成功但 gitignore 失败后重跑补不上）→ 早返回路径也调 `write_gitignore`。nit：精确 `assert_eq!` 断言 + bin 测试补 gitignore 断言 + 4 个回归测试（追加/去重/补齐/保留）。codex nit 3（TOCTOU）不修——各 worktree 独立路径不触发，codex 自己承认「当前拓扑未证实触发」。共 9 个 scaffold 测试全过。
- **PR-5 已合并**（[PR #76](https://github.com/snowzhaozhj/rewriteInRust/pull/76)，squash 到 master `637f636`，2026-07-20）。**ORCH-01 全 5 PR 落地完毕，Sprint F 除既有 ORCH-01 外全 ✅。**
- **最新合并 PR**: [#77](https://github.com/snowzhaozhj/rewriteInRust/pull/77)（M4-QUAL-05 质量度量数据流修复，已合并 `f817fe7`）；[#76](https://github.com/snowzhaozhj/rewriteInRust/pull/76)（ORCH-01 PR-5 真实并行演练）；[#75](https://github.com/snowzhaozhj/rewriteInRust/pull/75)（ORCH-01 PR-4 编排集成测试）
- **当前工作 = Sprint E（Go 端到端验收 M4-VAL-01~08）✅ 全达标**（2026-07-21，见 [m4-sprint-e-acceptance.md](m4-sprint-e-acceptance.md)），验收文档 + VAL-07 设计文档同步待审查（分支 `feat/m4-sprint-e-go-acceptance`，不自行合并）。
- **Sprint E 进展（2026-07-20）**：
  - **VAL-01 ✅ 选型**：semver（主力，1656 行/4 文件单包）+ go-humanize（12 文件多模块，含 math/big）。均交叉验证真实存在、MIT、纯标准库、0 并发/cgo/reflect。降级点决策=**如实记录不强造**（用户拍板）。工作区 `/tmp/{semver,humanize}-migrate`，绝不动本仓库。
  - **VAL-02 ✅ semver 迁到 done**：Phase A 忠实翻译 1532 行 Rust ← 1656 行 Go（全干净直译，0 降级）；编排器**独立验证**（不信 subagent 自报）整组 `cargo check` 0 error + `clippy --all-targets -D warnings` 0 warning；模块 `file:collection.go`（coupled_batch 4 文件）→ done。regexp→regex crate、指针 receiver→&self、Go error→Result、sql driver/json→独立方法、包级 var→AtomicBool。
  - **VAL-04 ✅ semver 差异测试 276/276 等价**：Go 录制程序（replace 指向本地 src）录 222 行为点→Rust `tests/differential.rs` 276 断言逐条对照，**编排器独立负向验证**（篡改 fixture→立即报 `1/276 不一致`→还原恢复，证明断言非空跑）。translator 标的 3 存疑点（equal nil 短路 / from_utf8_lossy / 全局开关）全部确认等价、非 bug。
  - **VAL-03 ✅ go-humanize 迁到 done**：Phase A 忠实翻译 1074 行 Rust（12 `.rs`）；`big.Int`→num-bigint、`big.Rat`→num-rational 忠实；**唯一自然降级**=`BigCommaf(big.Float)`（Rust 无无系统依赖任意精度二进制浮点忠实等价，保留 `TODO(port)`/`unimplemented!()`）。模块 `file:big.go`（12 成员 coupled_batch）→ done，metrics 写回 87/87。**编排器独立验证**：`cargo check` + `clippy --all-targets -D warnings` 全绿。
  - **VAL-04 ✅ go-humanize 差异测试 87/87 等价**：record/main.go 录 16 类行为点→`tests/differential.rs` 逐条对照；**编排器独立负向验证**（篡改 fixture[0]→立即报 `1/87 不一致`→还原恢复绿，证断言非空跑）；末尾 `assert!(total>=80)` 防空跑假绿。
  - **VAL-05 ✅ 质量度量达标**：semver + go-humanize 均 final_score=100 / behavior_coverage=1.0 / test_pass_rate=1.0 / degrade_rate=0，与既有语言基线同档。
  - **VAL-06 ✅ graduate**：两项目均正确推进 sprint_loop→graduate；graduate 态再调用返回配置错误（幂等守护生效）。
  - **VAL-07 ✅ 设计文档同步**：08-roadmap §M4（Go 完成 + C/Kani 推迟 + Community Tier 1 + 巩固线状态）+ 04-toolchain（tree-sitter-go + Go 工具链脚注）+ 02-architecture（已落地适配器注记）+ PLAN §11（M4 状态列）+ 03 §7.5（QUAL-05 已登记）。
  - **VAL-08 ✅ 全量回归**：`just ci` 757 测试全绿。
  - **暴露工具缺口（[issue #78](https://github.com/snowzhaozhj/rewriteInRust/issues/78)，独立 PR）**：`stats quality` 的 `project_loc_ratio` 把源侧 Go `_test.go` 计入 tokei LOC，稀释比率（humanize 0.465/semver 0.316，真实非测试比约 0.9）——`count_loc` 应支持排除测试文件；不阻断验收（loc_ratio 按设计不进评分卡、#77 已加 warning，final_score 未受影响）。
  - **M4-QUAL-05 质量度量数据流缺口已修（PR #77 已合并 `f817fe7`）**：真实 semver 迁移曾暴露 `stats quality` 的 `final_score`/`behavior_coverage`/`loc_ratio` 全 null——根因是 `ModuleState.test_pass_rate`/`known_differences` 有 schema 无 CLI 写入，且 quality 的 loc_ratio 硬编码 None。修复：①新增 `state record-metrics`（支持部分覆盖、composite key 归一，响应返回 canonical + 实际值，非法通过率拒绝且不落盘）；②`stats quality --source/--rust` 经语言无关 `count_loc` 计算项目级 LOC 比并显式 warning 粒度近似（**不走 compare_structure**，避免 Go/C 控制流嵌套 NotImplemented 连带丢 LOC；roots 重叠时比率留空不污染评分）；③串行 run + 并行 workflow 主 writer 接线，`TranslationResult` 可选回传 metrics，成功/失败样本均落盘，机械 batch 明确豁免；④core+e2e（含 Go/overlap/并行协议兼容真回归）守卫。semver 真实回归：behavior_coverage=1.0、loc_ratio=0.9567、final_score=100、avg=100。**4 视角审查已修**：主审 STATUS 开放 PR 矛盾；设计契约并行主 state 未写/失败样本漏记/输入值域/响应回显；专项 overlap 污染评分；nit（behavior_coverage 粒度、Ratio rustdoc、无 AI 降级口径登记）。`just ci` 757/757 + deny/shellcheck 全绿。

- **SKILL.md ↔ CLI 命令清单守卫 ✅ 已合并（PR [#89](https://github.com/snowzhaozhj/rewriteInRust/pull/89)，2026-08-10 02:42 squash 到 master `e9220bf`，远端 CI 针对该 sha 5 项全过）**——2026-08-08 起，**不依赖 #88**（从 master 起、已验证在 master 基线上守卫全过）。合并前曾 rebase 到 `8d41a7f` 并推送 `6261355`，远端 CI 针对该 sha 复跑（见下「CI 一次都没触发」那条教训）。收口 #86 记账 TODO ①。`06` 那边由 #86 钉住，但 `06:105` 表头同时要求同步 SKILL.md 清单、`SKILL.md:31` 自称「已穷举顶层子命令」，此前**零自动化检查**（#85 那次是一次性人工验证）。编排器直接读 SKILL.md，漂了就照抄不存在的命令。新增 4 测试（2 守卫 + 2 解析器合成字符串单测），追加进既有 `design_command_table.rs` 以复用 `cli_leaf_commands()`；四视角审查后增至 7。
  - **⚠️ 教训：CI 一次都没触发，等了 35 分钟才发现原因是 PR `CONFLICTING`**。推了 4 轮提交、`gh pr checks 89` 始终返回「no checks reported」——我先当成 GitHub 排队延迟。真因是 master 期间前进了一个提交（`8d41a7f` #87 合并后的 STATUS 更新）与本分支改了 STATUS 同一段，**GitHub 对冲突 PR 不跑 workflow**。判据：`gh pr view --json mergeable` 返 `CONFLICTING`、`gh run list --branch <b>` 返回空（真在排队会有 `queued` 行）。处置：`git fetch && git rebase origin/master` 解冲突（仅 STATUS 一处措辞）+ `--force-with-lease` 推，`MERGEABLE` 后 CI 正常触发。**「no checks」不等于「还在排队」，先查 `mergeable`**；长时开着的分支要定期 `git fetch` 看 master 是否前进。
  - **格式与 06 表格不同故不能复用 `parse_design_table`**：清单是锚点行之后连续 8 个「`- **<分组>**：`cmd`、`cmd`…`」的行内反引号列表。
  - **#86 记账预警的两个坑，实测后一个成立、一个不成立**（按实测写，不照抄记账）：⒜ **成立**——文档别处也有同格式的 `- **X**：` 段落（`守护`/`恢复`/`幂等`/`待签批`），全文件按行格式匹配会一并收进来；**其中缩进两格、真能落进区块的是 3 个**（`L1 存在性`/`L2 结构校验`/`幂等`）。故须以「命令清单 + 已穷举」锚点行定位区块、按**缩进量**收尾（紧随其后就有一个同格式但缩进为零的 `profile --adapter-tools` 一级列表项）。**本条初版记述被设计契约与专项两个视角订正**：原写「命中 13 行、5 个别处段落、会拖出大批伪幽灵」——13/5 取自未写明「须紧跟反引号」这个条件的更窄口径（按注释字面的正则是 30 行零缩进 / 41 行任意缩进），而「大批伪幽灵」经实测为 **0 条**（单独摘掉⒜只污染分组标签集合，被拖进的段落不含合法命令名、会被命令判据滤空），同一文件另一处注释写的「3 个」才对、两处自相矛盾。**在一个专门消灭「文档给出的具体值查不出来源」的 PR 批次里，我自己写了这类值**，已按实测全部订正。⒝ **不成立**——记账预警「`graph export [--format json|dot|mermaid]` 的 `json|dot|mermaid]` 会残留成命令名一部分」，实测该风险只存在于「正则替换占位符」的实现；**逐 token 在首个占位符处 break** 时管道段落在 break 之后、根本到不了。已在代码注释如实记录该实测结论。
  - **判据不写死清单**：命令项判据取 `Cli::command()` 的顶层子命令名（滤掉 `started`/`ok`/`error`/`timeout`/`agent_done`/`advanced:false`/`rule_version`/`--to`/`--status` 等十余个值域与裸选项反引号）；总数阈值取 `cli_leaf_commands().len()`。
  - **负向实证五轮**（独立 worktree，全部报红）：① 删 `graph rdeps` → 两测试红且归因精确到该命令；② 加幽灵命令 `graph blame` → 红；③ 整组删除「校验」→ 红；④ 摘掉命令项判据 → 红（伪幽灵被抓）；⑤ **摘掉区块收尾判定 → 只有分组冻结守卫红、`matches_cli` 却 PASS**（被拖进来的段落恰好不含合法命令名、被判据滤空）。⑤ 证明**两条守卫各有不可替代职责**：只写 `matches_cli` 会让区块锚定零覆盖——该结论已写进代码注释。
  - **当下一致性 35/35**（守卫首跑即绿，与 #86 记账一致；**正因首跑即绿才必须做上述负向实证**）。`06:105`、`SKILL.md:31`、测试文件头三处标注守卫存在。`just ci` 全绿。
  - **四视角审查抓出 4 imp + 5 nit，全部处置**（每条编排器独立复现后才修；测试 4 → 7）：
    - **imp【主审 + 设计契约各自独立复现，结构性盲区】「不认识」被当成「不存在」静默丢弃**：命令判据 `top.contains(first)` 同时扛两个正交职责（滤值域 + 判定是不是命令），于是**顶层名在 CLI 里不存在的项直接从视野消失、不报幽灵**。实测加 `audit unsafe --deny-new` / `doctor` 两条虚构命令，两守卫**全绿**；主审用**真实下线序列**复现更狠——CLI 下线 `graduate`、06 表按 CI 报错改好、SKILL.md 漏改 → **5 个守卫全绿放行**，而编排器照抄得到 `unrecognized subcommand 'graduate'`。06 侧早已用 `unparsed_rows` 把「不认识」改成显式报归因（该文件注释里正引以为戒），**我在隔壁重新实现时没沿用——这是本 PR 第二次犯同一个错**（第一次是 HTML 注释渲染语义）。修：新增 `unclassified_items`，每个反引号项必须能分类为「命令」或「已登记非命令」，分不了的报出。**修复中又踩一次同型错**：初版 `is_known_non_command` 按形态判「无空格单 token 即值域」，实测仍漏 `doctor`（它与 `started` 形态完全相同，形态判据原理上区分不了）→ 改显式值域清单（6 项）。
    - **imp【专项独立抓出，判据自遮蔽，最绕的一条】值域清单与 CLI 顶层命令名撞名时守卫被自己遮蔽**：`KNOWN_VALUE_TOKENS` 与 `cli_top_level_names()` 是两个可相交集合，一旦相交，清单说明文字里那个值域反引号就**「转正」成命令项、替真正缺登记的新命令填坑**。编排器独立复现：CLI 新增顶层命令 `timeout`（与 `--status` 值域同名）、SKILL.md 一字不改 → `matches_cli` **PASS**（`missing`/`ghost` 双双为空）。更危险的是分组冻结虽报红但归因指向「命令归属变了」，**照它的提示把 `timeout` 加进期望表就把守卫彻底关掉**——报错信息亲手指挥维护者关闭自己。修：新增 `value_tokens_do_not_collide_with_cli_commands` 把不相交升为显式不变量，撞名时报真因 + 给两条可行处置 + 明确禁止那条会关掉守卫的错误修法；分组冻结的报错文案也补同一警告。清单里躺着 13 个这类反引号，`timeout`/`error`/`ok` 都是相当自然的子命令名。
    - **imp【codex，文案过宽】** `SKILL.md:31` 原写「双向钉住」而守卫只比对**叶子命令路径**、参数完全不校验。codex 实证 + 编排器复现：清单写 `state resume --definitely-not-a-real-flag` 守卫全绿，实际执行返 `kind=cli_parse`。已收窄为「钉住子命令路径这一层、参数不在守卫范围内、照抄参数不保证能解析」。
    - **imp【设计契约，数字失实】** 见上一条⒜的订正说明。
    - **nit 全修**：`groups_are_frozen` 名强于实（codex 实测把 `state resume` 跨组搬家后标签/总数/命令集合全不变、两守卫 PASS）→ 改逐组冻结成员集合；`strip_placeholders` 只在 `--` 处 break 而 06 侧用 `-`（主审实测写 `graph build -r` 会报「`graph build` 缺失」这种误导归因）→ 统一；`SkillGroup` 无消费方的 `PartialEq/Eq` 派生删掉；`if !cmd.is_empty()` 经实证不可达 → 按本仓惯例如实标注为防御性而非删除；区块定位失败原报「35 条全部未登记」（响亮但归因差，真因是锚点措辞被润色或 Markdown 格式化器把两格缩进重排成四格）→ 改直接报真因并列出两处格式前提。
    - **登记两项已知边界**（设计契约 DEVIATION/PASS）：⒜ 守卫拓扑是「两侧各自对齐 CLI」，`06 ↔ SKILL.md` **无直接比对边**（两侧一致性由「都对齐 CLI」推出），命令**说明文本**的分歧不在守卫范围内——已在 `06:105` 措辞点明；⒝ 本 PR 按 CLAUDE.md 豁免表**本可跳过设计契约视角**（不动 `types/`/CLI JSON/state schema/状态机/枚举/pub 字段），但它抓到的两条恰落在豁免规则盲区里——改的不是契约的值，而是「契约被守住了」这个**元声明**。建议豁免条件补一句「或改动涉及 `docs/design/` 中关于实现的事实性声明」，记为独立待办。
    - **负向实证累计八轮**（五轮初版 + 三轮修复后）：修复后重验三种幽灵形态（`audit unsafe` / `doctor` / `graph blame`）**全部报红且归因区分正确**（错顶层名 → 「既不是 CLI 命令」；对顶层名错叶子 → 「不存在的命令」）；真实下线场景（改名 `graduate-project` + 06 同步 + SKILL 漏改）现报红；撞名场景报在真因上；跨组搬家现报红。

### M3 遗留债清理（为 M4 打地基）✅ 完成（2026-06-30）

**目标（用户 2026-06-29 设定）**：完成 M3 全部任务并达到验收标准（✅），清理 pre-existing 工程债，为 M4 打好坚实地基（✅ 5 项全清）。

5 项 CoupledBatch pre-existing 工程债全部清理 + 审查 + 合并：

| 项 | 内容 | PR | 关键决策 |
|----|------|----|---------|
| ③ read_failures 硬门禁 | 占比 ≥50% 阻断全 0-size 退化 plan | [#53](https://github.com/snowzhaozhj/rewriteInRust/pull/53) | MDR-012 |
| ② topo-sort 参数 | **撤 --members**（违反「破环不在此命令」冻结契约）、新增 --reverse | [#53](https://github.com/snowzhaozhj/rewriteInRust/pull/53) | MDR-012：组感知顺序归 populate |
| ④ transition 组归一 | 复用 state deps 的 member_files 归一 | [#53](https://github.com/snowzhaozhj/rewriteInRust/pull/53) | — |
| ⑤ 孤儿清理回归 | 默认 decompose 代表漂移孤儿 e2e | [#54](https://github.com/snowzhaozhj/rewriteInRust/pull/54) | — |
| ① danger→规则注入 | CLI 落 state + plugin 消费闭环 | [#55](https://github.com/snowzhaozhj/rewriteInRust/pull/55) | MDR-013：state 只落原始类别，RULE 映射归 translator |

- **审查**：批次 A 4 视角、B 2 视角、C 4(C1)+2(C2) 视角，全部无 important，共识 nit 全修。
- **新增 MDR**：MDR-012（批次 A 三项偏离）、MDR-013（danger 落 state）。
- **后续 TODO**（MDR-013 登记，非阻塞，留待 M4）：io_side_effect 补专属 RULE；DangerCategory 上移 types 层恢复类型安全；RULE-6/12/15 porting-template 完整展开。

> M3 收尾 + 遗留债清理均已完成；PLAN-M3 验收清单回填 [x] + 头部完成横幅；本文件「当前位置」标记 M3 + 地基 ✅。

### 下一步：M4「完善」——规划已定稿（[PLAN-M4.md](PLAN-M4.md) v0.2，2026-06-30）

**主线决策（双主线）**：经 2 路调研（代码就绪度 + 技术可行性交叉验证）→ 分析 → R1 三路对抗审查（设计契约/主线决策/可执行性）→ 重定位 → R2 用户审查修正产出。

- **巩固线（真正的「完善」）~17d**：迁移质量度量框架（源行为覆盖率/degrade 率/人工修订率/final_score）+ Community 结构偏离度诊断（Tier 1）+ 既有 TS/Python 真实基线 + 循环健壮性（checkpoint 硬化/watchdog stall 恢复/额度韧性续跑）+ MDR-013 三项清债。
- **Go 扩语言线（roadmap 承诺）~31d**：复用 trait 架构接 Go；**关键 critical 修正**——Go 包系统需**扩 trait 暴露目录列举**（`resolve_import` 的 `exists`-only 签名无法探任意命名包代表文件，扩 trait 是 baseline 非 fallback）；Go 验收用质量度量框架设真实门槛（多模块，非单模块编译）。
- **明确推迟/砍**：C（无类型 IR 下语义难度+ROI）/ Kani（**推迟**，与 proptest 互补不替代，当前 ROI 不足）/ Community Tier 2/3（Tier 1 已纳入）/ Strangler Fig（降文档，离线场景下共存需求不强）/ 并行编排程序化调度器（当前 SKILL.md 编排满足需求，ROI 不足）/ index.json（YAGNI）。
- **Sprint 结构**：A 债务收口+Go前置 → B 质量度量+既有基线+Community诊断 ‖ C Go Adapter Core → D Plugin Go → E Go 端到端验收 → F 健壮性+编排收口。共 37 任务 ~48d，两线可独立分批交付。
- **配比决策（2026-06-30 用户拍板）**：双主线并行——Sprint A 完成后，B（巩固线）与 C（Go 线）可并行启动。

#### Sprint C：Go Adapter Core 进行中（2026-07-02，PR-C2 分支 `feat/m4-sprint-c-pr-c2-go-core`）

按 PLAN-M4 §Sprint C 执行策略拆 3 PR：PR-C1（Foundation ✅ PR [#59](https://github.com/snowzhaozhj/rewriteInRust/pull/59) 已合并）→ PR-C2（Core Analysis ✅ PR [#60](https://github.com/snowzhaozhj/rewriteInRust/pull/60) 已合并）→ **PR-C3（Validation，GO-08/GO-09，已交付待审查）**。

**PR-C3 Validation（GO-08 fixture + GO-09 集成测试，已交付待审查）**：

| 任务 | 状态 | 交付 |
|------|------|------|
| M4-GO-08 Go fixture（4 个） | ✅ | `fixtures/go-{linear,diamond,circular,pkg}-deps`，各含 `go.mod`（module 前缀）+ 源码 + `ground-truth.json`（节点/边/拓扑偏序，双向严格校验格式，对齐 py fixture） |
| M4-GO-09 Go graph 集成测试 | ✅ | `tests/go_ground_truth.rs`（27 测试）：4 fixture nodes/edges/topo 双向严格校验 + Go 特有断言 |

- **fixture 覆盖矩阵**：
  - **linear**（utils→service→main）：跨包 import + 跨包函数调用到代表文件、多返回值签名 round-trip、const/var 激活 Variable + 导出判定、**同包** composite literal 构造（Constructor sub_kind）+ 局部绑定方法调用。
  - **diamond**（main→{left,right}→geom）：菱形包 import、struct 同包嵌入 → extends、interface 隐式实现**不连 Implements**（D-M4-02）、interface/struct 签名。
  - **circular**（a↔b + shared 环外）：包级 SCC 环检测、topo expect_error、shared 不在环、migration_sequence has_cycles。
  - **pkg**（store 多文件包）：`_test.go`/平台后缀 `_windows.go` **完全排除**（无 File 节点）+ `//go:build ignore` → **孤立 File 节点**（跳符号）；跨包调用解析到代表文件（字典序第一非 `_test.go`）；**GO-09 decompose 同包凝聚**——预算 35 恰容 store 包 3 文件同目录凝聚、装不下 main.go，验证同包凝聚 + 跨目录边界（非"预算无穷大全并"平凡通过）。
- **规避已知精度限制**（PR-C2 记账 TODO）：跨包只做「import + 调用代表文件内符号」，构造调用放同包内——避免 qualified composite literal 丢包前缀、非代表文件符号漏边污染 ground-truth。fixture 描述已注明。
- **验证**：core 594 测试全绿（+27 Go fixture 测试）；`just ci` 全过；`cargo run -- graph build --root ../fixtures/go-linear-deps` → node=11/edge=13（status=warning：Go 全量降级 + 无 migration-state，符合 CLI 契约）。
- **PR #61 审查（4 视角全跑）**：主审 / 设计契约 / 专项测试覆盖 / 异构交叉。
  - **主审**：通过，无 important；4 nit（其一 go.rs:25 stale `TODO(PR-C3)` 注释已订正）。
  - **设计契约**：6/6 PASS，无 important（节点/边类型、Variable 激活、文件过滤、decompose 凝聚、schema 均与 design/PLAN-M4/D-M4-02/MDR-011 一致）；1 pre-existing（design 04 §5.7.1 表格 calls/exports 源方向措辞 drift，非本 PR 引入，记 TODO）。
  - **专项测试覆盖**：**1 important 已修**——CLI 层 `graph build` Go dispatch 无自动化测试（验收 [x] 仅手验）→ 补 `cli_e2e.rs::smoke_graph_build_go_detects_and_degrades`（断言 node=11/edge=13 + status=warning，证明确实路由到 GoAdapter）；nit 分层判断（单文件语义已由 41 个 PR-C2 单测守护，不在端到端层重复）。
  - **异构交叉（codex）**：**3 important**——① decompose 测试写死 budget=35 且依赖 tagged.go 撑体积 → 改**预算自适应**（从 store 目录 File footprint 推导，增删文件/改 ignore 归组均不失效）；② import 指向"代表文件"固化精度限制 → **研判为 GO-03 既定契约**（trait 无符号表，代表文件是 baseline，design-checker PASS），非缺陷，保留 + 已注明；③ node `type` 字段未校验 → assert_node_attributes 加 **node_type 断言**（一并闭合主审 nit#4）。2 nit：store_test.go 跑 `go test` 会失败 → 改非空构造；extends 是嵌入映射约定 → 描述已注明。
- **审查后新增测试**：cli_e2e Go smoke（1）+ go_ground_truth node_type 断言强化；均绿。
- **后续 TODO（记账）**：① 跨文件 Contains fixup（go.rs 模块头，后续 PR）；② design 04 §5.7.1 表格 calls/exports 源方向措辞同步（设计契约 pre-existing）。
- **不在本 PR**：R3 build.rs Go 专用 Calls 兜底、跨文件 Contains fixup 等 PR-C2 记账项（留 GO-09 后续/后续 PR）。

**PR-C2 Core Analysis（GO-02~07，已交付待审查）**：

| 任务 | 状态 | 交付 |
|------|------|------|
| M4-GO-03 扩 trait（关键路径） | ✅ | `LanguageAdapter::resolve_import` 加 `list_dir` 回调 + 新增 `configure_project(&mut,root)` 默认空钩子；build.rs 两 edge 函数构造 `list_dir`（`build_dir_index` 目录索引）、`build_graph_inner`+`build_graph_incremental` 两处注入 `configure_project`；TS/Python impl 忽略新参数 |
| M4-GO-03 包 resolve | ✅ | `configure_project` 读 go.mod module 前缀；`resolve_import` 剥前缀→`list_dir` 枚举包目录→`pick_representative_go_file`（字典序第一非 `_test.go`）；stdlib/第三方/部分段误匹配→None |
| M4-GO-02 import+过滤 | ✅ | 单/分组 import、别名/点/下划线（`_`→SideEffect）；`can_handle` 排 `_test.go`+GOOS/GOARCH 平台后缀；`analyze_file` 内容级 `//go:build` 门控（排除→仅 File 节点） |
| M4-GO-04 符号+激活 Variable | ✅ | func/method(receiver 归属,剥指针/泛型,限定名 `T.Method`)/struct→Class/interface→Interface/alias+defined→TypeAlias/const+var→**Variable(激活 M2 预留)**；首字母大写导出(`is_uppercase` 非 ascii)；Contains/Extends(struct+interface 嵌入)+后置 Exports 边 |
| M4-GO-05 调用+绑定 | ✅ | `pkg.Func`/`x.Method`/`Foo{}`/`&Foo{}` 构造；instance_type_bindings（短变量/赋值/局部 var/receiver 变量；工厂调用不绑定） |
| M4-GO-06 签名 | ✅ | func/method 剥 body（含多返回值/可变参/泛型）；type/interface/struct 整声明文本入 signature |
| M4-GO-07 interface 隐式实现 | ✅ | 不强连 Implements 边（D-M4-02），方法集经类型 signature 承载 |

- **对抗验证驱动的关键决策**（设计+验证 workflow：4 设计 agent + node-types 权威确认 + 3 对抗验证 agent）：
  - **module_path 注入用 `configure_project` 钩子**（构造器方案不可行——registry 创建 adapter 时不知 project_root；R2 CONFIRMED，两处注入漏一处则该路径 Go 跨包边全丢）。
  - **spike 死断言稳健**（R4/R6）：decompose 阶段1 按目录分桶全对合并（不要求边连通），同包 `.go`（含孤立/空文件）必归同一 DecompUnit；端到端死断言 owner=GO-09（PR-C3）。
  - **跨包 Calls 精度已知限制**（R3）：代表文件不含被调符号→漏边（非错边）；采「记录+decompose 目录凝聚兜底」，**不加 build.rs Go 专用 Calls 兜底**（保语言无关层纯净，符号级精确需符号表超范围），R3 build.rs 回退推迟 PR-C3/GO-09。
  - **Variable 激活无 panic**：Variable/TypeAlias 已在枚举、唯一 `match`(build.rs) 带 `_` 兜底；Exports doc 已同步补 Variable/TypeAlias 目标（design-checker 必查）。
- **验证**：641 测试全绿（新增 ~29 Go 单测 + 契约扩展）；`just ci` 全过（fmt+clippy -D+test+deny+shellcheck），TS/Python 无回归。
- **PR #60 审查（4 视角全跑）**：主审/设计契约/专项(silent-failure+类型+测试)/异构交叉。**4 项 important 全修**：
  - **I-1 分组 `var (...)` 漏建 Variable/Exports**（主审+专项）：tree-sitter-go `var_declaration` 分组多包一层 `var_spec_list`（const 直挂，不对称），旧代码只遍历直接子 → 分组 var 块整块漏建（击穿本 PR「激活 Variable」目标）。修：下钻 `var_spec_list`；补分组单测 + 契约固化 `var_spec_list`；订正错误注释。
  - **I-2 局部变量绑定跨函数作用域错边**（异构#1，突破「漏边非错边」底线）：`instance_type_bindings` 文件级表被同名局部变量跨函数污染。修：改**函数作用域**绑定——`build_go_fn_scope` 预扫 receiver+形参+局部绑定，`x.M`→`Type.M` 在作用域内即时定型（同名冲突 poison 退化漏边）；顺带修异构#2 形参方法调用漏边；build.rs 无需改。补作用域隔离回归测试。
  - **I-3 configure_project 双注入点零回归守卫**（专项）：补 Go 跨包边回归测试，同守 `build_graph_inner` + `build_graph_incremental`（DB 存在自有路径）两注入点。
  - **I-4 go.mod 异常静默丢跨包边**（专项）：`configure_project` 改返 `Vec<String>` 警告，有 go.mod 却无 module 声明时汇入图 warnings（区分可静默的 GOPATH 模式）；补 adapter + build 两级测试。
  - **nit 已清**：`exported_names`/`TypeAlias`/`Variable` 注释订正、`parse_go_module_path` 容忍 tab。测试基线 567 core（+7 新测）全绿。
- **后续 TODO（记账，PR-C3/后续）**：① 跨文件 Contains fixup（方法与类型分属同包不同文件时 Contains 边静默丢，仿 fixup_extends）；② 跨包 composite literal 绑定精化（qualified_type 丢包前缀，异构#3）；③ FFI 接口收集须限定 Function 节点（Variable 导出会抬高 count_exports）；④ `//go:build` 复杂括号表达式求值（异构#4）；⑤ pre-existing：`cli_e2e.rs:2273` 的 `--all-targets` clippy len_zero（Sprint B 引入，`just ci` 不含 --all-targets 故不阻塞）。

**PR-C1 Foundation（GO-10 + GO-01，已交付待审查）**：

| 任务 | 状态 | 交付 |
|------|------|------|
| M4-GO-10 grammar 契约 | ✅ | `tests/ast_contract_go.rs`：固化 21 个 tree-sitter-go 节点 kind + 字段（字段以 tree-sitter-go-0.21 node-types.json 为准），grammar 漂移先红于此 |
| M4-GO-01 detect_tier | ✅ | `go.rs` 实现复杂度分档：并发（go/select/chan/send）+ 反射（reflect）+ cgo（"C"）+ unsafe → Full；func/method/type → Standard；纯 const/var/package → Trivial；语法错误保守 Full。9 单元测试 |

- **关键坑**：Go grammar 把 `\n` 作 source_file 匿名子节点吐出（Python grammar 无），顶层遍历须 `is_named()` 过滤，否则纯换行被误判为实质内容。
- **验证**：go 相关 19 测试全绿；`just ci` 全过（fmt+clippy -D+test+deny+shellcheck），TS/Python 无回归。
- **不在本 PR**：analyze_file/resolve_import/扩 trait/classify_file/fixture（PR-C2/C3）；故 Go 项目 `graph build` 仍返 analyze_file NotImplemented（预期）。

#### Sprint B：质量度量框架 + 社区检测 ✅（PR [#58](https://github.com/snowzhaozhj/rewriteInRust/pull/58) 已合并）

| 任务 | 状态 | 交付 |
|------|------|------|
| M4-QUAL-01 质量度量框架 | ✅ | `stats/quality.rs`：QualityReport/ModuleQuality/DeterministicIndicators/AiIndicators 类型 + compute_quality + final_score §7.5 公式 + 28 单元测试；CLI `stats quality` 子命令 |
| M4-QUAL-04 社区检测 | ✅ | `stats/community.rs`：**自实现 Louvain 社区检测**（PR #58 审查中移除 graphrs 依赖）→ NMI/ARI vs 目录分区 → deviation_score；CLI `stats community` 子命令 |
| M4-QUAL-03 Plugin 接线 | ✅ | review.md 仪表板接线 stats quality/community；verifier.md 新增 AI 指标输出 schema |
| M4-QUAL-02 设计文档更新 | ✅ | 03 §7.5 登记三项新增度量（degrade_rate/behavior_coverage/revision_rate） |

- **PR #58 审查修复**：Louvain ΔQ 公式修正 + sigma_in 双计数（见提交 09711e3）；移除 graphrs 自实现 Louvain（96443d8）。

#### Sprint A：债务收口 + Go 接入前置 ✅ 完成（PR [#57](https://github.com/snowzhaozhj/rewriteInRust/pull/57) 已合并）

| 任务 | 状态 | 交付 |
|------|------|------|
| M4-DEBT-01 io RULE 归属 | ✅ | **裁定并入 RULE-10（标准库 IO 映射）**，不新开 RULE（保持 26 类）；translator.md 定向表 + TS/Python porting-template 补「标准库 IO 映射」节；concern() 文案加 RULE-10 引用 |
| M4-DEBT-02 DangerCategory 上移 | ✅ | 枚举从 `lang/mod.rs` 移到 `types/common.rs`，加 `Deserialize`+`#[serde(other)]` 兜底 `Unknown` 变体；`ModuleState.danger: Vec<String>` → `Vec<DangerCategory>`；lib.rs 去 `as_str()` 转换、`union_danger` 按 `as_str()` 重排保旧字典序；新增 4 个 serde 双向/兜底/旧版兼容测试 |
| M4-DEBT-03 RULE-6/12/15 展开 | ✅ | TS/Python porting-template 各补「并发模式/unsafe 使用策略/全局状态处理」三节（映射表+陷阱）；concern() 文案语言中立化（去 TS 口径硬编码）；各模板 frontmatter bump `rule_version`（+RULE-6/10/12/15）；translator.md 脚注同步 |
| M4-LANG-01 Go registry 接线 | ✅ | workspace 引 `tree-sitter-go=0.21`；`registry.rs` 加 Go 臂；`lang/go.rs` 骨架（language/can_handle/resolve_extensions/detect_source_root 实，余 `todo!()`）；新增 `create_go_adapter` 测试 |

- **验证**：559 测试全绿（基线 552 +4 serde +3 go 骨架测试）；`just ci` 全过（fmt+clippy -D+test+deny+shellcheck）。
- **审查（4 视角全跑，PR [#57](https://github.com/snowzhaozhj/rewriteInRust/pull/57)）**：主审/设计契约/专项/异构交叉。**1 important 必修 + 4 nit/文档同步全落实**：
  - **important（4 方一致）**：Go registry 接线后 `todo!()` 让 Go 项目 graph build/populate **panic 崩进程**（回归，违反 CLI 统一 JSON）。修：骨架方法非 panic 化——`analyze_file` 返 `Err(NotImplemented)`、`detect_tier` 返保守 `Full`、删 `classify_file` override 用 trait 默认 `conservative()`；新增 3 个 go 骨架回归测试。
  - **设计文档同步**：09-schema danger 字段（`Vec<String>`→`Vec<DangerCategory>` + unknown 兜底说明）；MDR-013 决策 2/3 标注被 DEBT-02 取代 + 后续 TODO 三项收口标注；translator.md 文末补 RULE-10。
  - **nit**：`detect_source_root` go.mod 返 `Some(".")` 而非 `None`（避免误导 fallback warning）；Unknown 有损单向性在类型层文档注明（PLAN 授权 + 不可触发理由：danger 恒为分类器 6 类、跨版本由 schema_version 管）。
  - **Unknown 有损往返研判**：异构定 HIGH、主审 MEDIUM、专项 nit。研判为**理论回归、单版本不可触发**（danger 只由分类器产 6 类，Unknown 仅手工编辑/跨版本时现）；PLAN-M4 DEBT-02 已授权 `#[serde(other)]`；保真方案（`Unknown(String)`）破 Copy + as_str 签名冲突 + 手写 serde 出错面，ROI 不足。采文档充分注明 + 测试锁边界。
- **待办**：等用户审阅拍板合并（不自行 merge）。

### 历史：Sprint D 端到端验收（M3-VAL-01~08）✅

- **M3-VAL-01 选型**：jmespath + textdistance（纯计算/数据处理，有 pytest 覆盖）
- M3-VAL-02/03：两项目各 ≥1 模块 done（cargo check+test+clippy 过）
- M3-VAL-04：差异测试框架（pytest 行为录制 JSON fixture → Rust 对比）
- M3-VAL-05/06：性能回归（TS 实测无退化）+ graduate Python 路径验证
- M3-VAL-07 ✅ PR #42（设计文档同步）
- M3-VAL-08：全量回归 + 覆盖率 ≥70%

### M2 遗留（Sprint A 已全部关闭）

| 项目 | 处理 |
|------|------|
| FFI 方向不匹配 | ✅ MDR-007：取消 FFI，degrade_skip 唯一路径 |
| TS 特有概念泛化 | ✅ LANG-05：constructor_bindings → instance_type_bindings |
| DEVIATION 4 项待 MDR | ✅ MDR-008：4 项偏差补录 |
| F2-FFI 验收缺口 | ✅ MDR-007 标记为"设计变更取消" |

### Sprint A 完成清单

| 任务 | 状态 | 说明 |
|------|------|------|
| LANG-01 adapter 工厂 | ✅ | `lang/registry.rs` + `create_adapter()` |
| LANG-02 resolve_import 下沉 | ✅ | trait 新增方法，build.rs 通过 adapter 调用 |
| LANG-03 build_graph 泛化 | ✅ | 4 个便捷函数改用工厂 + `build_graph_for_lang` |
| LANG-04 alias 漏边修复 | ✅ | 函数调用分支补 alias_to_original 查找 |
| LANG-05 instance_type_bindings | ✅ | constructor_bindings 改名 + 删 TODO(M3) |
| LANG-06 配置泛化 | ✅ | source_language: Option + default_excludes_for_lang |
| LANG-07 stats 泛化 | ✅ | collect_source_files(lang) + source_max_nesting |
| FFI-CLOSE | ✅ | ffi.rs deprecated + MDR-007 |
| DEV-01 DEVIATION MDR | ✅ | MDR-008 补录 4 项偏差 |

### 当前工作：Sprint B（Python Adapter Core）

**目标**：实现 `PythonAdapter`，可解析 Python 源码、构建依赖图、检测复杂度分档。

**PR 拆解（3 步走）**：

| PR | 任务 | 预估 | 并行策略 |
|----|------|------|---------|
| **PR-B1 Foundation** | PY-01 + PY-09 | ~1d | 串行，所有后续前置 |
| **PR-B2 Core Analysis** | PY-02 + PY-03 + PY-04 + PY-05 + PY-06 | ~5d | 内部双线并行：Track A (import→resolve) ∥ Track B (symbol→call+signature) |
| **PR-B3 Validation** | PY-07 + PY-08 | ~2.5d | 串行，验收层 |

**依赖图**：
```
PY-01 ─┬→ PY-02 → PY-03 ─────┐
        ├→ PY-04 → PY-05/06 ──┼→ PY-08
        └→ PY-09               │
                    PY-07 ─────┘
```

**进度**：
- [x] PR-B1：PY-01 adapter 骨架 + PY-09 注册/契约
- [x] PR-B2：PY-02 import 解析 + PY-03 resolve + PY-04 符号 + PY-05 调用 + PY-06 签名
- [x] PR-B3：PY-07 fixture（4 个）+ PY-08 集成测试（23 测试）+ CLI graph build 语言检测泛化

**PR-B3 交付**：
- 4 个 Python fixture：`py-linear-deps`（线性+`__all__`+async+构造调用）/ `py-diamond-deps`（菱形+继承 extends）/ `py-circular-deps`（环检测+shared 不在环）/ `py-pkg-deps`（`__init__.py` 包+re-export 透传偏序+`TYPE_CHECKING` StaticType）
- `python_ground_truth.rs`：24 测试，节点/边**双向严格校验**（含 sub_kind，防多余/缺失/标注错误漏检）+ 拓扑偏序 + Python 特有断言（extends 无 Implements、signature round-trip、StaticType import、构造 sub_kind、循环 SCC 精确同环）
- CLI `cmd_graph_build`：源语言优先取 config（避免热路径重复全树扫描），未配置才 `detect_language` 探测，失败显式告警回退 TS；非 TS 强制全量并提示降级；新增 `build_graph_full(root, lang, profile)`；TS 增量路径不回归
- `cli_e2e.rs` 新增 Python graph build 端到端用例（探测→降级→status=warning）
- `cargo run -- graph build --root fixtures/py-linear-deps` 输出 node=12/edge=15 ✓
- **审查**：4 视角全跑（主审/设计契约/专项/异构交叉）；6 项测试保真+CLI 健壮性问题已修，无遗留 important

### 当前工作：Sprint C（Plugin Python 适配）

**目标**：Plugin 层支持 Python 项目迁移分析和翻译（PLG-01~06）。

**PR 拆解（修正 PLAN-M3 偏离后）**：

| PR | 任务 | 说明 |
|----|------|------|
| **PR-C1** | PLG-01修正 + PLG-02 | Python adapter 资产：`analysis-tools.json` + `porting-template.md` |
| **PR-C2** | PLG-03 + PLG-04 | translator.md / analyzer.md / verifier.md 多语言分支 |
| **PR-C3** | PLG-05 + PLG-06 | degrade_skip 降级报告增强 + Plugin Python 端到端验证 |

> **PLG-01 偏离修正**：PLAN-M3 字面要求建 `adapter.json` + `detect.sh`，但实际架构中
> TS adapter 目录仅 `analysis-tools.json` + `porting-template.md`——语言检测在 `analyze.md`
> Step 2（读特征文件）、依赖分析由 CLI `graph build`（tree-sitter）完成，设计文档 06 §11.2
> 的 shell 脚本模式从未落地。Python adapter 对齐 TS 实际结构，不建 adapter.json/detect.sh。

**进度**：
- [x] PR-C1：Python adapter 资产（[#38](https://github.com/snowzhaozhj/rewriteInRust/pull/38)，审查必修全落实，待合并）
  - 审查：迁移规则正确性 + 设计契约 2 视角全跑；2+1 项 important 已修（regex 反向引用/环视、dict 插入顺序、PLG-01 偏离落 MDR-009）+ 多项 nit
  - MDR-009：适配器 shell 脚本模式取消，adapter 目录契约 = analysis-tools.json + porting-template.md
- [x] PR-C2：translator.md/analyzer.md/verifier.md 多语言分支（PLG-03 + PLG-04，待审查/合并）
  - translator.md（PLG-03）：核心规则节加「语言基线」——TS 内嵌表仅 source_language=typescript 套用，非 TS 以 `adapters/<lang>/porting-template.md` 为权威；RULE-2 表标 TS 基线；Phase A 加 Python 特化小节（`self` 参数转换 / `__init__.py` 包→mod 树 / 无 type-only import 区分）
  - analyzer.md（PLG-04）：R6 源语言特化分析——Python 框架识别（django/flask/fastapi 等）+ 动态特性扫描（getattr/eval/metaclass/monkeypatch）记入 `gaps.dynamic_features`（输出格式示例同步加键）
  - verifier.md（PLG-04）：9 维度表后加「源语言特化探测案例」——Python 替换 TS 案例（int 任意精度 / dict 插入序 / str 码点 vs UTF-8 / GIL·multiprocessing 进程隔离 / except pass·try-finally / Decimal 禁降级 f64）
  - 自检：改动区无死链；plugin validate 通过
  - **审查**：4 视角（主审/设计契约/专项全跑，异构 skip：34 行纯文档不涉算法/解析器）；1 important + 3 nit 已修
    - important（主审查证 python.rs StaticType，design-checker 漏判）：「Python 无 type-only import」表述错误 → 改为「无 `import type` 语法关键字，但 `TYPE_CHECKING` 块是惯用仅类型导入，图层已标 StaticType」（translator + analyzer）
    - nit：dynamic_features 条目格式点明为 `"file: 简述"` 字符串；translator 语言基线补「无适配器模板语言降级回退 TS + TODO(port)」
    - nit 未采纳：self 段指针化（保留结构映射防 run 阶段丢失，专项亦认可可接受）
- [x] PR-C3：degrade_skip 降级报告增强 + 端到端验证（PLG-05 ✅ + PLG-06 进行中）

> **遗留待办**：✅ 已由 PR [#42](https://github.com/snowzhaozhj/rewriteInRust/pull/42) 处理（M3-VAL-07）——① 设计文档 06 §11.2 按 MDR-009 改写为两文件契约；② verifier.md 第 58/87 行 `权威来源：05 §6.x` 死链已清理。待合并。

### M3 多语言扩展点（调研结论，2026-06-24）

**已就绪**：
- `LanguageAdapter` trait 6 方法已抽象（`language/can_handle/resolve_extensions/import_specifier_extensions/analyze_file/detect_tier`）
- `SourceLang` 枚举已预定义 TypeScript/Python/C/Go
- `profile/detect.rs` tokei 映射已含 Python/C
- Plugin 层 `SKILL.md` / `analyze.md` 已考虑多语言分发
- 设计文档 06 §11 有完整的语言扩展架构设计

**需泛化（TS 硬编码）**：
- `detect.rs`: 直接实例化 `TypeScriptAdapter`（需 adapter 工厂）
- `graph/build.rs`: `build_graph_ts()` 等 4 个便捷函数硬编码 TS adapter
- `stats/compare.rs`: `collect_ts_files()` / `ts_max_nesting()` / 独立创建 TS parser（绕过 adapter 抽象）
- `types/config.rs`: 默认 `source_language: TypeScript` / exclude 含 `node_modules`
- Plugin `translator.md`: 类型映射表以 TS 为基线
- Plugin `adapters/`: 仅有 `typescript/` 目录

## 历史归档

- **M1 详细记录**：[STATUS-M1-archive.md](STATUS-M1-archive.md)
- **M2 详细记录**：[STATUS-M2-archive.md](STATUS-M2-archive.md)（Sprint D/E/F 任务清单、PR 记录、审查修复、已知问题处理状态）
- **M2 计划**：[PLAN-M2.md](PLAN-M2.md)（55 项任务 + 5 项验收，Sprint A→F）
- **M2 Sprint F 验收**：[sprint-f-acceptance.md](sprint-f-acceptance.md)
