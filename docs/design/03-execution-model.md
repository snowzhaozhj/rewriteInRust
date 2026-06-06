> [返回主索引](./README.md)

# 四、执行模式（Sprint 循环模型）

## 4.1 从线性阶段到 Sprint 循环

原设计的线性阶段划分在实际执行中存在问题：
- 大项目不可能等所有测试搭好再开始迁移
- 迁移过程中会发现新规则，需要回头更新 PORTING 规则
- 不同模块可能处于不同阶段

**修订**：改为 Sprint 循环模型，分两层循环。

## 4.2 外循环：Sprint 级（跨会话/天/周）

```
Sprint N:
  1. Sprint Planning
     - 选择本 Sprint 要迁移的模块（按拓扑排序）
     - 源项目变更检测（双轨/Strangler Fig 策略）— 脚本化执行 `rustmigrate graph build` 增量重建并比对指纹
       对比源项目当前版本 vs source-ref/ 基线，STRUCTURAL 变更命中目标模块时自动告警（见 § 4.6.1）
     - 确认 PORTING 规则是否需要更新
     - 确认测试基础设施是否就绪

  2. 执行（多个 Work Unit）
     - 每个 Work Unit = 一个完整的 Claude Code 会话
     - 每个 Work Unit 迁移 1-3 个模块
     - 产出：Rust 代码 + 测试 + MDR

  3. Sprint Review（由 `/migrate review` 触发，扩展为完整 Sprint Review 流程）
     - 集成验证（Tier 0 + Tier 1 + 按需 Tier 2）— 由 `full-verify.sh` 执行
     - 更新 PARITY.md — 由 Skill 主上下文自动更新
     - 回顾 PORTING 规则，追加新发现的规则（附 changelog）— 人工 + AI 辅助
     - 更新 KNOWN_DIFFERENCES.md — 由 verifier 即时写入，Review 时人工审批
     - **知识沉淀**（[见文档体系 > 增量知识沉淀](./05-documentation-system.md#611-增量知识沉淀架构)）：提取 patterns/anti-patterns，写入 SPRINT_LEARNINGS.md — verifier 提取 + 人工审阅
     - 评估是否需要调整迁移策略 — 人工决策

  4. Sprint Retrospective
     - 哪些规则频繁触发失败？→ 补充到 PORTING 规则
     - 哪些工具信噪比低？→ 调整 Tier 级别
     - 上下文管理是否够用？→ 调整模块粒度
```

**行动指南**：每个 Sprint 以 `migration-state.json` 中的 Sprint 元数据为准，包含 Sprint 目标、已完成模块、阻塞项。

### 4.2.1 执行模式分层

| 模式 | 入口 | 适用场景 | 并行度 | 阶段 |
|------|------|---------|--------|------|
| **Skill 交互式** | `/migrate analyze`、`/migrate run`、`/migrate review` | 分步调试、学习流程、小项目 | 串行 | MVP (M1) |
| **Workflow 批量** | `ultracode` workflow 定义文件 | 多模块并行迁移、CI/CD 集成 | 多 agent 并行 | M2+ |
| **/goal 自主循环** | `/goal "迁移 module X, Y, Z"` | 自主迁移循环（analyze→run→review 自动串联） | 串行但自主 | M2+ |

**Workflow 批量模式要点**（M2 阶段设计）：
- 每个 agent 在独立 worktree 中工作（避免文件冲突）
- 按拓扑排序分批，同一批内可并行
- 每个 agent 执行 `/migrate run` 的内循环
- 汇总阶段合并结果到主分支

## 4.3 内循环：模块级（单会话内）— Phase A/B 双阶段翻译

