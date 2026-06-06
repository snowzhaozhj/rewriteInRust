# Rust 迁移验证工作台 — 项目设计文档

> **版本**: v0.4 | **日期**: 2026-06-06 | **基于**: 18 路深度调研 + 103 agent deep research + 4 路审查反馈 + 3 项补充调研 + 8 条精细审查反馈 + 11 条精准修订

---

## 目录

1. [项目定位](#一项目定位)
2. [核心方法论](#二核心方法论)
3. [架构设计](#三架构设计)
4. [执行模式](#四执行模式sprint-循环模型)
5. [工具链选型](#五工具链选型)
6. [文档与知识沉淀体系](#六文档与知识沉淀体系)
7. [测试与验证策略](#七测试与验证策略)
8. [迁移动机驱动的策略路由](#八迁移动机驱动的策略路由)
9. [常见陷阱与缓解](#九常见陷阱与缓解)
10. [Claude Code 插件结构](#十claude-code-插件结构)
11. [工作流灵活性与扩展](#十一工作流灵活性与扩展)
12. [风险评估](#十二风险评估)
13. [实施路线图](#十三实施路线图)
14. [关键数据参考](#十四关键数据参考)

---

> ### 命名约定
>
> 本文档使用以下统一术语，避免混淆：
>
> | 术语 | 含义 | 示例 |
> |------|------|------|
> | **状态（State）** | 编排器状态机中的节点，描述迁移流程的当前位置 | INIT, PROFILE, PLAN, SCAFFOLD, SPRINT_LOOP, GRADUATE |
> | **里程碑（Milestone, M）** | 实施路线图中的交付阶段，描述产品成熟度 | M0 基础搭建, M1 MVP, M2 质量提升, M3 多语言, M4 完善 |
> | **Sprint** | SPRINT_LOOP 状态内的迭代循环，每个 Sprint 迁移一批模块 | Sprint 1, Sprint 2, ... |
>
> 文档中不再使用 "Phase 0/1/2/..." 等编号表述。

---

## 一、项目定位

### 1.1 我们要解决什么问题

任何人都能对 AI 说"把这个项目用 Rust 重写"，但结果一地鸡毛——语义丢失、边界遗漏、翻译腔代码、测试不过。数据显示：AI 生成代码的问题率是人类的 1.7 倍，逻辑错误多 75%，安全漏洞密度高 2.74 倍（*证据等级：论文验证，ACM CCS 2025*）。

**核心洞察**：代码生成正在商品化，**验证才是价值所在**。我们的定位不是"帮你自动迁移"，而是"帮你证明迁移是对的"。

### 1.2 我们不做什么

- **不做端到端自动迁移工具** — 与 Code Metal（$2 亿融资）正面竞争必输
- **不做通用代码翻译平台** — 太抽象，用户不知道干什么
- **不替代 Claude Code** — 我们是增强层，不是替代品
- **不承诺"等价性覆盖"** — 改为**不等价证据收集系统**：目标是发现差异，而非证明相同

### 1.3 我们做什么

一套 **Claude Code 插件生态**（Skills + Workflows + Hooks + SubAgents），将散落在各处的迁移最佳实践编码成**可重复执行的验证工作流**。

**核心价值主张**（按优先级）：

| 优先级 | 价值 | 说明 |
|--------|------|------|
| P0 | **验证基础设施** | 行为录制→差异测试→属性测试→模糊测试的自动化管线 |
| P1 | **方法论编码** | 把 Bun/Claw-Code 等成功案例的方法论打包进开箱即用的工作流 |
| P2 | **持久化产出物** | PORTING.md、PARITY.md、KNOWN_DIFFERENCES.md、MDR、测试集 |
| P3 | **项目自适应** | 根据源语言、项目形态、迁移动机自动选择策略 |
| 辅助 | AI 翻译能力 | 基于 LLM 的代码翻译——降级为辅助能力，不是核心卖点 |

### 1.4 目标用户

| 用户 | 核心诉求 | 我们提供什么 |
|------|---------|-------------|
| 想用 AI 做 Rust 迁移但质量不高的开发者/团队 | 质量保障 | 验证管线 + 方法论 |
| 有合规需求（内存安全）必须迁移到 Rust 的企业 | 审计证据 | KNOWN_DIFFERENCES.md + unsafe 审计报告 |
| 想系统化学习迁移方法论的工程师 | 学习路径 | PORTING.md + MDR 决策记录 |

---

## 二、核心方法论

### 2.1 设计原则：从 Understand Anything 学到的

UA 52,950 stars 的成功密码：**把 LLM 从"对话伙伴"变成"流水线中的处理节点"**（*证据等级：商业案例*）。

我们的设计遵循同样的原则：
- 用户执行 `/migrate`，流水线自动跑完所有阶段
- 确定性工具（tree-sitter/AST）做结构分析，LLM 做语义翻译
- 所有中间产物持久化，支持断点续传
- 主 SKILL.md 足够详细（UA 的主 SKILL.md 有 45KB）

### 2.2 三层范式：AI-工具-人类

| 层 | 角色 | 负责什么 | 信任度 |
|----|------|---------|--------|
| AI | 高吞吐执行 | 语义理解、代码翻译、测试生成 | 低——必须被验证 |
| 工具 | 确定性约束 | 编译器、Lint、AST 分析、覆盖率、模糊测试 | 高——确定性输出 |
| 人类 | 判断与责任 | 架构决策、发布节奏、兼容性、unsafe 审计 | 最终决策者 |

### 2.3 意图驱动而非逐行直译

来自 CSDN 深度文章和 pi_agent_rust 项目的核心方法论（*证据等级：社区实践*）：

1. **逻辑解构** — AI 阅读源码，总结核心职责、数据契约、副作用、异常流（不含任何源语言语法）
2. **环境约束** — 人类定义架构"宪法"（PORTING.md）
3. **原生重塑** — 用 idiomatic Rust 重新实现，而非翻译

**行动指南**：每个模块迁移前，先让 AI 生成纯文本的"意图摘要"，人类确认后才开始翻译。

### 2.4 学术前沿技术集成

| 技术 | 来源 | 效果 | 证据等级 |
|------|------|------|---------|
| 编译器反馈迭代修复 | SafeTrans (ACM CCS 2025) | 成功率 54% → 80% | 论文验证 |
| 依赖图拓扑排序翻译 | DepTrans (ACM FSE 2026) | 仅需修改 <15% 目标代码 | 论文验证 |
| 多候选生成+最优选择 | LAC2R / MCTS | 避免陷入局部最优修复 | 论文验证 |
| Few-shot 引导修复 | SafeTrans | 为每类错误准备匹配的修复示例库 | 论文验证 |
| 等价性验证 | MatchFixAgent (ICML 2026) | 99.2% 等价性判定覆盖率 | 论文验证 |
| 编译环境反馈 | Environment-in-the-Loop (ACM ReCode 2026) | 编译环境作为循环参与者 | 论文验证 |

---

## 三、架构设计

### 3.1 整体架构

```
┌─────────────────────────────────────────────────────────────────┐
│                      用户入口 (Skills)                           │
│  /migrate-init  /migrate-plan  /migrate-run  /migrate-verify    │
│  /migrate-status  /migrate-graduate  /migrate-unsafe-audit      │
├─────────────────────────────────────────────────────────────────┤
│                      编排层 (Workflows)                          │
│  Sprint 管理 → 策略路由 → 状态机 → 断点续传 → 错误恢复           │
├──────────────────────────────┬──────────────────────────────────┤
│         分析层               │            转换层                 │
│  tree-sitter (多语言 AST)    │  LLM 翻译引擎（意图驱动）        │
│  Mypy (Python 类型提取)      │  PORTING.md 规则引擎             │
│  TS Compiler API (TS 类型)   │  syn+quote (Rust 代码生成)       │
│  dependency-cruiser (JS 依赖)│  ast-grep (模式匹配重写)         │
│  import-linter+grimp (Py依赖)│                                  │
├──────────────────────────────┴──────────────────────────────────┤
│                      验证层 (Harness) — 核心价值                 │
│  Tier 0: cargo check → clippy → cargo-nextest                   │
│  Tier 1: cargo-llvm-cov → insta → proptest → cargo-deny/audit  │
│  Tier 2: cargo-fuzz → cargo-mutants → Miri → criterion → loom  │
│  差异测试框架 → 行为录制 → 不等价证据收集                        │
├─────────────────────────────────────────────────────────────────┤
│                      质量门禁                                    │
│  F1 Hook: PostToolUse 写入 .rs → cargo check (Tier 0)            │
│  F2 指令: Skill SKILL.md 步骤完成 → 测试套件 (Tier 0+1)          │
│  F3 Skill: /migrate-verify 手动触发 → 集成验证 (Tier 0+1+2)      │
├─────────────────────────────────────────────────────────────────┤
│                      FFI 桥接层 (增量迁移/降级路径)               │
│  napi-rs (Node.js) │ PyO3 (Python) │ cxx/bindgen (C/C++)        │
├─────────────────────────────────────────────────────────────────┤
│                      产出物 (Artifacts)                          │
│  .rust-migration/                                               │
│  ├── PORTING.md              # 迁移规则宪法（26 类）             │
│  ├── PARITY.md               # 迁移进度跟踪（Sprint 聚合）      │
│  ├── KNOWN_DIFFERENCES.md    # 已知行为差异登记簿               │
│  ├── AGENTS.md               # AI 行为约束                      │
│  ├── migration-state.json    # 状态机 + Sprint 元数据           │
│  ├── .rustmigrate.toml       # 项目配置                         │
│  ├── decisions/              # MDR 迁移决策记录                  │
│  ├── intermediate/           # 中间产物                          │
│  ├── test-fixtures/          # 行为录制测试集                    │
│  └── reports/                # 验证报告                          │
└─────────────────────────────────────────────────────────────────┘
```

### 3.2 关键架构决策

#### 3.2.1 砍掉"通用类型 IR"

**原设计**（G1）：从 TS/Python/Go 提取类型 → 统一中间表示 → idiomatic Rust 类型。

**修订**：砍掉统一 IR，改为**语言专用类型提取 + LLM 映射**。理由：
- 通用 IR 复杂度高，维护成本大
- LLM 已经擅长做类型映射
- 不同语言的类型系统差异太大，统一 IR 会丢失语义

**行动指南**：每种语言实现独立的 `LanguageAdapter`（见第十一章），负责类型提取，映射交给 LLM + PORTING.md 规则。

#### 3.2.2 拓扑排序保留，跨语言图对比砍掉

**保留**：依赖图拓扑排序指导迁移顺序（G2 的核心功能）。
**砍掉**：源/目标代码结构图的"对比验证"——过度设计，测试覆盖已经够用。

#### 3.2.3 PROFILE 与 PLAN 边界清晰化

| 状态 | 职责 | 性质 |
|------|------|------|
| PROFILE | 客观事实采集：语言、框架、依赖数、代码行数、测试现状 | 纯自动化，无需人类判断 |
| PLAN | 主观决策：迁移策略、规则制定、优先级排序 | 必须人类审查确认 |

#### 3.2.4 SubAgent 合并（7 → 4）

| Agent | 合并前 | 职责 |
|-------|--------|------|
| `analyzer` | project-profiler + rust-idiom-checker | 源码分析、项目画像、惯用法检查 |
| `translator` | code-translator + porting-guide-writer | 规则生成、代码翻译（意图驱动） |
| `verifier` | equivalence-checker + adversarial-reviewer | 等价性验证、对抗性审查、不等价证据收集 |
| `scaffolder` | test-scaffolder | 测试基础设施搭建、行为录制 |

### 3.3 必须自建的组件

| # | 组件 | 复杂度 | MVP? | 说明 |
|---|------|--------|------|------|
| G1 | ~~跨语言类型 IR~~ | — | — | **已砍掉**，改为语言专用适配器 |
| G2 | 依赖图拓扑排序引擎 | 中 | 是 | petgraph + 迁移顺序决策 |
| G3 | 统一差异测试框架 | 中 | 否 | 跨 HTTP/CLI/库接口的录制回放 + 对比引擎 |
| G4 | Rust Scientist 库 | 低 | 否 | 并行执行新旧代码路径，~300 行 |
| G5 | 统一依赖图格式转换器 | 低 | 是 | 各工具输出 → 统一格式 |
| G6 | AI 迁移编排器 | 高 | 是 | 状态机 + 错误恢复 + 断点续传 |

### 3.4 编排器状态机设计

```
                    ┌─────────┐
                    │  INIT   │
                    └────┬────┘
                         ▼
                    ┌─────────┐
              ┌─────│ PROFILE │   客观事实采集
              │     └────┬────┘
              │          ▼
         error│     ┌─────────┐
         recovery   │  PLAN   │   主观决策
              │     └────┬────┘
              │          ▼
              │     ┌─────────┐
              ├─────│ SCAFFOLD│   测试基础设施搭建
              │     └────┬────┘
              │          ▼
              │     ┌─────────────────────────┐
              │     │    SPRINT LOOP           │
              │     │  ┌───────────┐           │
              └────►│  │ TRANSLATE │──► CHECK  │──► 失败 3 轮 → DEGRADE
                    │  └───────────┘     │     │
                    │       ▲            ▼     │
                    │       │         VERIFY   │
                    │       │            │     │
                    │       └── RETRY ◄──┘     │
                    └─────────────────────────┘
                              │
                              ▼
                         ┌─────────┐
                         │GRADUATE │   知识固化 + 模式退出
                         └─────────┘

降级路径（DEGRADE）：
  → FFI 桥接（保持原实现，Rust 端调用）
  → 人工介入（标记 TODO，等人类处理）
  → 功能裁剪（协商后移除该功能）
```

**降级后依赖级联影响**：当模块 A 降级为 FFI 桥接时，依赖 A 的模块 B 的翻译需要知道 A 的接口类型（FFI 桥接 vs 原生 Rust）。编排器在 A 降级后需要：
1. 更新 `migration-state.json` 中 A 的状态为 `degrade(ffi)`，并记录 A 的 FFI 接口描述
2. 后续模块 B 的翻译上下文中注入 A 的接口类型信息——如果 A 是 FFI 桥接，B 需要通过 FFI 调用 A 而非直接 Rust 调用
3. 如果 A 后来从 FFI 桥接升级为原生 Rust，B 的调用方式也需要同步更新

**断点续传**：`migration-state.json` 记录每个模块的状态和最近一次成功的 checkpoint，重启后从 checkpoint 恢复。

**错误恢复**：每次翻译尝试的输入输出都持久化到 `intermediate/attempts/`，失败后可回溯分析。

### 3.5 LLM 上下文窗口管理

| 策略 | 说明 |
|------|------|
| 分层注入 | PORTING.md 规则按相关性注入，不是全量塞入 |
| 模块隔离 | 每个模块翻译在独立对话中完成，避免上下文污染 |
| 摘要压缩 | 依赖模块只注入接口签名，不注入实现 |
| 上下文预算 | 每次翻译的上下文 = 源码 + 相关规则 + 依赖接口 ≤ 100K tokens |

**行动指南**：编排器在调度翻译任务前，先计算上下文预算；超预算则拆分模块。

---

## 四、执行模式（Sprint 循环模型）

### 4.1 从线性阶段到 Sprint 循环

原设计的线性阶段划分在实际执行中存在问题：
- 大项目不可能等所有测试搭好再开始迁移
- 迁移过程中会发现新规则，需要回头更新 PORTING.md
- 不同模块可能处于不同阶段

**修订**：改为 Sprint 循环模型，分两层循环。

### 4.2 外循环：Sprint 级（跨会话/天/周）

```
Sprint N:
  1. Sprint Planning
     - 选择本 Sprint 要迁移的模块（按拓扑排序）
     - 确认 PORTING.md 规则是否需要更新
     - 确认测试基础设施是否就绪

  2. 执行（多个 Work Unit）
     - 每个 Work Unit = 一个完整的 Claude Code 会话
     - 每个 Work Unit 迁移 1-3 个模块
     - 产出：Rust 代码 + 测试 + MDR

  3. Sprint Review
     - 集成验证（Tier 0 + Tier 1 + 按需 Tier 2）
     - 更新 PARITY.md
     - 回顾 PORTING.md，追加新发现的规则
     - 更新 KNOWN_DIFFERENCES.md
     - 评估是否需要调整迁移策略

  4. Sprint Retrospective
     - 哪些规则频繁触发失败？→ 补充到 PORTING.md
     - 哪些工具信噪比低？→ 调整 Tier 级别
     - 上下文管理是否够用？→ 调整模块粒度
```

**行动指南**：每个 Sprint 以 `migration-state.json` 中的 Sprint 元数据为准，包含 Sprint 目标、已完成模块、阻塞项。

### 4.3 内循环：模块级（单会话内）

```
Work Unit（一个 Claude Code 会话）:

  Step 1: 上下文加载
    - 读取 migration-state.json 确认当前任务
    - 读取 PORTING.md 中相关规则
    - 读取目标模块源码 + 依赖接口

  Step 2: 语义解构
    - AI 生成意图摘要（纯文本，不含源语言语法）
    - 识别关键语义点：错误处理、并发、状态管理

  Step 3: 翻译 + 即时验证
    - 生成 Rust 代码
    - F1 反馈：cargo check（秒级，每次写入触发）
    - 编译失败 → 即时修复（最多 3 轮）

  Step 4: 测试验证
    - F2 反馈：cargo test + clippy（分钟级）
    - 测试失败 → 分析原因 → 修复或记录到 KNOWN_DIFFERENCES.md

  Step 5: 产出物更新
    - 更新 PARITY.md（模块状态）
    - 写 MDR（如有架构决策）
    - 更新 migration-state.json
```

### 4.4 三层反馈循环

| 层级 | 触发时机 | 延迟 | 内容 | 处理方式 |
|------|---------|------|------|---------|
| F1 编译反馈 | 每次写入 .rs 文件 | 秒级 | cargo check 错误 | 自动反馈给 LLM 重试 |
| F2 测试反馈 | 模块翻译完成 | 分钟级 | 测试失败 + clippy 警告 | AI 分析修复或标记差异 |
| F3 集成反馈 | Sprint Review | Sprint 级 | 集成测试 + 覆盖率 + 性能基准 | 团队决策是否通过 |

**行动指南**：Hook 配置中，F1 对应 `PostToolUse`（真实 Claude Code Hook 事件）；F2 在 Skill 的 SKILL.md 中通过分步指令要求"翻译步骤完成后执行验证命令"；F3 由 `/migrate-verify` Skill 手动触发。

### 4.5 问题前移矩阵

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

### 4.6 并行开发策略

迁移期间源项目可能还在演化，需要选择并行策略：

| 策略 | 适用场景 | 操作 | 风险 |
|------|---------|------|------|
| **功能冻结** | 小项目、短期迁移 | 迁移期间源项目不接受新功能 | 业务停滞 |
| **双轨开发** | 中型项目 | 源项目继续开发，每个 Sprint 同步变更到 Rust | 同步成本高 |
| **Strangler Fig** | 大型项目、长期迁移 | 通过路由层逐模块切换，新旧并行运行 | 架构复杂 |

**行动指南**：
- 在 PROFILE 阶段（项目画像）中决定并行策略
- 功能冻结：在 `migration-state.json` 中锁定源码 commit hash
- 双轨开发：每个 Sprint 开始前检查源项目变更，必要时更新 PORTING.md
- Strangler Fig：需要额外配置 FFI 桥接层和路由层

### 4.7 PROFILE → PLAN 中间步骤：原项目可复现基线

在 PROFILE（画像）和 PLAN（规划）之间，插入一个关键步骤：

1. 锁定源项目版本（git tag/commit hash）
2. 确认源项目能在本地完整构建和测试
3. 录制基线行为（CLI 输出、API 响应、测试结果）
4. 记录基线指标（测试覆盖率、性能数据、代码行数）

**行动指南**：如果源项目本地构建失败，**停止迁移**——先修复源项目。

### 4.8 项目级止损标准

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

**行动指南**：`/migrate-status` 在仪表板中展示止损指标的当前值和阈值距离。接近阈值时提前预警。

---

## 五、工具链选型

### 5.1 三档分级

| Tier | 含义 | 触发方式 | 失败影响 |
|------|------|---------|---------|
| **Tier 0 硬性门禁** | 每次写入/提交必须通过 | Hook 自动触发 | 阻塞继续 |
| **Tier 1 推荐** | 画像自动启用，可按需关闭 | Sprint Review 触发 | 警告但不阻塞 |
| **Tier 2 高级** | 用户显式启用 | 手动触发 | 可选 |

### 5.2 Tier 0：硬性门禁

每次代码变更必须通过，无例外。

| 类别 | 工具 | 用途 | 生产验证 |
|------|------|------|---------|
| 编译 | **cargo check** | 编译通过 | Rust 标准工具 |
| Lint | **cargo clippy** | 惯用性检查 | Rust 标准工具 |
| 测试 | **cargo-nextest** | 测试执行 | cargo test 全面升级 |

### 5.3 Tier 1：推荐（画像自动启用）

| 类别 | 工具 | 用途 | 何时启用 |
|------|------|------|---------|
| 覆盖率 | **cargo-llvm-cov** | LLVM 原生覆盖率 | 始终 |
| 快照 | **insta** | 快照测试（锁定输出） | 有 CLI/API 输出时 |
| 属性 | **proptest** | 属性测试（等价性验证） | 有纯函数时 |
| 许可证 | **cargo-deny** | 许可证合规 + 依赖审计 | Sprint Review 触发 |
| CVE | **cargo-audit** | 已知漏洞扫描 | Sprint Review 触发 |
| 搜索/重写 | **ast-grep** | 模式匹配 + 代码重写 | 始终 |
| 统计 | **tokei + scc** | 代码复杂度对比 | 始终 |
| 多语言 AST | **tree-sitter** | 源码结构分析 | 始终 |
| Rust 代码生成 | **syn + quote** | 宏/过程宏 | 需要代码生成时 |
| 性能基准 | **criterion** | 性能回归检测 | 默认 Tier 2；当 `migration_motives` 含 `performance` 时自动提升为 Tier 1 |
| unsafe 审计 | **cargo-geiger** | unsafe 使用统计 | 始终 |
| 任务运行 | **just** | 任务自动化 | 始终 |
| 文件监控 | **bacon** | 持续编译反馈 | 本地开发 |

**语言专用工具**：

| 源语言 | 工具 | 用途 |
|--------|------|------|
| JS/TS | **dependency-cruiser** | 依赖图分析 |
| Python | **Mypy** | 类型提取 |
| Python | **import-linter + grimp** | 依赖图分析 |

**FFI 桥接**（按需启用）：

| 目标 | 工具 | 生产验证 |
|------|------|---------|
| Node.js | **napi-rs** | SWC/Next.js |
| Python | **PyO3 + maturin** | OpenAI, Hugging Face |
| C/C++ | **bindgen + cbindgen** | Rust 官方标准 |

### 5.4 Tier 2：高级（用户显式启用）

| 类别 | 工具 | 用途 | 风险/注意 |
|------|------|------|---------|
| 模糊测试 | **cargo-fuzz** | 随机输入差异对比 | 需要 corpus 管理 |
| 变异测试 | **cargo-mutants** | 验证测试质量 | 大项目耗时长 |
| UB 检测 | **Miri** | unsafe 代码 UB 检测 | 不支持所有 FFI |
| 形式化 | **Kani** | 关键路径验证 | 474 issues，有限制 |
| 并发 | **loom / shuttle** | 并发正确性验证 | 需要专门编写测试 |
| 安全扫描 | **Semgrep/OpenGrep** | 安全模式检测 | Rust 规则较少 |
| 精细编译 | **cargo-careful** | 额外 UB 检测 | 编译慢 |

### 5.5 谨慎使用（TRIAL）

| 工具 | 风险 | 缓解措施 |
|------|------|---------|
| **petgraph** | bus factor=1，279 issues | 轻量场景可自建 adjacency list |
| **cxx** | 作者称 MVP | 复杂 C++ 考虑 autocxx + bindgen |
| **OXC** (oxc_parser) | 0.x API 不稳定 | 备选 tree-sitter-typescript |
| **Semgrep** (Rust 规则) | Rust 规则太少 | 仅作为补充，不作为主要安全工具 |

### 5.6 明确不用（AVOID）

| 工具 | 原因 | 替代 |
|------|------|------|
| GoReplay | 停滞，仅 HTTP/1.1 | mitmproxy |
| madge | 2024 年 8 月后无更新 | dependency-cruiser |
| Pyright (作为管道工具) | 无 Python API | Mypy |
| pydeps | 无法处理动态导入 | import-linter + grimp |
| cargo-tarpaulin | 被 cargo-llvm-cov 超越 | cargo-llvm-cov |
| bolero | 维护不够 | cargo-fuzz |
| D2 | pre-1.0 | Mermaid |
| FalkorDB | 过度设计 | petgraph + SQLite |

### 5.7 图存储策略

- **内存图处理**：petgraph（DAG 依赖图、拓扑排序）
- **持久化存储**：SQLite（节点+边表，JSON 属性字段）
- **查询深度**：控制在 4-5 层以内（SQLite 递归 CTE 性能范围）
- **可视化**：Mermaid（文档内嵌）+ Graphviz DOT（自动生成）

---

## 六、文档与知识沉淀体系

### 6.1 核心产出物总览

| 产出物 | 用途 | 生成方式 | 生命周期 |
|--------|------|---------|---------|
| PORTING.md | 迁移规则宪法 | AI 初版 + 人工审查 + Sprint 迭代 | 长期保留 |
| PARITY.md | 迁移进度跟踪 | 自动更新 | 迁移完成后归档 |
| KNOWN_DIFFERENCES.md | 已知行为差异登记 | 验证过程自动生成 + 人工确认 | 长期保留 |
| AGENTS.md | AI 行为约束 | 模板 + 项目定制 | 迁移完成后可丢弃 |
| MDR（迁移决策记录） | 架构决策溯源 | 人工编写或 AI 辅助 | 长期保留 |
| migration-state.json | 状态机 + Sprint 元数据 | 自动管理 | 迁移完成后归档 |
| test-fixtures/ | 行为录制测试集 | 自动录制 | 长期保留（回归测试） |

### 6.2 PORTING.md — 迁移规则宪法（26 类）

参考 Bun 的 576 行 PORTING.md，定义所有翻译规则。**渐进式生成**：PLAN 阶段生成必须的核心规则，后续 Sprint 按需追加。

| # | 规则类 | MVP? | 说明 |
|---|--------|------|------|
| 1 | 迁移阶段定义 | 是 | Sprint 目标和模块优先级 |
| 2 | 类型映射表 | 是 | 源类型 → Rust 类型 |
| 3 | 错误处理模式 | 是 | try/catch → Result<T,E>，anyhow vs thiserror |
| 4 | 内存管理/分配器策略 | 是 | GC → 所有权模型 |
| 5 | 指针/引用映射 | 是 | 指针 → 引用/智能指针 |
| 6 | 并发模式 | 否 | 锁/通道/异步映射 |
| 7 | 字符串处理 | 是 | UTF-8/UTF-16 差异 |
| 8 | 命名约定转换 | 是 | camelCase → snake_case |
| 9 | 模块/Crate 结构 | 是 | 包结构 → Cargo workspace |
| 10 | 标准库函数映射 | 是 | 常用函数对照表 |
| 11 | 禁止模式清单 | 是 | 禁止的反模式 |
| 12 | unsafe 使用策略 | 是 | 何时允许、如何标注 |
| 13 | 外部依赖映射 | 是 | npm/pip 包 → Rust crate |
| 14 | FFI 边界规则 | 否 | 桥接层设计规范 |
| 15 | 全局状态处理 | 是 | 全局变量 → OnceLock/lazy_static |
| 16 | 调度/热路径规则 | 否 | 性能敏感路径的特殊处理 |
| 17 | 测试模式翻译 | 是 | 测试框架映射 |
| 18 | 构建系统规则 | 是 | package.json/setup.py → Cargo.toml |
| 19 | 惯用法映射表 | 是 | 源语言惯用法 → Rust 惯用法 |
| 20 | 不确定性处理 | 是 | 留 TODO，禁止猜测 |
| 21 | **生命周期与所有权模式** | 否 | 引用生命周期、借用模式映射 |
| 22 | **异步运行时与并发原语** | 否 | tokio/async-std 选择、Future 映射、取消安全性审查（select!/timeout 中的 Future） |
| 23 | **序列化/反序列化兼容性** | 否 | JSON/protobuf 字节级兼容 |
| 24 | **日志/可观测性映射** | 否 | 日志框架 → tracing，格式兼容 |
| 25 | **平台特定行为映射** | 否 | OS API → cfg 条件编译 |
| 26 | **多态/动态分发映射** | 否 | 接口/继承/泛型 → trait/enum dispatch/泛型，虚表 vs 静态分发选择 |

**版本化演化**：每条规则有版本号和变更记录，每个 Sprint Review 可以修改规则，但必须记录变更原因。

**行动指南**：
- MVP 阶段只生成标记"是"的规则
- 每次翻译失败且原因是规则缺失时，追加新规则并标注"由 Sprint N 失败触发"
- 规则格式：`源语言模式 → Rust 等价物 + 注意事项 + 示例`

### 6.3 PARITY.md — 迁移进度跟踪

参考 Claw-Code 的 9-lane checkpoint 系统，增强为 Sprint 级聚合视图。

```markdown
# 迁移进度

## Sprint 聚合视图
| Sprint | 目标模块数 | 已完成 | 通过率 | 覆盖率 | 阻塞项 |
|--------|-----------|--------|--------|--------|--------|
| S1     | 3         | 2      | 95%    | 82%    | 模块 C 类型复杂 |

## 模块详情
| 模块 | 状态 | Sprint | 尝试次数 | 测试通过 | 覆盖率 | 已知差异 | 风险 |
|------|------|--------|---------|---------|--------|---------|------|
| utils/string | done | S1 | 1 | 24/24 | 91% | 0 | 低 |
| core/parser | testing | S1 | 2 | 18/22 | 76% | 1 | 中 |
| core/runtime | pending | S2 | 0 | — | — | — | 高 |
```

**管理层视图**：PARITY.md 顶部的聚合表可直接用于向管理层汇报。

### 6.4 KNOWN_DIFFERENCES.md — 已知行为差异登记簿

记录所有已知的、经过评审确认可接受的行为差异。

```markdown
# 已知行为差异

## KD-001: HashMap 迭代顺序
- **模块**: core/config
- **差异**: 原版 JS Object.keys() 保持插入顺序，Rust HashMap 顺序随机
- **影响**: 配置输出顺序不同，不影响功能
- **决策**: 接受差异，文档记录
- **审批**: @zhangsan 2026-06-10
- **关联 MDR**: MDR-003

## KD-002: 浮点精度
- **模块**: math/statistics
- **差异**: 第 15 位小数有差异（IEEE 754 实现差异）
- **影响**: 统计计算结果微小偏差
- **决策**: 接受，epsilon 阈值 1e-12
- **审批**: @lisi 2026-06-12
```

**行动指南**：验证阶段发现行为差异时，先记录到此文件，再由人类决定是"修复"还是"接受"。

### 6.5 MDR — 迁移决策记录

每个重要架构决策写一条 MDR，格式：

```markdown
# MDR-001: 错误处理策略选择

- **日期**: 2026-06-08
- **状态**: 已采纳
- **Sprint**: S1
- **背景**: 源项目使用 try/catch 异常处理，需要选择 Rust 错误处理策略
- **选项**:
  1. anyhow（简单，适合应用层）
  2. thiserror（类型化，适合库）
  3. 混合（库用 thiserror，应用层用 anyhow）
- **决策**: 选项 3
- **理由**: 迁移目标包含库和应用，分层处理最合理
- **后果**: 需要在 PORTING.md 中分别定义两种错误处理规则
```

**行动指南**：以下情况必须写 MDR：
- 异步运行时选择（tokio vs async-std）
- 错误处理策略
- Cargo workspace 结构
- FFI 桥接方案选择
- 功能裁剪决策

### 6.6 AGENTS.md — AI 行为约束

```markdown
# AI 行为约束

## 硬性规则
- 禁止删除源项目文件
- Git 操作限制：只允许 commit/branch，禁止 force push/rebase
- 禁止引入未经 cargo-deny 审查的依赖
- 不确定时必须留 TODO（格式：TODO(migrate): 描述 [不确定原因]）
- 每个 unsafe 块必须有 // SAFETY: 注释

## 翻译规则
- 先读 PORTING.md 相关规则，再开始翻译
- 翻译前先输出意图摘要，确认后再生成代码
- 禁止逐行直译，必须用 idiomatic Rust
- 优先使用标准库，其次用 PORTING.md 指定的 crate

## 测试规则
- 新代码必须有测试
- 修改已有代码必须跑相关测试
- 覆盖率不低于源模块
```

### 6.7 三级代码注释策略

| 级别 | 位置 | 内容 | 示例 |
|------|------|------|------|
| 模块级溯源 | 文件顶部 | 源文件路径 + 迁移 Sprint | `// Migrated from: src/utils/string.ts (Sprint S1)` |
| 函数级决策 | 函数上方 | 翻译决策说明 | `// 原版用 try/catch，此处改为 Result + ? 传播 (MDR-001)` |
| 行内保留 | 特殊行 | 不明确的语义保留原因 | `// 保持与原版相同的溢出行为 (wrapping_add)` |

### 6.8 CLAUDE.md 迁移配置

```markdown
# 迁移项目配置

## 核心规则
- 本项目正在从 [源语言] 迁移到 Rust
- 所有迁移规则见 .rust-migration/PORTING.md，必须严格遵守
- 迁移进度见 .rust-migration/PARITY.md
- 已知差异见 .rust-migration/KNOWN_DIFFERENCES.md
- AI 行为约束见 .rust-migration/AGENTS.md

## 当前状态
- Sprint [N]，当前聚焦模块：[模块名]
- 并行策略：[功能冻结/双轨/Strangler Fig]
- 源码锁定版本：[commit hash]

## 验证要求
- 每次写入 .rs：cargo check（自动）
- 每个模块完成：cargo test + clippy + 覆盖率 ≥ 原版
- 每个 Sprint：集成验证 + PARITY.md 更新
```

### 6.9 迁移产物生命周期

| 产出物 | 迁移期间 | 迁移完成后 | 说明 |
|--------|---------|-----------|------|
| PORTING.md | 活跃维护 | **长期保留** | 后续维护参考 |
| PARITY.md | 活跃更新 | 归档 | 不再需要 |
| KNOWN_DIFFERENCES.md | 活跃更新 | **长期保留** | 故障排查参考 |
| AGENTS.md | 活跃使用 | 安全丢弃 | 仅迁移期间有效 |
| MDR | 按需写 | **长期保留** | 架构决策溯源 |
| migration-state.json | 核心状态 | 归档 | 不再需要 |
| test-fixtures/ | 活跃使用 | **长期保留** | 回归测试资产 |

### 6.10 GRADUATE：知识固化与迁移毕业

迁移不只是"代码转完就结束"。需要一个正式的毕业流程：

**Graduation Criteria（毕业标准）**：
- [ ] 所有模块状态为 done 或 degrade(FFI)
- [ ] KNOWN_DIFFERENCES.md 中所有差异已评审
- [ ] 测试覆盖率 ≥ 原项目
- [ ] P0 级 unsafe 全部消除，P1 级 unsafe 全部封装审计完毕，P4 级 unsafe 全部重新归类（见 unsafe 矩阵）
- [ ] 性能基准无退化（允许 ±10%）
- [ ] CI/CD 已切换到 Rust 构建
- [ ] 团队完成 Rust 培训，能独立维护

**行动指南**：使用 `/migrate-graduate` skill 评估是否满足毕业标准。满足后：
1. 移除 AGENTS.md 和 CLAUDE.md 中的迁移配置
2. 归档 PARITY.md 和 migration-state.json
3. 保留 PORTING.md、KNOWN_DIFFERENCES.md、MDR、test-fixtures

---

## 七、测试与验证策略

### 7.1 测试分层（L0-L7）

| 层 | 名称 | 工具 | Tier | 目标 |
|----|------|------|------|------|
| L0 | **单元测试** | cargo test | 0 | 基础正确性 |
| L1 | **快照/黄金文件测试** | insta | 1 | 锁定输入/输出对（合并原 L1+L3，消除重叠） |
| L2 | 属性测试 | proptest | 1 | `for all x: old(x) == new(x)` |
| L3 | **E2E 差异测试** | 自建差异框架 | 1 | 整体行为等价 |
| L4 | 模糊测试 | cargo-fuzz | 2 | 随机输入差异对比 |
| L5 | 变异测试 | cargo-mutants | 2 | 验证测试真正保护了行为 |
| L6 | **性能回归** | criterion | 2（性能动机时提升为 1） | 无性能退化 |
| L7 | **并发正确性** | loom / shuttle | 2 | 并发模型正确性 |

**测试执行确定性保障**：
- proptest：固定 seed 记录到 `test-fixtures/proptest-regressions/`
- cargo-fuzz：corpus 持久化到 `test-fixtures/fuzz-corpus/`
- criterion：基线数据持久化到 `test-fixtures/benchmarks/`

### 7.2 测试基础设施搭建（SCAFFOLD 阶段修正）

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

Step 4: 模块迁移完成后，才生成 Rust 测试
  ├── 将黄金测试集翻译为 Rust
  ├── 将行为录制转为 insta 快照测试
  └── 基于接口契约生成 proptest
```

**行动指南**：测试搭建的核心是**行为录制和接口契约**，这两者不依赖 Rust 代码的存在。

### 7.3 验证管线（DAG 结构，非线性）

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

### 7.4 "翻译膨胀"检测标准

| 指标 | 健康范围 | 告警阈值 | 行动 |
|------|---------|---------|------|
| 代码行数比 | 1.2x - 2.0x | > 3.0x | 提示 LLM 简化 |
| 圈复杂度比 | 0.8x - 1.2x | > 1.5x | 审查逻辑结构 |
| 函数数量比 | 0.9x - 1.3x | > 2.0x | 检查是否过度拆分 |

### 7.5 行为等价性验证

| 项目类型 | 录制方式 | 对比方式 |
|---------|---------|---------|
| CLI 工具 | args → stdout/stderr/exitcode | 黄金文件逐字节对比 |
| HTTP 服务 | mitmproxy 录制请求/响应 | JSON diff + header 对比 |
| 库/SDK | FFI(PyO3/napi-rs) 调用原实现 | proptest 生成输入对比输出 |
| 有状态服务 | 共享数据库 schema | 操作后状态 snapshot 对比 |

---

## 八、迁移动机驱动的策略路由

不同动机决定不同的优先级和验收标准：

| 动机 | 迁移顺序 | 额外工具 | 验收标准 | 允许"不等价"？ |
|------|---------|---------|---------|---------------|
| 性能 | profiling 驱动，热路径优先 | criterion 必须 | benchmark ≥ 原版 | 是（更快的算法） |
| 内存安全 | unsafe 密集区优先 | cargo-geiger + Miri 必须 | CVE 消除 | 否 |
| 部署简化 | 整体迁移 | cross 交叉编译 | 单二进制部署成功 | 否 |
| 并发安全 | 并发热点优先 | loom/shuttle 推荐 | 编译通过 = 无数据竞争 | 否 |
| 合规 | 外部要求驱动 | cargo-deny 必须 | 审计报告通过 | 否 |

**行动指南**：PROFILE 阶段画像时确认迁移动机（支持多动机，`.rustmigrate.toml` 中 `migration_motives` 数组，首项为主要动机），据此自动配置 Tier 1/2 工具和验收标准。多动机场景下取各动机工具和验收标准的并集。

---

## 九、常见陷阱与缓解

### 9.1 技术陷阱

| 陷阱 | 说明 | 缓解措施 |
|------|------|---------|
| 逐行直译 | 得到 `Arc<Mutex<>>` 满天飞的代码 | 意图驱动重构：先解构语义，再用 Rust 惯用法重新实现 |
| HashMap 迭代顺序 | Rust HashMap 随机化哈希 | 差异测试 + 需要顺序时用 BTreeMap 或 IndexMap |
| UTF-8 vs UTF-16 | JS string.length 返回 UTF-16 code units | 专门的字符串语义测试用例 |
| 整数溢出 | Debug panic, Release wrapping | PORTING.md 中明确溢出策略 |
| 全局状态 | 模块级变量 → OnceLock/lazy_static | 全局状态审计作为 PROFILE 阶段的一部分 |
| 错误处理范式 | 异常冒泡 ≠ Result 传播 | 重新设计错误处理策略（MDR 记录） |
| 浮点精度 | 不同编译器/平台结果不同 | 数值计算的 epsilon 对比测试 |
| 析构顺序 | Rust Drop 与 GC finalizer 不同 | 资源清理逻辑的专项测试 |

### 9.2 跨语言语义陷阱补充

| 陷阱 | 源语言 | Rust 差异 | 缓解 |
|------|--------|----------|------|
| 闭包语义 | JS/Python 引用捕获 | Rust 需明确 move/borrow | PORTING.md 规则 + clippy |
| null 三态 | JS: null/undefined/absent | Rust: Option<T> | 类型映射时统一为 Option |
| 隐式类型转换 | JS: "1" + 1 = "11" | Rust 无隐式转换 | PORTING.md 禁止模式 |
| 正则方言 | JS/Python/Rust regex 差异 | 语法/Unicode 支持不同 | 正则表达式专项测试 |
| 字符串切片 panic | — | Rust 非 char 边界切片 panic | 使用 .get() 安全访问 |
| 模块初始化顺序 | 语言定义顺序 | Rust 无保证 | OnceLock 显式初始化 |
| 迭代器惰性 | Python generator 惰性 | Rust iterator 也惰性但语义不同 | 注意 collect 时机 |
| 整数大小 | JS number = f64 | Rust 多种整数类型 | 类型映射表明确 |
| 相等性语义 | JS == vs === | Rust PartialEq/Eq | PORTING.md 统一规则 |
| **Promise eager vs Future lazy** | JS Promise 创建即执行 | Rust Future 需要 executor 驱动，不 `.await` 不执行 | 审查所有 async 调用点，确保 Future 被驱动；PORTING.md 规则 22 专项覆盖 |
| **Send/Sync 约束传染** | 源语言无编译期线程安全标记 | 跨 `.await` 持有的类型必须 Send，共享引用必须 Sync | 迁移后大量类型编译失败；需提前审计并发共享点，选择 Arc/Mutex 或重构 |
| **可变性传播与架构重组** | 源语言允许多处同时修改对象 | `&mut` 排他借用，同一时刻只能有一个可变引用 | 无法直译多处同时写入的模式；需重构为消息传递、Cell/RefCell 或拆分数据结构 |
| **异步取消安全性（Cancel Safety）** | JS Promise 创建后无法取消，始终运行到完成 | Rust Future 可在任意 `.await` 点被 drop（取消）；`tokio::select!` / `tokio::time::timeout` 会导致未选中的 Future 被 drop | 如果 Future 持有锁或处于半完成写操作状态，被 drop 会导致数据不一致。迁移时需逐一审查 `select!`/`timeout` 中的 Future 是否取消安全；PORTING.md 规则 22（异步运行时）中专项覆盖取消安全性审查 |

### 9.3 流程陷阱

| 陷阱 | 说明 | 缓解措施 |
|------|------|---------|
| 上下文污染 | 同一对话反复纠正，错误累积 | 方向错了果断 Reset：清空 Git + 清空对话 |
| 认知债务 | AI 代码通过测试但没人理解 | 代码认领制度：每个模块有人能不借助 AI 解释 |
| 审查瓶颈 | AI 翻译 10K 行/h，人类审阅 200-500 行/h | 控制并行度，不追求生成速度 |
| 最后 20% | uutils 在 90% 兼容性停滞 | 接受部分模块用 FFI 桥接（降级路径） |
| 迁移疲劳 | 5 年加一个递归功能（lychee 案例） | Sprint 里程碑，每个 Sprint 有可见产出 |
| 性能倒退 | Arc 开销可能超过 GC | 迁移后必须跑 criterion benchmark |
| 源项目变更 | 迁移期间源项目继续开发 | 选择并行策略（功能冻结/双轨/Strangler Fig） |
| 多 agent 冲突 | 多 agent 并行时共享文件合并冲突 | 每个 agent 在独立 worktree 中工作 |

### 9.4 遗漏清单（易忽略项）

- [ ] 构建系统迁移（package.json → Cargo.toml，CI/CD 重配）
- [ ] 用户面向字符串提取和回归测试
- [ ] 许可证兼容性审计（`cargo-deny`）
- [ ] 全局状态和初始化顺序审计
- [ ] 插件系统可行性评估
- [ ] 监控/日志格式兼容性（tracing vs 传统日志）
- [ ] 回滚计划和双运行架构
- [ ] 序列化格式字节级兼容性测试
- [ ] 团队 Rust 学习曲线预算（3-6 个月达到生产力）
- [ ] 配置/环境变量处理
- [ ] 平台特定代码的 `cfg` 映射
- [ ] **Cargo workspace 结构设计**（MDR 记录）
- [ ] **异步运行时选择**（tokio/async-std，MDR 记录）
- [ ] **CI/CD 集成**（GitHub Actions/GitLab CI Rust 配置）

---

## 十、Claude Code 插件结构

### 10.1 Skills（用户入口）

| Skill | 触发 | 功能 | MVP? |
|-------|------|------|------|
| `/migrate-init` | 手动 | 初始化迁移项目，分析源码仓库，生成项目画像 | 是 |
| `/migrate-plan` | 手动 | 生成 PORTING.md + PARITY.md + AGENTS.md | 是 |
| `/migrate-test` | 手动 | 搭建测试基础设施，录制行为，生成测试套件 | 是 |
| `/migrate-run` | 手动 | 执行指定模块的迁移（内循环） | 是 |
| `/migrate-verify` | 手动 | 运行完整验证管线（F3 集成验证） | 是 |
| `/migrate-status` | 手动 | 查看迁移进度仪表板 | 是 |
| `/migrate-graduate` | 手动 | 评估毕业标准，从迁移模式过渡到原生开发 | 否 |
| `/migrate-unsafe-audit` | 手动 | unsafe 分类审计 + 清理优先级 | 否 |

### 10.2 SubAgents（4 个专职角色）

| Agent | 职责 | 核心工具 |
|-------|------|---------|
| `analyzer` | 源码分析、项目画像、依赖图构建、惯用法检查 | tree-sitter, dependency-cruiser, Mypy, tokei |
| `translator` | PORTING.md 规则生成、代码翻译（意图驱动）、多候选生成 | LLM, syn+quote, ast-grep |
| `verifier` | 等价性验证、对抗性审查、不等价证据收集、性能对比 | cargo-test, proptest, criterion, Miri |
| `scaffolder` | 测试基础设施搭建、行为录制、黄金测试集管理 | insta, cargo-fuzz, mitmproxy |

**行动指南**：每个 SubAgent 有独立的系统提示，包含其职责边界和可用工具列表。Agent 之间通过 `migration-state.json` 和产出物文件通信。

### 10.3 Hooks（自动化门禁）

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "event": "Write",
        "pattern": "**/*.rs",
        "command": "cargo check --message-format=json 2>&1 | head -50",
        "comment": "Tier 0 — cargo check: 每次写入 .rs 文件后自动编译检查 (F1)"
      }
    ]
  }
}
```

**F2 和 F3 的实现方式**：
- **F2（模块完成后验证）**：不通过 Hook 实现。在各 Skill 的 SKILL.md 中通过分步指令要求"翻译步骤完成后执行 `cargo nextest run --lib` + `cargo clippy -- -D warnings`"。
- **F3（Sprint Review 集成验证）**：不通过 Hook 实现。由 `/migrate-verify` Skill 手动触发，执行 `cargo deny check` + `cargo audit` 等完整验证管线。

**概念事件 → Claude Code 实际实现机制映射表**：

| 概念事件 | 反馈层级 | Claude Code 实现机制 | 说明 |
|---------|---------|---------------------|------|
| 写入 .rs 文件 | F1 | `PostToolUse` Hook（`Write` 事件） | 真实 Claude Code Hook 事件，自动触发 |
| 模块翻译完成 | F2 | Skill SKILL.md 分步指令 | 在指令中要求 Claude 执行验证命令 |
| Sprint Review | F3 | `/migrate-verify` Skill 手动触发 | 用户显式调用 |
| 迁移状态变更 | — | `migration-state.json` 文件写入 | 编排器自行管理 |

**Tier 0 覆盖确认**：Tier 0 的三个工具（cargo check / clippy / cargo-nextest）——cargo check 通过 `PostToolUse` Hook 自动触发（F1），clippy 和 cargo-nextest 通过 Skill SKILL.md 指令在翻译步骤完成后执行（F2）。

### 10.4 unsafe 分类管理

不只是"加注释"，而是分级管理：

| 优先级 | 类别 | 说明 | 处理方式 | "清理"含义 |
|--------|------|------|---------|-----------|
| P0 | 可立即消除 | 有 safe 替代方案 | 本 Sprint 内替换为 safe 代码 | **消除**——unsafe 块不再存在 |
| P1 | FFI 边界 | 调用外部 C 库必需 | 封装在最小 unsafe 块 + SAFETY 注释 + 审计确认 | **封装审计完毕**——unsafe 仍存在，但已封装在安全抽象后，且审计通过 |
| P2 | 性能关键 | safe 版本有显著开销 | benchmark 证明后保留 + Miri 测试 | 保留（有性能证据） |
| P3 | 暂无 safe 方案 | 等待 Rust 语言演进 | 标记 TODO + 定期重评估 | 保留（等待上游） |
| P4 | 历史遗留 | 迁移过程中临时引入 | 毕业前必须**重新归类**到 P0-P3 | **重新归类**——不允许以"历史遗留"状态毕业 |

**行动指南**：`/migrate-unsafe-audit` 自动扫描所有 unsafe 块，生成分类报告，标注清理优先级。毕业标准：P0 全部消除，P1 全部封装审计完毕，P4 全部重新归类到 P0-P3。

### 10.5 编排调度路径

每个 Skill 的执行并非单一 SubAgent 调用，而是按预定义序列调度多个 SubAgent 协作。MVP 阶段 SubAgent **串行执行**，通过文件通信 + 顺序约束实现协调。

#### Skill 内部调度序列

| Skill | 调度序列 | 关键产出物 |
|-------|---------|-----------|
| `/migrate-init` | `analyzer` → 写入 `migration-state.json` + 项目画像 | migration-state.json, source-graph.json |
| `/migrate-plan` | `analyzer`(补充分析) → `translator`(规则生成) → 写入 PORTING.md + PARITY.md | PORTING.md, PARITY.md, AGENTS.md |
| `/migrate-test` | `analyzer`(接口提取) → `scaffolder`(测试搭建) → 写入 test-fixtures/ | test-fixtures/, 行为录制 |
| `/migrate-run` | `translator`(翻译) → F1 循环 → `verifier`(验证) → F2 循环 → 更新状态 | Rust 代码, 测试, MDR |
| `/migrate-verify` | `verifier`(全量验证) → 生成报告 → 更新 PARITY.md | sprint-N-report.json |
| `/migrate-unsafe-audit` | `verifier`(unsafe 扫描) → 分类报告 | unsafe-audit.json |

#### MVP 阶段执行模型

```
Skill 入口
  │
  ▼
SubAgent A (串行)
  │── 读取 migration-state.json（输入）
  │── 执行任务
  │── 写入产出物文件（输出）
  │
  ▼
顺序约束检查
  │── 验证 SubAgent A 产出物存在且有效
  │── 失败 → 重试或报错退出
  │
  ▼
SubAgent B (串行)
  │── 读取 SubAgent A 的产出物（输入）
  │── 执行任务
  │── 写入产出物文件（输出）
  │
  ▼
状态更新
  └── 更新 migration-state.json
```

**文件通信协议**：
- SubAgent 间**不直接通信**，通过 `.rust-migration/` 下的文件传递数据
- 每个 SubAgent 的输入/输出文件路径在 Skill 脚本中硬编码
- 顺序约束：后序 SubAgent 启动前，检查前序产出物文件的存在性和有效性（JSON Schema 校验）

**编排机制的本质（MVP 阶段）**：
- MVP 阶段的编排**依赖 Claude 的指令跟随能力**，而非确定性程序控制。Skill 的 SKILL.md 通过强约束分步指令引导 Claude 的行为（如"第 1 步：调用 analyzer SubAgent；第 2 步：检查产出物；第 3 步：调用 translator SubAgent"）。
- 这意味着编排的可靠性取决于 LLM 对指令的遵守程度，而非代码级别的 if-else 分支。
- **M0 验证要求**：在 M0 阶段需要验证 Claude 能否可靠执行 3+ 步的 SubAgent 调度序列。如果指令跟随不够可靠，需要引入更细粒度的检查点或将长序列拆分为多个短 Skill。

**未来演进**：M2 阶段引入有限并行（analyzer + scaffolder 可并行），M4 阶段引入完整 DAG 调度。

### 10.6 产出物目录结构

```
.rust-migration/
├── PORTING.md                 # 迁移规则宪法（26 类，渐进式生成）
├── PARITY.md                  # 迁移进度跟踪（Sprint 聚合）
├── KNOWN_DIFFERENCES.md       # 已知行为差异登记簿
├── AGENTS.md                  # AI 行为约束
├── .rustmigrate.toml          # 项目级配置（见第十一章）
├── migration-state.json       # 状态机 + Sprint 元数据
├── intermediate/              # 中间分析产物
│   ├── source-graph.json      # 源码依赖图
│   ├── type-map.json          # 类型映射
│   ├── call-graph.json        # 调用图
│   └── attempts/              # 翻译尝试历史（断点续传用）
├── test-fixtures/             # 行为录制测试集
│   ├── golden/                # 黄金文件 (input/output 对)
│   ├── recordings/            # HTTP/CLI 录制
│   ├── proptest-regressions/  # proptest seed 记录
│   ├── fuzz-corpus/           # 模糊测试语料
│   └── benchmarks/            # 性能基线数据
├── decisions/                 # MDR 迁移决策记录
│   ├── MDR-001-error-strategy.md
│   └── MDR-002-async-runtime.md
└── reports/                   # 验证报告
    ├── coverage.json
    ├── complexity-comparison.json
    ├── unsafe-audit.json
    └── sprint-N-report.json
```

---

## 十一、工作流灵活性与扩展

### 11.1 .rustmigrate.toml 配置文件

```toml
[project]
name = "my-project"
source_language = "typescript"       # typescript | python | c | cpp | go
source_root = "./src"
rust_root = "./rust-src"
source_commit = "abc123"             # 锁定源码版本

# 排除不参与迁移的路径（glob 模式）
exclude = [
  "src/vendor/**",                   # 第三方代码
  "src/**/*.test.ts",                # 源语言测试文件
  "src/**/__mocks__/**",             # Mock 文件
  "dist/**",                         # 构建产物
]

[strategy]
# 支持多动机（数组），第一个为主要动机，影响工具选型和验收标准
migration_motives = ["performance", "memory_safety"]  # performance | memory_safety | deployment | concurrency | compliance
parallel_strategy = "feature_freeze" # feature_freeze | dual_track | strangler_fig
max_concurrent_agents = 3
max_retry_rounds = 3                 # 翻译失败最大重试轮数
degrade_strategy = "ffi"             # ffi | manual | skip

[tools]
tier0 = true                         # 硬性门禁（不可关闭）
tier1 = true                         # 推荐工具
tier2 = false                        # 高级工具（默认关闭）

[tools.tier2_override]
cargo_fuzz = false
cargo_mutants = false
miri = false
kani = false
loom = false
criterion = false                    # 默认 Tier 2；当 migration_motives 含 performance 时自动提升为 Tier 1

[testing]
coverage_threshold = 80              # 覆盖率门槛（百分比）
proptest_cases = 256                 # 属性测试用例数
fuzz_duration_secs = 60              # 模糊测试时长
benchmark_tolerance = 0.10           # 性能回归容忍度（10%）

[context]
max_tokens_per_translation = 100000  # 每次翻译上下文预算
module_summary_strategy = "interface_only"  # interface_only | full

[workspace]
cargo_workspace = true               # 使用 Cargo workspace
crate_naming = "kebab-case"          # 子 crate 命名风格
```

**行动指南**：`/migrate-init` 自动根据项目画像生成初版配置，用户可手动调整。

### 11.2 语言扩展架构

设计为**目录约定 + JSON Schema 契约**的适配器模式。每种源语言对应一个适配器目录，包含检测、分析、模板等脚本和配置文件。

#### 目录约定

```
.claude/skills/migrate/adapters/
├── typescript/
│   ├── adapter.json            # 适配器元数据（JSON Schema 契约）
│   ├── detect.sh               # 检测项目是否使用此语言
│   ├── extract-types.sh        # 类型提取（调用 TS Compiler API）
│   ├── extract-deps.sh         # 依赖图提取（调用 dependency-cruiser）
│   ├── porting-template.md     # PORTING.md 模板规则（语言专用）
│   ├── ffi-bridge.sh           # FFI 桥接配置（napi-rs）
│   └── analysis-tools.json     # 语言专用工具列表
├── python/
│   ├── adapter.json
│   ├── detect.sh
│   ├── extract-types.sh        # 调用 Mypy
│   ├── extract-deps.sh         # 调用 import-linter + grimp
│   ├── porting-template.md
│   ├── ffi-bridge.sh           # PyO3 + maturin
│   └── analysis-tools.json
└── c_cpp/
    └── ...                     # bindgen + cbindgen
```

#### adapter.json 契约（JSON Schema）

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "required": ["language_id", "display_name", "detect", "extract_types", "extract_deps"],
  "properties": {
    "language_id":    { "type": "string", "description": "语言标识，如 typescript / python / c_cpp" },
    "display_name":   { "type": "string", "description": "显示名称" },
    "detect":         { "type": "string", "description": "检测脚本路径（相对于适配器目录）" },
    "extract_types":  { "type": "string", "description": "类型提取脚本路径" },
    "extract_deps":   { "type": "string", "description": "依赖图提取脚本路径" },
    "porting_template": { "type": "string", "description": "PORTING.md 模板文件路径" },
    "ffi_bridge":     { "type": ["string", "null"], "description": "FFI 桥接脚本路径（可选）" },
    "analysis_tools": { "type": "string", "description": "分析工具配置文件路径" }
  }
}
```

#### 逻辑接口参考

适配器的脚本需覆盖以下逻辑接口（对应原概念设计中的方法）：

| 逻辑接口 | 对应脚本 | 职责 |
|---------|---------|------|
| `language_id` | adapter.json 字段 | 语言标识 |
| `detect` | detect.sh | 检测项目是否使用此语言，返回 0/1 |
| `extract_types` | extract-types.sh | 提取类型信息（语言专用，不走统一 IR） |
| `extract_dependencies` | extract-deps.sh | 提取依赖图，输出统一 JSON 格式 |
| `porting_template` | porting-template.md | 该语言的 PORTING.md 预置规则 |
| `ffi_bridge` | ffi-bridge.sh | FFI 桥接工具配置 |
| `analysis_tools` | analysis-tools.json | 语言专用分析工具列表 |

**MVP 支持**：TypeScript 适配器。
**后续迭代**：Python 适配器 → C/C++ 适配器 → Go 适配器。

每个适配器实现：
- 类型提取（TS Compiler API / Mypy / libclang）
- 依赖分析（dependency-cruiser / import-linter / 自建）
- PORTING.md 模板（语言专用规则预置）
- FFI 桥接（napi-rs / PyO3 / bindgen）

### 11.3 智能项目类型检测

| 信号 | 权重 | 检测方法 |
|------|------|---------|
| package.json 存在 | 高 | 文件检测 |
| tsconfig.json 存在 | 高 | 文件检测 |
| setup.py / pyproject.toml | 高 | 文件检测 |
| Makefile / CMakeLists.txt | 高 | 文件检测 |
| go.mod | 高 | 文件检测 |
| 文件扩展名分布 | 中 | tokei 统计 |
| import 语句模式 | 中 | tree-sitter 分析 |
| 框架特征文件 | 中 | 模式匹配（如 next.config.js, Django settings.py） |

**多语言项目处理**：
1. 语言热图：按文件数/代码行数识别主要语言
2. FFI 边界检测：自动发现跨语言调用点
3. 迁移策略决策树：先迁移叶子语言，保留核心语言到最后

### 11.4 五级渐进式用户旅程

| 级别 | 用户动作 | 工具介入深度 | 适用场景 |
|------|---------|-------------|---------|
| L1 探索 | `/migrate-init` | 只做画像和分析，不产生 Rust 代码 | 评估可行性 |
| L2 规划 | `/migrate-plan` | 生成 PORTING.md + PARITY.md | 准备阶段 |
| L2.5 测试搭建 | `/migrate-test` | 行为录制 + 测试基础设施搭建，不产生 Rust 代码 | 测试准备（L2 的后续子步骤，可选但推荐） |
| L3 执行 | `/migrate-run` | 逐模块迁移 + 验证 | 实际迁移 |
| L4 毕业 | `/migrate-graduate` | 评估毕业标准 + 知识固化 | 迁移收尾 |

**跨级别辅助工具**（可在任意级别使用）：

| 工具 | 用途 | 说明 |
|------|------|------|
| `/migrate-verify` | 运行完整验证管线（F3 集成验证） | L3+ 阶段主要使用，但 L2.5 也可用于验证测试基础设施 |
| `/migrate-status` | 查看迁移进度仪表板 | 任意阶段均可使用，了解当前迁移状态 |

**行动指南**：用户可以在任意级别停留，不强制推进。L1 的画像报告本身就有价值（评估迁移可行性和成本）。L2.5 的测试搭建建议在 L3 执行前完成，但不强制——`/migrate-run` 内循环也会在模块迁移完成后生成测试。

### 11.5 验证管线 DAG 自定义

`.rustmigrate.toml` 中可以自定义验证管线的启用节点：

```toml
[pipeline]
# Tier 0（不可关闭）
cargo_check = true
clippy = true
cargo_test = true

# Tier 1（可关闭）
coverage = true
snapshot = true
property_test = true
complexity_check = true

# Tier 2（默认关闭）
fuzz = false
mutation = false
miri = false
formal = false
concurrency = false
```

---

## 十二、风险评估

| 风险 | 严重度 | 可能性 | 缓解措施 |
|------|--------|--------|---------|
| LLM 进步使工具过时 | 高 | 中 | 核心价值在验证层而非生成层——即使 LLM 翻译变完美，验证仍然需要 |
| Code Metal 下沉到中端 | 高 | 低 | 保持开源+轻量定位，做他们不做的验证层 |
| 用户基数小 | 中 | 中 | 产出物（PORTING.md、测试集、MDR）独立有价值 |
| 每种语言对需大量维护 | 中 | 高 | 优先支持 TS→Rust，LanguageAdapter 降低扩展成本 |
| UA 扩展到迁移场景 | 中 | 中 | 差异化在验证层，不在理解层 |
| petgraph 维护风险 | 低 | 中 | 轻量场景可自建 DAG |
| dependency-cruiser 单人风险 | 低 | 中 | 准备 fork 计划 |
| 过度设计 | 中 | 高 | **MVP 聚焦 TS 单模块，拒绝 scope creep** |

---

## 十三、实施路线图

### M0: 基础搭建（2 周）

**目标**：项目骨架 + 第一个 Skill 能跑

**交付物**：
- [ ] Claude Code 插件项目结构搭建
- [ ] `/migrate-init` 基础版（TS 项目画像）
- [ ] `migration-state.json` schema 定义
- [ ] `.rustmigrate.toml` 配置 schema
- [ ] TypeScript LanguageAdapter 骨架

**验收指标**：在 1 个 TS 开源项目上跑通 `/migrate-init`，生成项目画像。

### M1: MVP（6-8 周）

**目标**：跑通 TypeScript → Rust 的**单模块纯函数/CLI 子模块**迁移

**范围限定（MVP 必须）**：
- [ ] `/migrate-init` 完整版（TS 项目画像 + 依赖图）
- [ ] `/migrate-plan` — PORTING.md 生成（核心 15 条规则）
- [ ] `/migrate-run` — 单模块迁移内循环
- [ ] Tier 0 门禁集成（cargo check + clippy + cargo test）
- [ ] 黄金文件测试框架
- [ ] 编译器反馈迭代修复（最多 3 轮）
- [ ] `migration-state.json` 状态管理 + 断点续传
- [ ] PARITY.md 自动更新
- [ ] 基础 MDR 模板

**MVP 不包含（后续迭代）**：
- 多候选生成
- 属性测试 / 模糊测试
- 多 agent 并行
- 行为录制框架
- `/migrate-graduate`
- `/migrate-unsafe-audit`

**验收指标**：
- 在 3 个真实 TS 小项目（<5K 行）中完成至少 1 个纯函数模块的迁移
- 迁移后代码通过 Tier 0 门禁
- 黄金文件测试 100% 通过

### M2: 质量提升（8-12 周）

**目标**：验证管线完整，翻译质量可靠

**交付物**：
- [ ] 多候选生成 + 最优选择
- [ ] 属性测试（proptest 等价性验证）
- [ ] 模糊测试（cargo-fuzz 差异对比）
- [ ] 变异测试（cargo-mutants 测试质量验证）
- [ ] 覆盖率门禁（cargo-llvm-cov）
- [ ] 行为录制框架（CLI + 库接口）
- [ ] KNOWN_DIFFERENCES.md 自动生成
- [ ] 降级路径实现（FFI 桥接）
- [ ] `/migrate-verify` 完整验证管线
- [ ] `/migrate-unsafe-audit` 基础版
- [ ] Sprint 循环外循环支持

**验收指标**：
- 在 3 个真实 TS 中型项目（5K-20K 行）中完成多模块迁移
- 属性测试覆盖核心函数
- 翻译膨胀率 < 3.0x
- 降级路径（FFI 桥接）在至少 1 个复杂模块上成功

### M3: 多语言支持（8-16 周）

**目标**：支持 Python → Rust

**交付物**：
- [ ] Python LanguageAdapter（Mypy 类型提取 + PyO3 桥接）
- [ ] Python 专用 PORTING.md 模板
- [ ] 统一差异测试框架
- [ ] `/migrate-graduate` 毕业评估
- [ ] 性能基准对比自动化（criterion 集成）
- [ ] 并发测试（loom/shuttle 集成）
- [ ] 依赖图可视化（Mermaid 自动生成）

**验收指标**：
- 在 2 个真实 Python 项目中完成至少 1 个模块迁移
- Python FFI 桥接（PyO3）在迁移项目中可用
- 毕业评估能正确识别"已完成"vs"未完成"状态

### M4: 完善（持续）

- C/C++ LanguageAdapter（bindgen + cbindgen）
- Go LanguageAdapter
- Kani 集成（关键路径形式化验证）
- 社区反馈驱动的规则库积累
- 多 agent 并行编排优化
- Strangler Fig 模式工具支持

---

## 十四、关键数据参考

### 成本估算

> **注意**：以下"预估成本"栏**仅指 LLM API 调用成本**，不含人力、基础设施、CI/CD 等其他费用。

| 规模 | 预估时间 | 预估成本（仅 LLM API） | 备注 |
|------|---------|----------------------|------|
| 1K 行 | 1-3 天 | $3-$30 | 纯函数模块 |
| 10K 行 | 2-4 周（1-2 人） | $30-$300 | 含测试搭建 |
| 50K 行 | 2-4 个月（2-4 人） | $150-$1500 | 含 FFI 桥接 |
| 100K+ 行 | 6-12 个月（团队） | 视项目而定 | 必须用 Strangler Fig |

### 成功案例基准

| 项目 | 规模 | 耗时 | 结果 | 证据等级 | 可参考维度 |
|------|------|------|------|---------|-----------|
| Bun (Zig→Rust) | 100 万行 | 11 天 | 99.8% 测试通过 | **商业案例**（Bun 团队博客） | 测试驱动验证流程、大规模迁移节奏 |
| Claw-Code (TS→Rust) | 48K 行 | 4 天 | 功能完整 | **社区传闻**（GitHub 项目） | AI 辅助翻译工作流、PORTING.md 实践 |
| Pokemon Showdown (JS→Rust) | 10 万行 | 7 天 | 功能完整 | **社区传闻**（GitHub 项目） | 大型 JS 项目迁移模式、模块拆分策略 |
| Cloudflare Pingora (C→Rust) | 从零构建 | N/A | CPU-70%, 内存-67% | **商业案例**（Cloudflare 博客） | 性能动机验收标准、FFI 桥接方案 |
| Discord (Go→Rust) | 单服务 | N/A | 消除 GC 延迟尖刺 | **商业案例**（Discord 博客） | 并发安全动机、GC→所有权模型迁移 |

> **注意**：Bun 和 Claw-Code 的极端速度可能包含未公开的前期准备工作，不应作为时间估算基准。

### 关键论文

| 论文 | 会议 | 核心贡献 | 与本项目关系 |
|------|------|---------|-------------|
| SafeTrans | ACM CCS 2025 | 迭代修复 54%→80% | 反馈循环设计基础 |
| DepTrans | ACM FSE 2026 | 7B 模型超 32B，依赖图引导 | 拓扑排序翻译策略 |
| Environment-in-the-Loop | ACM ReCode 2026 | 编译环境作为反馈参与者 | F1 反馈循环理论依据 |
| MatchFixAgent | ICML 2026 | 99.2% 等价性判定 | 验证层方法参考 |
| Hayroll | PLDI 2026 | C 宏翻译 | C→Rust 适配器参考 |
| LLMigrate | arXiv 2025 | 调用图引导，<15% 修改 | 依赖图分析策略 |

---

## 附录 A：migration-state.json Schema

```json
{
  "version": "0.4",
  "state": "sprint_loop",
  "state_history": [
    {
      "state": "init",
      "entered_at": "2026-06-06T10:00:00Z",
      "exited_at": "2026-06-06T10:05:00Z"
    },
    {
      "state": "profile",
      "entered_at": "2026-06-06T10:05:00Z",
      "exited_at": "2026-06-06T11:00:00Z"
    },
    {
      "state": "plan",
      "entered_at": "2026-06-06T11:00:00Z",
      "exited_at": "2026-06-06T14:00:00Z"
    },
    {
      "state": "scaffold",
      "entered_at": "2026-06-06T14:00:00Z",
      "exited_at": "2026-06-06T16:00:00Z"
    },
    {
      "state": "sprint_loop",
      "entered_at": "2026-06-06T16:00:00Z",
      "exited_at": null
    }
  ],
  "project": {
    "name": "my-project",
    "source_language": "typescript",
    "source_commit": "abc123",
    "source_loc": 15000,
    "created_at": "2026-06-06T10:00:00Z"
  },
  "sprint": {
    "current": 2,
    "history": [
      {
        "id": 1,
        "started_at": "2026-06-06T10:00:00Z",
        "completed_at": "2026-06-13T18:00:00Z",
        "target_modules": ["utils/string", "utils/math"],
        "completed_modules": ["utils/string", "utils/math"],
        "porting_md_version": "1.2",
        "notes": "首个 Sprint，规则追加了整数溢出处理"
      }
    ]
  },
  "modules": {
    "utils/string": {
      "status": "done",
      "substatus": null,
      "sprint": 1,
      "attempts": [
        {
          "timestamp": "2026-06-07T14:00:00Z",
          "result": "success",
          "retry_count": 1,
          "checkpoint": "intermediate/attempts/utils-string-001.json"
        }
      ],
      "test_pass_rate": "24/24",
      "coverage": 91,
      "known_differences": 0,
      "risk": "low"
    },
    "core/parser": {
      "status": "testing",
      "substatus": "proptest_failing",
      "sprint": 2,
      "attempts": [
        {
          "timestamp": "2026-06-14T09:00:00Z",
          "result": "partial",
          "retry_count": 2,
          "checkpoint": "intermediate/attempts/core-parser-002.json"
        }
      ],
      "test_pass_rate": "18/22",
      "coverage": 76,
      "known_differences": 1,
      "risk": "medium"
    }
  },
  "config_ref": ".rustmigrate.toml"
}
```

---

## 附录 B：MVP Skill 的 SKILL.md 骨架

以下为 `/migrate-init` 和 `/migrate-run` 的 SKILL.md 骨架结构示例，展示分步指令格式、上下文加载、SubAgent 调用和检查点的编写方式。

### /migrate-init SKILL.md 骨架

```markdown
# /migrate-init — 初始化迁移项目

## 前置条件
- 当前目录是源项目根目录
- 源项目可构建、可测试

## 分步指令

### Step 1: 检测项目类型
读取当前目录的文件结构，识别源语言和框架。
检查以下文件是否存在：package.json, tsconfig.json, pyproject.toml, go.mod, CMakeLists.txt。

### Step 2: 调用 analyzer SubAgent
使用 analyzer SubAgent 执行项目画像分析。
输入：项目根目录路径。
等待产出物：`.rust-migration/intermediate/source-graph.json`。

**检查点**：验证 source-graph.json 存在且包含 `modules` 和 `dependencies` 字段。
如果验证失败，报告错误并停止。

### Step 3: 生成初始状态
基于 analyzer 的产出物，生成以下文件：
- `.rust-migration/migration-state.json`（初始状态：PROFILE）
- `.rust-migration/.rustmigrate.toml`（默认配置）

**检查点**：验证 migration-state.json 的 state 字段为 "profile"。

### Step 4: 输出摘要
向用户展示项目画像摘要：源语言、代码行数、模块数、依赖数、建议的迁移策略。
提示用户下一步执行 `/migrate-plan`。
```

### /migrate-run SKILL.md 骨架

```markdown
# /migrate-run — 执行模块迁移

## 前置条件
- `.rust-migration/migration-state.json` 存在且 state 为 "sprint_loop"
- `.rust-migration/PORTING.md` 存在
- 目标模块已在 Sprint 计划中

## 上下文加载
1. 读取 `migration-state.json`，确认当前 Sprint 和目标模块
2. 读取 `PORTING.md` 中与目标模块相关的规则（按模块类型筛选）
3. 读取目标模块源码
4. 读取依赖模块的接口签名（仅接口，不含实现）

## 分步指令

### Step 1: 语义解构（调用 translator SubAgent）
调用 translator SubAgent，要求其生成目标模块的意图摘要。
输入：源码 + 相关 PORTING.md 规则。
产出物：`.rust-migration/intermediate/{module}-intent.md`。

**检查点**：意图摘要文件存在且非空。

### Step 2: 代码翻译（调用 translator SubAgent）
调用 translator SubAgent，基于意图摘要生成 Rust 代码。
输入：意图摘要 + PORTING.md 规则 + 依赖接口。
产出物：Rust 源文件写入 `rust_root` 对应路径。

**检查点**：Rust 文件存在。
注意：写入 .rs 文件后 PostToolUse Hook 会自动触发 cargo check（F1 反馈）。

### Step 3: F1 编译反馈循环
如果 cargo check 失败，将错误信息反馈给 translator SubAgent 修复。
最多重试 3 轮（由 .rustmigrate.toml 的 max_retry_rounds 控制）。
3 轮后仍失败 → 进入 DEGRADE 流程。

**检查点**：cargo check 通过。

### Step 4: F2 测试验证
翻译步骤完成后，执行以下验证命令：
- `cargo nextest run --lib` — 运行单元测试
- `cargo clippy -- -D warnings` — lint 检查

如果测试失败：调用 verifier SubAgent 分析失败原因。
可修复 → 修复后重新执行 Step 4。
不可修复 → 记录到 KNOWN_DIFFERENCES.md。

**检查点**：测试通过率 ≥ 预期，clippy 无 warning。

### Step 5: 状态更新
更新 `migration-state.json` 中该模块的状态。
更新 `PARITY.md` 中该模块的进度行。
如有架构决策，写入 MDR。
```

---

## 附录 C：证据等级说明

本文档引用的案例和数据按以下等级标注：

| 等级 | 含义 | 可信度 |
|------|------|--------|
| **论文验证** | 发表在同行评审会议/期刊 | 高——有实验数据和复现方法 |
| **商业案例** | 企业官方博客/技术报告 | 中高——有生产数据但可能选择性披露 |
| **社区传闻** | GitHub 项目/个人博客/论坛 | 中低——可能缺少关键细节，需独立验证 |
