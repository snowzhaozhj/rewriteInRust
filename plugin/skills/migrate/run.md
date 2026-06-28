# /migrate run — 执行模块迁移（Phase A/B 双阶段）

翻译指定模块：意图摘要 → Phase A 忠实翻译 → 对抗审查 → Phase B 惯用化 → 测试验证 → 签批。确定性计算走 `rustmigrate` CLI，翻译/审查走 SubAgent（`rust-migrate:translator` / `rust-migrate:verifier`）。共享约定（全局锁、CLI 解析、SubAgent 校验、失败恢复）见 [SKILL.md](./SKILL.md)。

## 前置条件
- `migration-state.json` 存在，项目 state 为 `sprint_loop`，`modules` 非空。
- `.rust-migration/porting/` 存在且含规则文件。
- 目标模块已在迁移序列中。

项目级状态推进（`init→…→sprint_loop`）由 `/migrate analyze` 全部完成。本命令进入时前置即应满足；不满足说明 analyze 未跑完，应先重跑 `/migrate analyze`。run 自身**只做模块级转换**（`state transition --module <M>`），不推进项目级状态。

## 调用形式
`/migrate run <module>`，可选 `--retry`（从断点重入）、`--force`（重做 degrade_*）、`--degrade=ffi|manual|skip`（确认降级方式）。

## 流程
开始时取全局锁，结束或异常退出时释放（见 SKILL.md「全局锁」）。并行模式下锁策略有区别——编排器持锁，SubAgent 不取锁（见 SKILL.md「并行模式下的锁策略」）。

### 0. 复杂度分档路由（M2-TIER-01c）

读 `modules[target].tier` 决定翻译循环路径（`tier` 由 `populate-modules` 的 AST 检测自动填充）：

| tier | 循环路径 | 跳过步骤 |
|------|---------|---------|
| `trivial` | 直翻（单候选）+ 编译 + 导出可见性核对 + 签批 | 跳步骤 4-5（意图摘要/确认）、步骤 7a（候选选优）、步骤 7b-9（对抗审查/Phase B）、维度 9 退化为符号一致性 |
| `standard` | 保留意图摘要 + Phase A 多候选（2 候选）+ 选优 + 审查 + 测试 | 跳步骤 9 中的 loom/shuttle 插桩 |
| `full` | 完整 11 步（含 Phase A 多候选 + 选优），同 M1 扩展 | 无 |
| `null`（未检测） | 等同 `full`（保守兜底） | 无 |

**降档/升档机制（M2-TIER-01d）**：
- **失败自动升档**：trivial/standard 模块在步骤 6（编译）或步骤 10（测试）失败时，自动升档到 full 并从步骤 2 重跑。升档记入 `attempts`（`reason: "tier升档 standard→full: 编译失败"`）。
- **分档结果可观测**：tier 记录在 `modules[target].tier`（state get 可查），升档事件记入 `attempts` 审计序列。
- **维度 9 永不跳过**：trivial 无运行时行为，维度 9 退化为「导出符号+类型签名集合一致性」核对（非完整 7 字段）。

### 1. 断点续传路由
读 `modules[target].status` / `substatus`，按下表跳转，让中断的 run 从正确入口恢复：

| status / substatus | 动作 |
|---|---|
| `done` | 报错退出：终态不可重做（确需重迁则人工重置状态后再跑） |
| `paused` | 报错退出：模块暂停中，先 `--degrade=ffi\|manual\|skip` 确认降级方式再续 |
| `degrade_*` 无 `--force` | 报错：降级是人类决策，须 `--force` |
| `degrade_*` + `--force` | 重置为 `translating`（清 substatus/attempts）→ 续第 2 步 |
| `translating` + `contract_ready`（SCC 组：契约门已过） | 跳第 6 步的 6b 逐文件填空（契约+stub 已就绪，不重出契约） |
| `translating` + `phase_a_in_progress`（SCC 组：部分成员已填） | 跳第 6 步 6b，按磁盘事实核对 `{group}-progress.json` 只重派未完成的成员文件 |
| `translating` + `contract_revision`（SCC 组：契约增量修订中） | 回第 6 步 6a 重出契约+stub、过契约门后再续 6b（修约触发见 6b「契约不足的修约路径」） |
| `translating` + `phase_a_complete_awaiting_review` | 跳第 7 步 |
| `translating` + `phase_b_optimization_in_progress` / `phase_b_failed_at_round_N` | 跳第 9 步 |
| `compile_fixing` | 跳第 9 步 |
| `testing` | 跳第 10 步 |
| `reviewing` | 跳第 11 步 |
| `pending` / `translating`(substatus=null) + `composite_kind=batch` | 跑步骤 2-3（blocked 解除 + 依赖就绪门禁），然后**直接进入步骤 6「Batch 组轻量路径」**（跳过步骤 4/5 意图摘要/确认）；content-hash 决定跳过翻译或整批重派 |
| `pending` / `translating`(substatus=null) | 正常从第 2 步开始（非 batch 组的默认路由） |

