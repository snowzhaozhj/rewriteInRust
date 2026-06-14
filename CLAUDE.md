# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概述

**Rust 迁移验证工作台**：Claude Code Plugin + `rustmigrate` CLI，帮助开发者将 TS/Python/C 项目迁移到 Rust。

**当前阶段**：M1 MVP（Phase 0 合约已冻结）。设计文档 v0.9.4 已完成 9 轮对抗审查收敛。

## 多会话续接（重要）

新会话启动后按以下顺序读取上下文：
1. **本文件**（CLAUDE.md）→ 项目概览 + 约束 + 命令
2. **`docs/STATUS.md`** → 当前位置（哪个 Sprint、进行中任务、下一步）
3. **`docs/PLAN.md`** → 对应 Sprint 的目标和验收标准

**Worktree 注意**：EnterWorktree 默认从 `origin/master` 分支，进入 worktree 前先 `git fetch origin` 确保本地远程引用是最新的，否则会丢失已合并到 master 的提交。

## 仓库结构

```
cli/                  Rust CLI workspace（rustmigrate-core + rustmigrate）
  crates/core/        核心逻辑（graph/state/profile/validate/scaffold）
  crates/cli/         CLI 入口（clap）
plugin/               Claude Code Plugin
  .claude-plugin/     plugin.json
  skills/migrate/     SKILL.md + analyze/run/review
  agents/             4 个 SubAgent（analyzer/translator/verifier/scaffolder）
  hooks/              hooks.json + scripts/
docs/design/          设计文档 v0.9.4（10 个 Markdown 子文件）
docs/PLAN.md          实施计划（Sprint 分解 + 依赖 + 验收）
docs/STATUS.md        当前状态快照（每次会话结束更新）
docs/learnings/       开发知识沉淀
docs/decisions/       项目自身 MDR
docs/review/          设计审查循环产物（9 轮历史）
fixtures/             验证用 TS 项目
```

## 开发命令

```bash
just check      # cargo check
just test       # cargo nextest
just lint       # cargo clippy -D warnings
just fmt        # cargo fmt
just ci         # 全量 CI 本地模拟（fmt-check + lint + test + deny + shellcheck）
just build      # cargo build
```

## Commit 规范

```
<type>(<scope>): 简要描述

类型: feat / fix / docs / refactor / test / chore
scope: M0-S0 / M1-CLI-03 / M1-PLG-01 等（引用 PLAN.md 任务 ID）
```
- commit message 用中文
- Co-Authored-By 行自动添加
- 设计文档修改仍用 `docs:` 前缀

## 核心术语

- **Milestone (M)**：M0 验证 / M1 MVP / M2 质量 / M3 多语言 / M4 完善
- **Sprint**：实施计划中的迭代单位（见 docs/PLAN.md）
- **State**：编排器状态机节点（INIT/PROFILE/PLAN/SCAFFOLD/SPRINT_LOOP/GRADUATE）
- **Phase A/B**：模块级翻译阶段（忠实翻译 vs 惯用化优化）
- **source-graph.db**：源码图主存储（SQLite），JSON 为导出格式

## 编码约束

- CLI 输出统一 JSON 格式：`{"status":"ok|error|warning", "data":{...}, "warnings":[...]}`
- Rust 代码遵循 clippy -D warnings
- CLI 与 Plugin 通过文件系统 + JSON 通信,不直接耦合
- 设计细节以 `docs/design/` 为唯一权威（实现时对照,不二次文档化）

## 开源参考（~/workspace/explore/）

| 参考项目 | 学什么 |
|---------|--------|
| guppy | petgraph 封装（图引擎实现时对照） |
| ast-grep | tree-sitter 集成 + CLI workspace 结构 |
| codegraph | SQLite 图存储 |
| cargo-modules | CLI 输出格式 |
| OpenSpec | Plugin 结构参考 |
| claw-code | PARITY.md + 迁移方法论 |
| Understand-Anything | SKILL.md 组织 |

## 文件权威来源

- CLI 命令列表 → `docs/design/06-plugin-structure.md`
- 图数据模型 → `docs/design/04-toolchain.md § 5.7.1`
- 状态机定义 → `docs/design/02-architecture.md § 3.4` + `09-appendix-schemas.md`
- 实施计划 → `docs/PLAN.md`
- 当前状态 → `docs/STATUS.md`

## Fixture 与测试

4 个 TS fixture 项目用于验证 CLI 准确性：
```
fixtures/
  linear-deps/    线性依赖（基本 topo-sort）
  diamond-deps/   菱形 + type-only + barrel + implements + calls
  circular-deps/  循环依赖（环检测）
  edge-cases/     空文件 / 语法错误 / 纯类型
```

每个 fixture 含 `ground-truth.json`（偏序约束格式），CLI 输出必须满足其中的约束。

验证命令：
```bash
cargo nextest run -p rustmigrate-core    # 单元/集成测试
cargo run -- graph build --root fixtures/linear-deps  # 手动验证
```

## 质量门

每个任务标记 done 前必须通过 4 层检查：
1. **代码级**：`just fmt-check && just lint` 全过
2. **行为级**：fixture ground-truth 偏序约束满足
3. **集成级**：`just test` 全过（含下游命令）
4. **审查级**：实现与 `docs/design/` 对应章节逐项一致（字段、枚举、schema、默认值）

可通过 Skill 工具调用 `/gate` 一键执行以上 4 层检查。

## 阶段交付流程

每个阶段（Sprint / Phase / Worker）完成后：

1. 质量门：调用 `/gate` skill 确认 4 层全过
2. 更新 `docs/STATUS.md`
3. commit 引用任务 ID（如 `feat(M1-GRAPH): 图构建模块`）
4. 独立分支提 PR
5. PR 审查：调用 `/pr-review-toolkit:review-pr`，额外运行 `design-checker` agent 检查设计文档一致性；再跑 `codex:codex-rescue`（对抗审查）和 `/code-review` skill
6. 修复 critical/important issues 后通知用户审阅

禁止合并多阶段为一个 PR。

## 续接快速参考

**新会话开始**：读 CLAUDE.md → `docs/STATUS.md` → `docs/PLAN.md` 对应任务

## 设计文档一致性检查

修改 `docs/design/` 后运行：
```bash
grep -rn "source-graph\.json" docs/design/ --include="*.md"
grep -rn "PORTING\.md" docs/design/ --include="*.md"
grep -rn "\.claude/rules" docs/design/ --include="*.md"
grep -rn "4 步\|4步" docs/design/ --include="*.md"
```
