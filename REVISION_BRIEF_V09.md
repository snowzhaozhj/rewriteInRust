# v0.9 修订摘要

基于 8 路专项研究的核心改进。所有修改针对 PROJECT_DESIGN.md v0.8.1。

---

## 1. Skill 命名空间合并：8 个独立命令 → `/migrate` 命名空间前缀 + 3-4 个子命令

**位置**：TL;DR（第 9-18 行）、第十章 10.1 节（第 1112-1127 行）、11.4 节用户旅程（第 1515-1531 行）、附录 B SKILL.md 骨架（第 1927-2026 行）、全文所有 `/migrate-*` 引用
**严重度**：必须

**现状问题**：
- 当前设计 8 个独立 Skill（`/migrate-init`、`/migrate-plan`、`/migrate-test`、`/migrate-run`、`/migrate-verify`、`/migrate-status`、`/migrate-graduate`、`/migrate-unsafe-audit`）
- 社区共识：所有 skill 共享 25,000 token 预算，SKILL.md 有 500 行上限
- 8 个独立 SKILL.md 总量过大，且对 CE 用户而言命令数过多
- OpenSpec 等成功案例仅 3 个命令

**修改内容**：
- 将 8 个 Skill 合并为命名空间前缀 `/migrate` + 子命令模式
- MVP 核心命令：
  - `/migrate analyze` — 合并原 `/migrate-init` + `/migrate-plan` + `/migrate-test`（分析源码、生成规则、搭建测试基础设施）
  - `/migrate run` — 执行模块迁移（保持原有内循环逻辑）
  - `/migrate review` — 合并原 `/migrate-verify` + `/migrate-status`（验证管线 + 进度仪表板）
- 非 MVP 命令：
  - `/migrate graduate` — 合并原 `/migrate-graduate` + `/migrate-unsafe-audit`（毕业评估 + unsafe 审计）
- `analyze` 内部通过 SKILL.md 分步指令实现原 init→plan→test 的串行流程，用户无需记住 3 个命令的执行顺序
- TL;DR 中的"8 个 Skill 命令（MVP 6 个）"改为"3 个核心命令（MVP 3 个，后续 +1）"
- 11.4 节四级用户旅程简化为：L1 探索（`/migrate analyze`）→ L2 执行（`/migrate run`）→ L3 审查（`/migrate review`）→ L4 毕业（`/migrate graduate`）
- 附录 B 的 SKILL.md 骨架更新为新命令名

**关联影响**：
- 10.5 节编排调度路径表需要按新命令重写
- 全文约 40+ 处 `/migrate-*` 引用需替换
- M1 路线图中的交付物从"6 个 SKILL.md"改为"3 个 SKILL.md"

---

## 2. 翻译流程增加 Phase A/B + 对抗性审查

**位置**：第四章 4.3 节内循环（第 375-404 行）、10.2 节 SubAgent 职责（第 1129-1137 行）、附录 B `/migrate-run` 骨架（第 1966-2026 行）
**严重度**：必须

**现状问题**：
- 当前内循环是线性的：语义解构 → 翻译 → 编译验证 → 测试验证
- 缺少 Bun 实践中验证有效的 Phase A/B 分离和对抗性审查步骤
- 翻译和优化混在一起，导致错误难以定位

**修改内容**：

将 4.3 节 Work Unit 内循环改为两阶段翻译：

```
Step 1: 上下文加载（不变）

Step 2: 语义解构（不变）

Step 3: Phase A — 忠实翻译
  - 生成 Rust 代码，优先保持与源码的 1:1 对应（便于 diff 对照审查）
  - Private 方法默认翻译（不省略），保持结构完整性
  - 标记系统：TODO(port) 标记未完成项，PERF(port) 标记已知性能问题，PORT NOTE 标记翻译决策
  - F1 反馈：编译验证（秒级）

Step 4: 对抗性审查
  - verifier SubAgent 对 Phase A 产出物执行对抗性审查
  - 逐维度比对源码与翻译结果（使用 7.7 节探测维度清单）
  - 产出物：审查报告（差异列表 + 修正建议）

Step 5: Phase B — 编译修正 + 惯用化优化
  - 基于审查报告修正语义偏差
  - 并发/内存管理部分允许重写（非直译），须记录 MDR
  - 惯用 Rust 优化（消除翻译腔）
  - F1 + F2 反馈循环

Step 6: 产出物更新（不变）
```

