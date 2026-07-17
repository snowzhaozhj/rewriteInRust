# 项目状态快照

> 每次会话结束前更新。新会话读此文件 → 找到 PLAN.md 对应任务 → 继续执行。

## 当前位置

- **Milestone**: M1 ✅ → M2 ✅ → **M3 ✅** → **M3 遗留债清理 ✅** → **M4「完善」执行中——巩固线 Sprint A ✅（PR #57）+ Sprint B ✅（PR #58）→ Go 线 Sprint C ✅ 全合并（C1 #59 / C2 #60 / C3 #61）→ Sprint D「Plugin Go 适配」✅ 全合并（#62 + #63）→ 巩固线 Sprint F「健壮性+编排收口」：GOV-01/ROB-01a/b/c 全合并（#64/#65/#66+#67/#68）→ **ORCH-01 决策反转**：先前「砍并行、回归串行」的决策经用户否决撤销，保留并行 + worktree 写隔离（[MDR-018](decisions/018-keep-parallel-migration.md)），分支 `docs/m4-orch-01-keep-parallel` 待审查**
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
- **最新合并 PR**: [#73](https://github.com/snowzhaozhj/rewriteInRust/pull/73)（ORCH-01 PR-2 worktree 统一）；[#71](https://github.com/snowzhaozhj/rewriteInRust/pull/71)（ORCH-01 PR-1 并行分层落 CLI）；[#68](https://github.com/snowzhaozhj/rewriteInRust/pull/68)（ROB-01c 额度续跑）；[#67](https://github.com/snowzhaozhj/rewriteInRust/pull/67)（ROB-01b 审查修复补交）；[#65](https://github.com/snowzhaozhj/rewriteInRust/pull/65)（ROB-01a）；[#64](https://github.com/snowzhaozhj/rewriteInRust/pull/64)（GOV-01）
- **ROB-01a 已合并**（2026-07-05，PR #65）：**4 视角审查全跑 → 2 important + 共识守护 + 2 MEDIUM + nit 全修**：① graduate 项目态守护（codex important，防矛盾终态）；② paused 纳入 `--force` 守护（专项 HIGH + 主审 + 设计契约共识，防绕过降级抉择）；③ canonical_module_key 不变量破坏 debug_assert→release 硬错（专项）；④ was_noop 时 cleanup 给 `skip:true`（专项，编排层幂等）；⑤ was_noop+backup 自愈、attempts 语义 MDR 点明、pre-existing len_zero 顺手修。CAS version 不递增判定为 pre-existing（transition 亦不递增）→ MDR-015 记 TODO。`just ci` 全绿（710 测试）。
- **Sprint F ROB-01b 交付**（2026-07-05，待审查）：**watchdog stall 检测 + 恢复路径**。现状缺口：系统只有「调用级总超时 + 产出物校验失败」两类可计数失败，**stdout 静默卡死**（agent 假死/外部命令 hang，无返回无报错）计数器兜不住。交付 **MDR-016 分工**（延续 MDR-015）：**检测归编排器**（CLI 是短命子进程、观测不到子进程 stdout，靠主会话 background bash + `BashOutput` 轮询 stdout 静默超 `stall_timeout_secs`）；**恢复归 CLI** `state recover --module <M> --policy retry|skip [--reason]`——① retry 委派 `reset_module(force)` 回退干净重译入口（复用幂等 + `member_files` 作用域）+ 追加 `stall-recover:retry` 审计；② skip **直设 `paused`**（决策点，headless 自动 degrade_skip）——**绕 `can_transition_to` 矩阵**（stall 可发生在 `translating`，而 `translating→paused` 不在矩阵，仿 reset 破坏性直设）；幂等（retry 已净/skip 已 paused|degrade → `was_noop`）；守护 `done`/`blocked`（非 stall 态）+ `graduate` 拒绝（无 `--force` 逃生口——recover 是程序化入口，误用暴露为错误）。core `recover_module`+`RecoverPolicy`/`RecoverOutcome`；CLI `cmd_state_recover`+`RecoverPolicyArg`；`OrchestrationConfig` 扩 `stall_timeout_secs`(600，与总超时正交)+`stall_recovery_policy`(RetryThenSkip)。**策略解析三方分工**：config 声明→编排器读 config+retry-round 解析 `--policy`→CLI 无状态确定性执行。Plugin：SKILL.md 新增「Watchdog stall 检测与恢复」单点 + run.md 计数器段补正交说明 + workflow.md 失败不阻塞补 worktree stall 分支。设计同步：06 CLI 表 + 两处 `[orchestration]`。12 新测（8 core + 1 cli_e2e + 3 config）。**下游**：ROB-01c（额度续跑）复用 `state recover --policy retry` 幂等重入。
- **ROB-01b 4 视角审查全跑 → 2 important + 共识 Medium + Low + nit 全修**（PR [#66](https://github.com/snowzhaozhj/rewriteInRust/pull/66)，待用户验收）：① **[codex HIGH]** recover 守护漏 `degrade_*`——retry 委派 `reset_module(force)` 会绕过「degrade→translating 须 --force 人类确认」边界、`retry;skip` 把依赖侧已视终态的 degrade_skip 变回非终态 → 守护改**全枚举显式 match**，degrade_* 拒绝；② **[专项]** `recovery.unblock_next` 命令语法错（`state deps` 是位置参数非 `--module`，编排器照做 cli_parse 失败）+ 语义偏（查的是该模块自身依赖就绪非「无依赖模块清单」）→ 改对；③ **[三方共识 Medium]** pending 纳入守护拒绝（skip 把未起步模块直设 paused、与 reset 不对称）；④ **[设计契约+codex Low]** skip 清 `substatus`（translating 瞬态标记挂 paused 语义不符），保留其他进度字段供降级分析；⑤ **[nit]** 09 转换矩阵补 reset/recover 绕矩阵例外脚注。设计契约 0 important（CLI JSON/状态机/枚举/config 六项逐条 PASS）。守护现为「仅放行运行态+paused，拒绝 pending/done/blocked/degrade_*」。`just ci` 全绿。
- **⚠️ ROB-01b 审查修复独立 PR（合并后补交）**：#66 初版在 4 视角审查跑完前即被合并（merge commit `04611e8`，parent = 初版 `0665ccd`），**2 个 important 修复未进 master**（degrade 守护 bug + unblock_next 命令 bug）。审查修复（含上条 5 项）已 cherry-pick 到新分支 `fix/m4-rob-01b-post-merge-review-fixes` 独立提 PR，**已合并（PR #67，merge commit `0a5ac5d`）**——master 的 ROB-01b 现已完整。
- **Sprint F ROB-01c 交付**（2026-07-05，待审查，分支 `feat/m4-sprint-f-rob-01c-quota-resume`）：**额度耗尽优雅暂停 + 断点续跑**。现状缺口：ROB-01a 已逐步原子 checkpoint、ROB-01b 已 stall 恢复，但**额度刷新后续跑的确定性入口**缺失（「哪些中断需幂等重入、哪些已完成不重跑、下一步做谁」只能手工翻 state）。交付 **MDR-017 分工**（延续 MDR-015/016）：**检测归编排器/harness**（CLI 观测不到 token 预算/API 额度，用 harness budget.remaining() 或人工判断）；**优雅暂停 = 当前原子步收尾后停止**（状态已 checkpoint，无需单独 pause 写）；**续跑计划归 CLI** `state resume`——**纯查询、无副作用、不加载 graph**，按 `ModuleStatus` 全枚举 match 归 5 桶：运行态（translating/compile_fixing/testing/reviewing）→ `interrupted`（各带 `recover_command`=`state recover --policy retry` 幂等重入）；`paused` → `awaiting_decision`（**续跑不复活**——规避绕过降级抉择的正确性 bug）；`pending` → `next`（用 `state deps` 判就绪）；`blocked` → `blocked`；终态（done/degrade_*）**不重跑**仅计入 `progress`（六桶计数，total==各桶之和）。实际重入**复用** `state recover`（不重复 mutation）；`pending` 就绪**复用** `state deps`。**不加额度阈值 config**（YAGNI，harness 已持 budget）。core `resume_plan`+`ResumePlan`/`InterruptedModule`/`ResumeProgress`；CLI `cmd_state_resume`。8 新测（7 core + 1 cli_e2e）。Plugin：SKILL.md 新增「额度耗尽优雅暂停与续跑」单点 + run.md 计数器段补正交说明 + workflow.md「失败不阻塞」补 budget-aware 暂停。设计同步：06 CLI 表 + MDR-017。**已合并 PR [#68](https://github.com/snowzhaozhj/rewriteInRust/pull/68)（2026-07-05，含 6 视角审查修复）。**
- **Sprint F ORCH-01 实现进行中**（2026-07-18）：决策反转已合并（PR #70），ORCH-01 按 5-PR 规划落地：**PR-1 CLI/state 并行分层落盘 ✅ 已合并（PR [#71](https://github.com/snowzhaozhj/rewriteInRust/pull/71)）→ PR-2 worktree 机制统一 ✅ 已合并（PR [#73](https://github.com/snowzhaozhj/rewriteInRust/pull/73)，原 #72 因 base 分支删除自动关闭、rebase 到 master 后重开）→ PR-3 两层 done 接活 🟡 分支 `feat/m4-orch-01-pr3-two-layer-done` 待审查** → PR-4 编排集成测试（mock 派发进 CI）→ PR-5 真实项目并行演练。
  - **三项方向决策（用户拍板）**：① worktree 保留手动 `git worktree add`（删 isolation:"worktree"，因其从 origin 建会丢本地 done 代码 + 编排器拿不到路径无法统一合并）；② `parallel_groups` 收口到 `scc_groups`（已确认 `SccGroup.sprint` = 同层拓扑独立可并行）；③ 验收 = mock 集成测试进 CI + 一次真实项目并行演练。
  - **PR-1（#71）交付**：删死字段 `MigrationSequence.parallel_groups` + `compute_parallel_groups`/`compute_level` 死函数；新增 CLI `graph parallel-groups`（按 sprint 聚合并行层，有环折叠为 is_cycle 组不报错，与 topo-sort 相反）；核实 `populate-modules` 已写 `ModuleState.sprint`，无需新增 schema（YAGNI）；深链栈溢出守护测试迁到 `compute_scc_level`；proptest/ground_truth 不变量改按 scc_groups 聚合验证。**4 视角审查全闭环**：设计契约 PASS；主审+专项 1 important（误删 `smoke_init` 的 `#[test]`）已修；异构 codex 确认收口在有环图下正确（前提：编排器把 group 当原子调度单位——已写进 CLI doc + 设计 06），并抓 1 important（有环并行不变量未覆盖 → 新增 `arb_multi_scc_graph` proptest）已修；nit 全修。
  - **PR-2（#73）交付**（纯 plugin 文档）：删 workflow.md 的 `isolation:"worktree"`（消除文件内两套 worktree 机制矛盾，统一手动 `git worktree add`）；workflow.md 改读不存在的 `migration_sequence.parallel_groups` → 按 `ModuleState.sprint` 筛并行层 + `graph parallel-groups` 命令；SKILL.md 消除「MVP 串行执行」矛盾 + 补命令清单。主审无 important + 设计契约 PASS（4 项），专项/异构豁免。
  - **TODO（记账）**：SKILL.md 命令清单完整审计——现缺 quality/community/rules/graduate/advance-sprint 等十余命令，PR-2 只补 parallel-groups + reset/recover/resume，全量审计独立维护。
  - **PR-3 交付**（两层 done 接活，非删除——[MDR-019](decisions/019-post-translation-review-gate.md):50 已定「随 ORCH-01 接活路径」）：新增 CLI `state batch-transition-done --module <M>...`（可重复），委托已存在但死的 `batch_transition_done`（machine.rs:1005），把整组 `agent_done` 模块批量升终态 `done`；逐模块独立转换、非 `agent_done` 跳过并记 `attempts`、`skipped` 非空降级 warning。workflow.md 步骤 2d 从「逐模块 `state transition --to done`」（无 agent_done 守卫）改为一条 batch 命令；设计 06 命令表新增行。2 个 e2e 覆盖全成功 + 部分跳过降级路径。与 MDR-019 签批门（`awaiting_final_review`，尚未实现，独立 PR）不冲突：两层 done 在**合并后整组 check → done**，签批门在 done 之后，互不重叠。
- **开放 PR**: PR-3（分支 `feat/m4-orch-01-pr3-two-layer-done`，待 4 视角审查 + 用户拍板合并）

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
