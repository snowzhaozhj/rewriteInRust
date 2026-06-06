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
│  ├── source-graph.db         # 源码图 SQLite 数据库             │
│  ├── source-ref/             # 源文件锁定副本（迁移期间保留）   │
│  ├── SPRINT_LEARNINGS.md     # Sprint 级知识总结                │
│  ├── DESIGN_ASSUMPTIONS.md   # M0 假设验证报告                  │
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

> **Phase A/B 与状态机的映射**：状态机图中的 `TRANSLATE` 是单一概念状态，但其内部包含 [03 § 4.3](./03-execution-model.md#43-内循环模块级单会话内-phase-ab-双阶段翻译) 的 Phase A（忠实翻译）→ 对抗性审查 → Phase B（惯用化优化）三个子步骤。Phase A/B 是 `TRANSLATE` 状态内的**内部子步骤**，状态转换只在 Phase B 完成且 `cargo check` 通过后才发生（`translating` → `compile_fixing` → `testing` → `reviewing`，映射见 [09 附录](./09-appendix-schemas.md#状态机概念名--json-字段值映射)）。对抗性审查不单独占用状态机节点，归属于 `VERIFY` 的 `reviewing` 子状态。

**降级分析报告内容**（人类据此选择 `--degrade=ffi|manual|skip`）：translator 在 3 轮失败后输出结构化 JSON 诊断，至少包含——失败分类（`compilation_error` / `type_complexity` / `dependency_resolution` / `semantic_gap`）、触发错误的代码片段与错误信息、本轮已尝试的修复策略、以及针对每种降级方式（FFI/手动/裁剪）的预估代价。Skill 在 `paused` 状态下将其渲染为终端表格（失败历史 + 三种降级方式对比）。降级决策后的骨架代码自动生成、无需人工手写：scaffolder 生成 FFI 绑定桩，translator 标记 TODO(port) 供人工处理，skip 则更新 PARITY.md 记录裁剪。详见 [06 § 10.5](./06-plugin-structure.md#105-编排调度路径)。

**降级后依赖级联影响** [M2+]：当模块 A 降级为 FFI 桥接时，依赖 A 的模块 B 的翻译需要知道 A 的接口类型（FFI 桥接 vs 原生 Rust）。编排器在 A 降级后需要：
1. 更新 `migration-state.json` 中 A 的状态为 `degrade(ffi)`，并记录 A 的 FFI 接口描述
2. 后续模块 B 的翻译上下文中注入 A 的接口类型信息——如果 A 是 FFI 桥接，B 需要通过 FFI 调用 A 而非直接 Rust 调用
3. 如果 A 后来从 FFI 桥接升级为原生 Rust，B 的调用方式也需要同步更新

**断点续传** [MVP]：`migration-state.json` 记录每个模块的状态和最近一次成功的 checkpoint，重启后从 checkpoint 恢复。

**错误恢复** [MVP]：每次翻译尝试的输入输出都持久化到 `intermediate/attempts/`，失败后可回溯分析。

### 3.4.1 MVP → M2 演进与向后兼容

MVP（M0-M1）用 SKILL.md 指令驱动编排 + 确定性文件检查点（编排判定由 LLM 指令跟随完成，检查点由校验脚本负责，二者职责见 [06 § 10.5](./06-plugin-structure.md#105-编排调度路径)）。M2 引入程序化状态机（独立 Rust orchestrator 二进制），把编排决策从 LLM 移到确定性代码。为保证 M1 已在跑的迁移项目升级到 M2 不被破坏，`migration-state.json` 演进遵循向后兼容契约：

- **版本字段**：`version` 为必填，采用语义化版本（0.1 → 0.2 → 1.0）。reader 按 `version` 判断兼容性。
- **允许的演进**：新增可选字段安全（旧 reader 忽略未知字段）；**字段重命名/语义变更属 breaking change**，必须配套自动迁移脚本，并在 M2 CLI 的 `init`/`validate state` 中集成版本检测与自动升级。
- **演进规则的适用范围**：同样适用于 `source-graph.db` 等持久化结构（schema 版本写入表元数据）。breaking change 才触发主版本号上调，废弃字段保留 2 个 release 的过渡期。

M2 交付物与工作量见 [08 § M2](./08-roadmap-and-reference.md#m2-质量提升8-12-周)；相关风险见 [07 § 12.1](./07-pitfalls-and-risks.md#121-风险矩阵)「M1→M2 schema 向后兼容性」。

## 3.5 LLM 上下文窗口管理

| 策略 | 说明 |
|------|------|
| 分层注入 | PORTING 规则按相关性注入，不是全量塞入 |
| 模块隔离 | 每个模块翻译在独立对话中完成，避免上下文污染 |
| 摘要压缩 | 依赖模块只注入接口签名，不注入实现 |
| 上下文预算 | 每次翻译的上下文 = 源码 + 相关规则 + 依赖接口 ≤ 100K tokens（深链/超预算处理见 §3.5.1） |

**行动指南**：编排器在调度翻译任务前，先计算上下文预算；超预算则按 §3.5.1 决策树拆分或降级。

### 3.5.1 上下文预算运行时检查与拆分策略

**预算计算（运行时检查）**：编排器在调用 translator 前，按以下公式预估当前任务的上下文占用，预留 buffer 给 verifier 与翻译输出：

```
budget_used ≈ token(源码) + token(注入的 porting 规则) + token(依赖接口摘要) + buffer
  token(源码)       ≈ 源码字节数 / 4（粗估；实际由编排器调用 tokenizer 校准）
  token(依赖接口)    ≈ 每个导出接口签名 200–500 tokens（按签名复杂度，interface_only 模式仅注入签名）
  buffer            ≈ 10K（留给意图摘要 + 对抗审查 + 翻译输出余量）
```

意图摘要长度不设硬性上限，由编排器按源码行数动态控制（避免固定 token 上限在复杂模块下失真）。

**拆分决策树**（目标：单模块预估 ≤ 80K，给 verifier 与输出留余量）：

| 条件 | 处理 |
|------|------|
| ≤ 1K 行 且 依赖接口 < 10 | 直接翻译 |
| 1K–5K 行 且 依赖接口 < 20 | 全量翻译，预算检查通过即可 |
| > 5K 行 **或** 依赖接口 ≥ 20 | 按功能边界拆分；功能边界不清时按类内聚拆分。拆分后需对受影响子图重算迁移顺序（topo-sort） |
| `transitive_dependency_depth ≥ 5`（拓扑最长路径，定义为"深链"） | 优先翻译公共中间层，对该模块采用"先翻译依赖再翻译本体"或依赖注入降低链深 |
| 循环依赖 或 拆分后仍 > 100K | 触发降级路径（见下） |

**深链可行性说明**：在 5K–20K 行的中型 TS 项目中，深链（depth ≥ 5）通常占模块的少数（基于 dependency-cruiser 对真实项目依赖图的经验，多为单向浅链）。interface_only 仅注入签名，签名 token 通常为实现的 1/3 量级，因此多数深链经接口压缩后可控；少数无法压缩到预算内的，走下方降级路径。M2+ 阶段需用 1–2 个真实中型项目验证此压缩率与降级触发频率。

**超预算降级行为（禁止硬填满预算）**：当预估 > 100K 且无法拆分时，translator 不得截断源码硬塞，而应输出 JSON 诊断报告（实际 token 预估、缺失/被裁的接口、拆分建议）并暂停，由 Skill 决策。`/migrate run` 在调用 translator 前先做此预估：若 > 95K 则提示用户确认并建议拆分。降级行为是否自动触发由 `.rustmigrate.toml` 的 `[context]` 配置控制（见 [06 § 11.1](./06-plugin-structure.md#111-rustmigratetoml-配置文件)）。