- 10.2 节 translator SubAgent 职责增加"Phase A 忠实翻译 + Phase B 优化"
- 10.2 节 verifier SubAgent 职责增加"Phase A→B 中间的对抗性审查"
- 附录 B `/migrate-run` 骨架按新流程重写

---

## 3. F1 反馈改为 rust-analyzer LSP 驱动

**位置**：第四章 4.4 节三层反馈（第 406-414 行）、10.3 节 Hooks（第 1163-1222 行）、3.1 节架构图（第 156-202 行）
**严重度**：重要

**现状问题**：
- 当前 F1 通过 PostToolUse Hook 触发 `cargo check`
- rust-analyzer LSP 已内置于 Claude Code 开发环境，能自动提供编译诊断
- 用 Hook 触发 cargo check 是重复工作，且 Hook 增加了延迟

**修改内容**：
- F1 反馈层改为"rust-analyzer LSP 自动诊断"，删除 PostToolUse → cargo check 的 Hook 配置
- 10.3 节 Hook 保留范围调整为：
  - `cargo fmt` — PostToolUse Hook，写入 .rs 文件后自动格式化
  - `PreToolUse` 文件保护 — 防止 agent 修改源项目文件或关键产出物
- `check.sh` 从 `.claude/scripts/` 中移除（rust-analyzer 替代）
- `verify.sh` 和 `full-verify.sh` 保留
- 4.4 节表格 F1 行更新：

| 层级 | 触发时机 | 延迟 | 内容 | 处理方式 |
|------|---------|------|------|---------|
| F1 编译反馈 | 每次写入 .rs 文件 | 秒级 | **rust-analyzer LSP 自动诊断** | 自动反馈给 LLM 重试 |

- 3.1 节架构图中质量门禁层更新：

```
│  F1: rust-analyzer LSP 自动诊断（无需 Hook）                      │
│  F2 Hook: .claude/scripts/verify.sh → 测试套件 (Tier 0+1)        │
│  F3 Skill: /migrate review 手动触发 → 集成验证 (Tier 0+1+2)      │
│  格式化 Hook: PostToolUse → cargo fmt (仅 .rs 文件)               │
│  文件保护 Hook: PreToolUse → 防止修改源项目文件                    │
```

**权衡**：rust-analyzer 在超大项目中可能有性能问题；保留 `verify.sh` 中的 cargo check 作为 F2 的一部分（确定性兜底）。

---

## 4. PORTING.md 拆分为通用规则 + 项目专有规则

**位置**：第六章 6.2 节 PORTING.md（第 598-636 行）、10.6 节产出物目录结构（第 1303-1338 行）、第十章插件结构整体
**严重度**：必须

**现状问题**：
- 当前所有 26 类规则都在单个 PORTING.md 中
- 通用规则（类型映射、命名约定等）每个项目重复编写
- 无法随 Plugin 分发通用规则

**修改内容**：

拆分为两层：

**通用规则**（随 Plugin 分发）：
- 位置：`.claude/rules/` 目录，使用 paths 条件加载
- 内容：语言通用的类型映射、命名约定、标准库映射、禁止模式等（约 60% 的确定性模板规则）
- 格式：每个规则一个 `.md` 文件，通过 Claude Code 的 rules 机制按 paths 条件自动加载
- 示例结构：
  ```
  .claude/rules/
  ├── ts-type-mapping.md          # paths: ["*.ts", "*.tsx"]
  ├── ts-naming-convention.md     # paths: ["*.ts", "*.tsx"]
  ├── rust-idioms.md              # paths: ["*.rs"]
  ├── error-handling-patterns.md  # paths: ["*.rs"]
  └── unsafe-policy.md            # paths: ["*.rs"]
  ```

**项目专有规则**（项目本地）：
- 位置：`.rust-migration/porting/` 目录
- 内容：项目特有的规则（外部依赖映射、业务逻辑处理策略、特殊模式等）
- 格式：Markdown 文件，按规则类分文件
- 保留 Changelog 机制
- 示例结构：
  ```
  .rust-migration/porting/
  ├── dependency-mapping.md       # 项目特有的依赖映射
  ├── business-logic-rules.md     # 业务逻辑翻译策略
  ├── known-workarounds.md        # 项目特有的 workaround
  └── changelog.md                # 规则变更记录
  ```

