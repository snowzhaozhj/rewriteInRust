# CLAUDE.md

## 项目概述

**Rust 迁移验证工作台**：Claude Code Plugin + `rustmigrate` CLI，将 TS/Python/C 项目迁移到 Rust。

当前阶段：M1 MVP（Phase 0 合约已冻结）。

## 续接协议

新会话读：本文件 → `docs/STATUS.md`（当前位置）→ `docs/PLAN.md`（目标与验收）

## 仓库结构

```
cli/crates/core/      核心逻辑（graph/state/profile/validate/scaffold）
cli/crates/cli/       CLI 入口（clap）
plugin/               Plugin（skills/agents/hooks）
docs/design/          设计文档 v0.9.4（唯一权威）
docs/PLAN.md          实施计划
docs/STATUS.md        状态快照（每次会话结束更新）
docs/decisions/       MDR
fixtures/             验证用 TS 项目（含 ground-truth.json 偏序约束）
```

## 命令

```bash
just check    # cargo check
just test     # cargo nextest
just lint     # cargo clippy -D warnings
just fmt      # cargo fmt
just ci       # fmt-check + lint + test + deny + shellcheck
```

## 约束

- CLI 输出统一 JSON：`{"status":"ok|error|warning", "data":{...}, "warnings":[...]}`
- `clippy -D warnings` 零警告
- CLI 与 Plugin 通过文件系统 + JSON 通信，不直接耦合
- 设计细节以 `docs/design/` 为唯一权威，不二次文档化

## Commit

```
<type>(<scope>): 中文描述
类型: feat/fix/docs/refactor/test/chore
scope: PLAN.md 任务 ID（如 M1-GRAPH-01）
```

## 术语

| 缩写 | 含义 |
|------|------|
| M0-M4 | 验证 / MVP / 质量 / 多语言 / 完善 |
| Phase A/B | 忠实翻译 / 惯用化优化 |
| source-graph.db | 源码图 SQLite 主存储 |
| State | INIT→PROFILE→PLAN→SCAFFOLD→SPRINT_LOOP→GRADUATE |

## 权威来源

| 主题 | 文件 |
|------|------|
| CLI 命令 | `docs/design/06-plugin-structure.md` |
| 图数据模型 | `docs/design/04-toolchain.md § 5.7.1` |
| 状态机 | `docs/design/02-architecture.md § 3.4` + `09-appendix-schemas.md` |

## 质量门

任务完成前必须通过：
1. `just fmt-check && just lint` 全过
2. fixture ground-truth 偏序约束满足
3. `just test` 全过
4. 与 `docs/design/` 对应章节一致

## 交付流程

每个阶段（Sprint/Phase/Worker）独立交付：

1. 质量门 → 2. 更新 STATUS.md → 3. 独立分支提 PR → 4. `/pr-review-toolkit:review-pr` → 5. 修复 critical/important 后通知用户

禁止合并多阶段为一个 PR。

## 开源参考（~/workspace/explore/）

guppy(petgraph封装) · ast-grep(tree-sitter+CLI) · codegraph(SQLite图存储) · cargo-modules(输出格式) · OpenSpec(Plugin) · claw-code(PARITY.md) · Understand-Anything(SKILL.md)

## 设计文档一致性检查

修改 `docs/design/` 后运行：
```bash
grep -rn "source-graph\.json\|PORTING\.md\|\.claude/rules\|4 步\|4步" docs/design/ --include="*.md"
```