### 2. 解除 blocked + 循环依赖检测
遍历所有 `status=blocked` 模块，若其 `blocked_by` 引用的模块都已 `done`/`degrade_*`，则 `state transition --module <M> --to <pre_blocked_status> --reason 'blocked_by resolved'` 自动恢复并记日志。对 blocked 子图做 DFS 环检测：发现环即报错中止、输出环路径、记入 `metadata.last_error`（防 blocked 模块互相等待死锁）。

> 源码 SCC（循环依赖）在 populate 阶段已被折叠成**一个模块组**（`member_files` 含组内全部源文件），翻译粒度=单文件、SCC 仅作整组编译门禁（步骤 6「SCC 组 Phase A」：契约+stub→契约门→逐文件填空→整组真门，见 [translator.md](../../agents/translator.md)「SCC 模块组翻译」），不再因循环依赖被标 `blocked`。这里的环检测只针对 `blocked` 子图的等待死锁，与源码 SCC 无关。

### 3. 目标依赖就绪门禁
`rustmigrate state deps <module>` 取**组感知**依赖就绪结果（破环 M2-SCALE-SCC）：它把 composite 组成员的文件级依赖映射回组代表 key、剔除组内自依赖、按终态判就绪，直接输出 `{dependencies, all_ready, blocking, unresolved}`。`all_ready=false` 则中止本次 run，把目标模块标 `blocked`、用 `blocking`（未就绪组代表 key）填 `blocked_by` 和 `pre_blocked_status`。

> `unresolved` 非空（依赖未登记为模块，state 与 source-graph 不同步）只发 warning、**不计入 blocking**——避免把缺失 key 填进 `blocked_by` 触发 check-blocked 永久非终态死锁。出现时应重新 `graph build` + `populate-modules` 同步状态，而非据此 block。

> 勿用 `graph deps`（纯图、文件级）做此门禁：折叠后组内非代表成员（如 `types.ts`）不在 `modules` 表，逐个查 status 会静默落空。`state deps` 已处理 member→组代表映射。

**裁剪依赖 context 注入（degrade_skip 上游处理，见 MDR-007 `blocked_by_skip`）**：`dependencies` 中 `status=degrade_skip` 的依赖是终态、算「就绪」不阻塞，但其能力已从迁移范围裁剪——目标模块若照常 `use` 它会编译失败。故筛出这些被裁剪依赖，读各自 `intermediate/{dep}-degrade-report.json` 的 `recommended_alternatives`，组装「裁剪依赖清单」（每项：被裁剪模块 key + 推荐替代 crate + 理由），在第 4 步意图摘要与第 6 步 Phase A 派发 translator 时一并注入。translator 据此把对被裁剪依赖的调用改写为推荐 crate；无推荐（清单项 `recommended_alternatives` 空）则留 `TODO(port): 依赖 <dep> 已裁剪，需人工选型替代` 标记，不留悬空引用。degrade-report.json 缺失（早期 skip 未落报告）按无推荐处理。

### 4. 意图摘要（translator）
调 translator（**前/后记 subagent_call**，见 SKILL.md「SubAgent 编排」，step_index=4）生成 `.rust-migration/intermediate/{module}-intent.md`。**L2 校验**：9 个 required 属性全非空、`interfaces` ≥1（Schema 见 [translator.md](../../agents/translator.md)）。缺字段视为不完整，按失败恢复重试。

