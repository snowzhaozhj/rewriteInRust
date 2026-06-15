# M2 实施计划：质量提升

> 本文件是 M2 阶段的唯一执行计划。M1 阶段计划保留在 `PLAN.md §1-§9.5`。
> 权威设计来源：`docs/design/` 各章节（本文件引用但不重复）。

---

## §1 M2 总览

### 1.1 目标

将 rustmigrate 从 M1 MVP（串行、单 sprint、全步骤循环）提升为**可实用的迁移工具**：

1. **效率**：复杂度自适应循环（trivial/standard/full 分档），减少无效 token 消耗
2. **规模**：并行 sprint（parallel_groups 消费 + worktree 隔离 + Workflow 批量）
3. **质量**：验证管线增强（proptest/fuzz/覆盖率/行为录制）+ 降级 FFI
4. **可靠性**：类型安全重构 + 状态机加固 + schema 向后兼容
5. **可用性**：CLI 扩展（5 个新命令）+ headless 模式 + CI/CD 集成

### 1.2 验收标准（对齐 08 §M2）

| 指标 | 标准 | 测试方法 |
|------|------|---------|
| 规模验收 | 3 个 5K-20K 行 TS 项目，每项目 ≥3 模块迁移完成 | 真实项目端到端 |
| 降级验收 | ≥1 模块降级 FFI 成功 | circular-deps 或真实循环 |
| 并行吞吐 | ≥1.5 模块/小时（3 agent 并行） | Workflow 批量模式计时 |
| SQLite WAL 配置回归 | `journal_mode=WAL` + `busy_timeout` 连接配置正确 | 配置断言测试（D3 worktree 下编排器集中 writer，**无并发写门禁**，见 §2 D5） |
| 性能无退化 | 单模块翻译时长 + graph build 时长 相对 M1 基线波动 ≤±10%（吞吐为 M2 新指标，无 M1 基线，不纳入此项） | 同 fixture 回归对比，基线由 Sprint A M2-PERF-BASE 快照 |
| 测试质量 | proptest 1000 次无 panic + fuzz 24h 无 crash | 自动化运行 |
| 翻译膨胀 | 膨胀率 <3.0x | tokei 源/Rust LOC 对比 |
| 全流程耗时 | 单模块 full 档完整循环 <60min | 单模块端到端计时（多模块整体由吞吐门禁覆盖，见 §2 D5） |

### 1.3 M2→M3 升级判据（充要条件）

**性能指标（08 §M2 原文）**：
(a) P50 ≥1.5 模块/小时 (b) P99 ≥0.8 模块/小时 (c) ~~SQLite 冲突率 <10% 且锁等待 ≤20ms~~ → **集中 writer 模型，SQLite 并发写门禁 N/A；仅保留 WAL 配置回归**（见 §2 D5） (d) 性能波动 ≤±10%

**质量指标（必要条件，补充）**：
(e) §1.2 验收标准全表通过（含覆盖率 ≥70%、膨胀率 <3.0x、降级 FFI 成功）

若 worktree 启动 + merge 开销导致吞吐 <1.5，降 max_concurrent_agents=2 后 P50 ≥1.2 仍可接受。

### 1.4 与 M1 的关系

- M1 的 202 个测试 + 4 fixture 是 M2 的回归基线
- M2 不破坏 M1 已跑通的 linear/diamond/circular/edge-cases fixture
- M1 遗留的 3 个 deferred TODO 纳入 M2 对应任务

### 1.5 范围裁剪决策

设计文档 08 §M2 列出了 ~30 项交付物。经可行性评估，本计划做以下范围裁剪：

| 类别 | 纳入 M2 | 推迟 M2.5/M3 | 理由 |
|------|---------|-------------|------|
| 复杂度自适应 | ✅ M2-TIER-01 | — | 最高 ROI，纯增量，无锁风险 |
| 并行 sprint | ✅ M2-SCALE 全系列 | — | 规模化必需 |
| 类型重构 | ✅ M2-REFAC（去掉已完成的 08） | — | 代码质量基础 |
| 验证增强 | ✅ VER-01/02/04/05 + 覆盖率 | VER-03 行为录制框架 | 行为录制框架工作量 L 且 M2 验收不强依赖 |
| CLI 扩展 | ✅ 5 个设计文档已定命令 | — | 设计文档已冻结清单 |
| 降级 FFI | ✅ M2-ADV-02 | 降级决策学习 | 核心功能 vs nice-to-have |
| graduate | ✅ M2-ADV-03 | — | 状态机完整性 |
| 高级功能 | ✅ ADV-04/05/06/07/08/09/10 | — | 多为小功能 |
| 状态机程序化 | ❌ 推迟 | ✅ M2.5 | 工作量 XL，M2 用 SKILL.md + 加固 CLI 足够 |
| 向后兼容框架 | ✅ 基础版（version 字段 + 检测） | 完整迁移脚本 | 先做检测，完整迁移 M2.5 |
| 图 schema 扩展 | ❌ 推迟 | ✅ M3 | TypeAlias/Variable 不影响迁移顺序；Community 依赖 Leiden 无成熟 crate |
| FTS5 全文搜索 | ❌ 推迟 | ✅ M3 | <20K 行项目不需要跨字段搜索 |
| Leiden 社区检测 | ❌ 推迟 | ✅ M3 | 无成熟纯 Rust crate |
| CI/CD 集成 | ✅ 基础版 | 完整 dogfooding | M2 验收需 CI 就绪 |
| 规则治理工具化 | ❌ 推迟 | ✅ M3 | M2 规则量未达触发阈值 |
| 自定义 lint crate | ❌ 条件推迟 | 视 M2 规则统计 | M1 规则 <15 条，未触发 |
| /goal 自主循环 | ❌ 推迟 | ✅ M2.5 | 依赖状态机程序化 |
| 变异测试 | ❌ 推迟 | ✅ M2.5 | 验收不强依赖 |
| KNOWN_DIFFERENCES 自动生成 | ✅ | — | verifier 已有基础 |
| 完整错误码体系 | ✅ 基础版（P0 错误码） | 完整 30-40 条 | 先覆盖高频错误 |
| 等价深度扩展 | ✅ good/moderate 两级 | — | 小改动 |

---

## §2 未决设计决策（M2 启动前定夺）

### D1: done 终态语义

**矛盾**：02 §3.4 行 170（`degrade_* → translating 通过 --force`）暗示可重做 vs 09 附录 A 矩阵（done 无出边，硬终态）。

**推荐决策**：**done 是硬终态，不可 force 重做**。
- 理由：done 意味着通过了全部验证（Tier 0/1/2），重做会破坏已建立的质量保证
- 如果用户确实想重做，应通过 `graduate --rollback`（M3 功能）回退到 reviewing
- degrade_* → translating 的 `--force` 仅适用于降级状态，不涉及 done

**影响**：M2-ADV-03 graduate 命令以 done 为前置条件，无需处理 done→重做的分支。

### D2: blocked 进入规则

**矛盾**：02 §3.4 图「可从任意状态进入」vs 09 附录 A「仅活跃状态可进入 blocked」。

