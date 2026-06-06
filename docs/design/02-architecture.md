> [返回主索引](./README.md)

# 三、架构设计

## 3.1 整体架构

```
┌─────────────────────────────────────────────────────────────────┐
│              Plugin 入口 (rust-migrate-plugin)                   │
│                                                                  │
│                      用户入口 (Skills)                           │
│  /migrate analyze   /migrate run   /migrate review              │
│  /migrate graduate                                               │
├─────────────────────────────────────────────────────────────────┤
│                编排层 (SKILL.md 分步指令 + SubAgent 调度)          │
│  Sprint 管理 → 策略路由 → 状态机 → 断点续传 → 错误恢复           │
├──────────────────────────────┬──────────────────────────────────┤
│         分析层               │            转换层                 │
│  tree-sitter (多语言 AST)    │  LLM 翻译引擎（意图驱动）        │
│  Mypy (Python 类型提取)      │  迁移规则引擎（porting/ 目录）    │
│  TS Compiler API (TS 类型)   │  syn+quote (Rust 代码生成)       │
│  dependency-cruiser (JS 依赖)│  ast-grep (模式匹配重写)         │
│  import-linter+grimp (Py依赖)│                                  │
├──────────────────────────────┴──────────────────────────────────┤
│                      验证层 (Harness) — 核心价值                 │
│  Tier 0: cargo check → clippy → cargo-nextest                   │
│  Tier 1: cargo-llvm-cov → insta → proptest → cargo-deny/audit  │
│          → cargo-geiger                                          │
│  Tier 2: cargo-fuzz → cargo-mutants → Miri → loom              │
│  （criterion 默认 Tier 2，性能动机时提升为 Tier 1）              │
│  （完整工具分级详见 5.2-5.4 节）                                 │
│  差异测试框架 → 行为录制 → 不等价证据收集                        │
├─────────────────────────────────────────────────────────────────┤
│                      质量门禁（独立脚本，agent 无法跳过）          │
│  F1: rust-analyzer LSP 自动诊断（无需 Hook）                     │
│  F2 Hook: .claude/scripts/verify.sh → 测试套件 (Tier 0+1)       │
│  F3 Skill: /migrate review 手动触发 → 集成验证 (Tier 0+1+2)     │
│  格式化 Hook: PostToolUse → cargo fmt (仅 .rs 文件)              │
│  文件保护 Hook: PreToolUse → file-guard.sh 防止修改源项目文件    │
├─────────────────────────────────────────────────────────────────┤
│                      FFI 桥接层 (增量迁移/降级路径)               │
│  napi-rs (Node.js) │ PyO3 (Python) │ cxx/bindgen (C/C++)        │
├─────────────────────────────────────────────────────────────────┤
│                      产出物 (Artifacts)                          │
│  .rust-migration/                                               │
│  ├── porting/                # 项目专有迁移规则                  │
│  ├── PARITY.md               # 迁移进度跟踪（Sprint 聚合）      │
│  ├── KNOWN_DIFFERENCES.md    # 已知行为差异登记簿               │
│  ├── AGENTS.md               # AI 行为约束                      │
│  ├── migration-state.json    # 状态机 + Sprint 元数据           │
│  ├── .rustmigrate.toml       # 项目配置                         │
│  ├── context/                # 项目知识（patterns/anti-patterns）│
│  ├── decisions/              # MDR 迁移决策记录                  │
│  ├── intermediate/           # 中间产物                          │
│  ├── test-fixtures/          # 行为录制测试集                    │
│  └── reports/                # 验证报告                          │
└─────────────────────────────────────────────────────────────────┘
```

## 3.2 关键架构决策

### 3.2.1 砍掉"通用类型 IR"

**原设计**（G1）：从 TS/Python/Go 提取类型 → 统一中间表示 → idiomatic Rust 类型。

**修订**：砍掉统一 IR，改为**语言专用类型提取 + LLM 映射**。理由：
- 通用 IR 复杂度高，维护成本大
- LLM 已经擅长做类型映射
- 不同语言的类型系统差异太大，统一 IR 会丢失语义

**行动指南**：每种语言实现独立的 `LanguageAdapter`（见第十一章），负责类型提取，映射交给 LLM + PORTING 规则。

### 3.2.2 拓扑排序保留，跨语言图对比砍掉

**保留**：依赖图拓扑排序指导迁移顺序（G2 的核心功能）。
**砍掉**：源/目标代码结构图的"对比验证"——过度设计，测试覆盖已经够用。

### 3.2.3 PROFILE 与 PLAN 边界清晰化

| 状态 | 职责 | 性质 |
|------|------|------|
| PROFILE | 客观事实采集：语言、框架、依赖数、代码行数、测试现状 | 纯自动化，无需人类判断 |
| PLAN | 主观决策：迁移策略、规则制定、优先级排序 | 必须人类审查确认 |

### 3.2.4 SubAgent 合并（7 → 4）

| Agent | 合并前 | 职责 |
|-------|--------|------|
| `analyzer` | project-profiler + rust-idiom-checker | 源码分析、项目画像、惯用法检查 |
| `translator` | code-translator + porting-guide-writer | 规则生成、代码翻译（意图驱动）、Phase A/B 双阶段 |
| `verifier` | equivalence-checker + adversarial-reviewer | 等价性验证、对抗性审查、不等价证据收集 |
| `scaffolder` | test-scaffolder | 测试基础设施搭建、行为录制 |