### 5. 意图确认门禁（人类决策，默认开启）
向用户展示 intent.md 全文，请其「确认 / 修订」。修订则 translator 重新生成，最多 2 轮；第 3 轮仍不满意 → 置 `paused` + `requires_manual_review`，停。`.rustmigrate.toml` 设 `auto_confirm_intent=true` 可跳过本门禁（power-user）。

### 6. Phase A 忠实翻译（translator）

**先判模块形态**：读 `modules[target].member_files` 与 `composite_kind`（M3-DEC-01）。
- `composite_kind=batch`（机械合批组）→ 下面「Batch 组轻量路径」。
- `member_files` 为空（单文件模块）→ 下面「单文件 Phase A」。
- `member_files` 非空且 `composite_kind=cycle`（或缺省的循环组）→「SCC 组 Phase A（契约门 → 逐文件填空）」。

#### Batch 组轻量路径（机械合批）

机械合批组（`composite_kind=batch`）的成员全为可证明机械文件（纯类型/常量/barrel），无环、footprint 受控。**不走 SCC 契约重路径**，改走简化流程：一次翻完 + 一次编译 + 一次签批。状态机复用单文件模块同款简单流（`translating → done`），无 SCC substatus、无 `{group}-progress.json`、无成员级断点。

> 设计权威：`docs/decomposition-redesign.md` §7（轻量路径规范 + 状态机接入）。

**流程**：

1. **Content-hash 跳过检测**（恢复优化）：读各成员源文件当前 content-hash（`rustmigrate graph fingerprint <file>`），对比 `intermediate/{batch}-source-hashes.json`（上次翻译时快照）。跳过需**全部满足**：
   - 全部成员 Rust 产出文件已存在于 `rust_root/`
   - 源码 hash 全部未变（与快照一致）
   - `cargo check` 通过
   - 导出符号/签名校验通过（各成员 `.rs` 的 `pub` 导出覆盖源文件 exported symbols）
   
   全部满足 → **跳过翻译，直接进签批（步骤 5）**。任一条件不满足 → 整批重新翻译（步骤 2）。
   > 这是原子恢复的唯一优化入口——因为是"一次翻完"的原子操作，成本本就低，不值得做成员级断点；content-hash + 签名校验保证产出物完整且源码未变时不重复工作。

2. **翻译（translator，一次翻完整批）**：调 translator（**前/后记 subagent_call**，step_index=6）。输入：
   - 全部成员源文件（按逆拓扑序排列，`rustmigrate graph topo --members <batch-key> --reverse`）
   - 外部依赖 interfaces（`rustmigrate graph interfaces --deps-of <batch-key>`，仅 batch 外部依赖）
   - 适用的 porting rules（同单文件 Phase A）
   - 裁剪依赖清单（若有，同步骤 3「裁剪依赖 context 注入」）

   产出：每个成员对应的 `.rs` 文件写入 `rust_root/` + `intermediate/{batch}-source-hashes.json`（各成员源文件 content-hash 快照，供后续恢复比对）。translator 指令详见 [translator.md](../../agents/translator.md)「Batch 组翻译」。

3. **L1 校验**：全部成员 `.rs` 文件存在于 `rust_root/`、非空、无 `todo!()` 残留。缺文件或含 `todo!()` 视为翻译不完整，按失败恢复重试。

4. **整组编译**：`cargo check`。失败 → 重试 translator（≤2 次，`max_retries_per_step`）；仍失败 → `paused` + `requires_manual_review`（同单文件模块失败升级）。回滚清理：删 `rust_root/` 下本 batch 成员的 `.rs` 文件，保留 `intermediate/` 产物。

5. **签批**（确定性检查，无需 verifier subagent）：
   - `TODO(port)` 计数 = 0（grep 全部成员 `.rs`）
   - `cargo check` 通过（步骤 4 已保证）
   - 导出符号一致性：各成员 `.rs` 的 `pub` 导出覆盖源文件的 exported symbols（`rustmigrate stats compare --batch <batch-key>` 或人工核对）
   - 全部通过 → `state transition --module <M> --to done`