**推荐决策**：**仅活跃状态（pending/translating/compile_fixing/testing/reviewing/paused）可进入 blocked**。
- 理由：done/degrade_* 是终态或半终态，进入 blocked 无意义
- M1 代码 `can_transition_to()` 已按此实现

**影响**：无代码变更。更新 02 §3.4 图的措辞以消除歧义。

### D3: M2-SCALE 写隔离方案

> 决策记录：[MDR-003](decisions/003-m2-parallel-write-isolation.md)（含被否决方案 + 逃生口 + 审查过程）。

**矛盾**：PLAN P0-A.3 标注"两方案待定"，但 06 §10.5 已预设 worktree 方案。

**推荐决策（本次复审定稿，用户已批准）**：**git worktree + 约束包**——恢复 06 §10.5 原始设计，SubAgent 在 worktree 内做完整 crate 自检，git 冲突检测守卫共享文件编辑，编排器 merge 后整体 check 为唯一 done 真门。

> **决策演进（2026-06-15，两轮 codex 对抗审查）**：原 6-Agent 选「方案 B 隔离目录」→ 复审一度自造「隔离完整 crate 副本 + 自 check」修正版 → **用户三点质疑 + codex 第二轮对抗审查推翻该修正版**。结论：**Rust 编译单元是 crate 不是文件**，「隔离单模块 crate 副本」要么编译不过（引用兄弟模块符号），要么退化为复制整个 rust_root = 低配 worktree（无 git 去重/合并语义）。修正版还有结构性硬伤：① per-module 自检是**假信心**（对着注入接口 stub 单编过，merge 后真实类型不一致照崩）② orphan rule/coherence 冲突（E0119）、Cargo feature 合并冲突、proc-macro/derive 展开**只有整 crate 编译才能发现**——任何「隔离后合并」方案都有此盲区。worktree 装的是**完整 rust_root 快照**（所有 done 模块在场），自检是**真实完整 crate 检查**，且 git 冲突检测天然守卫共享文件编辑。最终回归原始设计 worktree。

**关键前提**：① M2 编排器是 SKILL.md（**LLM 驱动**，§11 程序化推迟 M2.5），上下文经济靠「SubAgent 写盘（其 worktree）+ 只回传摘要」满足——worktree 同样满足，与隔离目录无差。② `parallel_groups`=拓扑层、同层 import 独立（topo.rs:209-211），run.md:38 翻译前强制依赖全 done——故 worktree 里 done 依赖在场、并发兄弟独立缺席不影响编译。③ **输出 crate 结构与并行机制正交**：M2 沿用单 crate 输出；worktree 是并行机制，对单 crate / 未来 workspace 输出都适用。多 crate workspace 是**输出质量**议题（受 crate 间禁环约束、由目标架构决定），**非并行债**，不在 M2/M3 为并行而做。

- **方案要点（worktree + 约束包）**：
  1. **每 agent 一个 git worktree**：编排器从 rust_root（cargo init 已 git init）`git worktree add` 出 sprint N 各模块的工作树（含全部 done 代码）。SubAgent 在自己 worktree 内 `translate → cargo check（完整 crate 真自检）→ compile_fix → test`，保留 M1 per-module 循环；代码/rustc 错误/修复全程在子上下文，只回传 touched-list `{module, status:agent_done, own_files, shared_touched, self_check, test}`（详见下方通信协议）。
  2. **共享编辑策略 = D（porting-rule 最小化）+ A（自由改+轻协议）**（codex 第四轮验证排序 D>A>B>C，**否决原「禁止+回传批准」**——它破坏自检+制造同步 round-trip，且 worktree+merge 本就解决 silent 覆盖）：
     - **D 最小化共享写面（porting 规则）**：并行模块**优先用既有共享 API + `Error::Other(String)`/`anyhow` 逃生口，不擅自扩展共享类型**；真正的复杂共享 API 扩展**留串行 cleanup**。减少共享写面比事后智能合并可靠（编排器是非确定性 LLM）。
     - **A 执行底座 + 5 条最小协议**：SubAgent 在 worktree 内**自由改任意文件（自检必需）**，回传 `{own_files, shared_touched}`（代码留盘）；**(协议1) 状态只能标 `agent_done`（substatus），不得标最终 `done`**；**(协议3) 禁止删除/改签名既有共享 API，新增只允许 append**；其余协议见要点 4/§7。
     - ⚠️ **codex 纠正**：共享编辑「文本冲突可控，但语义冲突真实存在」——尤其**孤儿规则逼 `From/Display/Serialize` impl 落进共享类型所在文件**（单 crate + 共享 Error 下不罕见），叶子优先只能压低频率、不能消除。
  3. **dependency-mapping 前置强约束生态选择**：`dependency-mapping.md` 须前置规定唯一生态（anyhow xor thiserror、异步运行时唯一、序列化库唯一等），防止并发模块各引不同 crate 产出 Frankenstein Cargo.toml。new_deps 合并须**并 feature 集 + 校验 default-feature 不冲突**（非仅版本取高）；**(协议2) Cargo.lock 以合并后重新 `cargo`-解析/验证为准**。
  4. **两层 done + merge 后整体 check 为唯一真门**：worktree 自检过 = `agent_done`（substatus，**非终态**）；编排器装配（要点「结构化合并」）→ **整组 `cargo check`/`cargo test` 过才置最终 `done`**（兜住 orphan/coherence/feature/宏/命名空间 等只能整组编译暴露的冲突）。**(协议4) git 冲突不由 LLM 编辑**，走串行 rebase 重译。
  5. **reconcile 机制 + 轮次上限（防活锁）**：非结构化共享 `.rs` git 冲突 → **串行 reconcile**：冲突模块按依赖序逐个 rebase 到已合并主线后**重译该模块**（非编排器手解冲突块）。**(协议5) reconcile 设轮次上限**（默认同 `max_retry_rounds=3`）；超限 → **该 sprint 降级为串行翻译 / 转人工 review**（杜绝 A合并→B重译又改共享→C重译 的活锁）。
  6. **图缺陷回退串行 + 修图**：整体 check 若暴露「同层模块间本不该有的跨模块引用」（REFAC-10 档1 图不完整漏边所致），判为**图缺陷**——对相关模块回退串行翻译，并记录待修图（REFAC-10 档2，M3）。
