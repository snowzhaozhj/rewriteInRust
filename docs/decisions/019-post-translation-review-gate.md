# MDR-019: 译后签批门重建——证据包默认 + 窄预签自动放行策略

- **状态**: 已决策
- **日期**: 2026-07-05（2026-07-14 随 [MDR-018](018-keep-parallel-migration.md) 保留并行重新表述）
- **范围**: 修复实现对设计的偏离——`reviewing`「最终签批门」被弱化为编排器自动 `--to done`。按经**异构对抗审查（codex）收敛**的设计重建。关联 [MDR-013](013-danger-signal-to-state.md)（danger 落 state）、[MDR-018](018-keep-parallel-migration.md)（保留并行翻译 + 串行人工审批；**本门是二者共存的机制**）。本文件记决策 + 文件级落地清单；实际改 `cli/` + `plugin/` + `docs/design/` 到实现 PR。

## 背景

设计明确要求译后人类签批：`03-execution-model.md:627`「**不自动宣布成功**——翻译成功后停在 needs_review 而非自动标 done，verifier 通过后仍需人类最终确认」；`02-architecture.md:181`「`reviewing` 是测试通过后的**最终签批门禁**」。但实现（`plugin/skills/migrate/run.md` 步骤 11，第 199-200 行）是：测试通过进 `reviewing` 后，编排器在确定性前置（`TODO(port)=0` / 覆盖率 / `bug_replica` 已确认）满足时**直接** `state transition --to done`，**无人签批**——把「最终签批」实现成了自动状态推进。

此偏离在 ORCH-01 复盘时暴露：迁移的可审性依赖「每个模块译完后停下等人签批」这道门。[MDR-018](018-keep-parallel-migration.md) 决定**保留并行翻译、审批保持串行**——并行译好的模块正是靠这道门排队供人逐个审。若无此门，并行会让多模块结果直接自动标 done、无人过目，可审性彻底落空。这也厘清了「并行是否破坏可审」：破坏可审的从来不是并行翻译（它只填快待审队列），而是**缺失签批门**。

主会话首版方案（译后门**按风险分级**：高风险强制人工、低风险自动放行）经**异构对抗审查（codex，读文件 + 逐条引证）被打穿**：风险分级依赖的信号本身不可靠——
- `run.md:69`：`danger=[]` **空值语义重载**（既表「无危险」也表「`--no-decompose` 未分类」），**不可据空推断模块安全**；
- `03:243`：`ModuleState.risk` 是「M1 起恒为 `Low` 的死字段」（零读取点）；`PLAN.md:589`：per-module risk 恒 Low 未填；
- `03:647`：「测试通过率是**必要不充分**条件，覆盖率只证明代码被执行过、不证明行为等价」；
- `PLAN.md:548`：判据基于本文件 AST 可见信号、不依赖跨文件 calls，**recall ~70%**（会漏）；
- `03:729`：purity 检测 M0 样本一致率 ≥80% 但「**不作 M1 阻塞门槛**」。

结论：用不可靠信号做「自动跳过人」的门，会造出**「带认证外观的漏网」——比全自动更糟**（伪装已按风险治理过）。

## 决策

重建 `reviewing` 为**译后签批门**，三态：

1. **默认——停 `reviewing`，展示证据包，等人签批。**
   证据包 = 汇编**既有产物**：源意图摘要（`{module}-intent.md`）、Rust diff、测试结果 + 覆盖率对比、结构偏离（`stats compare`）、danger 命中、`bug_replica`、`known_differences`、Phase B MDR / `PORT NOTE`。人审查的是「**为何可以相信它等价**」，不是从零读代码——这同时化解「可审 vs 磨叽」张力（审证据比读代码快，故无须靠跳过模块防疲劳）。

2. **强制人工（绝不自动放行）——命中任一即必须停 `reviewing` 等人：**
   danger 非空或分类来源不可信 / 覆盖率低于源或低于阈值 / L2·L3 差异测试不可执行 / `bug_replica` 待确认 / Phase B 新增代码路径缺 MDR 依据 / `known_differences` 增加 / 公共 API·错误语义·并发模型·数值边界·I/O 副作用发生变化。

3. **自动放行（仅此路径，且审计留痕）——仅当用户预签的窄策略全部命中：**
   `composite_kind=batch` 全机械 + 分类器**确实运行过**（非未分类） + `danger=[]` 且来源可信 + 导出符号/类型签名/常量值全一致 + 无 `TODO(port)`/`bug_replica`/`requires_manual_review` + 无覆盖率例外/`known_differences` + content-hash 未变。
   放行审计 reason 写 `auto_approved_by_policy:<policy_id>`，**不伪装「无需审查」**。

## 理由

1. **证据包解「可审 vs 磨叽」**：审查快在于「审证据」而非「读代码」，故不必靠跳过模块防 review 疲劳。
2. **风险信号不可靠**（见背景逐条引证）→ 分级只能调**审查深度**，**不能单独决定「自动跳过人」**；否则漏网还被贴上「已治理」可信外衣。
3. **自动放行须是用户预签的例外**，而非编排器自宣成功——对齐设计 §7.4「Approval Token / 不自动宣布成功」。

