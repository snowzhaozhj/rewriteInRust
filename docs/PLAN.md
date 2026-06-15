# rustmigrate 实施计划

> 本文件是项目执行的唯一权威计划。新会话读 CLAUDE.md → STATUS.md → 本文件即可接续执行。

---

## §1 项目结构蓝图

### Core Crate 目录结构

```
cli/crates/core/src/
├── lib.rs              # pub mod 声明 + re-export
├── types/
│   ├── mod.rs          # 类型模块入口
│   ├── common.rs       # NodeId, EdgeId, Span, SourceLang 等基础类型
│   ├── graph.rs        # SourceNode, Dependency, SourceGraph 结构
│   ├── state.rs        # MigrationState, Phase, ModuleStatus 枚举/结构
│   └── config.rs       # MigrateConfig, ProfileConfig, ThresholdConfig
├── error.rs            # thiserror 统一错误类型 MigrateError
├── response.rs         # JSON 输出结构 Response<T> { status, data, warnings }
├── graph/
│   ├── mod.rs          # 图模块入口（模块职责注释）
│   ├── build.rs        # tree-sitter 解析 → SourceGraph 构建
│   ├── query.rs        # 图查询（neighbors, paths, subgraph）
│   ├── topo.rs         # 拓扑排序 + 迁移序列生成
│   └── persist.rs      # SQLite 读写（source-graph.db）
├── state/
│   ├── mod.rs          # 状态机入口
│   ├── machine.rs      # 状态转换逻辑
│   └── validate.rs     # 状态合法性校验 + 前置条件检查
├── profile/
│   ├── mod.rs          # 项目分析入口
│   └── detect.rs       # 语言检测 + 复杂度评估
├── scaffold/
│   ├── mod.rs          # 脚手架生成入口
│   └── template.rs     # Cargo.toml / mod 结构模板
├── stats/
│   ├── mod.rs          # 统计入口
│   └── coverage.rs     # 迁移进度 + 覆盖率统计
└── validate/
    ├── mod.rs          # 验证入口
    └── rules.rs        # 验证规则（Tier 0/1/2）
```

### 模块依赖 DAG

```
types ← error ← response
  ↑        ↑
  │    ┌───┴───────┐
  │    │           │
graph/build   state/machine
graph/query   state/validate
graph/topo        ↑
  │               │
  ↓               │
graph/persist  profile → scaffold
                         stats
                         validate
```

**核心规则**：
- `graph/build`, `graph/query`, `graph/topo` 不依赖 `graph/persist`（纯计算与 IO 分离）
- `state` 不依赖 `graph`（仅依赖 `types::graph` 中的类型定义）
- `persist` 是唯一接触文件系统/数据库的模块

### 文件所有权规则（并行开发时）

| 文件/目录 | Owner | 说明 |
|-----------|-------|------|
| `types/` | Phase 0 统一 | 合约冻结后只读 |
| `error.rs`, `response.rs` | Phase 0 统一 | 合约冻结后只读 |
| `graph/*` | Worker A | Phase 1 独占 |
| `state/*`, `validate/*` | Worker B | Phase 1 独占 |
| `profile/*`, `scaffold/*`, `stats/*` | Worker C | Phase 1 独占 |
| `plugin/hooks/*` | Worker D | Phase 1 独占 |
| `lib.rs` | 仅追加 mod 声明 | 任何 Worker 可追加，不删改已有行 |
| `main.rs` | Phase 2 集成 | Phase 1 禁改 |
| `Cargo.toml` | 各 Worker 加自己的 dep | 冲突时协调 |

---

## §2 测试 Harness

### Fixture 项目

| Fixture | 场景 | 覆盖验证点 |
|---------|------|-----------|
| `linear-deps` | A→B→C 线性依赖 | 拓扑排序基础正确性、单链迁移序列 |
| `diamond-deps` | A→B,A→C,B→D,C→D | 菱形依赖处理、并行度识别 |
| `circular-deps` | A↔B 循环引用 | 环检测、错误报告、强连通分量 |
| `edge-cases` | re-export/type-only/dynamic-import/namespace | 边界情况健壮性 |

每个 fixture 包含：
- `src/` — TypeScript 源码
- `ground-truth.json` — 期望输出（偏序约束格式）
- `README.md` — 场景说明

### ground-truth.json 偏序约束格式

```json
{
  "nodes_must_exist": ["src/a.ts", "src/b.ts"],
  "edges_must_exist": [["src/a.ts", "src/b.ts", "import"]],
  "topo_order_constraints": [
    { "before": "src/b.ts", "after": "src/a.ts" }
  ],
  "node_count_range": [3, 5],
  "edge_count_range": [2, 6]
}
```

偏序而非全序：只约束必须满足的顺序关系，允许实现选择等价节点的排列。

### Insta 快照策略

```rust
// settings 全局配置
insta::Settings::new()
    .set_snapshot_suffix("cli")
    .redact("[timestamp]", insta::dynamic_redaction(|v, _| "[TIMESTAMP]"))
    .redact("[absolute_path]", |v, _| "[PATH]")
    .redact("[hash]", |v, _| "[HASH]");
```

- 所有时间戳 → `[TIMESTAMP]`
- 绝对路径 → `[PATH]`
- 内容哈希 → `[HASH]`
- DB 文件不做快照，只快照 JSON 输出

### CI 配置

```yaml
# just ci 等价流程
steps:
  - cargo fmt --check
  - cargo clippy -D warnings
  - cargo nextest run
  - cargo deny check
  - shellcheck plugin/hooks/scripts/*.sh
```

---

## §3 执行架构

### Phase 定义与时序

```
Phase 0 (合约冻结)    ───→ types/ + error + response + schema.sql 就绪
    │
    ▼
Phase 1 (4路并行)     ───→ 各模块独立实现 + 单元测试
    │  Worker A: graph
    │  Worker B: state + validate
    │  Worker C: profile + scaffold + stats
    │  Worker D: Plugin hooks
    │
    ▼
Phase 2 (集成)        ───→ main.rs 路由 + Thin E2E 通过
    │
    ▼
Phase 3 (并行Plugin)  ───→ SubAgent + SKILL.md analyze
    │
    ▼
Phase 4 (收敛)        ───→ 翻译循环 + MVP 验收
```

### 并行规则

1. Phase 1 各 Worker 只改自己所有权范围内的文件
2. `main.rs` 在 Phase 1 期间**禁止修改**
3. `types/` 在 Phase 0 冻结后，变更需走合约变更协议
4. `lib.rs` 仅允许追加 `pub mod xxx;` 行
5. `Cargo.toml` 的 `[dependencies]` 各 Worker 可加，冲突时最后集成方解决

### 合约变更协议

当需要修改已冻结的 `types/` 或 `error.rs` 时：

1. 提出方在 `docs/decisions/` 创建 MDR（Migration Decision Record）
2. MDR 中说明：变更内容、影响范围、向后兼容性
3. 所有 Worker 确认无冲突（或描述适配方案）
4. 合并变更，各 Worker 适配
5. 非破坏性追加（新增字段/变体）可快速通过，破坏性变更需全量评估