- **装配是结构化合并,非纯 copy-out**（codex 纠正）：各模块新增 `.rs` 复制进 rust_root（同名 helper/trait/`foo.rs` vs `foo/mod.rs` 布局碰撞 → 当冲突处理）；`Cargo.toml`（deps+feature 集）/`lib.rs`（mod 声明）必须**程序化结构合并**（toml_edit / 语法级 union）；其余共享 `.rs` git merge，冲突→要点 5 串行 reconcile。
- **进度保证（无死锁，最坏退化串行）**：① 翻译期 agent 各在独立 worktree、不持共享锁、互不等待 → 无循环等待土壤 ② reconcile 串行全序 + 轮次上限 → 无活锁 ③ 仅编排器写共享状态（state.json/主 rust_root）→ 无写竞争。**卡住的模块**（自检/整体 check 3 轮不过）→ pause→degrade（headless 由 ADV-07 自动 degrade=ffi/skip 达终态，**依赖 ADV-07 改状态机，见下**）；同 sprint 其他模块照常完成，仅 sprint 推进等其达终态；下游由 auto-unblock（M2-CLI-06）解阻塞。**最坏情况 = 全 sprint 退化串行 = M1 速度，慢但不卡死。**
- **headless 不挂起依赖状态机改动**（codex 指出）：02/09 现仍写「3 轮失败→paused→人工确认 degrade」。「headless 不挂起」要 **M2-ADV-07 真正把 paused→自动 degrade 写进状态机**（02 §3.4 + 09 附录 A），否则该保证缺文档依据 → ADV-07 须同步改设计文档（并入 DESIGN-03 校对范围）。
- **sprint barrier 精确定义**（codex 指出缺失）：终态 = `done`/`degrade_*`；`agent_done`/merge-pending/reconcile-pending 均为**非终态**（不触发推进）；失败 worktree 丢弃后模块置 paused（非终态，阻塞本 sprint 推进直至 degrade）。
- **target dir 策略（实现期，我已定）**：先各 worktree **独立 `CARGO_TARGET_DIR`** 保正确（避免并发 cargo 锁争用），Sprint F 实测启动/编译开销，超标再用 sccache / 共享 target 优化。
- **回滚与一致性（补 P9/P10）**：worktree 隔离使「回滚一个模块」= 丢弃其 worktree（不污染主树）；partial-batch 失败下，编排器只对**整体 check 通过的模块**置 done 并单写 migration-state.json，未过模块退回 translating/paused，state.json 与 graph migration_status 由编排器同一事务序更新。
- **数据缺口与逃生口（诚实标注）**：worktree 优于「轻量 staging（agent 不自检、全部错误堆 merge 后修）」的核心论据是「M1 是 per-module 编译反馈循环、首轮普遍带编译错误」——但 codex 复核指出**该频率 M1 未留痕**（migration-state/attempts 未入库），属合理推断非实证。故 **Sprint F 必测两项**：① 首轮（worktree 内自检前）编译通过率 ② worktree target 冷编/锁开销。**若实测首轮通过率高 且 target 成本高 → 记录「降级为轻量 staging」为已论证的简化路径**（staging：agent 只翻译不自检 + 编排器整体 check + 串行修复 + 越界/碰撞拒绝）。即「先上 worktree 保 M1 反馈环，用数据决定是否可简化」，而非反向赌首轮干净。

**影响**：
- M2-SCALE-02 = 「git worktree 编排 + worktree 内完整自检（agent_done）+ 共享编辑 D+A（porting 最小化 + 自由改 + touched-list 回传 + 禁删/改签名既有 API）+ dependency-mapping 前置 + 结构化合并 + 共享 .rs 冲突串行 reconcile（轮次上限）+ merge 后整组 check 唯一 done 真门 + 图缺陷回退串行」（§7 同步重写）
- 设计文档：04 §5.7.3 `shared_db_path`（只读）仍适用；08 §M2 / 06 §10.5「多 agent 工作区=独立 worktree+merge 前冲突检测」**本就如此，无需改措辞**（修正版才偏离，现回归）——M2-DESIGN-03 仅需补「集中 writer（D5）」与 dependency-mapping 前置约束
- M2-SCALE-LOCK：编排器持锁不变

### D4: NodeData.complexity vs TIER-01 分档

**现状**：两套分级体系未关联——04 §5.7.1 的 `Simple/Moderate/Complex` 按 LOC，M2-TIER-01 的 `trivial/standard/full` 按语义特征。

**推荐决策（本次复审定稿）**：**新增 `tier`，删除死字段 `risk`，保留 `complexity`；不引入持久化 `tier_signals`**。
- `complexity`（Simple/Moderate/Complex）保留原有 LOC-based 语义，用于统计/展示（persist.rs 有读写点，活字段）
- `ModuleState` 新增 `tier: Option<ModuleTier>`（Trivial/Standard/Full），由 AST 语义特征驱动，决定翻译策略
- **分档理由可观测但不进 schema**：判此档的危险信号（如 `["async","try-catch"]`）写入 run 日志 + `AttemptRecord`，**不**新增持久化 `ModuleState.tier_signals` 字段。理由：tier_signals 为复审一度自造、主干代码与设计文档**零先例**（codex 复核 `rg tier_signals` 无命中），作为持久化 schema 字段属过度设计、徒增向后兼容面；「失败自动升档时定位原因」用日志即可满足。
- **删除 `ModuleState.risk: RiskLevel` + `default_risk()`**：codex 复核确认零读取点（`rg '\.risk\b'` 无任何判断/分支/输出命中），M1 起恒 `Low` 死字段。**注意删除非「直接删」**，需同步：CLI 构造点（lib.rs:962）、machine.rs/coverage.rs 测试构造、插件文档 `risk:low` 表述（06-plugin-structure.md:119、analyze.md:37）、09 schema 口径。该清理**并入 M2-TIER-01a**（同一处由「填 risk」改为「填 tier」，自然移除 risk）。
- 理由：complexity 不动（避免 source-graph.db 迁移）；risk 是 state.json 运行时字段（可重建），删除无数据迁移代价。
- 设计文档需补充 TIER-01 分档定义章节（建议 03 或 04）——并入 M2-DESIGN-03。

### D5: SQLite 并发模型与验收口径（本次复审新增）

**矛盾**：08 §M2（line 169/181/182/187）+ 04 §5.7.3（line 393-399）以「多 agent 共享 `source-graph.db` 并发写 + WAL 串行化」为 M2 默认架构，并把「SQLite 冲突率<10% 且锁等待≤20ms」列为 M2→M3 硬性升级判据(c)。但 D3 选定 worktree 方案（编排器单写 `migration-state.json` + SubAgent 在 worktree 内只读 graph、不直写 db），**并发写场景不存在**，该门禁失去测试对象。复审草稿直接删 WAL 测试，却留下 §1.2/§1.3/F5/Level5 仍要求该项的自相矛盾。

**决策（用户已批：降级为 WAL 配置回归）**：
- 翻译期 `source-graph.db` 只读：唯一写入口为 `graph build → save_to_db`（lib.rs:585 / persist.rs:25）；run 期状态走 `state transition` 写 `migration-state.json`，由编排器单写（codex 确认无 SubAgent 直写 graph 路径）。
- 保留**一条 WAL 配置回归测试**：断言 `PRAGMA journal_mode=WAL` + `busy_timeout` 连接配置正确（防御未来回退并发写模式时配置丢失），**移除「冲突率/锁等待」量化门禁**。
- M2→M3 升级判据(c) 改为「集中 writer 模型，SQLite 并发写门禁 N/A；仅保留 WAL 配置回归」。
- 同步更新设计文档 04 §5.7.3 / 08 §M2：标注「D3 worktree 下编排器集中 writer 取代原共享 DB 并发写架构；原 WAL 并发写策略保留为未来 SubAgent 直写 db 模式的可选回退」——并入 M2-DESIGN-03。

**影响**：PETGRAPH-01 = petgraph 副本隔离验证 + WAL 配置回归（非并发压测）；§1.2/§1.3/F5/Level5 已同步降级。

---

## §3 Sprint 结构

M2 分为 6 个 Sprint，预计 4-5 周：