## 与设计的关系

本决策**不改设计意图**，是把实现拉回设计已写明的「不自动宣布成功」（`03:627` / `02:181`），并补齐设计缺失的**译后门交互规范**（现有 §4.3.1 只规范了译前意图门）。

## 落地清单（实现 PR）

**CLI（`cli/crates/core/`）**
- `types/state.rs`：`reviewing` 加 substatus **`awaiting_final_review`**（测试过、待人签批），对标译前门的 `phase_a_complete_awaiting_review`。理由：现 `reviewing` 是瞬时态，加门后模块要停此等人，断点续跑须区分「等签批」vs「审到一半被中断」，否则 `resume_plan` 会重跑 verifier。
- `types/state.rs`：打回重译加合法转移 **`Reviewing → Translating`**（现矩阵 `Reviewing => Done | Blocked`，见 `can_transition_to`）+ 单测。用于「人指出哪不对 → 带反馈定点返工」，≤2 轮自动返工、第 3 轮 `→paused`（对标意图门修订流程）。**不复用** `agent_done`/`batch_transition_done`——那是并行派发的 **agent 级完成标记**（worktree 内译完、合并前），与本门的 `awaiting_final_review`（已合并 + 编译 + 测试过、待人签批）语义不同，另立门户（[MDR-018](018-keep-parallel-migration.md) 保留并行、这套机制随 ORCH-01 接入活路径，非删除）。
- `types/config.rs`：加**自动放行策略**配置（窄合取条件，非单 `bool`；对标已有 `auto_confirm_intent`）；默认关（全停 `reviewing`）。
- **分类 provenance**：消解 `danger=[]` 语义重载——分类结果加「确实运行过分类器」标记，`populate-modules` 落 state，供自动放行策略区分「无危险」vs「未分类」（MDR-013 只落原始类别，未标是否分类过）。

**Plugin（改前读 `docs/learnings/agent-skill-prompt-guide.md`）**
- `run.md` 步骤 11（199-200）：确定性前置过 → `--to reviewing --substatus awaiting_final_review` → 汇编 + 展示证据包 → 等签批；批准 `--to done`；命中强制人工清单则绝不自动；命中窄策略才自动放行（审计 `auto_approved_by_policy`）。
- `run.md` 步骤 1 断点路由（第 50 行 `reviewing | 跳第 11 步`）：细化为 `reviewing + awaiting_final_review` → 跳「展示 + 等签批」，不重跑 verifier。
- `run.md` batch 6.5（119-123）/ SCC 快进（143）：`reviewing→done` 三步快进纳入策略判定——机械 batch 命中窄策略可放行，否则停等签批。
- `SKILL.md`：人类决策点约定补译后门 + resume 行为（`reviewing + awaiting_final_review` = 待人类、不自动 done）。

**并行路径护栏（ORCH-01 PR-3 落地 `batch-transition-done` 后暴露，异构审查确认）**
- **batch 命令与签批门同边**：`state batch-transition-done` 走 `reviewing → done` 边，而本门坐落于**这条边本身**。落地本门后，`workflow.md` 步骤 2d 的「全过 → batch 升 done」自动路径**必须**先判自动放行策略：命中强制人工清单 / substatus 为 `awaiting_final_review` 的模块**不得**进入 batch 调用，须停 `reviewing + awaiting_final_review` 等签批。否则并行路径绕过人签批（06 命令表 `batch-transition-done` 行已注护栏）。
- **并行回传须推进到 `reviewing`**（PR-3 遗留 TODO ①）：现 `workflow.md` 回传只设 `--substatus agent_done`（status 仍非 `reviewing`），batch 的 `→done` 会被矩阵全拒。本门落地时须明确：并行整组 merge + check 过后，编排器把该层模块推进到 `reviewing`（再按上一条判签批 / batch 放行），SubAgent 不执行 run.md 步骤 11。
- **整组失败回退边缺失**（PR-3 遗留 TODO ②）：`workflow.md` 步骤 2d 整组 check 失败要 `reviewing → compile_fixing`，但矩阵 `Reviewing => Done | Blocked` 无此边。与本门新增的 `Reviewing → Translating` 返工路径**统一定义**（返工/编译修复都从 `reviewing` 出发回到活跃态），避免两处各加一条冗余边。

**设计文档（唯一权威）**
- `03-execution-model.md`：补「**译后签批门交互规范**」节（对标 §4.3.1 意图门规范：证据包内容 / 强制人工触发清单 / 自动放行窄策略 / 审计口径）；`627` 处补落实说明。
- `09-appendix-schemas.md`：补 `awaiting_final_review` substatus + 自动放行策略 config schema + 分类 provenance 字段。
- `02-architecture.md`：`reviewing` 状态说明补三态。

## 影响

- **支撑 [MDR-018](018-keep-parallel-migration.md) 的并行模型**：「并行翻译 + 串行审批」的共存正由本门实现——并行译好的模块在此排队，人串行签批；无本门则并行退化为无人过目的自动标 done。
- **关联 MDR-013**：danger provenance 在此消解 `run.md:69` 空值语义重载。
- **工作量**：~2.5–3.5d（证据包汇编 + 分类 provenance 为主）。