### Workflow / Background Agent 使用方式

| Phase | 机制 | 原因 |
|-------|------|------|
| Phase 1 | Workflow + worktree | 4 路并行编码，文件隔离避免冲突 |
| Phase 3 | Background agents | Plugin 开发与 CLI 独立，无文件冲突 |
| Phase 2/4 | 单线程顺序 | 集成工作需全局视角，不可并行 |

### 续接协议

**写入方（会话结束前 4 步）**：
1. 更新 `docs/STATUS.md`：当前 Phase / in-progress 任务 / 下一步 / 阻塞项
2. 已完成任务在本文件标 `[x]`
3. commit message 引用任务 ID（如 `feat(M1-GRAPH): graph build 实现`）
4. 若有未完成的并行任务，记录各 Worker 进度到 STATUS.md

**读取方（新会话开始 4 步）**：
1. 读 `CLAUDE.md` → 项目概览 + 约束 + 命令
2. 读 `docs/STATUS.md` → "我在哪"（当前 Phase + 进度）
3. 读本文件对应 Phase/Sprint 段 → "要做什么，怎么算完"
4. 如果任务需要设计细节 → 读 `docs/design/` 对应章节

---

## §4 Sprint 0：假设验证（M0，2-3 天）

精简为 2 个核心 Spike，只验证最高风险假设。

### Spike S0：Plugin 加载 + crate 编译

| 维度 | 内容 |
|------|------|
| **Goal** | 验证 Claude Code Plugin 能正常加载，skill/agent/hook 三者 work；Rust CLI 可编译到合理体积 |
| **Steps** | 1. 创建最小 plugin.json + 一个 skill<br>2. 注册一个 SubAgent（echo）<br>3. 注册一个 hook（onFileCreate）<br>4. `cargo build --release` 观察二进制大小 |
| **Pass** | skill 可触发 + agent 可调用 + hook fire 可观测；release binary < 50MB |
| **Fail→PlanB** | 回退非 Plugin 方案（纯 CLI + 手动协调） |
| **产出物** | `plugin/.claude-plugin/plugin.json` 最小版 + `docs/decisions/001-plugin-viability.md` |
| **Done** | 输入: 空项目 / 命令: 安装 plugin 后触发 skill / 产物: hook 日志 + agent 响应 / 阈值: 三者均 work |

### Spike S3：tree-sitter TS 精度

| 维度 | 内容 |
|------|------|
| **Goal** | 验证 tree-sitter-typescript 对 TS 源码的 exports/imports/calls 提取精度 |
| **Steps** | 1. 准备 20 个 TS 代码片段（覆盖 named/default/re-export/dynamic）<br>2. 用 tree-sitter 提取三类关系<br>3. 与手工标注对比计算 precision/recall |
| **Pass** | exports/imports/calls 三项 F1 ≥ 0.90 |
| **Fail→PlanB** | 降级到 TypeScript Compiler API（通过 `ts-morph` 或子进程调用 `tsc`） |
| **产出物** | `fixtures/ts-precision-bench/` + 精度报告 + `docs/decisions/002-parser-choice.md` |
| **Done** | 输入: 20 段 TS 代码 / 命令: `cargo test -p rustmigrate-core tree_sitter` / 产物: 精度表 / 阈值: F1 ≥ 0.90 |

### Sprint 0 GATE

- [x] 2 个 Spike 全部有结论（Pass 或 触发 PlanB）
- [x] 决策文档写入 `docs/decisions/`
- [x] M1 执行路径已确定

---

## §5 Phase 0：冻结合约（M1-TYPES，1 天）

**目标**：定义所有公共类型、错误、响应格式和数据库 schema，作为 Phase 1 并行开发的稳定合约。

### 任务清单

| 任务 ID | 内容 | 文件 |
|---------|------|------|
| M1-TYPES-01 | 基础类型定义 | `types/common.rs` |
| M1-TYPES-02 | 图相关类型 | `types/graph.rs` |
| M1-TYPES-03 | 状态相关类型 | `types/state.rs` |
| M1-TYPES-04 | 配置相关类型 | `types/config.rs` |
| M1-TYPES-05 | 统一错误类型 | `error.rs` |
| M1-TYPES-06 | JSON 响应结构 | `response.rs` |
| M1-TYPES-07 | SQLite schema | `schema.sql`（嵌入 `persist.rs` 或独立文件） |

### 四元组 Done

| 输入 | 命令 | 产物 | 阈值 |
|------|------|------|------|
| 设计文档 §04/§09 | `cargo check` | types/ + error.rs + response.rs 编译通过 | 零 warning；所有 pub 类型有 doc comment |

### 完成标志

- [ ] `cargo check` 通过，无 warning
- [ ] 所有 pub struct/enum 有 `///` 文档注释
- [ ] `schema.sql` 包含 nodes/edges/metadata/schema_versions 四表
- [ ] 各类型与 `docs/design/09-appendix-schemas.md` 一致

---

## §6 Phase 1：四路并行实现（M1 核心，5-7 天）

### Worker A：Graph 模块（M1-GRAPH）

**目标**：完整的源码图构建、查询、拓扑排序和持久化。

**文件所有权**：`graph/build.rs`, `graph/query.rs`, `graph/topo.rs`, `graph/persist.rs`, `graph/mod.rs`

| 子任务 | 内容 |
|--------|------|
| M1-GRAPH-01 | `build.rs`：tree-sitter 解析 TS → SourceGraph（petgraph StableGraph） |
| M1-GRAPH-02 | `query.rs`：neighbors / paths / subgraph / stats 查询 |
| M1-GRAPH-03 | `topo.rs`：拓扑排序 + 环检测 + 迁移序列生成 |
| M1-GRAPH-04 | `persist.rs`：SQLite 读写（rusqlite） |

**四元组 Done**：

| 输入 | 命令 | 产物 | 阈值 |
|------|------|------|------|
| `fixtures/linear-deps/` | `cargo nextest -p rustmigrate-core` | 全部 graph 测试通过 + insta 快照 | ground-truth 偏序约束 100% 满足 |
| `fixtures/diamond-deps/` | 同上 | 菱形依赖正确处理 | 拓扑序满足偏序约束 |
| `fixtures/circular-deps/` | 同上 | 环检测 + 错误报告 | 检测到环并报告涉及节点 |

**验证命令**：
```bash
cargo nextest run -p rustmigrate-core --filter-expr 'test(graph::)'
```

### Worker B：State + Validate 模块（M1-STATE）

**目标**：迁移状态机 + 状态转换校验 + 验证规则引擎。

**文件所有权**：`state/machine.rs`, `state/validate.rs`, `state/mod.rs`, `validate/rules.rs`, `validate/mod.rs`

| 子任务 | 内容 |
|--------|------|
| M1-STATE-01 | `machine.rs`：状态机（INIT→PROFILE→PLAN→SCAFFOLD→SPRINT_LOOP→GRADUATE） |
| M1-STATE-02 | `validate.rs`：状态转换前置条件检查 |
| M1-STATE-03 | `validate/rules.rs`：Tier 0/1/2 验证规则定义 |
| M1-STATE-04 | 模块级 `ModuleStatus` 转换（CLI `state transition`：解析 ModuleStatus + substatus/reason 落盘 + 合法性前置校验 + 原子写）；区别于 01 的项目级状态机。当前 CLI 诚实占位（`implemented:false`），**M1 收尾补做**（09-appendix） |