```
Sprint A (基础加固)      ▓▓░░░░░░░░░░░░░░░░░░░░░░  2-3 天
Sprint B (类型+图精度)   ░░▓▓▓▓▓░░░░░░░░░░░░░░░░░  5-6 天
Sprint C (核心功能双线)  ░░░░░░░▓▓▓▓░░░░░░░░░░░░░  4-5 天
Sprint D (并行+高级)     ░░░░░░░░░░░▓▓▓▓▓▓▓░░░░░░  7-9 天
Sprint E (验证+CLI扩展)  ░░░░░░░░░░░▓▓▓▓░░░░░░░░░  3-4 天（与D并行）
Sprint F (M2 验收)       ░░░░░░░░░░░░░░░░░░▓▓▓▓▓▓  7-10 天
                         ├── 关键路径 ~25-33 天 ──────┤
                         ├── 日历约 5-7 周（含 PR 审查间隙）──┤
```

### 关键路径

```
路径 A（自适应循环）: A(VER-04:1d) → B(REFAC-09:1d) → C(TIER-01:3d + ADV-07:2d) → F(7-10d)
  总长: ~14-18d（TIER-01 解除对 REFAC-10 的依赖后缩短 3-4d；REFAC-10 仍在 Sprint B 但不阻塞 TIER-01）

路径 B（并行 sprint，关键路径）: A(REFAC-06:0.5d) → C(SCALE-P:1.5d → SPRINT:1d) → D(SCALE-02:5-7d → SCALE-01:2-3d) → F(7-10d)
  总长: ~17-23d

关键路径 = max(A, B) + Sprint E 并行 ≈ 25-33d（纯开发时间，不含 PR 审查间隙）
```

> 注：SCALE 编号沿用 PLAN.md 原始编号，实际执行序为 SCALE-02 → SCALE-01（先写隔离再 Workflow 编排）。

---

## §4 Sprint A：基础加固（2-3 天）

**目标**：清理 M1 技术债，为后续功能提供可靠基础。解决未决设计决策。

### 任务清单

| 任务 ID | 内容 | 文件 | 工作量 | 依赖 |
|---------|------|------|--------|------|
| M2-DESIGN-01 | 统一 done 终态语义（更新 02 §3.4 图 + 09 附录 A 措辞） | docs/design/ | 0.5d | — |
| M2-DESIGN-02 | 统一 blocked 进入规则（更新 02 §3.4） | docs/design/ | 0.5d | — |
| M2-VER-04 | populate 数据卫生：孤儿 pending 清理 + docstring 修正 + record-subagent-call 无 init e2e | machine.rs, cli e2e | 1d | — |
| M2-VER-05 | Timestamp ISO8601 校验：非法格式返回明确错误 | common.rs, machine.rs | 0.5d | — |
| M2-REFAC-05 | parse_node_extra 错误可见化 | persist.rs | 0.5d | — |
| M2-REFAC-06 | MigrationSequence 字段私有化 + getter | topo.rs, 调用方 | 0.5d | — |
| M2-REFAC-07 | StateMachine.load() 后置校验 | machine.rs | 0.5d | — |
| M2-REFAC-13 | ToolStatus 枚举化 + LocReport 派生 totals | types/state.rs, stats/ | 0.5d | — |
| M2-REFAC-14 | ErrorData structured context（details: Option<Value>） | response.rs, lib.rs:659 | 0.5d | — |
| M2-COMPAT-01 | migration-state.json 版本检测基础：init 时写入 version 字段 + validate state 版本检查 | machine.rs, validate/ | 1d | — |
| M2-ADV-06 | stats compare 非占位实现（tokei + tree-sitter 函数计数结构对比） | lib.rs:1143, stats/ | 1d | — |
| M2-CTX-01 | 上下文预算实证校验：用真实项目实测 02 §3.5.1 预算表准确度，结果反馈规则库改进（08 §M2 标注前 2 周优先） | docs/design/, DESIGN_ASSUMPTIONS.md | 1d | — |
| M2-DESIGN-03 | 设计文档架构同步：04 §5.7.3 / 08 §M2 并发写架构 → D3 worktree 下编排器集中 writer（D5）+ **06 §10.5 worktree 措辞本就正确无需改**，仅补「dependency-mapping 前置生态约束 + 共享编辑 D+A（porting 最小化 + 禁删/改签名既有 API）+ 两层 done + reconcile 轮次上限」（D3 约束包）+ 补 TIER-01 分档定义章节（D4） | docs/design/ | 0.5d | — |
| M2-PERF-BASE | M1 性能基线快照：fixture `graph build` 时长 + 单模块翻译时长 → baseline 文件，供 F6「≤±10%」对比（无基线则 F6 不可测） | docs/, 脚本 | 0.5d | — |

### 并行策略

- DESIGN-01/02 + VER-04/05 + REFAC-05/06/07/13/14 全部独立，可 3 路并行
- ADV-06 和 COMPAT-01 独立于上述任务

### 完成标志

- [ ] 2 个设计决策写入 docs/decisions/
- [ ] populate 孤儿清理 + 幂等测试通过
- [ ] Timestamp 非法格式返回 `INVALID_TIMESTAMP` 错误
- [ ] stats compare 非占位，输出源/Rust 结构对比 JSON
- [ ] migration-state.json 含 `version` 字段，validate state 检查版本
- [ ] M1 性能基线快照落盘（M2-PERF-BASE），供 F6 ≤±10% 对比
- [ ] 设计文档架构同步完成（M2-DESIGN-03：D3/D4/D5 影响项）
- [ ] `just ci` 全绿 + M1 fixture 回归通过

---

## §5 Sprint B：类型安全 + 图精度提升（5-6 天）

**目标**：完成核心类型重构，提升图构建精度（TIER-01 分档的数据基础）。

### 任务清单

| 任务 ID | 内容 | 文件 | 工作量 | 依赖 |
|---------|------|------|--------|------|
| M2-REFAC-01 | SourceNode 构造器 + pub(crate) 封装 | types/graph.rs, build.rs, 所有构造点 | 1d | — |
| M2-REFAC-02 | ImportInfo 枚举化（ImportKind + SymbolKind enum） | lang/mod.rs, build.rs | 1d | — |
| M2-REFAC-03 | sub_kind 类型化 → EdgeSubKind enum | types/graph.rs, build.rs | 0.5d | — |
| M2-REFAC-04 | migration_status/rust_kind → enum | types/graph.rs, persist.rs | 0.5d | — |
| M2-REFAC-09 | Arrow function 提取（lexical_declaration 处理） | lang/typescript.rs | 1d | REFAC-01 |
| M2-REFAC-10 | 跨文件方法调用解析（档 1：唯一方法名连边 + 局部 receiver 绑定） | graph/build.rs | 3-4d | REFAC-09 |
| M2-REFAC-11 | fixup_extends 名称索引（HashMap 替代 O(N)） | graph/build.rs | 0.5d | — |
| M2-REFAC-12 | walk_ast class 递归（嵌套 dynamic import 捕获） | lang/typescript.rs | 0.5d | — |
| M2-REFAC-15 | module key 人类友好归一化（`--human` 显示映射） | types/state.rs, lib.rs | 0.5d | — |

### 并行策略

