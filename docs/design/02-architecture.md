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

> **已落地适配器**（截至 M4）：TypeScript（M1）、Python（M3）、Go（M4 Go 线）。三者共享同一 `LanguageAdapter` trait 契约（`language`/`can_handle`/`resolve_extensions`/`analyze_file`/`resolve_import`/`detect_tier` + `configure_project` 钩子）。Go 的包系统映射需扩 trait 暴露目录列举（D-M4-01），详见 [06 § 11.2 语言扩展架构](./06-plugin-structure.md#112-语言扩展架构)。C/C++ 推迟（D-M4-03）。

### 3.2.2 拓扑排序保留，跨语言图对比砍掉

**保留**：依赖图拓扑排序指导迁移顺序（G2 的核心功能）。
**砍掉**：源/目标代码结构图的"对比验证"——过度设计，测试覆盖已经够用。

### 3.2.3 PROFILE 与 PLAN 边界清晰化

| 状态 | 职责 | 性质 |
|------|------|------|
| PROFILE | 客观事实采集：语言、框架、依赖数、代码行数、测试现状 | 纯自动化，无需人类判断 |
| PLAN | 主观决策：迁移策略、规则制定、优先级排序 | 必须人类审查确认 |

> PORT-REVIEW 接受（BS3-05）：本表对 PROFILE（自动）/PLAN（人工）的分工已清晰，02 内无矛盾。发现指出的 Sprint Planning「选择模块 vs 重算拓扑」歧义、README TL;DR 命令↔阶段映射缺失分别属 03 § 4.2 与 README 范畴，由各自文件就地一句话澄清，不在 02 新增映射表以守净删除纪律。

### 3.2.4 SubAgent 合并（7 → 4）

| Agent | 合并前 | 职责 |
|-------|--------|------|
| `analyzer` | project-profiler + rust-idiom-checker | 源码分析、项目画像、惯用法检查 |
| `translator` | code-translator + porting-guide-writer | 规则生成、代码翻译（意图驱动）、Phase A/B 双阶段 |
| `verifier` | equivalence-checker + adversarial-reviewer | 等价性验证、对抗性审查、不等价证据收集 |
| `scaffolder` | test-scaffolder | 测试基础设施搭建、行为录制 |

> **verifier 双角色的责任边界（避免交叉失败模糊）**：verifier 在 `/migrate run` 序列中出现两次——Step 3（09 SKILL.md 编号，对应 03 Step 4）审查 **Phase A 持久化代码**（`intermediate/attempts/{module}-phase-a.rs`）的 1:1 结构，Step 5（09 编号，对应 03 Step 6）测试 **Phase B 产出物**的正确性。两步通过 `migration-state.json` 的 `phase_a_version`（Phase A 当前版本文件指针）与 `phase_a_audit_passed`（结构审查是否通过）两个字段解耦：Step 3 审查不通过则置 `phase_a_audit_passed=false`、translator 重做新版 Phase A 并更新指针后重审，通过才进入 Step 4；Step 5 测试失败时 verifier 对比当前 `phase_a_version` 指向的审查报告差异列表，据此判定失败源自 Phase A（上游）还是 Phase B 优化（本步）。字段定义与流程见 [09 附录 A](./09-appendix-schemas.md#附录-amigration-statejson-schema) 及 [附录 B Step 3.5/Step 5](./09-appendix-schemas.md#附录-bmvp-skill-的-skillmd-骨架)。
>
> **F2（verify.sh）失败的恢复分诊**：§ 3.1 的 F2 门禁脚本及其**判定逻辑本身**不可被 agent 改写或跳过，但失败诊断的处置权归 verifier——可变更对象仅限被测的 Phase B 代码（代码 bug 走 Phase B 既有 3 轮重试）或 verifier 自己生成的 test fixture（假设过严时修正，记入 MDR 且不计入翻译重试轮数，单模块迭代上限 2 次），环境/依赖类失败则记入 KNOWN_DIFFERENCES.md 标 `requires_manual_review`；三类判定清单与执行细节见 [03 § 4.3 Step 6](./03-execution-model.md#43-内循环模块级单会话内-phase-ab-双阶段翻译)。

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
               │GRADUATE │    │   │  BLOCKED（仅活跃状态可进入） │
               └─────────┘    │   │  依赖模块降级/阻塞时触发     │
             知识固化+退出     │   │  解除后恢复 pre_blocked_status│
                              │   └─────────────────────────────┘
                         └─────────┘

降级决策流程（交互模式：人类确认制；headless：自动 degrade_skip）：
  1. 3 轮翻译失败后 → **暂停**（不自动降级）
  2. 生成编译失败诊断报告（事后诊断：失败原因、建议降级方式、影响范围；区别于 § 3.5.1 的事前预算检查）
  3a. **交互模式**：人类通过 `/migrate run --degrade=ffi|manual|skip` **显式确认**降级方式
  3b. **headless 模式（M2-ADV-07）**：自动 `paused → degrade_skip`，不挂起等人工
  → FFI 桥接（保持原实现，Rust 端调用）
  → 人工介入（标记 TODO，等人类处理）
  → 功能裁剪（协商后移除该功能）

恢复路径（DEGRADE → TRANSLATE）：
  → 由用户通过 `/migrate run --module=X --force` 显式触发
  → 重新进入翻译循环，清除降级标记，重置重试计数
  → 适用场景：PORTING 规则更新后、LLM 能力提升后、人工提供了额外指导后
```

> **终态与 blocked 进入规则**（M2-DESIGN-01/02 定稿，权威矩阵见 [09 附录 A § 合法状态转换](./09-appendix-schemas.md#合法状态转换)）：
> - **`done` 是硬终态**：无出边，通过全部验证（Tier 0/1/2 + `TODO(port)`=0）后到达，**不可经 `--force` 重做**（如确需重迁须人工重置该模块状态后重跑）。上图「恢复路径（DEGRADE → TRANSLATE）」的 `--force` **仅适用 `degrade_*` 降级状态、不涉及 `done`**。项目级毕业由 `/migrate graduate` 评估、以模块 `done` 为前置（M2-ADV-03）。
> - **`blocked` 仅活跃状态可进入**：`pending`/`translating`/`compile_fixing`/`testing`/`reviewing`/`paused` 六个活跃状态可进入 `blocked`；`done`/`degrade_*`（终态/半终态）进入 blocked 无意义、不允许。解除后恢复 `pre_blocked_status`（见上图与 [09 附录 A](./09-appendix-schemas.md#合法状态转换)）。

> **Phase A/B 与状态机的映射**：状态机图中的 `TRANSLATE` 是单一概念状态，但其内部包含 [03 § 4.3](./03-execution-model.md#43-内循环模块级单会话内-phase-ab-双阶段翻译) 的 Phase A（忠实翻译）→ 对抗性审查 → Phase B（惯用化优化）三个子步骤。Phase A/B 是 `TRANSLATE` 状态内的**内部子步骤**，状态转换在 Phase B 首次产出代码后发生：`cargo check` 通过则 `translating` → `testing`；失败则 `translating` → `compile_fixing` 进入重试循环，通过后 `compile_fixing` → `testing` → `reviewing`（映射见 [09 附录](./09-appendix-schemas.md#状态机概念名--json-字段值映射)）。对抗性审查在 TRANSLATE 内部执行（Step 3），不触发状态转移；VERIFY 的 `reviewing` 子状态为测试通过后的最终签批门禁（TODO(port) 清零 + 验收）。
>
> PORT-REVIEW 接受（BS3-01）：本注与 [09 substatus 约定](./09-appendix-schemas.md#状态机概念名--json-字段值映射)已自洽说明 Phase A 失败停留在 `translating` 的 `phase_a_complete_awaiting_review` 子态、断点续传重入点，02 内无矛盾；Step 4/Step 5 的 checkpoint 入口细节属 03 § 4.3 执行流程，由其就地补注，不在 02（状态机定义权威）重复以免膨胀。

**编译失败诊断报告内容**（即 09 附录 B 所称「降级分析报告」；属 **3 轮编译失败后的事后诊断**，与 § 3.5.1「预估超预算检查」的事前诊断语义分离，人类据此选择 `--degrade=ffi|manual|skip`）：translator 在 `compile_fixing` 阶段第 3 轮失败后输出结构化 JSON 诊断，至少包含——失败分类（`compilation_error` / `type_complexity` / `dependency_resolution` / `semantic_gap`）、触发错误的代码片段与错误信息、本轮已尝试的修复策略、以及针对每种降级方式（FFI/手动/裁剪）的预估代价。**职责与 UX 链路（一句话定策略）**：JSON 诊断由 translator SubAgent 输出 → Skill 主上下文从 `migration-state.json` 读取并在 `paused` 状态下渲染为终端表格（失败历史 + 三种降级方式对比）→ 用户通过 `/migrate run --degrade=ffi|manual|skip` 或交互式三选一确认（**参数优先，交互式为兜底**）。降级决策后的骨架代码自动生成、无需人工手写：scaffolder 生成 FFI 绑定桩，translator 标记 TODO(port) 供人工处理，skip 则更新 PARITY.md 记录裁剪。失败处理流程见 [06 § 10.2.2](./06-plugin-structure.md#1022-失败恢复机制)，调度路径见 [06 § 10.5](./06-plugin-structure.md#105-编排调度路径)。

**降级后依赖级联影响** [M2+]：当模块 A 降级为 FFI 桥接时，依赖 A 的模块 B 的翻译需要知道 A 的接口类型（FFI 桥接 vs 原生 Rust）。编排器在 A 降级后需要：
1. 更新 `migration-state.json` 中 A 的状态为 `degrade(ffi)`，并记录 A 的 FFI 接口描述
2. 后续模块 B 的翻译上下文中注入 A 的接口类型信息——如果 A 是 FFI 桥接，B 需要通过 FFI 调用 A 而非直接 Rust 调用
3. 如果 A 后来从 FFI 桥接升级为原生 Rust，B 的调用方式也需要同步更新

> **降级决策学习（M2+ nice-to-have）**：MVP 的 PAUSE 流程对每个失败模块逐一人工确认降级方式，以保证迁移质量控制。后续迭代（M2+）可在 `migration-state.json` 新增 `degrade_decision_history` 字段及自动推荐逻辑，基于失败分类（`compilation_error`/`type_complexity`/`dependency_resolution`/`semantic_gap`）与模块特征匹配历史决策，减少大规模迁移（20K+ 行）中的重复人工确认；属性能/可用性优化项而非关键路径，工作量见 [08 § M2](./08-roadmap-and-reference.md#m2-质量提升8-12-周)。

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
| ≤ 1K 行 且 依赖接口 < 10（依赖模块数，即 imports 边数） | 直接翻译 |
| 1K–5K 行 且 依赖接口 < 20 | 全量翻译，预算检查通过即可 |
| > 5K 行 **或** 依赖接口 ≥ 20 | 按功能边界拆分；功能边界不清时按类内聚拆分。拆分后需对受影响子图重算迁移顺序（topo-sort） |
| `transitive_dependency_depth ≥ 5`（拓扑最长路径，定义为"深链"） | 优先翻译公共中间层，对该模块采用"先翻译依赖再翻译本体"或依赖注入降低链深 |
| 拆分后仍 > 100K | 触发降级路径（见下） |

> **循环依赖不再触发降级（破环：M2-SCALE-SCC，见 [MDR-004](../decisions/004-scc-fold-break-cycle.md)）**：源码强连通分量（SCC）由 `state populate-modules` **缩点折叠为一个 composite 模块组**（`ModuleState.member_files` 列成员），不再列为降级触发条件。论据：Rust 同一 crate 内 mod 间循环 `use` 合法，源码环不是翻译障碍。**折叠后翻译粒度=单文件**（SCC 是编译门禁单元≠翻译单元）：契约+stub→契约门→逐文件填空→整组编译门，见 [MDR-006](../decisions/006-scc-per-file-stub-first.md)。仅当**单个 SCC 大到超上下文预算**时才退化为 FFI 切分兜底（当前 TODO）。流程见 [03 § 4.2 循环依赖处理](./03-execution-model.md#42-外循环sprint-级跨会话天周)。

**深链可行性说明**：在 5K–20K 行的中型 TS 项目中，深链（depth ≥ 5）通常占模块的少数（基于 dependency-cruiser 对真实项目依赖图的经验，多为单向浅链）。interface_only 仅注入签名，签名 token 通常为实现的 1/3 量级（定性估计，待 Spike 3 实测校准），因此多数深链经接口压缩后可控；少数无法压缩到预算内的，走下方降级路径。

> **定量验证计划**：本节阈值（≥20 依赖 / ≥5K 行 / depth≥5）与 interface_only 压缩率均为定性估计，定量验证见 [08-roadmap-and-reference.md M0 Spike 3](./08-roadmap-and-reference.md#m0-假设验证周2-3-周)（对 3–5 个真实中型 TS 项目实测深链占比、压缩率、超预算模块占比）。M1 需根据 `DESIGN_ASSUMPTIONS.md` 的 Spike 3 实测结论调整本决策树阈值；若实测压缩率不足或超预算占比偏高，则在 `DESIGN_ASSUMPTIONS.md` 记录「高依赖链项目处理预案」（自动降级或人工预处理）。

**预估超预算检查（事前；禁止硬填满预算）**：下文阈值相对 `[context].max_tokens_per_translation`（记为 B，默认 100K）按比例定义：直接翻译 ≤ 0.80B；提示确认 0.80B–0.95B；强制拆分 > 0.95B。token 预估是确定性计算，由 **Skill 主上下文在 `/migrate run` 调用 translator 前**执行（而非 translator 自身），用上方预算计算公式调用 tokenizer 粗估，得 `total_estimated` 后据此分支：≤0.80B 直接翻译；0.80B–0.95B 提示用户确认并建议拆分后可继续；>0.95B 自动触发拆分建议 / FFI 降级 / 人工处理三选一，并将选择记入 `migration-state.json`。该检查归 Skill 主上下文而非 translator，调度位置见 [06 § 10.5](./06-plugin-structure.md#105-编排调度路径)。若 translator 仍收到 > 100K 任务（如拆分未生效），不得截断源码硬塞，而应输出与 § 3.4 `编译失败诊断报告` 同构的诊断（实际 token 预估、缺失/被裁的接口、拆分建议）并暂停由 Skill 决策。降级行为是否自动触发由 `.rustmigrate.toml` 的 `[context]` 配置控制（见 [06 § 11.1](./06-plugin-structure.md#111-rustmigratetoml-配置文件)）。

> **类型复杂度作为前置降级信号（M2+）**：上方决策树仅按行数 / 依赖数判定拆分，未把「类型复杂度」作为独立降级因子。`type_complexity` 目前只是 3 轮失败后的事后失败分类（见 § 3.4 编译失败诊断报告），用户须先投入翻译才被告知该模块应走 FFI。M2+ 可由 analyzer 在 PROFILE 阶段前置预判：基于导出类型数、泛型约束 / 条件类型 / 联合-交集类型频率、嵌套深度等指标输出模块「可翻译性」标注，写入 `.rust-migration/PROFILE.md` 供 PLAN 决策（高复杂度直接建议 FFI），把事后失败前移为前置风险识别。MVP 不引入量化评分，保持 PROFILE 纯事实采集（§ 3.2.3）；此为 nice-to-have 优化项，工作量见 [08 § M2](./08-roadmap-and-reference.md#m2-质量提升8-12-周)。