**四元组 Done**：

| 输入 | 命令 | 产物 | 阈值 |
|------|------|------|------|
| 状态转换序列（合法+非法） | `cargo nextest -p rustmigrate-core` | state 测试全过 | 合法转换成功；非法转换返回明确错误 |

**验证命令**：
```bash
cargo nextest run -p rustmigrate-core --filter-expr 'test(state::) | test(validate::)'
```

### Worker C：Profile + Scaffold + Stats 模块（M1-PROFILE）

**目标**：项目分析、Rust 脚手架生成、迁移进度统计。

**文件所有权**：`profile/detect.rs`, `profile/mod.rs`, `scaffold/template.rs`, `scaffold/mod.rs`, `stats/coverage.rs`, `stats/mod.rs`

| 子任务 | 内容 |
|--------|------|
| M1-PROFILE-01 | `profile/detect.rs`：语言检测 + 文件统计 + 复杂度评估 |
| M1-PROFILE-02 | `scaffold/template.rs`：生成 Cargo.toml + src/ 骨架 |
| M1-PROFILE-03 | `stats/coverage.rs`：模块迁移进度 + 覆盖率计算 |
| M1-PROFILE-04 | profile 工具可用性检测：按 `analysis-tools.json` 验证适配器工具+版本 → `ADAPTER_TOOL_MISSING`；检测 `cargo-nextest` → `RUST_TOOL_MISSING`。当前 CLI 占位 + warning，**M1 收尾补做**（06:90） |
| M1-PROFILE-05 | `stats loc`：tokei 源码/Rust LOC 统计，替换当前借用 coverage 迁移进度的占位语义。**M1 收尾补做**（06:99） |

**四元组 Done**：

| 输入 | 命令 | 产物 | 阈值 |
|------|------|------|------|
| `fixtures/linear-deps/` | `cargo nextest -p rustmigrate-core` | profile + scaffold + stats 测试全过 | 检测到 TS 语言；scaffold 产出可编译的 Cargo.toml |

**验证命令**：
```bash
cargo nextest run -p rustmigrate-core --filter-expr 'test(profile::) | test(scaffold::) | test(stats::)'
```

### Worker D：Plugin Hooks（M1-HOOK）

**目标**：Plugin hooks 实战化，覆盖关键生命周期事件。

**文件所有权**：`plugin/hooks/hooks.json`, `plugin/hooks/scripts/*`

| 子任务 | 内容 |
|--------|------|
| M1-HOOK-01 | `hooks.json` 定义（onFileCreate/onFileEdit/postToolUse） |
| M1-HOOK-02 | `scripts/on-rust-file-create.sh`：新 .rs 文件自动 clippy |
| M1-HOOK-03 | `scripts/post-build.sh`：cargo check 结果反馈 |

**四元组 Done**：

| 输入 | 命令 | 产物 | 阈值 |
|------|------|------|------|
| 创建 .rs 文件 | 触发 hook | hook 日志显示 fire + clippy 输出 | hook 在 5s 内 fire；脚本 exit 0 |

**验证命令**：
```bash
shellcheck plugin/hooks/scripts/*.sh
# 手动：在 Claude Code 中创建 .rs 文件观察 hook 是否 fire
```

---

## §7 Phase 2：集成验证（M1-INTEGRATE，2-3 天）

**目标**：将 Phase 1 各模块合并到 `main.rs`，实现全命令路由，跑通 Thin E2E。

### 任务清单

| 任务 ID | 内容 |
|---------|------|
| M1-INTEG-01 | `main.rs` 全命令路由（clap subcommands: init/graph-build/graph-topo/profile/scaffold/stats/validate） |
| M1-INTEG-02 | Thin E2E：`init → graph build → graph topo` 链路端到端通过 |
| M1-INTEG-03 | 所有命令输出符合 JSON 格式规范 |
| M1-INTEG-04 | `just ci` 全量通过 |
| M1-INTEG-05 | 边类型方向对齐设计文档：Contains 补 File→Function/Class；Calls 改为 Function→Function（需调整 build.rs 调用归属逻辑）；Exports source 改为 Module 节点 |

### 四元组 Done

| 输入 | 命令 | 产物 | 阈值 |
|------|------|------|------|
| `fixtures/linear-deps/` | `cargo run -- init . && cargo run -- graph build && cargo run -- graph topo` | 三步链路成功输出 JSON | 每步 status:"ok"；topo 结果满足 ground-truth 偏序 |

### 完成标志

- [ ] `main.rs` 包含全部命令路由
- [ ] `just ci` 全绿
- [ ] Thin E2E（init→build→topo）在 fixture 项目上通过
- [ ] 所有命令输出 `{"status":"ok|error", "data":{...}, "warnings":[...]}` 格式

---

## §8 Phase 3：并行 Plugin 实现（M1-PLUGIN，3-5 天）

**目标**：SubAgent 完善 + SKILL.md analyze 完整实现，可对真实项目执行分析。

### 任务清单

| 任务 ID | 内容 |
|---------|------|
| M1-PLG-01 | analyzer SubAgent：调用 CLI graph build + 产出分析报告 |
| M1-PLG-02 | translator-rule SubAgent：生成翻译规则 JSON |
| M1-PLG-03 | scaffolder SubAgent：调用 CLI scaffold + 验证产出 |
| M1-PLG-04 | SKILL.md analyze 完整流程（7 步序列） |
| M1-PLG-05 | 至少 1 次对 fixture 项目的真实执行验证 |

### SubAgent 产出物

| SubAgent | 输入 | 产出 |
|----------|------|------|
| analyzer | 项目路径 | `source-graph.db` + 分析报告 JSON |
| translator-rule | source-graph + 模块列表 | 翻译规则 JSON（type mappings + idiom rules） |
| scaffolder | 项目路径 + profile | Rust 项目骨架（Cargo.toml + src/ 结构） |

### 四元组 Done

| 输入 | 命令 | 产物 | 阈值 |
|------|------|------|------|
| `fixtures/linear-deps/` | `/migrate analyze`（在 Claude Code 中） | `.rust-migration/` 目录含 source-graph.db + state.json + porting/ | 至少 1 次完整执行成功 |

### 完成标志

- [ ] 3 个 SubAgent 各自可独立调用
- [ ] SKILL.md analyze 7 步序列至少 1 次完整通过
- [ ] 产出物结构符合 `docs/design/06-plugin-structure.md` §10.2

---

## §9 Phase 4：翻译循环 + MVP 验收（M1-TRANSLATE，5-7 天）

**目标**：实现翻译循环（Phase A/B + verifier），完成 MVP 验收。

### 任务清单