- **Wave 1（独立）**：REFAC-01、REFAC-02、REFAC-03、REFAC-04、REFAC-11、REFAC-12、REFAC-15 全部独立
- **Wave 2（串行）**：REFAC-09 → REFAC-10（依赖完整函数节点）
- Wave 1 和 Wave 2 可并行推进

### 回归验证

所有 REFAC 完成后必须跑：
```bash
just ci                    # 全量 CI
cargo nextest run          # 202 测试回归
# fixture Live 验证（至少 linear + diamond）
```

### 完成标志

- [ ] 所有 `Option<String>` 类型化完成（sub_kind/migration_status/rust_kind）
- [ ] SourceNode 构造器强制使用，外部无法构造非法组合
- [ ] ImportInfo 5 种非法布尔组合在编译期消除
- [ ] Arrow function `export const f = () => {}` 生成 Function 节点
- [ ] 跨文件方法调用档 1 上线（唯一方法名连边 + 局部 receiver）
- [ ] `just ci` 全绿 + ground-truth 偏序约束 100% 满足

---

## §6 Sprint C：核心功能双线（4-5 天）

**目标**：同时推进自适应循环和并行 sprint 基础，两线独立互不阻塞。

### 线 1：复杂度自适应循环

| 任务 ID | 内容 | 文件 | 工作量 | 依赖 |
|---------|------|------|--------|------|
| M2-TIER-01a | CLI：per-module 复杂度评估 + ModuleState.**tier** 自动填充（同处**删除死字段 risk/default_risk**，见 §2 D4） | detect.rs, machine.rs, types/state.rs | 1d | Sprint B **REFAC-09**（仅需 arrow function 节点；分档为模块内 AST 特征，**不依赖**跨文件 REFAC-10） |
| M2-TIER-01b | analyzer.md 扩展：per-module 语义信号输出 | plugin/agents/analyzer.md | 0.5d | TIER-01a |
| M2-TIER-01c | run.md 分档逻辑：trivial/standard/full 循环路径 | plugin/skills/migrate/run.md | 1d | TIER-01a |
| M2-TIER-01d | 降档/升档机制 + 可观测日志 | run.md, state machine | 0.5d | TIER-01c |
| M2-ADV-07 | 默认 TODO 决策策略（headless 模式）+ **headless 下 paused→自动 degrade 写进状态机**（02 §3.4 + 09 附录 A，否则 D3「不挂起」缺文档依据，codex 第四轮指出） | run.md, translator.md, machine.rs, docs/design/ | 1-2d | TIER-01c |

**M2-TIER-01 分档判据**：

| 档位 | 判据（AST 语义特征） | 循环 | 测试 |
|------|---------------------|------|------|
| **trivial** | 纯类型文件（仅 interface/type/enum 导出）或常量文件或 barrel（仅 re-export） | 批量直翻 + 编译 + 签批，跳 Phase B | 仅验编译 + 导出可见性 |
| **standard** | 无以下任何危险信号的普通模块 | 保留意图摘要 + Phase A + 审查 + 测试 | 语义等价测试 |
| **full** | 含任一危险信号：副作用 I/O / 并发(Promise.all/async) / 错误路径(try-catch) / 数值计算 / 全局状态 / 动态类型操作 / 条件类型/泛型约束 / unknown/never 不可判定 | 完整 11 步 | 完整 Tier 0+1+2 |

**关键原则**：
- 判据基于 **AST 可见语义特征**，非 LOC
- 任一危险信号或 unknown → 不降档（默认 full 兜底）
- 短 ≠ 低风险（`utils.clamp` 几行但含 NaN 陷阱 → full）
- **维度 9 意图一致性永不跳过**
- 降档可观测（state 记录分档结果）+ 失败自动升档重跑

**trivial 档的维度 9 退化形式**（消除「trivial 跳 Phase B / 仅验编译」与「维度 9 永不跳过」的表面冲突）：trivial（纯类型 / 常量 / barrel）无运行时行为可比对，其维度 9 **退化为「导出符号 + 类型签名集合一致性」核对**——源文件导出的 `interface/type/enum/const` 名称与签名 ⇿ Rust 侧 `pub` 类型/常量逐项对应，而非完整 7 字段语义契约（trivial 不生成完整 intent.md）。维度 9 的「契约核对」本质保留（核对类型契约而非行为契约），故「永不跳过」成立；standard/full 仍为完整 7 字段核对。

### 线 2：并行 sprint 基础

| 任务 ID | 内容 | 文件 | 工作量 | 依赖 |
|---------|------|------|--------|------|
| M2-SCALE-P | populate 改消费 parallel_groups（组索引→sprint 号）+ 增加 `--single-sprint` flag 兼容 M1 回归 | machine.rs, lib.rs | 1.5d | Sprint A REFAC-06 |
| M2-SCALE-SPRINT | sprint 推进：sprint N 全终态→current=N+1 + history 回填。**必须与 SCALE-P 同 PR 交付**（避免 populate 改了但推进没做的窗口期） | machine.rs | 1d | SCALE-P |
| M2-ADV-03 | graduate 命令：项目级毕业评估（所有模块 done/degrade_* 时可执行）+ graduation-report.json 产出 + 源文件归档标记。**不新增 `graduated` 模块状态**（设计文档 09 附录 A 将 GRADUATE 映射为项目级概念） | lib.rs, SKILL.md | 1d | Sprint A DESIGN-01 |

### 并行策略

- 线 1 和线 2 **完全独立**，可同时推进
- 线 1 内部：TIER-01a → TIER-01b/c → TIER-01d → ADV-07
- 线 2 内部：SCALE-P → SCALE-SPRINT（ADV-03 独立）

### 完成标志

- [ ] trivial 模块批量翻译跳过 Phase B，编译通过即 done
- [ ] full 模块行为等价于 M1 现有 11 步循环
- [ ] 降档/升档日志可观测（state 记录 `tier: trivial/standard/full`）
- [ ] headless 模式撞 TODO(port) 按 safe-default 自动决策
- [ ] populate 按 parallel_groups 分配 sprint（sprint=1 为叶节点组）
- [ ] sprint 推进逻辑测试通过
- [ ] graduate 命令产出 graduation-report.json + 源文件归档标记（**项目级毕业，不新增 `graduated` 模块状态**，见 ADV-03）
- [ ] M1 fixture 回归通过（full 档行为不变）

---

## §7 Sprint D：并行编排 + 高级功能（7-9 天）

**目标**：完成并行翻译全栈（核心 M2 价值），补齐高级功能。

### 并行编排（关键路径）