**跳过的步骤**：意图摘要(4) / 意图确认(5) / 多候选(7a) / 对抗审查(7b) / 结构门(8) / Phase B(9) / 测试生成(10)——机械文件无行为可测，编译即门禁。

**失败恢复**：同 SKILL.md 三步。重试耗尽 → `paused`，等用户 `--degrade` 确认。`intermediate/attempts/{batch}-*.rs` 始终保留。

**断点恢复路由**：batch 组不使用 SCC 专属 substatus（`contract_ready` / `phase_a_in_progress` / `contract_revision`）。中断后按 `translating`(substatus=null) 路由，进入本节时 content-hash 跳过检测自动处理"已完成但未标 done"的情况。

#### 单文件 Phase A
调 translator（**前/后记 subagent_call**，step_index=6）产出 Phase A 翻译。根据模块 tier 分两种模式：

- **单候选**（`trivial` 档）：产出 Rust 源文件（写 `rust_root/`）+ `_porting_manifest.json` + 持久化 `intermediate/attempts/{module}-phase-a.rs`。**L1 校验**：Rust 文件存在且编译通过、manifest 非空。
- **多候选**（`standard`/`full` 档，M2-ADV-01）：产出 2 个候选文件到 `intermediate/attempts/{module}-phase-a-candidate-{1,2}.rs` + 各自的 `{module}-candidate-{1,2}-manifest.json`。**不立即写 `rust_root/`**（等选优后写入）。**L1 校验**：两个候选文件均存在且各自编译通过、两个 manifest JSON 合法。

