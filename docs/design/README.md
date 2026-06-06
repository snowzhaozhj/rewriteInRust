# Rust 迁移验证工作台 — 项目设计文档

> **版本**: v0.9.4 | **日期**: 2026-06-06 | **基于**: 18 路深度调研 + 103 agent deep research + 7 轮审查反馈迭代 + 3 路独立审查（v0.8）+ 8 路专项研究（v0.9）+ 6 路图设计源码分析（v0.9.3）

---

### TL;DR — 30 秒理解本项目

**给谁用**：想用 AI 做 Rust 迁移、但担心翻译质量的开发者和团队。

**做什么**：一套 Claude Code Plugin（3 个核心命令：`/migrate analyze`、`/migrate run`、`/migrate review`，后续 +1 个 `/migrate graduate`），自动分析源项目 -> 生成迁移规则 -> 逐模块翻译 -> 用 8 层测试验证正确性。你只需要 `/migrate analyze` 开始，工具引导你完成后续步骤。

**核心差异**：验证管线开箱即用——cargo check / clippy / proptest / fuzz 的管线自动配置，**独立脚本门禁让 AI 无法跳过验证**（纯 prompt 做不到这一点）。产出物（迁移规则 + KNOWN_DIFFERENCES.md 差异登记 + MDR 决策记录）是团队协作和审计的标准化资产。

> **最小概念集**（入门只需理解这些）：
> - 3 个核心命令（MVP 3 个，后续 +1）：`/migrate analyze` -> `/migrate run` -> `/migrate review`（+ `/migrate graduate` 后续迭代）
> - 3 层反馈：F1 编译（rust-analyzer LSP 秒级自动）-> F2 测试（分钟级）-> F3 集成（Sprint 级手动）
> - 翻译双阶段：Phase A 忠实翻译 -> 对抗性审查 -> Phase B 惯用化优化
> - 其余概念（26 条规则、4 个 SubAgent、8 层测试等）在你推进到相应阶段时才需要理解

---

### 为什么不自己做