| 任务 ID | 内容 |
|---------|------|
| M1-TRANS-01 | translator SubAgent Phase A：忠实翻译（逐行对应） |
| M1-TRANS-02 | translator SubAgent Phase B：惯用化优化（Rust 惯用写法） |
| M1-TRANS-03 | verifier SubAgent：对抗审查（等价性检查） |
| M1-TRANS-04 | SKILL.md `run` 完整实现 |
| M1-TRANS-05 | SKILL.md `review` 完整实现（仪表板） |
| M1-TRANS-06 | 3 项目 MVP 验收 |

### 翻译循环流程

```
Phase A（忠实翻译）→ Tier 0 验证 → Phase B（惯用化）→ Tier 0+1 验证
                                                          │
                                                          ▼
                                                   verifier 对抗审查
                                                          │
                                                          ▼
                                                   PARITY.md 更新
```

### MVP 验收指标（3 项目）

| 指标 | 标准 |
|------|------|
| 功能正确 | cargo check + clippy 零 error |
| 自动测试 | 每个迁移模块 ≥1 个自动测试通过 |
| 结构可识别 | Rust 代码结构与原 TS 模块可对应（人工可读） |

### 四元组 Done

| 输入 | 命令 | 产物 | 阈值 |
|------|------|------|------|
| 3 个 TS fixture 项目 | `/migrate run` + `/migrate review` | Rust 代码 + 测试 + PARITY.md | cargo check pass + clippy pass + ≥1 test + 结构可识别 |

### 完成标志

- [ ] 3 个 fixture 项目各 ≥1 模块完成迁移
- [ ] 每个迁移模块：cargo check + clippy + ≥1 自动测试
- [ ] `/migrate review` 展示正确的进度仪表板
- [ ] 断点续传验证通过（中断后恢复不丢状态）

---

## §9.5 M1 收尾：analyze→run 衔接缺口（PLAN 填充）

> **来源**：2026-06-14 PR #9 的 Live 验证（M1-PLG-05）实跑暴露。M1-PLG-05 核心已通过（插件加载、`rust-migrate:` 命名空间确认、`/migrate analyze` 端到端触发 SubAgent、产出物真实）。但发现 **analyze→run 状态衔接结构性缺口**，是 TRANS-06 MVP 验收的硬前置。**新会话从这里接续。**

### 缺口本质
`analyze.md` Step 3 写着"填充 `migration-state.json` 的 modules/sprint 字段、推进 state"，但 **CLI 无任何命令填充 modules/sprint**（只有 init / state transition / graph build）。设计规定"写 migration-state.json 统一走 CLI"，故该步无法执行：实测 analyze 后 `modules=[]`、`sprint` 缺失、state 停在 `profile`。结果 `/migrate run`（前置要求 `sprint_loop`、Step 0.3 读 `modules[target].status`）**根本无法执行**。这是缺失的 **PLAN 操作**（topo-sort → modules + sprint 计划 + 推进到 sprint_loop）。

### 任务清单

| 任务 ID | 内容 | 阻塞 TRANS-06? | 估时 |
|---------|------|:---:|:---:|
| **M1-PLAN-01** | 新增 CLI `rustmigrate state populate-modules`：`load_graph` → `migration_sequence()` → 每模块写 `ModuleState{status:pending, sprint:1, risk:low, known_differences:0, attempts:[]}` → `set_sprint({current:1, history:[]})` → 原子落盘。复用 `topo.rs::migration_sequence`、`machine.rs::update_module/set_sprint`（均已就绪）。环图沿用 Step 2.8 `has_cycles` 门禁 | **是** | 1d |
| **M1-PLAN-02** | `analyze.md` Step 3 / `SKILL.md` 接线：调 `state populate-modules` 填充 + 用项目级 `state transition --to plan → scaffold → sprint_loop` 推进（populate 后/scaffolder 后）。检查点：state=sprint_loop、modules 非空 | **是** | 0.5d |
| **M1-DIAG-01** | 新增 CLI `rustmigrate state record-subagent-call`（core 加 `push_subagent_call`）+ SKILL 接线，兑现 `subagent_calls` append-only（设计 06:370 / 09:136）。当前恒 `[]`，诊断/卡死统计未落地 | 否（独立小 PR） | 0.5-1d |
| **M1-BOOT-01** | CLI 引导：实现设计提及的 `hooks/scripts/ensure-cli.sh`（检测/选择二进制）**或**文档化"先装 CLI 到 PATH"。当前 skill 裸调 `rustmigrate`，用户未装即失败 | 否 | 0.5d |

### 两个需定夺的设计决策（实现 M1-PLAN-01/02 前定）
1. **module key 命名**：用 NodeId 原值（`file:src/utils.ts`）**[推荐]** vs 去前缀（`utils`）。NodeId 原值与 run 阶段 `graph deps <module>` 的 `resolve_file_node` 解析一致，否则依赖门禁失配（**最大的坑**）。
2. **谁推进状态机**：SKILL 在 populate/scaffold 后显式 `state transition --to ...` 推进 **[推荐]** vs analyze 末尾一次性推进。现 `run.md` 把推进甩给 run 衔接序列但 run 又硬要求 sprint_loop，自相矛盾，必须有人推。

### MVP 最小可行范围（让 TRANS-06 跑通）
M1-PLAN-01 + M1-PLAN-02 + linear/diamond fixture e2e（init→build→populate→transition→run 前置满足）。**单 sprint**、靠 topo `order` + run Step 0.6 依赖门禁决定执行序即可。

### 推迟 M2
多 sprint 层级划分（`parallel_groups`→多 sprint）、跨模块并行、risk 评估、`sprint.history.target_modules` 预填、module key 人类友好归一化、`[persistence].backup_on_write`/`retention_days` config（machine.rs atomic_write 当前无条件备份、`.backup` 不过期；单文件覆盖式，低影响）。**populate 重填的替换语义**（重 analyze 后清理已不在迁移序列的 orphan 模块 key + 保留 sprint.history；当前为合并式写入，配合增量构建做，code-review 2026-06-15 发现，低）。**record-subagent-call 时间戳 ISO8601 格式校验**（当前 `Timestamp` 为透明 String 不校验，编排器传错序/非法时间戳会静默落盘，低）。

> **关键文件**：`cli/crates/core/src/graph/topo.rs`（migration_sequence/parallel_groups）、`cli/crates/core/src/state/machine.rs`（update_module:201/set_sprint:306，可能加 push_subagent_call）、`cli/crates/cli/src/lib.rs`（StateCommands:140 加子命令）、`cli/crates/core/src/types/state.rs`（ModuleState/SprintState schema）、`plugin/skills/migrate/analyze.md`（Step 3 接线）。

---

## §10 M2 模块级规划（质量提升，Sprint 5-8）

### §10.0 决策固化 + 审计修订（2026-06-15）

