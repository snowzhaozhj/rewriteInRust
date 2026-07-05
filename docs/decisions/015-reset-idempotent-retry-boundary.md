# MDR-015: `state reset` 幂等回退 + 产物清理职责边界（M4-ROB-01a）

- **状态**: 已决策
- **日期**: 2026-07-05
- **范围**: M4 Sprint F ROB-01a「checkpoint 硬化 + 幂等重试」——新增 `state reset` CLI 命令（core `reset_module` + `ResetOutcome`）、全字段 round-trip 完整性测试；SKILL.md/run.md 回滚约定收口。改 `cli/`（core + CLI）、`plugin/`、`docs/`。

## 背景

M3 实际反复遇到单模块失败/中断后的可靠性问题：重跑失败模块是否会腐蚀已有状态、断点续跑是否幂等。调查现状（2026-07-05）：

- **checkpoint 原子写已达标**：`atomic_write`（tmp → `write_all` → `sync_all` → `rename` → 父目录 `sync_all` + `.backup`）已覆盖崩溃/半截写。「checkpoint 硬化」只缺一个**全字段不丢**的回归守卫。
- **幂等重试是纯缺口**：「状态回退 + 产物清理」全靠 run.md 自然语言约定（run.md 旧 117/155/180「删 `rust_root` .rs、复位 translating」）由 LLM 执行，CLI 无对应原子命令。且 `transition_module` 对同态 `--to` 报 `InvalidTransition`（无自环）——**重跑不幂等**，`done` 重做只能「人工重置状态」（手工编辑 JSON）。
- **关键约束**：`ModuleState` **不记录该模块产出的 `.rs` 路径**（`member_files` 是源文件 NodeId）。CLI 无法可靠知道要删哪些 rust 产物——这正是 run.md 让 LLM 删的原因。

## 决策

### 决策 1：产物清理职责边界——CLI 做确定性状态回退 + 输出作用域，产物 `.rs` 删除归编排器

采「收窄版」：`state reset` 只做 **CLI 能确定性负责的事**，`rust_root` 下 `.rs` 的实际删除留给编排器（run.md）。

- **CLI 拥有**：① 原子、幂等的状态回退（`reset_module`）；② 输出 `ResetOutcome.member_files`（模块源作用域，composite → `member_files`；单文件 → `[module]`），CLI 层据此拼 `data.cleanup`（作用域 + 保留/清理指令 + `run --retry` 提示）。
- **编排器拥有**：`rust_root/*.rs` 删除——它（translator）写的它删，且重译已 overwrite-safe（run.md 6.1 content-hash 跳过检测 + L1 `todo!()` 残留判定，删掉 `.rs` 后跳过前置不满足 → 自动重译）。
- **为何不让 CLI 全包（加产出物清单字段 + CLI 删文件）**：需 `ModuleState` 新增字段、run/translator **每次写 `.rs` 都登记路径**——引入 **manifest 漂移**这一新失败模式（漏登 → 清理不全 → 腐蚀），与 ROB-01a 的可靠性目标相悖；且把破坏性 `fs` 删除放进确定性引擎会放大爆炸半径。3 门语言规模下权威 manifest 的收益抵不过漂移纪律 + 风险——与 MDR-014 砍 index.json 同一 YAGNI 立场。
- **为何不纯留文字约定**：ROB-01b（watchdog 恢复）/ ROB-01c（额度续跑）要**程序化、幂等地**调用回退，故 `state reset` 命令现在就该建，不能停在 run.md 散落 prose + 「人工重置」。
- **诚实划界**：CLI 无法复现编排器的 intermediate 文件 slug 命名，故 `cleanup` **不虚假枚举** staging 文件名，只给权威的 `member_files` 源作用域 + 人读指令。

### 决策 2：`reset_module` 语义——回退到干净重译入口，保留审计与结构冻结字段

status → `translating`，清全部「尝试进度」字段（`substatus` / `phase_a_version` / `phase_a_audit_passed` / `test_pass_rate` / `coverage` / `known_differences` / `blocked_by` / `pre_blocked_status`）。**保留** `attempts`（追加一条 `reset:<from>→<to>` 审计）、`tier` / `member_files` / `composite_kind` / `decomposition_*` / `danger`（结构性冻结——reset 是「重试」非「重新拆解」）。

- **`pending` 保持 `pending`**：尚未起步、无产物，无需前移 `translating`。
- **module key 归一**：复用 `transition_module` 的组代表归一（抽出私有 `canonical_module_key`，两处共用）——对折叠组非代表成员发 reset 时归一到组代表。

### 决策 3：幂等 = 已在干净入口时空操作、不追加审计、免落盘

`already_clean`（status ∈ {`translating`,`pending`} 且全部进度字段为空）时直接返回 `was_noop=true`，**不改任何字段、不追加审计**——保证 `reset;reset` 与 `reset` 状态严格一致（若 no-op 也 append 审计，二次调用会多一条记录，破坏幂等）。CLI 据 `was_noop` **免落盘**（避免多余 `.backup`/写）。

### 决策 4：终态 / 锚点守护须 `--force`

`done`（唯一真终态）/ `blocked`（依赖锚点）/ `degrade_*`（降级须人类确认）不带 `--force` → `MigrateError::Config` 报错、状态不动。防止误清断点续传锚点 / 静默重迁已完成模块。与 `transition_module` 的 `degrade_* → translating 须 --force` 立场一致。

### 决策 5：checkpoint 硬化验证 = 全字段 round-trip 测试

新增 `test_full_field_round_trip_preserves_all_fields`：构造**每个字段都非默认/非 None** 的 `MigrationStateFile` → 真实 `save`（原子写）→ `load` → 逐字段 `assert_eq`。钉住「save/load 不丢字段」——未来新增字段漏进序列化/反序列化会红。（原子写机制本身已有 `test_atomic_write_leaves_no_tmp` 等 20 测覆盖，本测补的是**字段完整性**维度。）

## 影响

- **新增**：core `MigrationStateMachine::reset_module` + `ResetOutcome`（导出）；`validate` 无关。7 reset 单测 + 1 全字段 round-trip 单测；`state reset` CLI 命令 + `cmd_state_reset`（1 cli_e2e：回退/幂等/done 守护/force 恢复/cleanup 作用域）。
- **重构**：`transition_module` 的组代表归一抽出为私有 `canonical_module_key`（行为不变，reset 复用）。
- **改**：SKILL.md 新增「失败/中途模块回滚：`state reset`」单点约定；run.md 断点路由 `done` 行 + 单文件 Phase A 失败回滚改引用 `state reset`（paused/保留状态的回滚非 reset 语义，不动）。
- **不改 schema**：`ModuleState` 无新字段（产物清单归编排器，见决策 1）。

## 后续 TODO（记账，非阻塞）

1. **ROB-01b（watchdog stall 检测 + 恢复）**：将复用 `state reset` 做失败模块回退——识别 agent 静默超时 → `reset` → 跳过/重试策略。
2. **ROB-01c（额度耗尽优雅暂停 + 续跑）**：断点恢复时对进行中模块调 `reset` 保证幂等重入。
3. **产物清理若未来需 CLI 强一致**：真实项目若暴露「编排器漏删部分 `.rs`」的实证腐蚀，再评估决策 1 的产出物清单方案（当前 YAGNI）。