- 6.2 节标题改为"迁移规则体系（通用 + 项目专有）"
- 26 类规则表增加一列"层级"（通用/项目专有）
- 50KB 以下可全量加载（约 12K tokens），但多语言场景必须拆分为按语言独立的规则文件

---

## 5. 知识分层简化：取消独立 knowledge/ 目录

**位置**：第六章 6.11 节增量知识沉淀架构（第 826-867 行）、10.6 节产出物目录结构（第 1303-1338 行）
**严重度**：重要

**现状问题**：
- 当前 patterns/ 和 anti-patterns/ 放在 `.rust-migration/` 下
- 通用知识和项目知识未区分分发方式
- 缺少与 Plugin 分发机制的对齐

**修改内容**：

知识存储拆分为两层，不建独立 knowledge/ 目录：

**通用知识**（随 Plugin 分发）：
- 位置：`skills/migrate/references/` 目录
- 内容：跨项目通用的翻译模式、反模式、最佳实践
- 格式：YAML frontmatter + Markdown（便于语义检索）
- 示例：
  ```
  skills/migrate/references/
  ├── patterns/
  │   ├── async-to-tokio.md
  │   └── error-handling-migration.md
  └── anti-patterns/
      ├── naive-mutex-wrap.md
      └── arc-everything.md
  ```

**项目知识**（项目本地）：
- 位置：`.rust-migration/context/` 目录（替代原 patterns/ + anti-patterns/ 散放）
- 内容：项目特有的翻译经验、失败记录
- 格式：YAML frontmatter + Markdown
- 示例：
  ```
  .rust-migration/context/
  ├── patterns/              # 项目特有的成功模式
  ├── anti-patterns/         # 项目特有的失败教训
  └── module-learnings/      # 替代原 intermediate/{module}-learnings.md
  ```

- 6.11 节 4 层知识存储表中 L3 层更新存储位置
- 10.6 节目录结构更新
- 不需要配套基础设施（检索靠 Claude Code 的文件读取即可，M2+ 可接入 AgentMemory）

---

## 6. 从第一天起设计为 Claude Code Plugin

**位置**：第十章整体（第 1112-1338 行）、3.1 节架构图（第 156-202 行）
**严重度**：必须

**现状问题**：
- 当前描述为"Claude Code 插件生态（Skills + Hooks + SubAgents）"，但未明确 Plugin 打包格式
- 各组件散放，未按 Plugin 标准结构组织

**修改内容**：

明确 Plugin 目录结构（10.1 节前新增 Plugin 概览）：

```
rust-migrate-plugin/                    # Plugin 根目录
├── plugin.json                         # Plugin 元数据（名称、版本、描述）
├── skills/
│   └── migrate/
│       ├── SKILL.md                    # 主 Skill（/migrate 命令入口）
│       ├── analyze.md                  # /migrate analyze 子命令
│       ├── run.md                      # /migrate run 子命令
│       ├── review.md                   # /migrate review 子命令
│       ├── graduate.md                 # /migrate graduate 子命令（非 MVP）
│       ├── adapters/                   # 语言适配器（原 11.2 节）
│       │   ├── typescript/
│       │   └── python/
│       └── references/                 # 通用知识库（参考资料）
│           ├── patterns/
│           └── anti-patterns/
├── agents/
│   ├── analyzer.md                     # 分析 SubAgent
│   ├── translator.md                   # 翻译 SubAgent
│   ├── verifier.md                     # 验证 SubAgent
│   └── scaffolder.md                   # 测试搭建 SubAgent
├── hooks/
│   ├── settings.json                   # Hook 配置
│   └── scripts/
│       ├── fmt.sh                      # cargo fmt（替代原 check.sh）
│       ├── verify.sh                   # F2 验证脚本
│       ├── full-verify.sh              # F3 完整验证脚本
│       └── file-guard.sh              # 文件保护（PreToolUse）
├── rules/                              # 通用迁移规则（按 paths 条件加载）
│   ├── ts-type-mapping.md
│   ├── rust-idioms.md
│   └── ...
└── README.md                           # Plugin 使用说明
```

- 3.1 节架构图用户入口层更新为 Plugin 入口
- 11.2 节语言适配器移入 Plugin 的 `skills/migrate/adapters/` 下
- 删除原来散落在 `.claude/` 各处的引用，统一到 Plugin 目录

---

## 7. 存储格式统一规则

**位置**：全文涉及文件格式的部分，主要是 6.1 节（第 580-596 行）、10.6 节（第 1303-1338 行）、附录 A/D（第 1753-2127 行）
**严重度**：重要