**已定 4 项设计决策**（详见 STATUS.md「已决设计决策」）：
1. **done 硬终态 + 显式 reopen**：返工走独立 `state reopen` 命令（**非扩转换矩阵**，须保持 `Done => false`，见 §10.0.1.2）记 history，废弃 `done+--force` 隐式重做 → 产出新任务 **M2-REFAC-08a**（见 Sprint 5.5）。
2. **blocked 仅 blockable 活跃态可进**：done/失败终态不可进 → 代码 `can_transition_to` 已实现此语义（§10.0.1.1），REFAC-08 仅补契约测试 + 删文档旧矛盾表述。
3. **per-module 难度信号来源 = analyzer 增产 + CLI 落库**（语义驱动非 LOC）→ 产出新前置任务 **M2-TIER-00**（见 B 段）。落地形态见 §10.0.1.4：结构化 `semantic_signals`、删旧 `risk`、tier 派生不落库。
4. **M2-SCALE 写隔离 = worktree + 组末 toml 程序化合并**（否决 deps 回传方案，因 SubAgent 循环内需独立编译自验证）。

**审计修订（前置缺口 / 风险 / 优先级）**：
- **新增 M2-TIER-00（TIER-01 硬前置）**：拆 00a/00b/00c，落地形态详见 §10.0.1.4（结构化 `semantic_signals`、删旧 `risk`、tier 派生不落库）。**验收**：annotate 后 `state get` 含 signals；缺信号时 TIER-01 选择器判 full 并 warning。**TIER-01 不得在 TIER-00 完成前开工。**
- **新增 M2-SCALE-00（SCALE 前置）**：populate 改消费 `parallel_groups`（组索引→sprint 号）。**PR 拆分以 §10.0.1.8 为准**：REFAC-06（纯 API 私有化）单独前置 PR；SCALE-00 + VER-04①（孤儿清理）合一 PR。**验收**：diamond fixture populate 后 sprint 号反映 parallel_groups 分层，非全 1。
- **ADV-07 提前为 A 的前置**：headless 默认 TODO 决策（原 Sprint 6）是并行的隐藏前置——串行可人工注入决策，并行 N 个 SubAgent 同时卡 TODO 无法人工介入。**A 开工前必须先做 ADV-07。**
- **worktree 成本缓解（并入 SCALE-02 验收）**：① 共享 `CARGO_TARGET_DIR` 避免每 worktree 重复编译 ② worktree 数 = 当前并发组大小（非全模块）③ 组末回收 ④ toml 合并同 crate 版本冲突策略 = 取较高 SemVer，不兼容则报冲突人工裁决。
- **Sprint 8 验收修正**：「全流程 < 60min」按并行重估并扣 worktree 开销；M2-VER-02「fuzz 24h」挪 nightly job、不作 CI 门。

### §10.0.1 Codex 对抗审计修订（2026-06-15）

> Codex 交叉核对代码，纠正/补强如下（已核实属实）：

1. **决策 #1/#2 代码已落地**：`can_transition_to()`（state.rs:98-124）已实现 done 硬终态（`Done => false`）+ blocked 仅 blockable 活跃态。→ REFAC-08 降为 S（补测试+文档同步），见 Sprint 5.5。
2. **reopen 改独立命令**（REFAC-08a）：扩转换矩阵会破坏 `Done => false` 硬终态语义；改为独立 `state reopen` 命令，内部追加 audit attempt，reason 强制非空，不扩 ModuleStatus 枚举。
3. **新增 REFAC-08b（SCALE 前置）**：`blocked_by` 计划语义有字段但无 CLI 写入口（transition 无 `--blocked-by`，machine.rs 进 blocked 只写 pre_blocked_status）。并行编排前必须补，否则 run.md 的 blocked 流程无法确定性落库。
4. **TIER-00 落结构化 `semantic_signals`，删除旧 `risk`（决策修订 2026-06-15）**：`ModuleState` 新增 `semantic_signals: Option<ModuleSemanticSignals>`（7 项语义维度，每项三值枚举 `SignalState = Absent|Present|Unknown` + 可选 `evidence: Vec<String>` 供人复查）；**删除恒 Low、从未真用的 `risk: RiskLevel` 字段 + `default_risk()`**（开发阶段 state.json 是运行时产物可重建，不为兼容买单）。**signals 是唯一持久化的「事实」；tier/risk 是「派生结论」，由纯函数按需计算、不落库**（单一数据源，杜绝 signals/risk 不一致）。**不引入 `confidence`**（三值枚举的 `Unknown` 已表达"无把握"，confidence 对降档决策冗余，YAGNI——TIER-01 真需要再加）。字段集：side_effects/concurrency/error_paths/numeric_boundaries/boundary_io/dynamic_behavior/type_complexity。→ TIER-00 拆 **00a**（schema：新增 ModuleSemanticSignals+SignalState、删 risk、改两处测试赋值）/ **00b**（CLI `state annotate-module-signals --module --signals-json`，校验枚举后落库）/ **00c**（analyze.md 接线产 per-module signals）。
5. **兜底落消费者非 populate**：populate 只初始化 signals=None；TIER-01 选择器规则 **`signals=None | 任一维度 Unknown | 任一危险维度 Present => full`** 并 emit warning（具体"危险维度集"留 TIER-01 开工定）。
6. **worktree 真正隐患 = migration-state.json lost update**（SCALE-02 必补）：现写入是整文件 atomic rename（machine.rs:362-378），只防半截 JSON 不防并发覆盖。须定义每 worktree 的 **state delta 格式 + 主 worktree 单写合并**。验收：两模块同时完成，主 state 保留两者 attempts/subagent_calls，无最后写覆盖。
7. **新增 SCALE-02a（toml merge spike）**：workspace 当前无 `toml_edit`/`semver` 依赖。先写独立纯函数 + fixture 测试（保 dependency table、SemVer 冲突可复现报错），再接 worktree。
8. **PR 拆分**：`SCALE-00 + REFAC-06 + VER-04①` 同 PR 偏大（跨 topo API + populate 行为 + 数据卫生）→ 拆两 PR：先 REFAC-06（纯 API 私有化），再 SCALE-00 + VER-04①。
9. **修订后依赖顺序**（两条独立线）：
   - **B 线（自适应循环）**：TIER-00（PR-1）→ TIER-01。
   - **A 线（并行规模）**：REFAC-06 → SCALE-00（+VER-04①）→ SCALE-02a(toml merge spike) → SCALE-01/02；其中 **REFAC-08b(blocked_by) + ADV-07(headless TODO 决策) 是 SCALE-01 的硬前置**（并行编排前必须就位）。
   - REFAC-08a(reopen) / 08b(blocked_by) 作为可用性前置从 Sprint 5.5 提前。两条线无强耦合，可按额度/优先级穿插，但各自前置不可跳。

### M2 可用性优先项（P0 — M1 实跑暴露，优先于 Sprint 5.5 内部重构）

> 来源：M1 Live 验证 + 并行/分档可行性探索（2026-06-14，已读代码落实约束，避免 M2 重复探索）。M1 证明翻译循环可用，但「单文件 module + 完整 11 步循环 + 串行」三者叠加对真实项目（上百文件 ≈ 30+h）不实用。
> **优先级建议：B 自适应循环 > A 并行 sprint > Sprint 5.5**——先让单次循环便宜（M、纯增量、无锁风险），再让多次并发（L、依赖 worktree）。