## 3.3 必须自建的组件

| # | 组件 | 复杂度 | MVP? | 说明 |
|---|------|--------|------|------|
| G1 | ~~跨语言类型 IR~~ | — | — | **已砍掉**，改为语言专用适配器 |
| G2 | 依赖图拓扑排序引擎 | 中 | 是 | petgraph + 迁移顺序决策 |
| G3 | 统一差异测试框架 | 中 | 否 | 跨 HTTP/CLI/库接口的录制回放 + 对比引擎 |
| G4 | Rust Scientist 库 | 低 | 否 | 并行执行新旧代码路径，~300 行 |
| G5 | 统一依赖图格式转换器 | 低 | 是 | 各工具输出 → 统一格式 |
| G6 | AI 迁移编排器 | 高 | 是 | 状态机 + 错误恢复 + 断点续传 |

## 3.4 编排器状态机设计

> **MVP 分层说明**：本节描述编排器的完整概念设计。MVP（M0-M1）仅通过 SKILL.md 分步指令 + migration-state.json 实现子集功能，不构建程序化状态机。完整状态机在 M2+ 阶段按需实现。

```
                    ┌─────────┐
                    │  INIT   │
                    └────┬────┘
                         ▼
                    ┌─────────┐
                    │ PROFILE │   客观事实采集
                    └────┬────┘
                         ▼
                    ┌─────────┐
                    │  PLAN   │   主观决策
                    └────┬────┘
                         ▼
                    ┌─────────┐
                    │ SCAFFOLD│   测试基础设施搭建
                    └────┬────┘
                         ▼
                    ┌─────────────────────────────────┐
                    │         SPRINT LOOP              │
                    │                                  │
                    │  TRANSLATE ──► CHECK ──► VERIFY  │
                    │      ▲    (最多3轮重试)    │     │
                    │      └────────────────────┘     │
                    │                                  │
                    └──────────┬───────────────────────┘
                               │              │
                          全部完成       3轮失败
                               │              ▼
                               │         ┌─────────┐
                    ┌──────────┤         │  PAUSE  │  暂停等待人类决策
                    │          │         └────┬────┘
                    │          │              │ 人类显式确认
                    │          │              ▼
                    │          │         ┌─────────┐
                    │          │         │ DEGRADE │  FFI/手动/裁剪
                    │          │         └─────────┘
                    ▼          │
               ┌─────────┐    │   ┌─────────────────────────────┐
               │GRADUATE │    │   │  BLOCKED（可从任意状态进入） │
               └─────────┘    │   │  依赖模块降级/阻塞时触发     │
             知识固化+退出     │   │  解除后恢复 pre_blocked_status│
                              │   └─────────────────────────────┘
                         └─────────┘

降级决策流程（人类确认制，不自动降级）：
  1. 3 轮翻译失败后 → **暂停**（不自动降级）
  2. 生成降级分析报告（失败原因、建议降级方式、影响范围）
  3. 人类通过 `/migrate run --degrade=ffi|manual|skip` **显式确认**降级方式
  → FFI 桥接（保持原实现，Rust 端调用）
  → 人工介入（标记 TODO，等人类处理）
  → 功能裁剪（协商后移除该功能）

恢复路径（DEGRADE → TRANSLATE）：
  → 由用户通过 `/migrate run --module=X --force` 显式触发
  → 重新进入翻译循环，清除降级标记，重置重试计数
  → 适用场景：PORTING 规则更新后、LLM 能力提升后、人工提供了额外指导后
```

**降级后依赖级联影响** [M2+]：当模块 A 降级为 FFI 桥接时，依赖 A 的模块 B 的翻译需要知道 A 的接口类型（FFI 桥接 vs 原生 Rust）。编排器在 A 降级后需要：
1. 更新 `migration-state.json` 中 A 的状态为 `degrade(ffi)`，并记录 A 的 FFI 接口描述
2. 后续模块 B 的翻译上下文中注入 A 的接口类型信息——如果 A 是 FFI 桥接，B 需要通过 FFI 调用 A 而非直接 Rust 调用
3. 如果 A 后来从 FFI 桥接升级为原生 Rust，B 的调用方式也需要同步更新

**断点续传** [MVP]：`migration-state.json` 记录每个模块的状态和最近一次成功的 checkpoint，重启后从 checkpoint 恢复。

**错误恢复** [MVP]：每次翻译尝试的输入输出都持久化到 `intermediate/attempts/`，失败后可回溯分析。

## 3.5 LLM 上下文窗口管理

| 策略 | 说明 |
|------|------|
| 分层注入 | PORTING 规则按相关性注入，不是全量塞入 |
| 模块隔离 | 每个模块翻译在独立对话中完成，避免上下文污染 |
| 摘要压缩 | 依赖模块只注入接口签名，不注入实现 |
| 上下文预算 | 每次翻译的上下文 = 源码 + 相关规则 + 依赖接口 ≤ 100K tokens |

**行动指南**：编排器在调度翻译任务前，先计算上下文预算；超预算则拆分模块。
