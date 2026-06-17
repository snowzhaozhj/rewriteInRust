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
开始时取全局锁，结束或异常退出时释放（见 SKILL.md「全局锁」）。

### 0. 复杂度分档路由（M2-TIER-01c）

读 `modules[target].tier` 决定翻译循环路径（`tier` 由 `populate-modules` 的 AST 检测自动填充，见 03 §4.3.2）：

| tier | 循环路径 | 跳过步骤 |
|------|---------|---------|
| `trivial` | 直翻 + 编译 + 导出可见性核对 + 签批 | 跳步骤 4-5（意图摘要/确认）、步骤 7-9（对抗审查/Phase B）、维度 9 退化为符号一致性 |
| `standard` | 保留意图摘要 + Phase A + 审查 + 测试 | 跳步骤 9 中的 loom/shuttle 插桩 |
| `full` | 完整 11 步（同 M1） | 无 |
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

### 3. 目标依赖就绪门禁
`rustmigrate graph deps <module>` 取依赖，检查是否全部 `done`/`degrade_*`。有未就绪依赖则中止本次 run，把目标模块标 `blocked`、填 `blocked_by`（未完成依赖）和 `pre_blocked_status`。

### 4. 意图摘要（translator）
调 translator（**前/后记 subagent_call**，见 SKILL.md「SubAgent 编排」，step_index=4）生成 `.rust-migration/intermediate/{module}-intent.md`。**L2 校验**：9 个 required 属性全非空、`interfaces` ≥1（Schema 见 [translator.md](../../agents/translator.md)）。缺字段视为不完整，按失败恢复重试。

### 5. 意图确认门禁（人类决策，默认开启）
向用户展示 intent.md 全文，请其「确认 / 修订」。修订则 translator 重新生成，最多 2 轮；第 3 轮仍不满意 → 置 `paused` + `requires_manual_review`，停。`.rustmigrate.toml` 设 `auto_confirm_intent=true` 可跳过本门禁（power-user）。

### 6. Phase A 忠实翻译（translator）
调 translator（**前/后记 subagent_call**，step_index=6）产出 Rust 源文件（写 `rust_root/`）+ `_porting_manifest.json` + 持久化 `intermediate/attempts/{module}-phase-a.rs`。**L1 校验**：Rust 文件存在且编译通过、manifest 非空。失败 ≤2 次重试；仍失败则回滚（删 `rust_root/{module}.rs` 部分写入，保留 intent.md + attempts/*，状态复位 `translating`/substatus=null）。成功后 `state transition --module <M> --substatus phase_a_complete_awaiting_review`（status 不变）。

### 7. 对抗审查（verifier）
调 verifier（**前/后记 subagent_call**，step_index=7）读 `attempts/{module}-phase-a.rs` + 源码 + 规则，产出 `{module}-review.md`。**L1**：存在、非空、含差异列表。失败 ≤2 次重试；仍失败回滚（删 review.md，保留 Phase A 代码，状态保持 `phase_a_complete_awaiting_review`）。

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

## 失败处理
任一 SubAgent 步骤失败按 SKILL.md「失败恢复」三步处理。`intermediate/attempts/*` 始终保留。回滚清理范围按各步骤标注执行（部分写入删除、中间产物保留、状态复位）。