| 任务 ID | 内容 | 文件 | 工作量 | 依赖 |
|---------|------|------|--------|------|
| M2-SCALE-02 | 写隔离实现（**D3 worktree + 约束包**）：git worktree 编排 + worktree 内完整自检（`agent_done`）+ 回传 touched-list（不回传代码）+ **porting 规则最小化共享写面**（用既有 API/逃生口）+ dependency-mapping 前置 + 结构化合并（toml/lib.rs union）+ 共享 .rs 冲突**串行 reconcile（轮次上限）**+ **整组 check 唯一 done 真门** + 图缺陷回退串行 | run.md, translator.md, SKILL.md, scaffold/, machine.rs | 5-7d | Sprint C SCALE-SPRINT |
| M2-SCALE-01 | Workflow 批量翻译：Agent tool 并发编排 + 错误恢复 | run.md, Workflow 定义 | 2-3d | SCALE-02 |
| M2-SCALE-LOCK | 全局锁改造：编排器持锁，SubAgent 不取锁 | run.md, SKILL.md | 0.5d | SCALE-01 |
| M2-PETGRAPH-01 | petgraph 副本隔离验证（SubAgent 各持独立图，无共享内存竞争）+ **WAL 配置回归**（断言 `journal_mode=WAL` + `busy_timeout` 配置正确，**非并发压测**）——翻译期 db 只读、state.json 编排器单写，无并发写场景（见 §2 D5） | 测试代码 | 0.5d | SCALE-01 |

**写隔离实现方案（D3 worktree + 约束包 决策执行）**：

**通信协议（codex 第四轮验证：D+A 组合，无声明式 schema）**：agent 不在翻译中途阻塞等批准——在 worktree 完整翻完（含它需要的共享改动），再一次性回传 touched-list；调和事后做、仅真冲突升级串行。

```
① 派发  编排器 → agent
    git worktree add .wt/{module}（从主 HEAD，含全部 done 代码，独立 CARGO_TARGET_DIR）
    派 SubAgent，CWD=worktree；注入 intent + 规则 + 依赖接口
    + porting 规则【最小化共享写面】：优先用既有共享 API；不够时用 Error::Other/anyhow 逃生口；
      禁删除/改签名既有共享 API，新增只 append；复杂共享扩展标记留串行 cleanup

② 翻译+自检  (全程在 agent 的 worktree，隔离)
    写 src/{module}.rs；按需 append 式改共享文件
    cargo check（完整 crate 真自检）→ fix 循环（M1 的 3 轮）→ test

③ 回传  agent → 编排器  (只回 touched-list，代码留盘=上下文经济)
    { module, status: agent_done,                       // 协议1：非终态
      own_files: ["src/parser.rs"],
      shared_touched: ["src/error.rs", "Cargo.toml"],   // 仅文件清单，无声明式 op
      self_check: pass, test: pass }

④ 合并  编排器（结构化为主）
    own 文件        → 复制进主 rust_root（同名/布局碰撞→当冲突）
    Cargo.toml/lib.rs → 结构化 union（toml_edit / mod 追加）；Cargo.lock 合并后重解析  // 协议2
    其他共享 .rs    → git merge

⑤ reconcile（仅 git 冲突时，协议4/5）
    冲突模块按依赖序逐个：worktree rebase 到已合并主线 → 重译该模块（非 LLM 手解冲突块）
    轮次上限（默认 3）；超限 → 本 sprint 降级串行 / 转人工 review

⑥ 真门  整组 cargo check / cargo test    // 协议1：唯一最终 done
    过 → agent_done 升 done + 编排器单写 state（state + graph migration_status 同序）
    不过 → compile_fixing 子流程（见下方）

⑦ 清理  git worktree remove
```

> **两层 done**：worktree 自检过 = `agent_done`（substatus，非终态，沿用 phase_*_complete 那套 substatus 模式，不新增顶层状态）；只有 ⑥ 整组 check 过才升最终 `done`。orphan rule/coherence(E0119)、feature 冲突、宏展开、命名空间撞名等**跨并发兄弟冲突只能整组编译暴露**，故 ⑥ 是唯一真门。
> **为何不要声明式 shared_edits schema**（codex）：schema 追不上 Rust 共享语义（impl 放置 / feature / pub use 命名 / coherence），膨胀且收益低。touched-list + 结构化合并器（toml_edit / mod union）+ git merge 已够。

**compile_fixing 子流程（merge 后整体 check 失败）**：
```
merge 后整体 cargo check 失败
  ├── 编排器解析 rustc 错误，分类：
  │   ├── 单模块本地错误（可归因到某模块文件）→ 该模块回 compile_fixing
  │   ├── 跨模块冲突（E0119 coherence / feature / 类型不一致）→ 相关模块整组回 compile_fixing
  │   └── 意外跨模块引用（同层本不该依赖）→ 判【图缺陷】：相关模块回退串行 + 记录待修图(REFAC-10档2/M3)
  ├── 对失败模块重启 SubAgent（在其 worktree 内）：收 rustc 错误 → 修复 + 自 check → 回传摘要
  ├── 编排器重新 merge → 再次整组 check
  ├── 最多重试 3 次（M1 max_compile_retries）
  └── 3 次失败 → 丢弃该模块 worktree、标 paused，继续其他模块（worktree 隔离=回滚不污染主树）
```

### 高级功能

| 任务 ID | 内容 | 文件 | 工作量 | 依赖 |
|---------|------|------|--------|------|
| M2-ADV-02 | 降级 FFI：基于 napi-rs 的 FFI binding 生成 + degrade_ffi 状态处理 + 环断点选择算法 | translator.md, machine.rs, scaffold/ | 3-4d | — |
| M2-ADV-04 | graph build --profile 性能画像 JSON | lib.rs:578 | 0.5d | — |
| M2-ADV-05 | graph interfaces --deps-of 批量输出 | lib.rs:718 | 0.5d | — |
| M2-ADV-08 | profile 自动定位 analysis-tools.json（CLAUDE_PLUGIN_ROOT） | profile/, SKILL.md | 0.5d | — |
| M2-ADV-09 | 完整子进程超时（tokio timeout / nix kill） | 相关进程调用点 | 0.5d | — |
| M2-ADV-10 | [persistence] 配置段实装（backup_on_write/retention_days） | machine.rs, config.rs | 0.5d | — |
| M2-ADV-01 | 多候选生成：同模块 ≥2 种翻译方案 + verifier 选优 | translator.md, verifier.md | 2d | Sprint C TIER-01c |

### 并行策略

- SCALE-02 → SCALE-01 → SCALE-LOCK → PETGRAPH-01 串行（关键路径）
- ADV-02/04/05/08/09/10 全部独立，可与 SCALE 并行
- ADV-01 需等 TIER-01c（分档逻辑完成后在 standard/full 档执行）

### 完成标志

- [ ] 3 agent 并行翻译 diamond fixture 5 模块无冲突
- [ ] 写隔离 merge 逻辑通过测试
- [ ] 降级 FFI 在 circular-deps fixture 上成功
- [ ] petgraph 副本隔离验证 + WAL 配置回归（非并发压测，见 §2 D5）
- [ ] headless 全流程（init→analyze→run→review）无人工干预可完成

---

## §8 Sprint E：验证增强 + CLI 扩展（3-4 天，与 Sprint D 并行）

**目标**：增强测试覆盖，实现 5 个 CLI 新命令。

### 验证增强

| 任务 ID | 内容 | 文件 | 工作量 | 依赖 |
|---------|------|------|--------|------|
| M2-VER-01 | proptest 属性测试：图操作不变量 | graph/ 测试 | 1d | — |
| M2-VER-02 | cargo-fuzz：解析器健壮性 | fuzz/ 目录 | 1d（+24h fuzz） | — |
| M2-COV-01 | 覆盖率门禁：cargo-llvm-cov 集成 + ≥70% target | CI 配置 | 0.5d | — |
| M2-SCALE-03 | 增量图更新：file_fingerprints 跳过未变更文件 | build.rs, persist.rs | 2d | — |