```
Work Unit（一个 Claude Code 会话）:

  Step 0: 源文件保留（首次迁移时执行一次）
    - 将待迁移模块的源文件复制到 `.rust-migration/source-ref/` 作为参考副本
    - 锁定源码版本（记录 commit hash 到 migration-state.json）
    - 原则：源文件保留直到 `/migrate graduate` 毕业（借鉴 Bun 的 .zig/.rs 并存模式）

  Step 1: 上下文加载
    - 读取 migration-state.json 确认当前任务
    - 读取 PORTING 规则中相关条目
    - 读取目标模块源码 + 依赖接口（优先读 source-ref 中的锁定版本）

  Step 2: 语义解构
    - AI 生成意图摘要（纯文本，不含源语言语法）
    - 识别关键语义点：错误处理、并发、状态管理

  Step 2.5: 意图确认门禁（人类决策点，MVP 默认开启）
    - 向人类展示意图摘要全文，暂停等待确认后才进入 Phase A
    - 意图摘要是翻译的语义契约，错误意图会污染整个内循环，故须前置拦截
    - 与 § 7.4 Approval Token / 不自动宣布成功属同类人类决策点
    - 确认方式沿用 § 4.2.1 Skill 交互式模式（不新增 CLI 命令）
    - power-user 可在 .rustmigrate.toml 设 auto_confirm_intent=true 跳过
      （/goal 自主循环与 Workflow 批量模式默认跳过；首次/高风险模块建议开启）

  Step 3: Phase A — 忠实翻译
    - 生成 Rust 代码，优先保持与源码的 1:1 对应（便于 diff 对照审查）
    - Phase A 不做优化：不得删除死代码、辅助函数、冗余字段或内联未使用常量
      （惯用化优化全部留到 Phase B）
    - 非平凡函数加 PORT NOTE 注释，标注源码行号范围或等价锚点（供 Step 4.5 结构校验）
    - Private 方法默认翻译（不省略），保持结构完整性
    - 标记系统（借鉴 Bun 的 TODO(port) 纪律——累积 2,327 个标记后统一清理）：
      - TODO(port) — 标记未完成项（翻译期间允许累积，Sprint Review 时统一清理）
      - PERF(port) — 标记已知性能问题
      - PORT NOTE — 标记翻译决策（说明为什么选择这种翻译方式）
    - 标记清理规则：每个 Sprint Review 统计 TODO(port) 数量，趋势必须下降；`/migrate review` 报告中包含标记计数
    - F1 反馈：rust-analyzer LSP 自动诊断（秒级）

  Step 4: 对抗性审查
    - verifier SubAgent 对 Phase A 产出物执行对抗性审查
    - 逐维度比对源码与翻译结果（使用 7.7 节探测维度清单）
    - 产出物：审查报告（差异列表 + 修正建议）

  Step 4.5: Phase A 结构校验门禁（进入 Phase B 前）
    - verifier 附带校验 Phase A 是否保持 1:1 结构（确认翻译器未提前优化）：
      函数数量比 0.9x–2.0x、代码行数比 1.2x–3.0x（与 § 7.5 告警阈值一致）、
      主控制流（循环/条件分支）数量与嵌套层级按源码 AST 大致对应
    - 比例越界 → 标记「Phase A 疑似已优化」，要求 translator 以忠实保留模式重做 Phase A
      再进入 Step 5（门禁，非软提示）

  Step 5: Phase B — 编译修正 + 惯用化优化
    - 基于审查报告修正语义偏差
    - 允许重写（非直译）的范围**仅限**：并发模式选择（Arc/Mutex vs channel vs 其他）、
      取消安全性重构、局部性能优化；这些重写**不得改变**函数签名、错误语义或可观测副作用
    - 任何此类重写须记录 MDR（在 Step 7 写入）
    - 惯用 Rust 优化（消除翻译腔）
    - 编译失败 → 先跑 `cargo fix --allow-dirty`（确定性自动修复）→ 剩余错误交给 AI 修复（最多 3 轮）
    - F1（rust-analyzer 诊断）反馈循环（此步骤只做编译验证和惯用化优化，不生成测试）
    - 注意：意图摘要的生成依据是**源语言语义**（接口契约 + 行为 spec），而非 Phase A 代码；
      因此 Phase B 的合规重写不应使意图摘要失效

  Step 6: 测试生成与验证（针对 Phase B 最终代码）
    - 测试在 Phase B 完成后生成，**以 Phase B 最终 Rust 代码为目标**（非 Phase A）
    - 由 verifier SubAgent 生成测试（非 translator 同步生成）
    - 语义影响复核：若 Phase B 的 MDR 涉及错误处理策略、返回值语义或并发可见性的改变，
      verifier 须先确认该改写与意图摘要保持一致（parity）——一致则照常生成测试；
      不一致则视为「语义漂移」，先更新 `{module}-intent.md` 并在 MDR 中说明，再生成测试
    - F2 反馈：cargo test + clippy（分钟级）
    - 测试失败 → 分析原因 → 修复或记录到 KNOWN_DIFFERENCES.md

  Step 7: 产出物更新
    - 更新 PARITY.md（模块状态）
    - 写 MDR（如有架构决策）
    - 更新 migration-state.json
```

## 4.4 三层反馈循环

| 层级 | 触发时机 | 延迟 | 内容 | 处理方式 |
|------|---------|------|------|---------|
| F1 编译反馈 | 每次写入 .rs 文件 | 秒级 | **rust-analyzer LSP 自动诊断** | 自动反馈给 LLM 重试 |
| F2 测试反馈 | 模块翻译完成 | 分钟级 | 测试失败 + clippy 警告 | AI 分析修复或标记差异 |
| F3 集成反馈 | Sprint Review | Sprint 级 | 集成测试 + 覆盖率 + 性能基准 | 团队决策是否通过 |

**行动指南**：F1 由 rust-analyzer LSP 自动提供，无需 Hook 配置；F2 在 Skill 的 SKILL.md 中通过分步指令要求"翻译步骤完成后执行 `verify.sh` 验证命令"；F3 由 `/migrate review` Skill 手动触发。

**权衡**：rust-analyzer 在超大项目中可能有性能问题；保留 `verify.sh` 中的 cargo check 作为 F2 的一部分（确定性兜底）。

## 4.5 问题前移矩阵

目标：尽可能在早期阶段发现问题，降低修复成本。

| 问题类型 | 能在 F1(编译反馈) 发现？ | 能在 F2(测试反馈) 发现？ | 必须到 F3(集成反馈)？ |
|---------|------------------------|------------------------|---------------------|
| 类型不匹配 | 是 | — | — |
| 借用/生命周期 | 是 | — | — |
| 逻辑错误 | — | 是（单元测试） | — |
| 行为不等价 | — | 是（差异测试） | — |
| 性能退化 | — | — | 是（benchmark） |
| 并发 bug | 部分（Send/Sync 编译期检查） | — | 是（loom/集成测试） |
| FFI 边界问题 | — | — | 是（集成测试） |
| 翻译膨胀 | — | 是（tokei 对比） | — |
| unsafe 安全性 | — | 否 | 是（Miri 需 Tier 2 启用 + cargo-geiger 全局） |

## 4.6 并行开发策略

迁移期间源项目可能还在演化，需要选择并行策略：

| 策略 | 适用场景 | 操作 | 风险 |
|------|---------|------|------|
| **功能冻结** | 小项目、短期迁移 | 迁移期间源项目不接受新功能 | 业务停滞 |
| **双轨开发** | 中型项目 | 源项目继续开发，每个 Sprint 同步变更到 Rust | 同步成本高 |
| **Strangler Fig** | 大型项目、长期迁移 | 通过路由层逐模块切换，新旧并行运行 | 架构复杂 |

**行动指南**：
- 在 PROFILE 阶段（项目画像）中决定并行策略
- 功能冻结：在 `migration-state.json` 中锁定源码 commit hash
- 双轨开发：每个 Sprint 开始前检查源项目变更（见 § 4.6.1），必要时更新迁移规则
- Strangler Fig：需要额外配置 FFI 桥接层和路由层

### 4.6.1 源项目变更追踪（双轨 / Strangler Fig 场景必需）

