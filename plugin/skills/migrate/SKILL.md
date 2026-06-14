---
name: migrate
description: 把 TypeScript / Python / C 项目迁移到 Rust 的验证工作台。当用户想要分析待迁移项目、生成迁移规则、逐模块翻译为 Rust、或查看迁移进度时使用——只要用户提到"迁移到 Rust""rewrite in Rust""把这个项目改写成 Rust""迁移进度"或对一个 TS/Python/C 仓库表达改写为 Rust 的意图，即使没有显式输入命令也应考虑此 skill。子命令：analyze（分析+建规则+搭测试）、run（模块翻译）、review（验证+仪表板）。
argument-hint: "[analyze|run|review] [module]"
---

# /migrate — Rust 迁移验证工作台

把源项目（TS/Python/C）迁移到 Rust 的编排入口。确定性计算（建图、统计、状态校验）由 `rustmigrate` CLI 承担；翻译策略、等价性判断等非确定性工作由 SubAgent 完成。本文件是路由 + 所有子命令共享的约定；具体流程在子命令文件中按需 Read。

## 路由

读取调用参数的第一个词，Read 对应子命令文件并严格按其分步指令执行：

| 参数 | 子命令文件 | 作用 |
|------|-----------|------|
| `analyze`（默认） | [analyze.md](./analyze.md) | 分析源码、生成迁移规则、搭建测试基础设施（init+plan+test 合并） |
| `run` | [run.md](./run.md) | 翻译指定模块（Phase A 忠实翻译 / Phase B 惯用化，M1 Phase 4 启用） |
| `review` | [review.md](./review.md) | 完整验证管线 + 迁移进度仪表板（M1 Phase 4 启用） |
| `graduate` | — | 毕业评估 + unsafe 审计（M2，非 MVP） |

无参数时默认 `analyze`（迁移的起点）。参数为未知词时，提示用户可用子命令而非猜测。

## 共享约定（所有子命令遵守）

### CLI 调用与输出解析
- 通过 Bash 调用 `rustmigrate <子命令>`，工作目录为源项目根。所有 CLI 输出是统一 JSON：`{status, data, warnings}`。
- **只解析 `data` 字段**取结构化结果；`status` 为 `error` 时按 `data` 中的错误码处理，不要从自然语言里猜成败。`warnings` 非空时如实转达用户，不要静默吞掉。
- 命令清单（M1 共 13 个）：`init`、`profile --root`、`graph build --root [--full]`、`graph topo-sort`、`graph deps <m>`、`graph interfaces <m> [--deps-of <t>]`、`graph stats`、`validate state`、`state get <m>`、`state transition --module --to [--substatus] [--reason]`、`stats loc`、`stats compare`、`scaffold workspace [--target] [--name]`。

### SubAgent 编排（权威：06-plugin-structure.md §10.2 / §10.5）
- 用 **Agent tool** 调用 SubAgent，参数 `subagent_type` 取 agent 名（`analyzer` / `translator` / `scaffolder` / `verifier`）。若 Plugin 命名空间要求前缀，则用 `rust-migrate:analyzer` 形式。参数名与命名空间均属运行时行为，待 M1-PLG-05 交互式 Live 验证确认（设计 §10.2.1 记为 `agentType`，以真实工具参数 `subagent_type` 为准）。MVP 阶段 SubAgent **串行执行**，通过 `.rust-migration/` 下的文件通信，不直接对话。
- **不解析 SubAgent 的返回文本判断成功**。每次调用后只做产出物的确定性校验：
  - **L1 存在性**：文件存在、非空、含关键标题（Markdown / 代码 / 配置产出物）。
  - **L2 结构校验**：JSON 产出物（`migration-state.json`、测试结果）格式合法、关键字段非空；`source-graph.db` 必要表存在。
- 校验失败时按失败恢复三步处理：①记录调用到 `migration-state.json.subagent_calls` ②诊断+重试（`max_retries_per_step` 默认 2）③重试耗尽则提示用户三选项「重试 / 部分跳过(降级) / 完整回滚」。中间产物 `intermediate/attempts/*` 始终保留。

### 产出物根目录
所有产出物在源项目下的 `.rust-migration/` 目录（`init` 创建）。关键文件：`migration-state.json`、`source-graph.db`、`porting/`（迁移规则）、`PARITY.md`、`AGENTS.md`、`test-fixtures/golden/`。写 `migration-state.json` 统一走 CLI（原子写：tmp→fsync→rename）。

> **诚实占位说明**：本 Plugin 的 `analyze` 流程（M1 Phase 3）已完整可执行；`run` / `review`（Phase 4）当前为骨架，未到可对真实项目跑通的程度，子命令文件中已显式标注，不糊弄。