**A. M2-SCALE 并行 sprint（丰富 Sprint 7 已有规划）**
算法已就绪（`topo.rs::parallel_groups` 叶优先 / 组内无依赖）但**从未被消费**——`populate`（lib.rs `cmd_state_populate_modules`）把所有模块塞 sprint=1。分层落地：
0. **前置（见 §10.0）**：M2-SCALE-00（populate 消费 parallel_groups）+ ADV-07（headless 默认 TODO 决策）必须先于本段。
1. **populate 改消费 `parallel_groups`** → 已提为 **M2-SCALE-00**（§10.0）；PR 拆分见 §10.0.1.8（REFAC-06 单独，SCALE-00 + VER-04① 合一）。
2. **run 并发编排**（=M2-SCALE-01）：编排器用 Agent tool 一次发多个 SubAgent。
3. **共享写隔离**（=M2-SCALE-02）：**已决 = worktree 隔离每模块 + 组末 toml 程序化合并**（§10.0；竞争点 `Cargo.toml`/`migration-state.json`/`rust_root/`）；成本缓解见 §10.0。
4. **全局锁改造**：现锁「单 /migrate 命令」粒度（SKILL.md），并发会误判 → 改编排器持锁、SubAgent 不各自取。
5. **状态机 sprint 推进**：sprint N 全终态 → current=N+1，回填 `SprintEntry.target_modules`。
> 工作量 **L**（跨 CLI + Plugin + 状态机三层）。

**B. M2-TIER-01 复杂度自适应循环【新增，PLAN 原无】**
完整 11 步对无逻辑文件浪费 token。分 trivial / standard / full 三档。
- **前置 = M2-TIER-00**（已提为独立任务，见 §10.0；CLI 落库范式同 M1-PLAN-01）：per-module 语义标注落 `ModuleState`。现状难度信息几乎为零——`ModuleState.risk` 恒 Low（lib.rs:962 唯一填充点硬编码）；项目级 `Complexity`（detect.rs）纯按 LOC 是**反面教材**；analyzer 的 complexity 仅分布计数、未落库且约定不写库（analyzer.md:19）。**TIER-01 不得在 TIER-00 完成前开工。**
- **判据原则**（写进 analyzer.md）：判据是**语义特征非 LOC**；任一危险信号或 unknown → 不降档（默认 full 兜底）；**短 ≠ 低风险**（`utils.clamp` 几行但含 NaN 数值陷阱，对抗审查恰好抓住）；判据基于本文件 AST 可见信号，不依赖跨文件 calls（recall ~70%）。
- **分档**：trivial（纯类型 / 常量 / barrel）批量直翻 + 编译 + 签批、跳 Phase B；standard 保留意图 + Phase A + 审查 + 测试；full 全跑。**维度 9 意图一致性永不跳过**；降档可观测 + 失败自动升档重跑。
- **测试策略也分档**（M1 实跑暴露：占位/trivial 模块被「每模块 ≥1 测试」硬要求逼出重复/trivial 测试，如占位链的 `*_always_none`、barrel 的 `reexports_visible`）：trivial 档只验**编译 + 导出可见性**，不强凑业务断言；**占位模块**（依赖未实现的 stub）验「占位契约」一次即可，不对每个占位函数重复 None 断言；standard/full 才做语义等价测试。把「≥1 测试」从无条件硬门改为**按档分级**。
> 工作量 **M**（纯增量，不碰锁 / 并发）。
> **附属**：porting-template.md 中 M1 推迟的 9 条翻译规则（RULE-4 所有权 / RULE-5 引用 / RULE-6 异步 / RULE-10 标准库 / RULE-11 禁止模式 / RULE-12 unsafe / RULE-15 全局状态 / RULE-17 测试 / RULE-19 惯用法）随 standard/full 分档启用，不独立建任务。

### Sprint 5：验证管线增强

| 任务 | 内容 | 验收 |
|------|------|------|
| M2-VER-01 | proptest 属性测试：图操作不变量 | 1000 次 fuzz 无 panic |
| M2-VER-02 | cargo-fuzz：解析器健壮性（**nightly job，不作 CI 门**，见 §10.0） | nightly 24h 无 crash |
| M2-VER-03 | 行为录制框架：TS 运行时行为 → 测试用例 | 可自动生成 Rust 测试 |
| M2-VER-04 | populate 数据卫生 + 契约加固（PR #10 审查遗留；**①与 M2-SCALE-00 同 PR**，见 §10.0） | ① populate 前清理不在新 sequence 的孤儿 pending 条目 ② docstring "整体重填"改为"按 NodeId 合并" ③ record-subagent-call 无 init 错误路径 e2e |
| M2-VER-05 | record-subagent-call ISO8601 时间戳校验 | Timestamp 非法格式返回明确错误而非透传 |

### Sprint 5.5：类型安全与代码质量重构（M1 审查遗留）

> 来源：M1 Phase 1 PR 审查中类型设计和错误处理 agent 的建议，非阻塞但影响 M2 可维护性。

| 任务 | 内容 | 验收 |
|------|------|------|
| M2-REFAC-01 | SourceNode 构造器：添加 `SourceNode::new()` + 将字段改为 `pub(crate)`，防止外部构造非法组合 | 所有构造点经过构造器；`rust_kind`/`rust_path`/`crate_name` 仅 RustTarget 节点可设置 |
| M2-REFAC-02 | ImportInfo 枚举化：将 `is_type_only`/`is_side_effect`/`is_dynamic` 三布尔替换为 `ImportKind` enum；`ImportedSymbol` 的 `is_default`/`is_namespace` 替换为 `SymbolKind` enum | 消除 5 种非法布尔组合 |
| M2-REFAC-03 | `sub_kind` 类型化：将 `Dependency.sub_kind: Option<String>` 替换为 `EdgeSubKind` enum（`Implements`/`Constructor`/`TypeOnly`） | 消除散布在 build.rs 中的字符串字面量 |
| M2-REFAC-04 | `migration_status`/`rust_kind` 类型化：替换 `Option<String>` 为对应 enum | 编译期类型安全 |
| M2-REFAC-05 | `parse_node_extra` 错误可见化：JSON 解析失败时记录 warning 而非静默默认值 | 数据库加载时畸形 extra 字段不再静默丢失 |
| M2-REFAC-06 | MigrationSequence 字段私有化：`order`/`parallel_groups`/`cycles` 改为 private + getter | 防止构造后意外修改 |
| M2-REFAC-07 | MigrationStateMachine.load() 后置校验：加载后验证 state 与 history 末条一致 | 手工编辑的 state.json 不会导致静默不一致 |
| M2-REFAC-08 | ~~转换表 can_transition_to~~ **主体已完成**（state.rs:98-124，done 已硬终态、blocked 仅 blockable 活跃态——决策 #1/#2 代码层已落地）。剩余仅补契约测试 + 设计文档同步（删行 206/379 旧矛盾表述）。工作量 **S** | 已有转换矩阵测试覆盖 reopen 拒绝路径；设计文档无残留矛盾 |
| M2-REFAC-08a | done 返工 = **独立 `state reopen` 命令**（非扩转换矩阵——`Done => false` 须保持）：reopen → translating，reason 强制非空，内部追加 audit attempt + 记 history | done 经 reopen 回活跃态有 attempt+history；`transition --to` 直接转出 done 仍被拒 |
| M2-REFAC-08b | `transition --to blocked` 增 `--blocked-by <module,...>` 参数（现 CLI 无入口、machine.rs 进 blocked 只写 pre_blocked_status 不写 blocked_by）；validate 校验引用模块存在。**SCALE 前置** | blocked 写入 blocked_by、恢复清空、引用不存在报错 e2e |
| M2-REFAC-09 | Arrow function 提取：walk_ast 增加 lexical_declaration 处理 | `export const f = () => {}` 生成 Function 节点 |
| M2-REFAC-10 | 跨文件方法调用解析：需 import→class→method 关联 | `service.doWork()` 正确解析到目标 Function 节点 |
| M2-REFAC-11 | fixup_extends 名称索引：HashMap 替代 O(N) linear find | 同名歧义 emit warning，大型项目不退化 |
| M2-REFAC-12 | walk_ast class 递归：extract_class 内处理 dynamic import 等嵌套模式 | 类方法内 `import('./lazy')` 被正确捕获 |
| M2-REFAC-13 | ToolStatus 枚举化 + LocReport 派生 totals | ToolStatus `Option<String>` → enum；`LocReport` 自动 `totals()` 方法 |
| M2-REFAC-14 | ErrorData structured context | `ErrorData.details: Option<Value>` 承载环路径等复杂上下文 |
| M2-REFAC-15 | module key 人类友好归一化 | `file:src/utils.ts` → `utils` 可选显示映射（CLI `--human` + state 存原 key） |