功能冻结策略下源码不变，无需追踪；但**双轨开发**与 **Strangler Fig** 策略下，源项目在迁移期间持续演化（bug 修复、功能变更），若 Rust 侧无法感知这些变更，会导致等价性验证失效或行为不一致。

**机制：复用既有结构指纹体系，不新建独立追踪文件**。源项目变更检测直接重用 [04 § 5.7.5 增量更新策略](./04-toolchain.md#575-增量更新策略)的三级变更检测（`NONE` / `COSMETIC` / `STRUCTURAL`）与结构指纹——图分析、增量更新、源码变更追踪共用同一套指纹，避免重复设计：

1. **基线指纹存储**：变更监控基线在 § 4.8 录制（baseline commit 的结构指纹写入 `file_fingerprints` 表，详见 § 4.8 第 5 点）。指纹存储于 `source-graph.db` 的 `file_fingerprints` 表（schema 权威见 [04 § 5.7.1](./04-toolchain.md#571-图数据模型)），**不引入 `source-tracking.json` 等并行结构**。
2. **变更检测**：在每个 Sprint Planning（§ 4.2 第 1 步）与 `/migrate run` 前执行，对比源项目当前版本与 `source-ref/` 基线版本，按模块输出变更级别与影响范围 `{module_path: {level: NONE|COSMETIC|STRUCTURAL, detail: {changed_functions, impact_depth}}}`。`impact_depth` 通过 `imports` 边反向 BFS 计算（同 § 5.7.5 传递性更新）。
3. **执行方式（不新增 MVP CLI 命令）**：M1 阶段功能冻结为默认并行策略，双轨/Strangler Fig 场景直接复用既有 `rustmigrate graph build` 增量重建——它本就计算上述三级变更并更新 `file_fingerprints`，对比基线与当前指纹即得变更报告，无需新建命令。是否将「变更报告」封装为独立 M2 子命令 `rustmigrate source check-changes`，归入 [06 § 10.0.1](./06-plugin-structure.md#1001-cli-工具架构rustmigrate) 的 M2 扩展候选评估（不突破 11+5 命令清单，命令权威以 06 为准）；MVP 阶段由 Sprint Planning 脚本调用 `graph build` + 指纹比对承担。
4. **STRUCTURAL 变更告警**：若检测到 `STRUCTURAL` 级变更命中目标模块或其依赖链，Sprint Planning（§ 4.2 第 1 步脚本化检测）自动向用户告警，提示需重新翻译受影响模块或同步迁移规则。

## 4.7 异步翻译策略

> **设计依据**：Claw-Code（TS→Rust）采用"边界异步、核心同步"策略，大幅降低了翻译复杂度；Pingora（C→Rust）则在 Tokio 之上构建了定制运行时。不同项目需要不同策略。

翻译时的异步处理不应硬性规定通用规则，而是按项目特征决定：

| 源项目特征 | 推荐策略 | 理由 |
|-----------|---------|------|
| 纯计算/CLI 工具 | **核心同步**，仅 I/O 边界异步 | 降低翻译复杂度，避免不必要的 async 传染 |
| Web 服务（Express/Flask） | **按需异步**，路由层 async，业务逻辑可同步 | 匹配 axum/actix 的异步模型 |
| 高并发运行时（事件循环） | **全栈异步**，须选择运行时并写 MDR | 必须用 tokio/async-std 重新设计并发模型 |

**行动指南**（与 PROFILE/PLAN 边界对齐，见 [02 § 3.2.3](./02-architecture.md#323-profile-与-plan-边界清晰化)）：
- **PROFILE（自动）**：analyzer SubAgent **客观检测**异步模式（扫描 Promise/async-await 语法、事件循环、回调风格），在画像摘要 stdout JSON 中输出 `async_pattern_summary: { detected_async_patterns: [...], recommended_strategy: "...", needs_user_decision: bool }`。检测是事实采集，不做决策。
- **PLAN（人类决策）**：PLAN 阶段依据上述检测结果**确认 `async_strategy` 选择**，写入 `.rustmigrate.toml` 的 `async_strategy` 字段，作为 translator Phase A 代码生成规则的上下文。
- 如果选择"全栈异步"，须写 MDR 记录运行时选择（tokio vs async-std）和取消安全性审查计划。

## 4.8 PROFILE → PLAN 中间步骤：原项目可复现基线

在 PROFILE（画像）和 PLAN（规划）之间，插入一个关键步骤：

1. 锁定源项目版本（git tag/commit hash）
2. 确认源项目能在本地完整构建和测试
3. 录制基线行为（CLI 输出、API 响应、测试结果）
4. 记录基线指标（测试覆盖率、性能数据、代码行数）
5. **录制变更监控基线**（支撑 § 4.6.1 源项目变更追踪）：在 Step 0 源文件保留（见 § 4.3 Step 0）后，为 `source-ref/` 中所有文件计算结构指纹，写入 `source-graph.db` 的 `file_fingerprints` 表，关联 baseline commit；这些指纹标注来源为 `source-baseline-fingerprint`（区别于图分析的常规指纹，也区别于 § 7.1 性能基线 `source_baseline.json`），作为后续变更检测（§ 4.6.1）的对比基准。复用既有指纹体系，不新建独立追踪文件。

**执行者**：此步骤由 `/migrate analyze` 的分析阶段末尾执行（analyzer SubAgent 完成画像后，Skill 主上下文执行基线录制脚本）。`source_commit` 写入 `.rustmigrate.toml`。

**行动指南**：如果源项目本地构建失败，**停止迁移**——先修复源项目。

## 4.9 项目级止损标准

当迁移进展持续不佳时，需要及时止损而非无限投入。以下阈值为参考值，可在 `.rustmigrate.toml` 的 `[stop_loss]` 节中配置覆盖。

| 指标 | 阈值（默认） | 触发动作 |
|------|------------|---------|
| DEGRADE 比例 | >40% 模块降级为 FFI 桥接 | 暂停迁移，评估是否继续——大面积降级意味着 AI 翻译能力不足以处理该项目 |
| LLM API 成本 | 超出预算 2x | 暂停迁移，评估是优化 prompt/模块粒度还是放弃 |
| Sprint 停滞 | 连续 3 个 Sprint 未完成任何模块 | 召集团队评审，分析阻塞原因，决定是否继续 |
| 质量评分回归 | 当前 Sprint 评分（§7.5 `final_score`）< 前 3 个 Sprint 均值 − 10%，连续 2 个 Sprint | 标记团队评审；若持续 3 个 Sprint，触发升级评审后再继续 |

```toml
# .rustmigrate.toml 中的止损配置（可选覆盖）
[stop_loss]
degrade_ratio_threshold = 0.4        # 降级模块比例阈值
cost_multiplier_threshold = 2.0      # LLM API 成本超预算倍数
stalled_sprint_threshold = 3         # 连续停滞 Sprint 数
score_regression_threshold = 0.10    # 质量评分回归阈值（相对前 3 Sprint 均值的下降比例）
```

**行动指南**：`/migrate review` 在仪表板中展示止损指标的当前值和阈值距离。接近阈值时提前预警。

## 4.10 性能与并行转换指南

M1（MVP）用 §4.2.1 的「Skill 交互式」串行模式，M2+ 用「Workflow 批量」并行模式。本节给出两者的吞吐预期、并发度依据和升级决策点。

**(1) M1 串行吞吐预期（理论值，M1 验收时用实测项目补充）**

以「单模块 1K 行需约 1 天（含 Phase A/B + 测试搭建 + 人工审查）」为反推基准：

| 模块数 × 平均规模 | 单模块耗时 | 总耗时预期 |
|------------------|-----------|-----------|
| 5 × 500 行 | ~0.5 天 | ~2-3 个工作日（1-2 Sprint） |
| 10 × 1000 行 | ~1 天 | ~5-7 个工作日（每 Sprint 2-3 天，含人工审查） |
| 20 × 1000 行 | ~1 天 | ~3-4 Sprint |

> 上表为**理论预期**，非实测；M1 验收时须用 3 个真实项目补充实际数据校准。

**(2) M2 并发度计算依据**

`max_concurrent_agents`（[06 § 11.1](./06-plugin-structure.md#111-rustmigratetoml-配置文件) 默认 3）取以下约束的最小值：

```
max_concurrent_agents = min(LLM_API_并发限制, 磁盘 I/O 饱和点, 可用 worktree 数)
```

参考配置：激进 = 5 / 平衡 = 3（默认）/ 保守 = 1。`.rustmigrate.toml` 注释中应说明「3」是平衡配置默认值。

**(3) 性能瓶颈分层**

| 环节 | 量级 | 说明 |
|------|------|------|
| Skill 调用延迟 | 秒级 | 编排开销，可忽略 |
| LLM 翻译处理 | 分钟级 | 主要耗时来源 |
| cargo 编译/测试 | 分钟级 | 大项目可能更久 |

> rust-analyzer 在超大项目的性能问题已在 §4.4 权衡段说明（保留 cargo check 兜底）。

**(4) M1 → M2 升级决策（可观测指标，非硬性阈值）**

`/migrate review` 仪表板展示以下指标，供用户判断是否升级到 M2 并行模式：
- 单次 Sprint 耗时持续 > 10 个工作日；**或**
- 磁盘吞吐持续 < 50 MB/s（串行 I/O 成为瓶颈）。

满足任一时，提示「考虑切换 Workflow 批量模式」。是否升级由用户决策（M1/M2 并存，不强制）。

## 4.11 CI/CD 集成（M2 范围）

M2 交付物「CI/CD 集成」的落地设计。区分两类关注点：**用户集成模板**（rustmigrate 如何在用户项目 CI 中被调用，M2 范围）与**项目自验证 dogfooding**（本项目 CI 是否用自身工具，M2 概念设计、不要求实现）。

### 4.11.1 用户集成模板（GitHub Actions）

rustmigrate 在用户项目 CI 中作为**验证门禁**调用（非自动迁移）。触发与分级策略：

| CI 事件 | 运行内容 | 失败动作 |
|---------|---------|---------|
| PR 打开 / 更新 | Tier 0（`cargo check` + `clippy` + `cargo test`）+ `rustmigrate review --json` 解析迁移状态 | status != "ok" 则 fail job |
| merge 到主干 | Tier 0 + Tier 1（黄金文件 + 属性测试） | 任一 Tier 失败则 fail |
| tag / release | Tier 0+1+2（含模糊测试差异对比、覆盖率门禁） | 完整管线，产出 KNOWN_DIFFERENCES 报告 |

模板要点：
- **缓存策略**：`source-graph.db` 通过 `actions/cache` 按源码 hash 持久化，避免每次重建图（图构建耗时门禁见 [08 § M1 MVP](./08-roadmap-and-reference.md#m1-mvp6-8-周)）。
- **产物上传**：`rustmigrate review` 生成的报告（PARITY.md、KNOWN_DIFFERENCES.md）通过 `actions/upload-artifact` 上传供审阅。
- **JSON 解析 + 失败判定**：CI 步骤解析 CLI 统一 JSON 输出（`{status, data, warnings}`，格式见 [06 § 10.0.1 CLI 与 Plugin 交互](./06-plugin-structure.md#1001-cli-工具架构rustmigrate)），`status != "ok"` 时令 job 失败。示例：

```yaml
# .github/workflows/migration-check.yml（用户项目，示意）
- name: rustmigrate review gate
  run: |
    rustmigrate review --json > review.json
    status=$(jq -r '.status' review.json)
    if [ "$status" != "ok" ]; then
      jq -r '.warnings[]' review.json   # 输出机器可读的失败详情
      exit 1
    fi
```

### 4.11.2 错误信息标准化

CLI 失败时输出机器可读的统一结构，便于 CI 提取。例如 `rustmigrate validate state` 在状态非法时返回：

```json
{ "status": "error", "data": null, "warnings": ["migration-state.json: 字段 current_state 取值 'FOO' 不在状态枚举内"] }
```

CI 步骤据 `status` 决定 job 成败，据 `warnings` 在日志中打印可定位详情。

### 4.11.3 可复现性保证

CI 集成要求确定性，避免"本地通过 / CI 失败"：
- `source-graph.db` 构建必须确定性：tree-sitter 解析顺序固定、vendored `Cargo.lock` 锁定依赖版本；**tree-sitter 版本须锁定**（`.rustmigrate.toml` 的 `[reproducibility]` 段，schema 权威见 [06 § 11.1](./06-plugin-structure.md#111-rustmigratetoml-配置文件)），跨用户版本差异会导致图构建不一致。
- 属性测试 / 模糊测试用**固定 seed**（proptest regression 文件入库），回归用例可复现。
- HashMap 迭代顺序等非确定源已在验证层处理（见 [§ 7.7 探测维度清单](#77-不等价证据探测维度清单)）。
- **LLM 输出非确定性**：翻译用固定 `temperature`/`top_p`（`[reproducibility]` 段配置），叠加确定性规则注入，把 LLM 自由度收窄到可复现范围。注意：Claude API 的 seed 参数支持取决于 API 可用性——不可用时，可复现性依赖固定 temperature/top_p + 确定性规则集，而非逐 token 复现。
- **中间产物 schema 版本化**：`migration-state.json`、`source-graph.db`、`type-map.json` 须含 `schema_version`（必填）字段；断点续传（checkpoint）恢复时校验版本——同版本直接恢复，跨版本走 schema 迁移（版本化与迁移工具权威见 [06 § 10.0.2 版本控制与向后兼容策略](./06-plugin-structure.md#1002-版本控制与向后兼容策略)），迁移失败则需人工恢复。
- **M1 可复现性验收标准**：两名独立用户对同一项目运行相同 `/migrate analyze` + `/migrate run`，产出的 Rust 代码哈希一致（元数据文件中的时间戳字段除外）。此为 M1 验收门槛之一（工作量归属见 [08 路线图](./08-roadmap-and-reference.md#m1-mvp6-8-周)）。

### 4.11.4 项目自验证（dogfooding，M2 概念设计）

> 本节为 M2 概念设计，**不要求 M2 实现**。

设想：本项目可在自身 CI 中用 rustmigrate 验证一个内置的 TS→Rust 微型 fixture（M1 已建的自建微型项目，见 [08 § M1 MVP 工作量分解](./08-roadmap-and-reference.md#m1-mvp6-8-周)），作为工具端到端回归的活体测试。落地时机与范围待 M2 后评估，避免过早投入。

---

# 七、测试与验证策略

## 7.1 测试分层（L0-L7）

| 层 | 名称 | 工具 | Tier | 目标 |
|----|------|------|------|------|
| L0 | **单元测试** | cargo test | 0 | 基础正确性 |
| L1 | **快照/黄金文件测试** | insta | 1 | 锁定输入/输出对（合并原 L1+L3，消除重叠） |
| L2 | 属性测试 | proptest | 1 | `for all x: old(x) == new(x)` |
| L3 | **E2E 差异测试** | 自建差异框架 | 1（M2+ 完整版）/ M1 使用简化脚本对比 | 整体行为等价 |

> **M1 阶段 L3 轻量替代**：M1 不构建完整的自建差异测试框架（G3 组件）。改用简单的 shell 脚本做输入输出对比——对 CLI 工具运行相同输入，`diff` 比较 stdout/stderr/exitcode；对库函数通过 FFI 桥接调用新旧实现并比较输出。完整差异测试框架在 M2 阶段实现。
>
> **依赖链路限制**：M1 的 shell 脚本 L3 方案仅适用于**叶子模块**（无内部消费者，黑盒输入输出自包含）。若模块 A 的输出是模块 B 的输入（内部链路依赖），源语言版 A 无法与 Rust 版 B 直接交互（类型不兼容），黑盒等价假设失效。此时应在迁移规划（PLAN）时三选一：(1) **整体迁移依赖链**（A+B 同 Sprint 一起迁），(2) **A 降级为 FFI 桥接**供 B 消费，或 (3) **B 的 L3 验收延后到 M2** 的完整 G3 框架。依赖关系由 §7.2 Step 2.1 记录。

| L4 | 模糊测试 | cargo-fuzz | 2 | 随机输入差异对比 |
| L5 | 变异测试 | cargo-mutants | 2 | 验证测试真正保护了行为 |
| L6 | **性能回归** | criterion | 2（性能动机时提升为 1） | 无性能退化 |
| L7 | **并发正确性** | loom / shuttle | 2 | 并发模型正确性 |

**测试执行确定性保障**：
- proptest：固定 seed 记录到 `test-fixtures/proptest-regressions/`
- cargo-fuzz：corpus 持久化到 `test-fixtures/fuzz-corpus/`
- criterion：基线数据持久化到 `test-fixtures/benchmarks/`
  - **基线时机**：Phase A 后采集源项目基线（`source_baseline.json`，仅参考）；Phase B 优化后采集 Rust 基线（`rust_baseline.json`，回归对比的**验收基准**）。
  - **回归容忍度**：`.rustmigrate.toml` 的 `[testing] benchmark_tolerance = 0.10` 表示接受 ≤10% 的相对性能偏差（语义解释以本节为准，配置位置见 [06 § 11.1](./06-plugin-structure.md#111-rustmigratetoml-配置文件)）。超出容忍度的性能回归须 MDR 经维护者批准方可合并。详细 CI 执行与目录结构见 [04 § 5.3](./04-toolchain.md#53-tier-1推荐画像自动启用) 与 [06 § 10.6](./06-plugin-structure.md#106-产出物目录结构)。

### 7.1.1 模块类型 × 测试层要求矩阵

L0-L7 分层定义了「能做什么」，但模块要达到 `done` 还需明确「哪些层是强制的」。下表按模块类型规定 M1（MVP）下的**强制（必须）/ 条件（满足条件时必须）/ 可选**层组合。层选择由检测到的模块类型决定（PROFILE/analyzer 输出），**不由 verifier 自由裁量**。

| 模块类型 | 强制（M1 必须） | 条件强制 | 可选（M2+ 提升） |
|---------|---------------|---------|----------------|
| 纯工具函数（无状态/无副作用） | L0 + L1 + L3-简化 + L2 | — | L4/L5 |
| 有状态对象/服务 | L0 + L1 + L3-简化 | 有持久化状态 → L1 操作序列快照 | L4/L5 |
| 异步/并发代码 | L0 + L1 + L3-简化 | **L7（loom/基础并发验证）必须** | L4 |
| FFI 绑定 | L0 + L1 | FFI 边界 → L3-简化（跨边界 I/O 对比） | L4 |
| 性能敏感模块（`performance` 动机） | L0 + L1 + L3-简化 | L6（criterion）必须 | L4/L5 |

> **M1 最低门槛**：**所有模块**至少 L0 + L1 + L3-简化；纯函数额外要求 L2（proptest）；异步/并发模块额外要求 L7。L4-L6 在 M1 默认不强制（除非对应迁移动机触发，见 §8）。verifier 在 Step 6 生成测试时按此矩阵选择测试层（其系统提示中纳入本矩阵）；`/migrate run` SKILL 按检测到的模块类型注入对应层要求，不留给 verifier 自由决定。

## 7.2 测试基础设施搭建（SCAFFOLD 阶段修正）

原设计存在循环依赖：Rust 代码不存在时不能写 Rust 测试。

**修正方案**：

```
Step 1: 评估原项目测试质量
  → 有测试：标记为"黄金测试集来源"
  → 测试不足：补充测试（在源语言中）

Step 2: 行为录制（不依赖 Rust 代码）
  ├── CLI 工具：录制 args → stdout/stderr/exitcode
  ├── HTTP 服务：mitmproxy 录制请求/响应
  ├── 库/SDK：录制函数调用的 input/output 对
  └── 有状态服务：录制操作序列和状态变更

Step 2.1: 模块依赖关系记录（支撑 L3 链路场景）
  ├── 在 PROFILE/SCAFFOLD 阶段通过图查询（已有 imports 边或 tree-sitter 模块分析）
  │   识别模块间出入依赖
  ├── 写入 .rust-migration/module-deps.json（如 {"module_a": {"depends_on": ["module_b","module_c"]}}）
  └── 对存在依赖的模块做"链式录制"：除单模块黑盒行为外，
      额外录制"模块 A 输出 → 模块 B 输入"的传递行为，保证 golden 文件在依赖链上一致

Step 3: 接口契约定义（不依赖 Rust 代码）
  ├── 函数签名 + 输入输出类型
  ├── 前置条件 / 后置条件
  └── 副作用描述

Step 4: 模块迁移完成后，生成 Rust 测试（确定性 + AI 混合）
  ├── 测试骨架从录制数据**确定性生成**（脚本模板化输入输出对 → Rust #[test] 函数）
  ├── 将黄金测试集翻译为 Rust（确定性模板）
  ├── 将行为录制转为 insta 快照测试（确定性模板）
  └── 基于接口契约生成 proptest（AI 辅助，需理解语义）
  注意：测试骨架优先由确定性脚本生成，仅语义复杂的测试逻辑交给 verifier SubAgent。
  时序：translator 完成翻译 → F1 编译验证通过 → 确定性测试骨架生成 → verifier 补充语义测试 → F2 测试验证。
```

**行动指南**：测试搭建的核心是**行为录制和接口契约**，这两者不依赖 Rust 代码的存在。测试生成职责归属 verifier SubAgent，与翻译（translator）解耦。

## 7.3 验证管线（DAG 结构，非线性）

```
                      ┌─────────────┐
                      │ 源码分析     │
                      │ (tree-sitter)│
                      └──────┬──────┘
                             ▼
                      ┌─────────────┐
                      │  AI 翻译    │
                      │ (多候选)     │
                      └──────┬──────┘
                             ▼
              ┌──────────────┼──────────────┐
              ▼              ▼              ▼
        ┌──────────┐  ┌──────────┐  ┌──────────┐
        │cargo check│  │cargo deny│  │ tokei    │
        └─────┬────┘  └─────┬────┘  │ 膨胀检测 │
              │              │       └─────┬────┘
              ▼              ▼             │
        ┌──────────┐  ┌──────────┐         │
        │  clippy   │  │cargo audit│        │
        └─────┬────┘  └──────────┘         │
              │                             │
              ▼                             ▼
        ┌──────────┐                 ┌──────────┐
        │cargo test │                │ 复杂度   │
        │(nextest)  │                │ 对比报告 │
        └─────┬────┘                └──────────┘
              │
     ┌────────┼────────┐
     ▼        ▼        ▼
┌────────┐┌────────┐┌────────┐
│llvm-cov││ Miri   ││geiger  │
└────────┘└────────┘└────────┘
              │
     ┌────────┼────────┐
     ▼        ▼        ▼
┌────────┐┌────────┐┌────────┐
│proptest ││fuzz    ││mutants │
└────────┘└────────┘└────────┘
```

**关键点**：验证管线中的独立节点可以并行执行，不必等待其他节点完成。

## 7.4 安全护栏（借鉴 RustLift）

| 机制 | 说明 | 实现方式 |
|------|------|---------|
| **Approval Token** | 批量执行前需要人类预览并授权令牌 | `/migrate run` 在执行 Sprint 批量翻译前，先展示待翻译模块列表和预估成本，用户确认后生成一次性令牌 |
| **Preview-before-spend** | AI 调用前预估 token 成本 | 编排器在调度翻译任务前，根据源码大小和上下文预算预估 token 消耗，超出阈值需用户确认 |
| **不自动宣布成功** | 翻译成功后停在 `needs_review` 而非自动标 `done` | 模块状态流转增加 `reviewing` 状态（已有），verifier 通过后仍需人类最终确认 |

## 7.5 质量评估分层评分卡

翻译质量评估使用确定性指标（工具可直接计算）+ AI 辅助指标（需语义理解）的分层评分卡：

**确定性指标（工具自动计算，权重 70%）**：

| 指标 | 健康范围 | 告警阈值 | 工具 |
|------|---------|---------|------|
| 编译通过 | 是 | 否 → 阻塞 | cargo check |
| 测试通过率 | 100% | < 95% | cargo nextest |
| 代码行数比 | 1.2x - 2.0x | > 3.0x | tokei |
| 圈复杂度比 | 0.8x - 1.2x | > 1.5x | scc |
| 函数数量比 | 0.9x - 1.3x | > 2.0x | tokei |
| clippy 警告数 | 0 | > 0 | cargo clippy |
| unsafe 块数 | 0（理想） | 按 P0-P4 分类 | cargo-geiger |

> **覆盖率作为等价代理的边界**：测试通过率是必要不充分条件。覆盖率本身只证明「代码被执行过」，不证明「行为等价」（源实现有 bug 时高覆盖率仍可能掩盖错误）。当 Rust 覆盖率低于源码时，需 L2 属性测试（proptest）或 L3 差异测试补充；若覆盖率 ≥ 源但 L2/L3 失败则**阻塞**。覆盖率作参考指标，不作充分条件。覆盖率门槛配置见 [06 § 11.1](./06-plugin-structure.md#111-rustmigratetoml-配置文件) `[testing] coverage_threshold`。
>
> **verifier 覆盖率判别规则（纳入其系统提示，直接影响 done/reviewing → blocked/needs_manual_review 状态转移）**：
> 1. 覆盖率 < 源码百分比 → 需 L2/L3 补充（优先 proptest），否则降级为 `requires_manual_review`；
> 2. 覆盖率 ≥ 源码但存在新增代码路径 → 对新路径做 case study（针对性测试）；
> 3. 覆盖率 < 40% → 无论如何须 `manual_review`。

**AI 辅助指标（verifier 评估，权重 30%）**：

| 指标 | verifier 评分检查表 |
|------|------|
| 惯用性 | 使用 idiomatic 模式；用户代码无裸 `unwrap`/`expect`；错误类型选择恰当 |
| 语义保真度 | 意图摘要与实现一致；边界情况处理一致；边界值保留 |
| 可维护性 | 代码结构对应源码；注释对陌生读者足够；耦合度评分 |

**评分公式**：每个指标归一化到 0-100（确定性指标按健康范围映射；布尔指标取 0/100），加权汇总：

```
final_score = deterministic_avg × 0.7 + ai_avg × 0.3
```

verifier 须以**结构化 JSON 输出**评估结果（含各指标分值与置信度），不输出自由文本评语，便于跨 Sprint 对标与回归检测。每 Sprint 的评分快照（含趋势）持久化为 `sprint-N-report.json`（schema 见 [09 附录](./09-appendix-schemas.md)）。

**阈值按迁移动机调整**：上表确定性指标的告警阈值非一刀切，按主要动机调整。调整规则的权威定义在 [§8 动机路由行动指南](#八迁移动机驱动的策略路由)（如内存安全放宽膨胀容忍、性能保持严格并新增 benchmark 必须项）。

## 7.6 行为等价性验证

| 项目类型 | 录制方式 | 对比方式 |
|---------|---------|---------|
| CLI 工具 | args → stdout/stderr/exitcode | 黄金文件逐字节对比 |
| HTTP 服务 | mitmproxy 录制请求/响应 | JSON diff + header 对比 |
| 库/SDK | FFI(PyO3/napi-rs) 调用原实现 | proptest 生成输入对比输出（**仅适用于纯函数**，见 §7.6.1） |
| 有状态服务 | 共享数据库 schema | 操作后状态 snapshot 对比 |

### 7.6.1 库函数 FFI 对比可行性检查清单

FFI 桥接（napi-rs/PyO3）调用原实现做 proptest 对比，**只对纯函数可靠**。实施前 verifier 须按下表判定。

**纯函数识别标准（4 个条件全满足）**：
1. 无全局/静态状态修改；
2. 无 I/O 操作（文件、网络、stdout）；
3. 无系统时间/随机源依赖；
4. 输出确定性（同输入恒同输出）。

识别方法：静态检查（图查询 `calls` 边是否触达 I/O/时间 API）+ 代码审查兜底。

**有状态库函数的处理方案（三选一，记录权衡）**：

| 方案 | 做法 | 成本/代价 |
|------|------|----------|
| 部分纯化 | 在隔离环境（重置状态）重跑 FFI 对比 | 需构造可重置桩，中等成本 |
| 接口契约验证 | 仅按接口契约断言（documented but unverified） | 低成本，但等价性未真正验证 |
| 降级 manual/skip | 标记 `requires_manual_review` 或跳过 FFI 对比 | 失去自动等价保证 |

**FFI 调用性能指导**：行为等价对比需在每条 proptest 用例中额外调用一遍原实现（FFI），在默认 `proptest_cases = 256`（见 [06 § 11.1](./06-plugin-structure.md#111-rustmigratetoml-配置文件)）下可能使测试耗时翻倍。建议：当单次 FFI 调用成本 > T ms（经验值 T≈10）时，改为抽样对比（固定参数化或对 nth_case 过滤抽样），将单函数测试耗时控制在 < 30s。

> 本清单作为 verifier 对抗审查的一个维度纳入其系统提示。

### 7.6.2 L3 差异测试的依赖状态前置检查（模块间依赖场景）

L3 差异测试通过 FFI 桥接调用**旧实现**做对比（库/SDK 场景）。其隐含假设是「被测模块的旧实现可独立运行」。但当模块 A 依赖尚未迁移完的模块 B 时，调用 A 的旧实现需要 B 的旧实现——在模块间部分迁移状态下，若 B 已被部分改写，`source-ref/` 中可能找不到 B 的完整可运行旧实现，FFI 对比的旧实现侧将无法构造，等价性验证失效。

**前置要求（纳入 verifier SubAgent 系统提示，执行 L3 FFI 对比前必须先检查）**：

1. **依赖状态检查**：执行 L3 FFI 对比前，verifier 先查询被测模块的依赖（通过 `rustmigrate graph deps <module>` 或 migration-state.json），判定每个依赖在 FFI 对比中能否提供稳定的旧实现侧。此逻辑与 [09 附录 B § /migrate run Step 0.6 依赖就绪检查](./09-appendix-schemas.md#附录-bmvp-skill-的-skillmd-骨架)的依赖遍历同构，仅判定目标不同（前者判翻译就绪，此处判 FFI 参考就绪）。
2. **可对比条件**：依赖 B 满足以下任一时，A 的 L3 FFI 对比可执行——(a) B 的 `source-ref/` 旧实现完整保留且未被改写（源文件保留纪律见 § 4.3 Step 0）；(b) B 已 `done` 且 Rust 侧可经 FFI 反向被旧实现调用（罕见，需双向桥接）。
3. **不满足时的处理**：若依赖链上存在无法提供旧实现侧的模块，沿用 § 7.1「依赖链路限制」的三选一策略——(1) 整体迁移依赖链（A+B 同 Sprint），(2) B 降级为 `degrade_ffi` 供对比复用，或 (3) A 的 L3 验收延后到完整 G3 框架。

**可观测性**：PARITY.md 为每个模块增加「依赖图状态（Dependency Graph Status）」列，标注其依赖是否均满足 L3 FFI 参考条件，使「哪些模块当前可做 L3 FFI 对比」对人类可见。

> **M2+ 自动化预留（设计建议）**：M2 时可向 migration-state.json 模块元数据（schema 权威见 [09 附录 A](./09-appendix-schemas.md#附录-amigration-statejson-schema)）增加可选布尔字段 `ffi_ready_dependencies`，由 `rustmigrate validate state` 程序化计算并标注，把 M1 的人工检查升级为自动判定。M1 阶段不引入该字段，依赖状态由 verifier 按上述前置要求人工核查。

## 7.7 不等价证据探测维度清单

verifier SubAgent 在对抗性审查阶段，应在以下维度系统性探测新旧实现的行为差异。此清单作为 verifier 的"检查表"，确保不遗漏常见差异点：

| # | 探测维度 | 具体探测点 | 典型差异来源 |
|---|---------|-----------|-------------|
| 1 | **边界值** | 空输入、最大值、零、负数、最小值 | 不同语言对边界值的默认处理不同 |
| 2 | **类型边界** | null/undefined/NaN、整数溢出点（i32::MAX+1）、类型强制转换边界 | JS number 是 f64，Rust 有严格整数类型 |
| 3 | **集合操作** | 空集合、单元素、大集合（>10K 元素）、迭代顺序依赖 | HashMap 迭代顺序随机化 |
| 4 | **时间/日期** | 时区边界（UTC+-12）、夏令时切换、闰秒、epoch 前日期 | 时区库实现差异 |
| 5 | **字符串** | 空串、Unicode 多字节字符、emoji（多码点）、超长字符串（>1MB）、特殊字符（\0, \r\n） | UTF-8 vs UTF-16 长度语义 |
| 6 | **并发** | 多线程竞态、取消/超时、死锁场景、共享状态一致性 | GC vs 所有权模型差异 |
| 7 | **错误路径** | 所有 catch/except 分支、嵌套错误、错误链传播、panic vs Result | 异常模型 vs Result 模型 |
| 8 | **浮点精度** | 累积误差（长链计算）、比较精度（epsilon）、NaN 传播、+-Inf、-0.0 | IEEE 754 实现/优化差异 |

**行动指南**：verifier SubAgent 的系统提示中应包含此清单。每个模块验证时，verifier 根据模块涉及的数据类型和操作，选择相关维度生成针对性测试用例。

**测试生成目标（verifier 系统提示要点）**：测试生成针对 **Phase B 最终代码**，非 Phase A。Phase B 允许的语义重写（仅限并发/内存管理，见 §4.3 Step 5 边界）须与意图摘要保持 parity；若 MDR 显示某重写改变了错误处理策略、返回值语义或并发可见性，verifier 须先校验更新后的语义是否仍符合意图摘要，不符则先更新 `{module}-intent.md`（标记语义漂移并在 MDR 中说明）再生成测试。

---

# 八、迁移动机驱动的策略路由

不同动机决定不同的优先级和验收标准：

| 动机 | 迁移顺序 | 额外工具 | 验收标准 | 允许"不等价"？ |
|------|---------|---------|---------|---------------|
| 性能 | profiling 驱动，热路径优先 | criterion 必须 | benchmark >= 原版 | 是（更快的算法） |
| 内存安全 | unsafe 密集区优先 | cargo-geiger + Miri 必须 | CVE 消除 | 否 |
| 部署简化 | 整体迁移 | cross 交叉编译 | 单二进制部署成功 | 否 |
| 并发安全 | 并发热点优先 | loom/shuttle 推荐 | 编译通过 = 无数据竞争 | 否 |
| 合规 | 外部要求驱动 | cargo-deny 必须 | 审计报告通过 | 否 |

**行动指南**：PROFILE 阶段画像时确认迁移动机（支持多动机，`.rustmigrate.toml` 中 `migration_motives` 数组，首项为主要动机），据此自动配置 Tier 1/2 工具和验收标准。多动机场景下取各动机工具和验收标准的并集。

**§7.5 确定性指标阈值的动机调整**：质量评分卡（§7.5）的告警阈值按主要动机调整——`内存安全` 动机下「代码行数比」告警阈值放宽（3.0x → 3.5x，安全检查/边界处理会引入额外代码）；`性能` 动机保持严格阈值且新增 benchmark 必须项；多动机取最宽松的阈值并集。调整后的阈值作为 verifier 评分的输入。