### CLI 扩展（设计文档 06 §10.0.1 已定义的 5 个命令）

| 任务 ID | 内容 | 文件 | 工作量 | 依赖 |
|---------|------|------|--------|------|
| M2-CLI-01 | graph rdeps：反向依赖查询 | lib.rs, query.rs | 0.5d | — |
| M2-CLI-02 | graph cycles：完整 SCC 环检测 + 环路径输出 | lib.rs, topo.rs | 0.5d | — |
| M2-CLI-03 | graph export：导出为 JSON/DOT/Mermaid | lib.rs, 新 export.rs | 1d | — |
| M2-CLI-04 | validate config：校验 .rustmigrate.toml | lib.rs, validate/ | 0.5d | — |
| M2-CLI-05 | state update：乐观锁状态更新（--cas-version CAS） | lib.rs, machine.rs | 1d | — |
| M2-CLI-06 | `validate state --check-blocked --auto-unblock`：DFS 环检测 + 拓扑逐层自动解除 blocked（`blocked_by` 全部 done/degrade_* 则恢复 `pre_blocked_status`）+ 环路径输出。**从推迟的「状态机程序化」抽出**——08 §M2 line 162 + 09 line 228 承诺 M2 交付；并行 sprint 解阻塞必需 | lib.rs, validate/, machine.rs | 1-2d | — |

### 其他

| 任务 ID | 内容 | 文件 | 工作量 | 依赖 |
|---------|------|------|--------|------|
| M2-ERR-01 | P0 错误码枚举化（~15 条高频错误码 + CI 重试建议） | error.rs, 09-appendix | 1d | — |
| M2-PARITY-01 | PARITY.md 等价深度扩展 + KNOWN_DIFFERENCES 自动生成 | verifier.md | 0.5d | — |
| M2-CICD-01 | CI/CD 基础：GitHub Actions workflow + dogfooding fixture | .github/, fixtures/ | 1d | — |

### 并行策略

- Sprint E 与 Sprint D **完全并行**
- VER 系列、CLI 系列、ERR/PARITY/CICD 互相独立

### 完成标志

- [ ] proptest 1000 次无 panic
- [ ] cargo-fuzz 24h 无 crash
- [ ] 覆盖率 ≥70%
- [ ] 6 个 CLI 命令可用 + 测试覆盖（含 auto-unblock 拓扑解除 + 环路径输出）
- [ ] 增量图更新：变更 1 文件 < 全量构建时间 50%
- [ ] P0 错误码枚举化完成
- [ ] CI workflow 在 PR 上自动运行

---

## §9 Sprint F：M2 验收（7-10 天）

**目标**：在 3 个真实 TS 中型项目上验证 M2 全部功能。

### 验收流程

1. **F1 项目选取 + 环境准备**（1-2 天）：
   - 3 个公开 TS 项目，5K-20K 行
   - 至少 1 个含 15-25 依赖的模块（覆盖中段上下文预算）
   - 至少 1 个含循环依赖（验证降级 FFI）
   - 环境搭建 + 初始 analyze 通过

2. **F2 自适应循环验证**（1-2 天）：
   - trivial 模块批量翻译正确
   - standard 模块翻译 + 测试通过
   - full 模块完整循环通过
   - 降档/升档机制触发并正确执行

3. **F3 并行 sprint 验证**（2-3 天）：
   - 3 agent 并行翻译（worktree 隔离），merge 后整体 check 无冲突
   - sprint 自动推进
   - 性能门禁：≥1.5 模块/小时
   - **D3 决策实测（worktree vs staging 逃生口数据，见 §2 D3）**：① 首轮（worktree 内自检前）编译通过率 ② worktree target 冷编/锁开销。若首轮通过率高且 target 成本高 → 记录「降级轻量 staging」简化路径

4. **F4 质量验证**（1 天）：
   - proptest + fuzz 在验收项目上通过
   - 翻译膨胀率 <3.0x
   - cargo check + clippy 零 error
   - 覆盖率 ≥70%
   - 翻译正确性：人工审阅 + verifier 对抗审查覆盖

5. **F5 修复 + 复验**（2-3 天缓冲）：
   - 真实项目暴露的 AST 边界问题修复
   - 性能门禁未达标时的调优
   - 回归验证：M1 fixture 全通过 + 202 + M2 新增测试全绿

### 验收标准（完整版）

| # | 指标 | 标准 | 来源 |
|---|------|------|------|
| F1 | 规模 | 3 个 5K-20K 行项目，每项目 ≥3 模块完成 | PLAN §10 |
| F2 | 降级 | ≥1 模块降级 FFI 成功 | PLAN §10 |
| F3 | 并行吞吐 | P50 ≥1.5 模块/小时（3 agent） | 08 §M2 |
| F4 | P99 吞吐 | P99 ≥0.8 模块/小时 | 08 §M2→M3 升级判据 |
| F5 | SQLite WAL 配置回归 | `journal_mode=WAL` + `busy_timeout` 配置正确（集中 writer，**无并发量化门禁**） | §2 D5 |
| F6 | 性能无退化 | 单模块翻译时长 + graph build 时长 相对 M1 基线 ≤±10%（基线见 M2-PERF-BASE） | 08 §M2 |
| F7 | 测试质量 | proptest 1000 无 panic + fuzz 24h 无 crash | PLAN §10 |
| F8 | 翻译膨胀 | <3.0x | 08 §M2 |
| F9 | 全流程耗时 | 单模块 full 档完整循环 <60min（多模块由 F3 吞吐覆盖） | §2 D5 |
| F10 | 覆盖率 | ≥70% | 08 §M2 |
| F11 | 图构建 | <10s（500 文件） | PLAN §10 |
| F12 | 回归 | M1 202 测试 + fixture 全通过 | 新增 |

---

## §10 任务全量索引

### 任务总数：55 项（+ 5 项验收活动）

> 注：M2-REFAC-08（ModuleStatus 转换表 can_transition_to）已在 M1 完成，不纳入 M2 任务。
> 本次复审新增 3 项：M2-DESIGN-03、M2-PERF-BASE（Sprint A）、M2-CLI-06 auto-unblock（Sprint E）。

| Sprint | 任务数 | 估时 |
|--------|--------|------|
| A 基础加固 | 14 | 2-3d |
| B 类型+图精度 | 9 | 5-6d |
| C 核心功能双线 | 8 | 4-5d |
| D 并行+高级 | 11 | 7-9d |
| E 验证+CLI | 13 | 3-4d（与D并行） |
| F 验收 | 5 项验证 | 7-10d |
| **合计** | **55+验收** | **~25-33d** |

### 完整任务 ID 索引

