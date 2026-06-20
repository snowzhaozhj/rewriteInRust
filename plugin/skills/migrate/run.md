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

读 `modules[target].tier` 决定翻译循环路径（`tier` 由 `populate-modules` 的 AST 检测自动填充，见 03 §4.3.2）：

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
| `translating` + `phase_a_complete_awaiting_review` | 跳第 7 步 |
| `translating` + `phase_b_optimization_in_progress` / `phase_b_failed_at_round_N` | 跳第 9 步 |
| `compile_fixing` | 跳第 9 步 |
| `testing` | 跳第 10 步 |
| `reviewing` | 跳第 11 步 |
| `pending` / `translating`(substatus=null) | 正常从第 2 步开始 |

### 2. 解除 blocked + 循环依赖检测
遍历所有 `status=blocked` 模块，若其 `blocked_by` 引用的模块都已 `done`/`degrade_*`，则 `state transition --module <M> --to <pre_blocked_status> --reason 'blocked_by resolved'` 自动恢复并记日志。对 blocked 子图做 DFS 环检测：发现环即报错中止、输出环路径、记入 `metadata.last_error`（防 blocked 模块互相等待死锁）。

> 源码 SCC（循环依赖）在 populate 阶段已被折叠成**一个模块组**（`member_files` 含组内全部源文件），由 translator 整组翻译（见 [translator.md](../../agents/translator.md)「SCC 模块组翻译」），不再因循环依赖被标 `blocked`。这里的环检测只针对 `blocked` 子图的等待死锁，与源码 SCC 无关。

### 3. 目标依赖就绪门禁
`rustmigrate state deps <module>` 取**组感知**依赖就绪结果（破环 M2-SCALE-SCC）：它把 composite 组成员的文件级依赖映射回组代表 key、剔除组内自依赖、按终态判就绪，直接输出 `{dependencies, all_ready, blocking}`。`all_ready=false` 则中止本次 run，把目标模块标 `blocked`、用 `blocking`（未就绪组代表 key）填 `blocked_by` 和 `pre_blocked_status`。

> 勿用 `graph deps`（纯图、文件级）做此门禁：折叠后组内非代表成员（如 `types.ts`）不在 `modules` 表，逐个查 status 会静默落空。`state deps` 已处理 member→组代表映射。

### 4. 意图摘要（translator）
调 translator（**前/后记 subagent_call**，见 SKILL.md「SubAgent 编排」，step_index=4）生成 `.rust-migration/intermediate/{module}-intent.md`。**L2 校验**：9 个 required 属性全非空、`interfaces` ≥1（Schema 见 [translator.md](../../agents/translator.md)）。缺字段视为不完整，按失败恢复重试。

### 5. 意图确认门禁（人类决策，默认开启）
向用户展示 intent.md 全文，请其「确认 / 修订」。修订则 translator 重新生成，最多 2 轮；第 3 轮仍不满意 → 置 `paused` + `requires_manual_review`，停。`.rustmigrate.toml` 设 `auto_confirm_intent=true` 可跳过本门禁（power-user）。

### 6. Phase A 忠实翻译（translator）
调 translator（**前/后记 subagent_call**，step_index=6）产出 Phase A 翻译。根据模块 tier 分两种模式：

- **单候选**（`trivial` 档）：产出 Rust 源文件（写 `rust_root/`）+ `_porting_manifest.json` + 持久化 `intermediate/attempts/{module}-phase-a.rs`。**L1 校验**：Rust 文件存在且编译通过、manifest 非空。
- **多候选**（`standard`/`full` 档，M2-ADV-01）：产出 2 个候选文件到 `intermediate/attempts/{module}-phase-a-candidate-{1,2}.rs` + 各自的 `{module}-candidate-{1,2}-manifest.json`。**不立即写 `rust_root/`**（等选优后写入）。**L1 校验**：两个候选文件均存在且各自编译通过、两个 manifest JSON 合法。

失败 ≤2 次重试；仍失败则回滚（删 `rust_root/{module}.rs` 部分写入，保留 intent.md + attempts/*，状态复位 `translating`/substatus=null）。成功后 `state transition --module <M> --substatus phase_a_complete_awaiting_review`（status 不变）。

### 7. 候选选优 + 对抗审查（verifier）

**7a. 候选选优**（仅 `standard`/`full` 档，M2-ADV-01）：
调 verifier（**前/后记 subagent_call**，step_index=7a）读取 2 个候选 + `_candidate_manifest.json` + 意图摘要 + 源码，产出 `{module}-selection.md`（选中候选编号 + 评分 + 对比）。**L1**：存在、非空、含「候选选优结果」标题。选优后 verifier 将选中候选复制为 `attempts/{module}-phase-a.rs` 并写入 `rust_root/`、生成 `_porting_manifest.json`。`trivial` 档跳过此步。

**7b. 对抗审查**：
调 verifier（**前/后记 subagent_call**，step_index=7b）读 `attempts/{module}-phase-a.rs`（多候选模式下为选优后的版本）+ 源码 + 规则，产出 `{module}-review.md`。**L1**：存在、非空、含差异列表。失败 ≤2 次重试；仍失败回滚（删 review.md，保留 Phase A 代码，状态保持 `phase_a_complete_awaiting_review`）。

### 8. Phase A 结构门禁
`rustmigrate stats compare` 校验 Phase A 1:1 结构（函数数比、行数比、控制流对应）。越界 → 标"疑似已优化"，要求 translator 以忠实模式重做 Phase A（删旧 review.md，重跑第 7 步）；重做仍越界 → `paused` + `requires_manual_review`。通过则记 `phase_a_audit_passed=true` + `phase_a_version`（content hash），进第 9 步。

### 9. Phase B 惯用化 + 编译修正（translator）
`state transition --module <M> --substatus phase_b_optimization_in_progress`。先 `cargo fix --allow-dirty`，剩余错误交 translator（**前/后记 subagent_call**，step_index=9；仅三类重写：并发 / 取消安全 / 局部性能）。编译失败则 `state transition --to compile_fixing --substatus "<当轮错误摘要>"`，最多 **3 轮**（`max_retry_rounds`）；失败持久化 `attempts/{module}-phase-b-partial.rs`、置 `phase_b_failed_at_round_N`（供 `--retry`）。

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
1. `state transition --module <M> --to degrade_skip --substatus "headless_auto_degrade" --reason "headless: 3轮失败自动降级"`
2. 继续处理同 sprint 其余模块

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