> **M1 遗留设计决策 → 已于 2026-06-15 全部定夺（见 §10.0）**：
> - `done + --force` 重做 → 硬终态 + 显式 reopen（产出 REFAC-08a）
> - blocked 进入 → 仅 blockable 活跃态（并入 REFAC-08）
> - per-module risk 评估 → analyzer 增产 + CLI 落库（产出 M2-TIER-00）

> **M2-REFAC-10 实现指引**（2026-06-14 调研补充，符号级 Calls 精度提升）：保 **precision=1.0 优先**，分档做：
> - **现实天花板**：方法调用 recall ~70% 封顶（PyCG, ICSE 2021, arxiv 2103.00587：P 99.2% / R 69.9%）——纯 tree-sitter 做不到 import 拓扑的 ~1.0，符号级 Calls 永远是**辅助信号**，别期望硬门级精度。
> - **档 1（低成本先做）**：`obj.method()` 仅当全库该方法名**唯一**时连边 + `new Foo(); x.bar()` 局部 receiver 绑定。白赚 recall，**不破 precision**（不做模糊匹配）。
> - **档 2（中期，每语言一个 extractor）**：GitNexus 式轻量 receiver 类型环境——per-file 收集「变量→类型」（Tier0 显式注解 + Tier1 构造器推断 + this/self），call site 按 receiver 类型过滤候选。蓝图见 `~/workspace/explore/GitNexus/type-resolution-system.md`；不依赖 tsconfig、天然多语言、与软门「宁漏不错」哲学一致。
> - **评测**：ts-morph 固定为 CI oracle（已有 `tools/graph-validation`），量化每次改动的 recall 提升、守 precision 不回退；**不进运行时**（避免绑 tsconfig + 锁死 TS）。
> - ⚠️ **避坑**：GitHub `stack-graphs` 已于 2025-09 归档，且只解 name binding 不解类型化方法分派，勿引入。

### Sprint 6：高级功能

| 任务 | 内容 | 验收 |
|------|------|------|
| M2-ADV-01 | 多候选生成：同一模块 ≥2 种翻译方案 | verifier 可比对选优 |
| M2-ADV-02 | 降级 FFI：无法纯 Rust 翻译时生成 FFI binding | FFI 调用正确 |
| M2-ADV-03 | graduate 命令：模块毕业（锁定+移除 TS 源） | 状态机正确转换 |
| M2-ADV-04 | `graph build --profile` 性能画像 JSON（代码 TODO(M2) lib.rs:578） | 输出 per-phase 耗时 + 节点/边计数 JSON |
| M2-ADV-05 | `graph interfaces --deps-of` 批量输出（代码 TODO(M2) lib.rs:718） | target 的 1-hop 邻居接口批量导出 |
| M2-ADV-06 | `stats compare` 结构对比（代码 TODO(M2) lib.rs:1143） | tokei + tree-sitter 函数计数做源/Rust 结构比对 |
| M2-ADV-07 | 默认 TODO 决策策略（headless 模式）。**已提前为 M2-SCALE-01 硬前置**（§10.0.1.9），不再排在 Sprint 6 末 | headless 撞 TODO(port) 时按 safe-default 自动决策，不卡 |
| M2-ADV-08 | profile 自动定位 analysis-tools.json | CLAUDE_PLUGIN_ROOT env 约定 + SKILL 接线，去掉 `--adapter-tools` 手工传参 |
| M2-ADV-09 | 完整子进程超时 | 当前仅 stdin(null)，需 tokio timeout 或 nix kill |
| M2-ADV-10 | `[persistence]` 配置段实装 | backup_on_write / retention_days 配置生效 |

### Sprint 7：并行与规模

| 任务 | 内容 | 验收 |
|------|------|------|
| M2-SCALE-00 | populate 消费 `parallel_groups`（组索引→sprint 号）；REFAC-06 为前置 PR、与 VER-04① 合一 PR（§10.0.1.8） | diamond fixture populate 后 sprint 号反映 parallel_groups 分层、非全 1 |
| M2-SCALE-01 | Workflow 批量翻译：多模块并行迁移（**硬前置：REFAC-08b + ADV-07 + SCALE-02/02a**，§10.0.1.9） | 5 模块并行迁移；`migration-state.json` 无 lost update；各模块独立通过编译+测试 |
| M2-SCALE-02 | worktree 隔离 + 组末 merge：① state delta 主 worktree 单写合并 ② `Cargo.toml` 程序化合并 ③ 共享 `CARGO_TARGET_DIR` ④ worktree 数=当前并发组大小、组末回收（§10.0.1.6） | 两模块并发完成，主 state 保留双方 attempts/subagent_calls 无覆盖 |
| M2-SCALE-02a | toml merge 纯函数 spike（引 `toml_edit`/`semver`）：deps 取并集、SemVer 取高、不兼容报错（SCALE-02 前置，§10.0.1.7） | fixture 双 `Cargo.toml` 合并保 dependency table；SemVer 冲突可复现报错 |
| M2-SCALE-03 | 增量图更新：仅重建变更文件的子图（含 lib.rs:577 file_fingerprints 跳过未变更文件） | 增量 < 全量时间 50% |

### Sprint 8：M2 验收

**M2 验收标准**：
- [ ] 3 个 5K-20K 行 TS 项目，每项目 ≥3 模块迁移完成
- [ ] 含 ≥1 个有循环依赖的模块（降级 FFI）
- [ ] proptest 通过（fuzz 走 nightly，不阻塞验收，见 §10.0）
- [ ] 图构建 < 10s（500 文件）
- [ ] 全流程 < 60min（含多模块，**按并行重估并扣 worktree 创建/merge 开销**，见 §10.0）