```
Sprint A: M2-DESIGN-01/02/03, M2-VER-04/05, M2-REFAC-05/06/07/13/14, M2-COMPAT-01, M2-ADV-06, M2-CTX-01, M2-PERF-BASE
Sprint B: M2-REFAC-01/02/03/04/09/10/11/12/15
Sprint C: M2-TIER-01a/b/c/d, M2-ADV-07, M2-SCALE-P, M2-SCALE-SPRINT, M2-ADV-03
Sprint D: M2-SCALE-02/01, M2-SCALE-LOCK, M2-PETGRAPH-01, M2-ADV-02/04/05/08/09/10/01
Sprint E: M2-VER-01/02, M2-COV-01, M2-SCALE-03, M2-CLI-01/02/03/04/05/06, M2-ERR-01, M2-PARITY-01, M2-CICD-01
Sprint F: 验收
```

### 依赖关系总览

```
Sprint A（无依赖）
  ↓
Sprint B（REFAC-09 依赖 REFAC-01; REFAC-10 依赖 REFAC-09）
  ↓
Sprint C 线1（TIER-01 依赖 Sprint B REFAC-09；**不依赖** REFAC-10，见 §6 TIER-01a）
Sprint C 线2（SCALE-P 依赖 Sprint A REFAC-06; ADV-03 依赖 DESIGN-01）
  ↓
Sprint D（SCALE-02 依赖 C 线2; ADV-01 依赖 C 线1 TIER-01c）
Sprint E（独立，与 D 并行）
  ↓
Sprint F（依赖 D + E 全部完成）
```

---

## §11 M2 推迟项（M2.5/M3 候选）

以下功能在设计文档中标注 M2 但经评估推迟：

| 功能 | 推迟到 | 理由 |
|------|--------|------|
| 状态机程序化（独立 orchestrator 二进制） | M2.5 | 工作量 XL，SKILL.md + CLI 加固足够。**注意：08 §M2 将此列为首要排序项，本计划有意偏离——M2 通过加固 CLI（VER-04/COMPAT-01/SCALE 系列）+ 保留 SKILL.md 编排实现同等可靠性，完整程序化推迟到 M2.5 积累更多编排经验后再做。** 用户已批准此偏离（复审 Q3）。**其确定性子项 auto-unblock（DFS 环检测+拓扑解除）已抽出为 M2-CLI-06 纳入 M2**——并行 sprint 解阻塞必需，且 09 line 228 明确承诺 M2 交付，不随二进制一同推迟。残留风险：并行编排（3 agent + merge）仍由非确定性 SKILL.md 驱动，靠 D3 worktree 方案的「worktree 隔离自检 + merge 后整体 check 真门 + 图缺陷回退串行」控制（见 §13 R2）。 |
| /goal 自主迁移循环 | M2.5 | 依赖状态机程序化 |
| 行为录制框架（VER-03） | M2.5 | 工作量 L，验收不强依赖 |
| 变异测试（cargo-mutants） | M2.5 | 验收不强依赖 |
| 图 schema 扩展（TypeAlias/Variable/Community） | M3 | 不影响迁移顺序 |
| FTS5 全文搜索 | M3 | <20K 行不需要 |
| Leiden 社区检测 | M3 | 无成熟 Rust crate |
| 规则治理工具化 | M3 | 规则量未达阈值 |
| 自定义 lint crate | 条件（M2 规则>15 条时） | |
| 降级决策学习（degrade_decision_history） | M3 | nice-to-have |
| 类型复杂度前置降级信号 | M3 | nice-to-have |
| migration-state.json 完整迁移脚本 | M2.5 | M2 先做版本检测 |
| 完整错误码体系（30-40 条） | M2.5 | M2 先覆盖 P0 |
| 跨文件方法调用档 2（per-file 类型环境） | M3 | 跨语言需求更明确时再建 |
| release-meta.json | M2.5 | 发版时补 |
| 适配器规则版本陈旧检测程序化 | M3 | |
| index.json 自动生成 | M3 | |
| Beads/AgentMemory 集成 | M3+ | |

---

## §12 质量门（同 M1，增加 M2 特有检查）

### Level 1-4 继承 PLAN §13

额外增加：

### Level 5：M2 特有

- [ ] 分档结果可观测（state 中记录 tier；分档信号写 run 日志/AttemptRecord）
- [ ] 并行翻译编排器单写无 data race（WAL 配置回归通过，见 §2 D5）
- [ ] 增量构建正确（变更文件重建，未变更跳过）
- [ ] 回归：M1 202 测试 + fixture 全通过

---

## §13 风险与缓冲

| # | 风险 | 概率 | 影响 | 缓解 |
|---|------|------|------|------|
| R1 | REFAC-10 跨文件方法调用工作量超预期 | 中 | 中 | 档 1 先做（低成本高收益），档 2 推 M3 |
| R2 | 并行 merge 冲突 / 跨模块编译冲突（coherence/feature/宏） | 中 | 高 | **D3 worktree + 约束包**：git 冲突检测守卫共享文件、merge 后整体 check 为真门、worktree 隔离回滚不污染主树 |
| R2b | **共享 API 变更串行化吞噬并行收益**（约束 2 把改公共 error/trait 的模块打回串行） | 中 | 中 | 叶子优先（sprint=1）先冻结共享基础；统计共享变更频率，过高则前置一轮「公共 API 设计 sprint」 |
| R2c | **图不完整致同层假独立**（REFAC-10 档1 漏边，两实际耦合模块同组并行） | 中 | 中 | 整体 check 暴露意外跨模块引用→判图缺陷、回退串行+记录修图（D3 要点 6） |
| R2d | **reconcile 活锁**（A 合并→B 重译又改共享→C 重译…无限） | 低 | 高 | reconcile 轮次上限（默认 3）+ 超限降级串行/人工（D3 要点 5，codex 第四轮指出） |
| R2e | **孤儿规则逼共享 impl 进共享文件**（多模块各需 From/Display/Serialize for SharedError，单 crate 下不罕见） | 中 | 中 | porting 规则最小化 + 叶子优先冻结 + 整组 check 兜 E0119；高频则该共享 impl 提前到串行 cleanup |
| R3 | 真实 5K-20K 项目暴露 M1 未覆盖的 AST 边界 | 高 | 中 | 图精度提升（Sprint B）+ 宽松验收 |
| R4 | 并行吞吐 <1.5 模块/小时（worktree 启动 + 独立 target 重编开销） | 中 | 中 | 降 max_concurrent_agents=2 重验；Sprint F 实测 target 策略（sccache/共享 target） |
| R5 | TIER-01 分档判据误判（full 误判为 trivial） | 低 | 高 | 默认 full 兜底 + 失败自动升档 |

### 缓冲

- Sprint F 验收预留 5-7 天，含修复时间
- 若 Sprint D 超期 >3 天，砍 ADV-01（多候选生成）推 M2.5
- 若 SCALE 路径阻塞，优先保证 TIER-01（单机效率提升 ROI 更高）

---

## §14 执行协议

### 每个 Sprint 的交付流程

1. 质量门：`/gate` 4 层全过
2. 更新 `docs/STATUS.md`
3. commit 引用任务 ID（如 `feat(M2-TIER-01): 复杂度自适应循环`）
4. 独立分支提 PR
5. PR 审查：`/pr-review-toolkit:review-pr` + `design-checker` + `/code-review`
6. 修复 critical/important issues 后通知用户审阅

### 续接协议

同 PLAN §3 续接协议。新会话读：CLAUDE.md → STATUS.md → 本文件对应 Sprint 段。
