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

  Step 3: Phase A — 忠实翻译
    - 生成 Rust 代码，优先保持与源码的 1:1 对应（便于 diff 对照审查）
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

  Step 5: Phase B — 编译修正 + 惯用化优化
    - 基于审查报告修正语义偏差
    - 并发/内存管理部分允许重写（非直译），须记录 MDR
    - 惯用 Rust 优化（消除翻译腔）
    - 编译失败 → 先跑 `cargo fix --allow-dirty`（确定性自动修复）→ 剩余错误交给 AI 修复（最多 3 轮）
    - F1（rust-analyzer 诊断）+ F2（cargo test + clippy）反馈循环
    - 注意：此步骤只做编译验证和惯用化优化，不生成测试

  Step 6: 测试生成与验证
    - 翻译完成后，由 verifier SubAgent 生成测试（非 translator 同步生成）
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
- 双轨开发：每个 Sprint 开始前检查源项目变更，必要时更新迁移规则
- Strangler Fig：需要额外配置 FFI 桥接层和路由层

## 4.7 异步翻译策略

> **设计依据**：Claw-Code（TS→Rust）采用"边界异步、核心同步"策略，大幅降低了翻译复杂度；Pingora（C→Rust）则在 Tokio 之上构建了定制运行时。不同项目需要不同策略。

翻译时的异步处理不应硬性规定通用规则，而是按项目特征决定：

| 源项目特征 | 推荐策略 | 理由 |
|-----------|---------|------|
| 纯计算/CLI 工具 | **核心同步**，仅 I/O 边界异步 | 降低翻译复杂度，避免不必要的 async 传染 |
| Web 服务（Express/Flask） | **按需异步**，路由层 async，业务逻辑可同步 | 匹配 axum/actix 的异步模型 |
| 高并发运行时（事件循环） | **全栈异步**，须选择运行时并写 MDR | 必须用 tokio/async-std 重新设计并发模型 |

**行动指南**：PROFILE 阶段（项目画像）中评估源项目的异步模式，在 `.rustmigrate.toml` 的 `async_strategy` 字段记录决策。如果选择"全栈异步"，须写 MDR 记录运行时选择（tokio vs async-std）和取消安全性审查计划。

## 4.8 PROFILE → PLAN 中间步骤：原项目可复现基线

在 PROFILE（画像）和 PLAN（规划）之间，插入一个关键步骤：

1. 锁定源项目版本（git tag/commit hash）
2. 确认源项目能在本地完整构建和测试
3. 录制基线行为（CLI 输出、API 响应、测试结果）
4. 记录基线指标（测试覆盖率、性能数据、代码行数）

**执行者**：此步骤由 `/migrate analyze` 的分析阶段末尾执行（analyzer SubAgent 完成画像后，Skill 主上下文执行基线录制脚本）。`source_commit` 写入 `.rustmigrate.toml`。

**行动指南**：如果源项目本地构建失败，**停止迁移**——先修复源项目。

## 4.9 项目级止损标准

当迁移进展持续不佳时，需要及时止损而非无限投入。以下阈值为参考值，可在 `.rustmigrate.toml` 的 `[stop_loss]` 节中配置覆盖。

| 指标 | 阈值（默认） | 触发动作 |
|------|------------|---------|
| DEGRADE 比例 | >40% 模块降级为 FFI 桥接 | 暂停迁移，评估是否继续——大面积降级意味着 AI 翻译能力不足以处理该项目 |
| LLM API 成本 | 超出预算 2x | 暂停迁移，评估是优化 prompt/模块粒度还是放弃 |
| Sprint 停滞 | 连续 3 个 Sprint 未完成任何模块 | 召集团队评审，分析阻塞原因，决定是否继续 |

```toml
# .rustmigrate.toml 中的止损配置（可选覆盖）
[stop_loss]
degrade_ratio_threshold = 0.4        # 降级模块比例阈值
cost_multiplier_threshold = 2.0      # LLM API 成本超预算倍数
stalled_sprint_threshold = 3         # 连续停滞 Sprint 数
```

**行动指南**：`/migrate review` 在仪表板中展示止损指标的当前值和阈值距离。接近阈值时提前预警。

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

| L4 | 模糊测试 | cargo-fuzz | 2 | 随机输入差异对比 |
| L5 | 变异测试 | cargo-mutants | 2 | 验证测试真正保护了行为 |
| L6 | **性能回归** | criterion | 2（性能动机时提升为 1） | 无性能退化 |
| L7 | **并发正确性** | loom / shuttle | 2 | 并发模型正确性 |

**测试执行确定性保障**：
- proptest：固定 seed 记录到 `test-fixtures/proptest-regressions/`
- cargo-fuzz：corpus 持久化到 `test-fixtures/fuzz-corpus/`
- criterion：基线数据持久化到 `test-fixtures/benchmarks/`

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

**AI 辅助指标（verifier 评估，权重 30%）**：

| 指标 | 说明 |
|------|------|
| 惯用性 | 是否使用 idiomatic Rust 模式 |
| 语义保真度 | 意图摘要与实现的一致性 |
| 可维护性 | 代码结构清晰度、注释充分度 |

## 7.6 行为等价性验证

| 项目类型 | 录制方式 | 对比方式 |
|---------|---------|---------|
| CLI 工具 | args → stdout/stderr/exitcode | 黄金文件逐字节对比 |
| HTTP 服务 | mitmproxy 录制请求/响应 | JSON diff + header 对比 |
| 库/SDK | FFI(PyO3/napi-rs) 调用原实现 | proptest 生成输入对比输出 |
| 有状态服务 | 共享数据库 schema | 操作后状态 snapshot 对比 |

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