失败 ≤2 次重试；仍失败则回滚（删 `rust_root/{module}.rs` 部分写入，保留 intent.md + attempts/*，状态复位 `translating`/substatus=null）。成功后 `state transition --module <M> --substatus phase_a_complete_awaiting_review`（status 不变）。

#### SCC 组 Phase A（契约门 → 逐文件填空）

源码循环依赖被折叠成的模块组（`member_files` 多文件），翻译粒度=单文件、SCC 仅作整组编译门禁。详见 [translator.md](../../agents/translator.md)「SCC 模块组翻译」。分两阶段：

**6a. 组契约 + stub + 契约门（组级一次）**：调 translator（**前/后记 subagent_call**，step_index=6）读全组导出签名（`rustmigrate graph interfaces <group> --members` 一次取整组；`<group>` 传组代表 module key，即该 SCC 组的 module 标识，命令按它定位整组），产出 `intermediate/{group}-contract.md`（6 字段）+ `rust_root/<group>/` 可编译 stub 骨架（签名齐全、body 全 `todo!()`、`mod.rs` 全 `mod` 声明、`Cargo.toml` 一次写全）。
- **契约门校验（L1）**：① stub 骨架 `cargo check` 通过（跨文件签名自洽、所有权类型可解析）；② `{group}-contract.md` 6 字段（`module_map`/`exported_symbols`/`ownership_graph`/`error_model`/`visibility`/`cross_file_calls`）齐全非空。任一不满足按失败恢复重试（≤2 次），重试耗尽 → `paused` + `requires_manual_review`。
- 契约门过 → **先**原子写 `intermediate/{group}-progress.json`（`{contract_valid: true, stub_check_passed: true, members: {}}`），**再** `state transition --module <M> --substatus contract_ready`（status 不变）。**顺序很关键**：先建 checkpoint 再标里程碑——否则若中断在两步之间，恢复按 `contract_ready` 跳 6b 时 progress.json 不存在（恢复逻辑见 6b 兜底）。

**6b. 逐文件并行填空**：对 `member_files` 每个成员文件派一个 translator（**前/后记 subagent_call**，step_index=6），输入=该文件源码 + `{group}-contract.md` + 对应 mod 的 stub，产出=把该 mod 的 `todo!()` 填成实现。**签名锁定**：`diff` stub 与 impl 仅 body 变化（fn 签名/struct 字段/mod 声明/Cargo.toml/共享 Error enum 逐字节不变）。并行隔离与同 worktree 派发见 [workflow.md](./workflow.md)；串行 run 下顺序填各成员。
- 置 `state transition --module <M> --substatus phase_a_in_progress`。每个成员文件填完即由编排器更新 `{group}-progress.json` 的 `members.{file}.phase_a=true`（细粒度 checkpoint，断点续跑只重派未完成成员）。
- **恢复以磁盘事实为准**（progress.json 与磁盘不是同一原子写边界）：续跑时**不要只信 progress.json**——逐成员核对其 `rust_root/` 文件：仍含 `todo!()` 或缺文件 = 未完成（重派），无 `todo!()` 且签名锁通过 = 已完成（即使 progress.json 漏记也跳过）。冲突时以磁盘为准、并修正 progress.json。这样 substatus/progress/磁盘三者间任意中断点都能正确续跑、不重复派发已填成员（Level 3 已实证此「磁盘事实裁决」）。progress.json 不存在则视为契约后零成员完成，按磁盘逐一核对。
- **契约不足的修约路径**（区别于「签名被改」）：成员填空时若发现**契约签名不够用**（如缺一个跨文件方法、所有权类型选错导致填不下去），这不是该成员的错、也不该靠重试死磕——走与 Phase B 同一套**契约增量**：回退到 6a 改 `{group}-contract.md` + stub → 契约门复验（stub check）→ 受影响成员按新契约重填。编排器据此记 `state transition --substatus contract_revision --reason "<缺口>"`、回滚相关成员的 `members.{file}.phase_a`。**单纯「签名被改」**（成员擅自改了够用的签名）才是打回该成员重填。
- **L1 校验**：每个成员 mod 无 `todo!()` 残留（Phase A 该填的已填）+ **签名锁校验**（`diff` 仅 body 变化，签名被改 → 打回该成员重填）。全部成员填完后整组 `cargo check` 通过（实现门预检；正式真门在 verifier 测试与并行编排步骤 2d）。
- 全部成员完成 → `state transition --module <M> --substatus phase_a_complete_awaiting_review`（status 不变），进第 7 步。

### 7. 候选选优 + 对抗审查（verifier）

**7a. 候选选优**（仅 `standard`/`full` 档，M2-ADV-01）：
调 verifier（**前/后记 subagent_call**，step_index=7a）读取 2 个候选 + `_candidate_manifest.json` + 意图摘要 + 源码，产出 `{module}-selection.md`（选中候选编号 + 评分 + 对比）。**L1**：存在、非空、含「候选选优结果」标题。选优后 verifier 将选中候选复制为 `attempts/{module}-phase-a.rs` 并写入 `rust_root/`、生成 `_porting_manifest.json`。`trivial` 档跳过此步。

> **SCC 组跳过 7a**：SCC 组逐文件填空无 per-module 候选文件（`{module}-phase-a-candidate-{1,2}.rs` 不存在），故 7a 不适用。SCC 的多候选（若启用）是**契约层**的——契约步（6a）产 2 套契约/stub，由 verifier 在 6a 内选优契约后再进 6b 填空（见 [translator.md](../../agents/translator.md)「多候选模式」SCC 例外）。默认 SCC 组走单契约，6a 只产一套，7a 直接跳过、进 7b 对整组做对抗审查。

**7b. 对抗审查**：
调 verifier（**前/后记 subagent_call**，step_index=7b）读 `attempts/{module}-phase-a.rs`（多候选模式下为选优后的版本）+ 源码 + 规则，产出 `{module}-review.md`。**L1**：存在、非空、含差异列表。失败 ≤2 次重试；仍失败回滚（删 review.md，保留 Phase A 代码，状态保持 `phase_a_complete_awaiting_review`）。

### 8. Phase A 结构门禁
`rustmigrate stats compare` 校验 Phase A 1:1 结构（函数数比、行数比、控制流对应）。越界 → 标"疑似已优化"，要求 translator 以忠实模式重做 Phase A（删旧 review.md，重跑第 7 步）；重做仍越界 → `paused` + `requires_manual_review`。通过则记 `phase_a_audit_passed=true` + `phase_a_version`（content hash），进第 9 步。

### 9. Phase B 惯用化 + 编译修正（translator）
`state transition --module <M> --substatus phase_b_optimization_in_progress`。先 `cargo fix --allow-dirty`，剩余错误交 translator（**前/后记 subagent_call**，step_index=9；仅三类重写：并发 / 取消安全 / 局部性能）。编译失败则 `state transition --to compile_fixing --substatus "<当轮错误摘要>"`，最多 **3 轮**（`max_retry_rounds`）；失败持久化 `attempts/{module}-phase-b-partial.rs`、置 `phase_b_failed_at_round_N`（供 `--retry`）。

> **SCC 组的 Phase B 仍逐文件、不退回整组**（否则撞上下文上限）：逐成员文件做三类惯用化。若 Phase B 需改某跨文件签名，**先改 `{group}-contract.md` + stub 再逐文件 apply**（不允许单个成员单方面改签名破其他文件引用）：改契约字段 → 更新 stub 签名 → stub `cargo check` 过（契约门复验）→ 受影响成员按新签名各自 apply。签名冻结基线随契约修订前移，但**逐文件层仍是惯用化重写（三类）而非填 `todo!()`**——Phase B 的「逐文件」指改造粒度，不是退回填空。

> **两个独立计数器**：SubAgent 超时 / 产出物校验失败计入 `max_retries_per_step`(2)；编译失败计入 `max_retry_rounds`(3)。任一耗尽 → pause→degrade：生成降级分析报告，置 `paused`，等用户 `--degrade=ffi|manual|skip` 确认。不要强行输出能编译但语义可疑的代码。

### 10. 测试验证（verifier）
`state transition --module <M> --to testing`。调 verifier（**前/后记 subagent_call**，step_index=10）生成测试并跑 `hooks/scripts/verify.sh`（nextest + clippy + 条件 loom/shuttle），产出测试结果 JSON（**L2**：通过率 ∈[0,1]）。**done 前置硬条件**：通过率 ≥ 预期、clippy 无 warning、`TODO(port)` 计数=0、无未确认的 `bug_replica`。任一不满足标 incomplete、停在 testing：用 `state transition --module <M> --substatus "incomplete" --reason "<未满足项摘要>"`（**不带 `--to testing`**——已在 testing，同态 `--to` 会被转换矩阵拒；省略 `--to` 时 CLI 走 substatus-only 路径合法，且 `--reason` 会把原因 append 进 `attempts`）。失败 ≤2 次重试，回滚保留 Phase B 产物。通过 → `state transition --module <M> --to reviewing`。

### 11. 最终签批
`state transition --module <M> --to done`（原子写）。更新 `PARITY.md`；如有架构决策写 MDR。若前置未满足（TODO(port)>0 / bug_replica 未确认 / coverage 不足），停在 `reviewing`、标 incomplete（同步骤 10：`--substatus "incomplete" --reason "<原因>"`、**不带 `--to`**，避免同态 `reviewing→reviewing` 被矩阵拒），不进 done。

## Headless 模式（M2-ADV-07）

`.rustmigrate.toml` 设 `headless=true` 时启用无人值守模式。核心变化：

### 默认 TODO 决策策略
headless 下 translator 遇到无法判定的 `any`/`unknown`/动态行为时，不留 `TODO(port)` 阻塞——而是按 safe-default 策略自动决策：
- `any` → `Box<dyn std::any::Any>` + 注释标注来源
- `unknown` → 泛型 `T` 或 `serde_json::Value`（按上下文选最安全的）
- 不可判定的动态行为 → `Error::Other(String)` 逃生口 + 标注 `// ADV-07: safe-default`
- 自动决策记入 `attempts`（`reason: "headless-safe-default: any→Box<dyn Any>"`）

### 自动 degrade（paused → degrade_skip）
headless 下模块进入 `paused`（3 轮失败）时，**不等人工确认**，自动执行：
1. 确保 translator 已产出 `intermediate/{M}-degrade-report.json`（含 `recommended_alternatives`，见 [translator.md](../../agents/translator.md)「降级分析报告」）——供依赖 `<M>` 的上游模块在第 3 步「裁剪依赖 context 注入」时复用推荐替代 crate。
2. `state transition --module <M> --to degrade_skip --substatus "headless_auto_degrade" --reason "headless: 3轮失败自动降级"`
3. 继续处理同 sprint 其余模块

自动 degrade 选择 `degrade_skip` 而非 `degrade_ffi`（FFI 生成需人工决策 binding 接口），保证 headless 不挂起。

## 并行编排（M2-SCALE-02）

> **批量入口**：Sprint 级多模块并行翻译由 [`/migrate workflow`](./workflow.md)（M2-SCALE-01）编排——按拓扑层分组、同层并行派发、逐层合并 + 整组验证。本节定义单模块 worktree 隔离的通信协议，是 workflow 的底层基础设施。

当 sprint 内同拓扑层有多个独立模块时，编排器可并行派发翻译任务（`max_concurrent_agents` 由 `.rustmigrate.toml` 配置，默认 3）。并行模式下**编排器全程持锁**，SubAgent 在独立 worktree 中工作、不取锁（详见 SKILL.md「并行模式下的锁策略（M2-SCALE-LOCK）」）。

### 通信协议（7 步）

1. **派发（编排器 → SubAgent）**：编排器为每个目标模块执行 `git worktree add .wt/{module} -b wt/{module} HEAD`，派发 `TranslationDispatch`（module_key + worktree_path + dependency_interfaces + porting_rules），设置 `CARGO_TARGET_DIR=.wt/{module}/target` 避免锁争用。porting 规则约束包要求：优先用既有共享 API；不够时用 `Error::Other`/`anyhow` 逃生口；禁删/改签名既有共享 API，新增只 append。
2. **翻译+自检（worktree 内隔离）**：SubAgent 在自己 worktree 内执行完整 M1 翻译循环（translate → cargo check → compile_fix → test），保留 per-module 编译反馈环。完成后 SubAgent 在 worktree 内 `git add -A && git commit`。
3. **回传（SubAgent → 编排器）**：只回传 `TranslationResult`（touched-list：own_files + shared_touched + self_check + test），代码留盘（上下文经济）。`agent_done` 是 substatus（非终态）。
4. **合并（编排器，git merge）**：编排器在主分支上逐个合并 worktree 分支——`git checkout main && git merge wt/{module}`。git 自动处理文件合并（包括 Cargo.toml、lib.rs 等）。
5. **reconcile（仅 git 冲突时）**：合并冲突时 `git merge --abort`，标记冲突模块需重译。冲突模块按依赖序在各自 worktree 内 rebase 到已合并主线重译（非 LLM 手解冲突块）。轮次上限默认 3，超限降级串行/转人工。冲突文件列表用 `git diff --name-only --diff-filter=U` 获取。
6. **真门（整组 check）**：全部分支合并后，在主 worktree 上执行整组 `cargo check` / `cargo test`。通过 → `agent_done` 升 `done`；不通过 → compile_fixing 子流程。**这是唯一 done 真门**。
7. **清理**：`git worktree remove .wt/{module} && git branch -D wt/{module}`。

### 两层 done

worktree 自检过 = `agent_done`（substatus，非终态）；只有步骤 6 整组 check 过才升最终 `done`。orphan rule/coherence(E0119)/feature 冲突/宏展开等跨并发兄弟冲突只能整组编译暴露。

### 类型定义

派发/回传结构定义在 `rustmigrate-core::types::parallel`（`TranslationDispatch` / `TranslationResult` / `PortingRules` / `AgentStatus` / `CheckStatus`），JSON 序列化用 snake_case。

## 失败处理
任一 SubAgent 步骤失败按 SKILL.md「失败恢复」三步处理。`intermediate/attempts/*` 始终保留。回滚清理范围按各步骤标注执行（部分写入删除、中间产物保留、状态复位）。

---

> **并行编排（真门整组 check + compile_fixing 子流程）详见 [workflow.md](./workflow.md)。** 本文档定义单模块串行流程（步骤 2-11）；workflow 复用这些步骤作为每个模块在 worktree 内的翻译循环，并在其上叠加 worktree 合并、整组验证、跨并发兄弟冲突（orphan rule / E0119 coherence / feature 冲突）的整组修复——这些都是串行单模块不会遇到的，故归口 workflow.md，避免两处重复描述同一套并行协议。