| 维度 | 手动方式（Claude Code + 好 prompt） | 本工具 |
|------|--------------------------------------|--------|
| 验证门禁 | 依赖 prompt 指令跟随，AI 可自我说服跳过 | **独立脚本门禁**（PostToolUse Hook），AI 无法绕过 |
| 迁移规则 | 每次手写 PORTING.md，格式不统一 | **26 类规则模板**（通用规则随 Plugin 分发 + 项目专有规则本地生成），渐进式生成，版本化演化 |
| 差异追踪 | 靠人记忆哪些行为不同 | **KNOWN_DIFFERENCES.md** 自动发现 + 分类 + 人类审批 |
| 断点续传 | 关闭会话后丢失进度 | **migration-state.json** 完整状态持久化 |
| 测试搭建 | 每个项目从头搭建测试基础设施 | **行为录制 -> 黄金文件 -> proptest** 自动化管线 |
| 知识积累 | 经验留在对话历史中，下次迁移重来 | **patterns/ + anti-patterns/** 跨项目复用 |
| 决策溯源 | 无记录，3 个月后不知道为什么选了 tokio | **MDR** 迁移决策记录长期保留 |
| 适用建议 | **<2K 行小项目直接手动迁移更划算** | **5K+ 行项目性价比显著提升** |

---

### 子文件导航

本设计文档拆分为主索引 + 子文件，可按需阅读：

| 子文件 | 内容 | 原章节 |
|--------|------|--------|
| [01-positioning-and-methodology.md](./01-positioning-and-methodology.md) | 项目定位 + 核心方法论 | 第一章 + 第二章 |
| [02-architecture.md](./02-architecture.md) | 架构设计（整体架构、状态机、上下文管理） | 第三章 |
| [03-execution-model.md](./03-execution-model.md) | 执行模式（Sprint 循环）+ 测试验证策略 + 策略路由 | 第四章 + 第七章 + 第八章 |
| [04-toolchain.md](./04-toolchain.md) | 工具链选型 | 第五章 |
| [05-documentation-system.md](./05-documentation-system.md) | 文档与知识沉淀体系 | 第六章 |
| [06-plugin-structure.md](./06-plugin-structure.md) | Plugin 结构 + 工作流扩展 | 第十章 + 第十一章 |
| [07-pitfalls-and-risks.md](./07-pitfalls-and-risks.md) | 常见陷阱 + 风险评估 | 第九章 + 第十二章 |
| [08-roadmap-and-reference.md](./08-roadmap-and-reference.md) | 实施路线图 + 关键数据参考 | 第十三章 + 第十四章 |
| [09-appendix-schemas.md](./09-appendix-schemas.md) | Schema 定义 + SKILL.md 骨架 | 附录 |
| [schemas/](./schemas/) | JSON Schema 示例文件（migration-state、source-graph、type-map、call-graph） | 附录数据 |

每个子文件顶部有 `> [返回主索引](./README.md)` 导航链接，可独立阅读。

---

### Plugin 目录结构概览

本项目从第一天起设计为 Claude Code Plugin，通过 `.claude-plugin/plugin.json` 标准格式打包分发：

```
rust-migrate-plugin/
├── .claude-plugin/plugin.json          # Plugin 元数据（name, version, skills 路径）
├── skills/
│   └── migrate/
│       ├── SKILL.md                    # /migrate 命令入口（路由分发）
│       ├── analyze.md                  # /migrate analyze 子命令
│       ├── run.md                      # /migrate run 子命令
│       ├── review.md                   # /migrate review 子命令
│       ├── graduate.md                 # /migrate graduate（非 MVP）
│       ├── adapters/                   # 语言适配器
│       └── references/                 # 参考指南（按需 Read）
│           ├── patterns/               # 翻译模式（如 async-to-tokio.md）
│           └── anti-patterns/          # 反模式（如 naive-mutex-wrap.md）
├── agents/                             # 4 个 SubAgent（内嵌核心规则）
│   ├── analyzer.md                     # 源码分析 + 项目画像
│   ├── translator.md                   # 代码翻译（含核心类型映射/命名规则）
│   ├── verifier.md                     # 等价性验证 + 对抗审查
│   └── scaffolder.md                   # 测试基础设施搭建
├── hooks/
│   ├── hooks.json                      # Hook 配置（cargo fmt + 文件保护）
│   └── scripts/
└── README.md
```

### 核心命令速览

| 命令 | 功能 | 内部调度序列 | MVP? |
|------|------|-------------|------|
| `/migrate analyze` | 分析源码、生成规则、搭建测试基础设施 | analyzer -> translator(规则) -> scaffolder(测试) | 是 |
| `/migrate run` | 执行模块迁移（Phase A/B 双阶段） | translator(A) -> verifier(审查) -> translator(B) -> verifier(测试) | 是 |
| `/migrate review` | 运行验证管线 + 进度仪表板 | verifier(全量验证) -> 报告 -> PARITY.md | 是 |
| `/migrate graduate` | 毕业评估 + unsafe 审计 | verifier(毕业评估) -> 毕业报告 | 否 |

---

### 命名约定

本文档使用以下统一术语，避免混淆：

| 术语 | 含义 | 示例 |
|------|------|------|
| **状态（State）** | 编排器状态机中的节点，描述迁移流程的当前位置 | INIT, PROFILE, PLAN, SCAFFOLD, SPRINT_LOOP, GRADUATE |
| **里程碑（Milestone, M）** | 实施路线图中的交付阶段，描述产品成熟度 | M0 假设验证, M1 MVP, M2 质量提升, M3 多语言, M4 完善 |
| **Sprint** | SPRINT_LOOP 状态内的迭代循环，每个 Sprint 迁移一批模块 | Sprint 1, Sprint 2, ... |

文档中不再使用 "Phase 0/1/2/..." 等编号表述。注意：翻译流程中的 "Phase A/Phase B" 是模块级翻译阶段（忠实翻译 vs 惯用化优化），与路线图阶段无关。

---

### 存储格式约定

| 格式 | 适用场景 | 文件示例 |
|------|---------|---------|
| **JSON** | 机器状态、程序读写 | `migration-state.json`、`source-graph.json`、报告文件 |
| **TOML** | 项目配置、人类可编辑 | `.rustmigrate.toml` |
| **YAML frontmatter + Markdown** | 知识/模式文件（需元数据 + 正文） | `patterns/*.md`、`anti-patterns/*.md` |
| **纯 Markdown** | 人类文档（规则、记录、差异） | 迁移规则、MDR、`KNOWN_DIFFERENCES.md` |

---

### v0.9 相较 v0.8.1 的主要变化

| # | 变化 | 严重度 |
|---|------|--------|
| 1 | Skill 命名空间合并：8 个独立命令 -> 3 个核心命令（`/migrate analyze`、`/migrate run`、`/migrate review`） | 必须 |
| 2 | 翻译流程增加 Phase A/B 双阶段 + 对抗性审查 | 必须 |
| 3 | F1 反馈改为 rust-analyzer LSP 驱动（替代 PostToolUse cargo check Hook） | 重要 |
| 4 | PORTING.md 拆分为通用规则（随 Plugin 分发）+ 项目专有规则（本地） | 必须 |
| 5 | 知识分层简化：取消独立 knowledge/ 目录，分为 Plugin 通用知识 + 项目本地知识 | 重要 |
| 6 | 从第一天起设计为 Claude Code Plugin（明确 plugin.json + 标准目录结构） | 必须 |
| 7 | 存储格式统一规则（JSON / TOML / YAML frontmatter + Markdown / 纯 Markdown） | 重要 |
| 8 | 新增 Workflow（ultracode）批量执行模式（M2+） | 重要 |
| 9 | 文档拆分为 INDEX + 子文件 | 必须 |
| 10 | SubAgent 调度路径按新 Skill 适配 | 建议 |
| 11 | M1 路线图交付物更新 | 必须 |

### v0.9.3 相较 v0.9.2 的主要变化

| # | 变化 | 严重度 |
|---|------|--------|
| 1 | 图设计全面重构：6 路开源项目源码分析（stack-graphs/guppy/CodeGraph/GitNexus/UA/cargo-modules），从 4 行扩展为 7 个子节，覆盖数据模型/内存引擎/持久化/构建管线/增量更新/查询/可视化 | 必须 |
| 2 | 图节点类型精细化：13 种节点（MVP 10 + M2 3）+ 12 种边（MVP 8 + M2 4），含设计决策推导 | 必须 |
| 3 | 迁移案例修正：Pokemon Showdown 确认为幻觉并移除；Bun PORTING.md 确认不在仓库中；新增 Claw-Code 深度分析 | 必须 |
| 4 | PARITY.md 增强：引入等价深度标签（strong/stub，M2 扩展四级），借鉴 Claw-Code 的多维度追踪 | 重要 |
| 5 | Clippy 作为迁移规则执行器：MVP 3-5 条 disallowed_methods 硬性门禁 | 重要 |
| 6 | unsafe 策略决策：按源语言设预算（TS→Rust 接近 0），三案例对比 | 重要 |
| 7 | 翻译流程补充：源文件保留模式 + TODO(port) 清理纪律 + 异步策略决策树 | 建议 |
| 8 | 新增 rusqlite 嵌入 crate（SQLite 图持久化 + FTS5 全文搜索） | 重要 |

---

### 证据等级说明

本文档引用的案例和数据按以下等级标注：

| 等级 | 含义 | 可信度 |
|------|------|--------|
| **论文验证** | 发表在同行评审会议/期刊 | 高——有实验数据和复现方法 |
| **商业案例** | 企业官方博客/技术报告 | 中高——有生产数据但可能选择性披露 |
| **社区传闻** | GitHub 项目/个人博客/论坛 | 中低——可能缺少关键细节，需独立验证 |

> **2026 年论文特别说明**：标注"待验证"的论文需在 M0 阶段补充 DOI 或 arXiv 链接。即使论文引用不可验证，相关设计理念（迭代修复、拓扑排序、多候选生成）已在工业实践中得到广泛验证，不影响方案可行性。