**现状问题**：
- 文档中未明确各类文件的格式选型原则
- patterns/ 文件格式未定义

**修改内容**：

在 6.1 节产出物总览表后新增"格式选型原则"：

| 格式 | 适用场景 | 文件示例 |
|------|---------|---------|
| **JSON** | 机器状态、程序读写 | `migration-state.json`、`source-graph.json`、`type-map.json`、报告文件 |
| **TOML** | 项目配置、人类可编辑 | `.rustmigrate.toml` |
| **YAML frontmatter + Markdown** | 知识/模式文件（需元数据 + 正文） | `patterns/*.md`、`anti-patterns/*.md`、`references/*.md` |
| **纯 Markdown** | 人类文档（规则、记录、差异） | `PORTING rules`、`MDR`、`KNOWN_DIFFERENCES.md`、`SPRINT_LEARNINGS.md` |

知识/模式文件的 YAML frontmatter 格式示例：
```markdown
---
id: async-to-tokio
category: pattern
language: typescript
tags: [async, tokio, concurrency]
created: 2026-06-10
sprint: S2
confidence: high
---

# Async/Await 到 Tokio 的翻译模式

...正文...
```

---

## 8. Workflow（ultracode）用于大规模并行迁移

**位置**：第四章执行模式（第 334-479 行）、第十三章路线图 M2（第 1663-1684 行）、新增或扩展第十一章
**严重度**：重要

**现状问题**：
- 当前仅有 Skill 交互式执行模式
- 大规模项目（50K+ 行）需要批量并行迁移，Skill 逐模块交互效率低
- 未提及 Claude Code Workflow 和 /goal 的集成

**修改内容**：

在第四章 4.2 节后新增 4.2.1 节"执行模式分层"：

| 模式 | 入口 | 适用场景 | 并行度 | 阶段 |
|------|------|---------|--------|------|
| **Skill 交互式** | `/migrate analyze`、`/migrate run`、`/migrate review` | 分步调试、学习流程、小项目 | 串行 | MVP (M1) |
| **Workflow 批量** | `ultracode` workflow 定义文件 | 多模块并行迁移、CI/CD 集成 | 多 agent 并行 | M2+ |
| **/goal 自主循环** | `/goal "迁移 module X, Y, Z"` | 自主迁移循环（analyze→run→review 自动串联） | 串行但自主 | M2+ |

Workflow 定义要点（M2 阶段设计）：
- 每个 agent 在独立 worktree 中工作（避免文件冲突）
- 按拓扑排序分批，同一批内可并行
- 每个 agent 执行 `/migrate run` 的内循环
- 汇总阶段合并结果到主分支

M2 路线图交付物增加：
- [ ] Workflow 定义文件（ultracode 格式）
- [ ] 多 agent worktree 隔离机制
- [ ] /goal 自主迁移循环支持

---

## 9. 文档拆分：2100+ 行主文件拆为 INDEX + 子文件

**位置**：PROJECT_DESIGN.md 整体结构
**严重度**：必须

**现状问题**：
- 当前 PROJECT_DESIGN.md 超过 2100 行，远超 Claude Code 上下文的高效阅读范围
- 附录中的 Schema 示例（约 400 行）应随代码管理
- 修改某一章节时需要加载全文

**修改内容**：

拆分为 INDEX 主文件 + 5 个子文件：

```
docs/design/
├── PROJECT_DESIGN.md               # INDEX（~300 行）：TL;DR + 目录 + 各章摘要 + 交叉引用
├── 01-positioning-methodology.md   # 第一章项目定位 + 第二章核心方法论（~200 行）
├── 02-architecture.md              # 第三章架构设计（~200 行）
├── 03-execution-testing.md         # 第四章执行模式 + 第七章测试验证（~400 行）
├── 04-plugin-structure.md          # 第十章插件结构 + 第十一章扩展（~500 行）
├── 05-roadmap-risks.md             # 第十二章风险 + 第十三章路线图 + 第十四章数据参考（~300 行）
└── schemas/                        # Schema 示例移到代码目录
    ├── migration-state.schema.json
    ├── source-graph.example.json
    ├── type-map.example.json
    └── call-graph.example.json
```