---

## §11 M3-M4 方向级规划

### M3：多语言支持（Python 优先）

| 方向 | 内容 |
|------|------|
| Python 适配器 | tree-sitter-python + 语言 trait 实现 |
| PyO3 翻译规则 | Python → Rust（PyO3 binding）模式库 |
| 统一差异测试 | Python runtime 行为录制 → Rust 测试 |
| 验收 | 1 个 <3K 行 Python 项目 ≥1 模块迁移 |

### M4：生态完善

| 方向 | 内容 |
|------|------|
| C/C++ 适配器 | tree-sitter-c + unsafe Rust 翻译 |
| Go 适配器 | tree-sitter-go + 并发模型映射 |
| Kani 形式验证 | 关键路径数学等价性证明 |
| 社区生态 | 文档 + 示例 + Plugin marketplace 发布 |

---

## §12 M1 预留约束（面向未来的设计决策）

以下约束在 M1 实现时必须遵守，为后续里程碑留出扩展空间：

| 约束 | 原因 | 实现方式 |
|------|------|---------|
| Language trait 可扩展 | M3 需要 Python/C 适配器 | 定义 `trait LanguageAdapter { fn detect(); fn extract_deps(); fn extract_types(); }` |
| schema_versions 表 | M2 需要 schema 升级 | SQLite 建表时包含 `schema_versions(version, applied_at)` |
| StableGraph | M2 增量更新需要稳定 NodeIndex | `petgraph::stable_graph::StableGraph` 而非 `Graph` |
| 配置文件向后兼容 | 所有版本需可读旧配置 | `config.rs` 中 `#[serde(default)]` + 版本字段 |
| Plugin JSON 通信 | CLI 与 Plugin 解耦 | 所有 CLI 输出为 JSON，不依赖 Plugin 存在 |

---

## §13 质量门模板（4 层）

每个任务完成时需通过对应层级的质量门：

### Level 1：代码级

- [ ] `cargo clippy -D warnings` 零 warning
- [ ] `cargo fmt --check` 通过
- [ ] 无 `.unwrap()`（测试代码除外）
- [ ] 每个 pub 函数有负例测试（输入非法时返回 Error）
- [ ] 无 `todo!()` / `unimplemented!()`（M1 交付时）

### Level 2：行为级

- [ ] ground-truth.json 偏序约束 100% 满足
- [ ] 所有命令输出符合 JSON 格式：`{"status":"ok|error", "data":{...}, "warnings":[...]}`
- [ ] 错误信息包含上下文（哪个文件、哪行、什么操作）

### Level 3：集成级

- [ ] 下游可消费：Plugin 能正确解析 CLI 的 JSON 输出
- [ ] `just ci` 全部通过
- [ ] Thin E2E 在所有 fixture 上通过

### Level 4：审查级

- [ ] 实现与 `docs/design/` 对应章节一致（接口/命名/行为）
- [ ] 无逻辑错误（状态机不可能到达的状态、未处理的边界）
- [ ] 模块边界清晰（无循环依赖、职责单一）

---

## §14 知识沉淀规范

### 四层知识体系

| 层 | 载体 | 内容 | 时机 |
|----|------|------|------|
| 代码自解释 | 命名 + 类型签名 | 函数做什么、参数/返回值含义 | 编码时 |
| 测试即文档 | `#[test]` + fixture | 预期行为 + 边界条件 | 编码时 |
| 模块注释 | `mod.rs` 顶部 `//!` | 模块职责 + 设计决策 + 依赖关系 | 模块完成时 |
| 决策记录 | `docs/decisions/NNN-*.md` | 重要选择的 why + tradeoff + 替代方案 | 关键决策时 |

### 规范要求

- 知识沉淀**非阻塞**：不因缺少文档而阻塞合并
- 但 reviewer 会检查：关键模块无 `mod.rs` 注释时标记 TODO
- `docs/decisions/` 格式：标题 + 背景 + 决策 + 理由 + 替代方案 + 后果
- 编号连续：`001-plugin-viability.md`, `002-parser-choice.md`, ...

---

## §15 风险与缓冲

### 已知风险表

| # | 风险 | 概率 | 影响 | 缓解措施 | 触发条件 |
|---|------|------|------|---------|---------|
| R1 | Plugin API 不如预期 | 中 | 高（全局影响） | Spike S0 验证；PlanB: 纯 CLI | S0 Fail |
| R2 | SubAgent 编排不可靠 | 中 | 中（Phase 3+） | Plan B3 混合编排；降低自动化比例 | 成功率 < 60% |
| R3 | tree-sitter TS 精度不够 | 低 | 中（graph 质量） | Spike S3 验证；PlanB: tsc API | S3 F1 < 0.90 |
| R4 | 真实项目复杂度超预期 | 中 | 中（验收延迟） | 先选简单项目；MVP 标准已降低 | 3 项目全失败 |
| R5 | 上下文窗口溢出 | 高 | 低（已有对策） | 拆分策略 + 续接协议 | 单次会话无法完成一个模块 |
| R6 | 并行开发合并冲突 | 低 | 低（已有机制） | 文件所有权表 + 合约变更协议 | Phase 1 |

### 时间线估算

```
Sprint 0 (M0)     ▓▓░░░░░░░░░░░░░░░░░░░░░░  2-3 天
Phase 0 (合约)    ░░▓░░░░░░░░░░░░░░░░░░░░░░  1 天
Phase 1 (并行)    ░░░▓▓▓▓▓░░░░░░░░░░░░░░░░░  5-7 天
Phase 2 (集成)    ░░░░░░░░▓▓░░░░░░░░░░░░░░░  2-3 天
Phase 3 (Plugin)  ░░░░░░░░░░▓▓▓░░░░░░░░░░░░  3-5 天
Phase 4 (翻译)    ░░░░░░░░░░░░░▓▓▓▓▓░░░░░░░  5-7 天
缓冲              ░░░░░░░░░░░░░░░░░░▓▓░░░░░░  ~2 周
                  ├── M1 约 4-5 周 ──┤缓冲├
                  ├────── 总计 ~6-7 周到 M1 ──────┤

M2 (质量)         ░░░░░░░░░░░░░░░░░░░░▓▓▓▓░░  4-5 周
                  ├────── 总计 ~10-12 周到 M2 ─────────┤
```

### 关键路径

```
Sprint 0 → Phase 0 → Phase 1(Worker A: graph) → Phase 2 → Phase 4
                              ↑ 关键路径                    ↑ 关键路径
```

**关键路径说明**：
- `graph` 模块是 Phase 2 集成的前置依赖（Thin E2E 需要 graph build + topo）
- Phase 4 翻译循环依赖 Phase 2 集成完成
- Phase 3（Plugin）与 Phase 2 可部分并行（Plugin 不依赖 main.rs 路由）

### 缓冲使用规则

- 缓冲时间仅用于：风险触发的 PlanB 实施 / 意外技术障碍 / 验收不通过返工
- 不用于：新功能 / 范围蔓延 / 完美主义优化
- 若 Phase 1 超期 > 3 天，触发范围缩减（砍 stats 模块，后移到 Phase 3）