主文件（INDEX）内容：
- 保留完整 TL;DR（精简到 ~20 行）
- 保留目录，每个条目链接到子文件
- 每个章节保留 2-3 行摘要，链接到详细内容
- 保留命名约定和核心设计原则
- 附录 A/D 的 Schema 示例移到 `schemas/` 目录，主文件仅保留引用链接

拆分原则：
- 第五章工具链、第六章文档体系、第八章策略路由、第九章陷阱 → 分散归入最相关的子文件（工具链归入 03，文档归入 04，策略路由归入 03，陷阱归入 05）
- 每个子文件可独立阅读，必要的上下文在文件头部通过引用说明

---

## 10. 附带修正：SubAgent 数量与 Skill 合并的适配

**位置**：10.2 节（第 1129-1137 行）、10.5 节编排调度路径（第 1239-1301 行）
**严重度**：建议

**现状问题**：
- Skill 从 8 个缩减到 3-4 个后，每个 Skill 内部的 SubAgent 调度序列会变长
- 原 10.5 节的调度序列表需要按新 Skill 重写

**修改内容**：

更新 10.5 节编排调度路径表：

| Skill | 调度序列 | 说明 |
|-------|---------|------|
| `/migrate analyze` | `analyzer` → `translator`(规则生成) → `scaffolder`(测试搭建) → 写入所有初始化产出物 | 原 init+plan+test 合并，序列最长（4 步） |
| `/migrate run` | `translator`(Phase A) → `verifier`(对抗审查) → `translator`(Phase B) → `verifier`(测试验证) | Phase A/B 双阶段 |
| `/migrate review` | `verifier`(全量验证) → 生成报告 → 更新 PARITY.md + 状态仪表板 | 原 verify+status 合并 |
| `/migrate graduate` | `verifier`(毕业评估 + unsafe 审计) → 生成毕业报告 | 原 graduate+unsafe-audit 合并 |

注意 `/migrate analyze` 的 4 步序列是 M0 Spike 1 验证的主要对象——如果指令跟随不够可靠，此命令应拆为子步骤（Plan B1）。

---

## 11. M1 路线图交付物更新

**位置**：第十三章 M1 节（第 1617-1661 行）
**严重度**：必须

**现状问题**：
- M1 交付物列表需要与上述所有修改对齐

**修改内容**：

M1 工作量分解更新：

| 交付物 | 预估人天 | 说明 |
|--------|---------|------|
| 3 个 SKILL.md（analyze/run/review） | 4-6 | 每个 Skill 约 1.5-2 天（含调试）；analyze 最复杂 |
| 4 个 SubAgent agent.md | 3-4 | 不变 |
| 2 个 Hook 脚本（fmt.sh + verify.sh） | 1 | 减少 1 个（check.sh 被 rust-analyzer 替代） |
| 文件保护 Hook（file-guard.sh） | 0.5 | 新增：PreToolUse 文件保护 |
| 通用规则文件（.claude/rules/） | 2-3 | 新增：TS 通用规则拆分为独立文件 |
| TS 语言适配器 | 3-5 | 不变 |
| Plugin 打包结构 | 1-2 | 新增：plugin.json + 目录组织 |
| migration-state.json 管理 | 2-3 | 不变 |
| .rustmigrate.toml | 1 | 不变 |
| 集成测试 + 调试 | 5-8 | 不变 |
| **合计** | **22-33** | 与原估算基本持平 |

---

## 修订原则

- 用中文
- 只改需要改的部分，不全面重写
- 标注哪些是 MVP 必须，哪些是后续迭代
- 每条修改标注位置（章节 + 行号范围）和严重度
- 严重度定义：
  - **必须**：不改则设计无法落地或与研究结论矛盾
  - **重要**：不改可工作但有明显优化空间
  - **建议**：锦上添花，可延后处理

## 修改严重度汇总

| # | 修改项 | 严重度 |
|---|--------|--------|
| 1 | Skill 命名空间合并 8→3-4 | 必须 |
| 2 | Phase A/B 翻译流程 | 必须 |
| 3 | F1 改为 rust-analyzer LSP | 重要 |
| 4 | PORTING.md 规则拆分 | 必须 |
| 5 | 知识分层简化 | 重要 |
| 6 | Plugin 格式设计 | 必须 |
| 7 | 存储格式统一 | 重要 |
| 8 | Workflow 批量模式 | 重要 |
| 9 | 文档拆分 | 必须 |
| 10 | SubAgent 调度适配 | 建议 |
| 11 | M1 路线图更新 | 必须 |
