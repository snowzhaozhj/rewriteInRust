> [返回主索引](./README.md)

# 十、Claude Code 插件结构

## 10.0 Plugin 概览

本项目从第一天起设计为 Claude Code Plugin，遵循标准 Plugin 打包格式。

### .claude-plugin/plugin.json

```json
// plugin/.claude-plugin/plugin.json
{
  "name": "rust-migrate",
  "version": "0.1.0",
  "description": "Rust 迁移验证工作台 — AI 辅助的代码迁移 + 验证管线",
  "author": { "name": "rewriteInRust" }
}
```

> **注意**：`skills/`、`agents/`、`hooks/hooks.json` 通过 Plugin 目录约定自动发现，**不在 plugin.json 中显式声明**（`agents`/`commands` 等路径字段是"替换默认"语义，误声明反而绕过默认扫描）。`author` 必须是对象（`{name, ...}`），非字符串——官方校验器强制。Plugin 不支持 `rules/` 目录分发（见 10.1.1 规则分层策略）。

### Monorepo 布局与安装（.claude-plugin/marketplace.json）

本仓库是 monorepo（`cli/` Rust + `plugin/` 插件 + `docs/`），插件本体位于 `plugin/` 子目录。要让用户经标准流程安装，须在**仓库根**放 `.claude-plugin/marketplace.json`，用 `source` 指向插件子目录：

```json
// .claude-plugin/marketplace.json（仓库根）
{
  "name": "rust-migrate-catalog",
  "description": "Rust 迁移验证工作台插件市场",
  "owner": { "name": "snowzhaozhj" },
  "plugins": [
    { "name": "rust-migrate", "source": "./plugin", "description": "..." }
  ]
}
```

- `source` 相对 marketplace root（即 `.claude-plugin/` 所在目录=仓库根），不能含 `..`。
- 用户安装：`/plugin marketplace add snowzhaozhj/rewriteInRust` → `/plugin install rust-migrate@rust-migrate-catalog`。
- 本地开发免 marketplace：`claude --plugin-dir ./plugin`（限单次会话）。
- 校验（提交前必跑）：`claude plugin validate .`（marketplace）+ `claude plugin validate ./plugin`（插件本体含 skill/agent/hook frontmatter）。

### Plugin 目录结构（方案 D 混合策略）

```
rust-migrate-plugin/
├── .claude-plugin/plugin.json        # Plugin 元数据
├── bin/                              # 预编译 CLI 二进制
│   └── rustmigrate-{os}-{arch}      # 如 rustmigrate-darwin-arm64
├── skills/
│   └── migrate/
│       ├── SKILL.md                  # 主入口（路由 + 通用约定）
│       ├── analyze.md                # /migrate analyze 子命令指令
│       ├── run.md                    # /migrate run 子命令指令
│       ├── review.md                 # /migrate review 子命令指令
│       ├── graduate.md               # /migrate graduate（非 MVP）
│       ├── adapters/                 # 语言适配器
│       │   └── typescript/
│       └── references/               # 参考指南（按需 Read）
│           ├── patterns/
│           │   ├── async-to-tokio.md
│           │   └── express-to-axum.md
│           └── anti-patterns/
│               └── naive-mutex-wrap.md
├── agents/                           # SubAgent 定义（内嵌核心规则）
│   ├── analyzer.md                   # 含核心分析规则
│   ├── translator.md                 # 含核心翻译规则（类型映射、命名约定等）
│   ├── verifier.md                   # 含核心验证规则
│   └── scaffolder.md                 # 含核心测试规则
├── hooks/
│   ├── hooks.json                    # Hook 配置
│   └── scripts/
│       ├── fmt.sh                    # cargo fmt（PostToolUse，写入 .rs 后自动格式化）
│       ├── verify.sh                 # F2 验证脚本
│       ├── full-verify.sh            # F3 完整验证脚本
│       └── file-guard.sh             # 文件保护（PreToolUse）
├── scripts/
│   └── ensure-cli.sh                 # 检测/选择正确的预编译二进制
└── README.md
```

> **规则分发策略**：Plugin 不支持 `rules/` 目录分发。核心规则（短、高频、必须遵守）内嵌在 `agents/*.md` 中随 Agent 启动加载；参考指南（长、按需、条件触发）放在 `skills/migrate/references/` 下由 SKILL.md 条件 Read；项目专有规则由 `/migrate analyze` 生成到 `.rust-migration/porting/` 目录，由 SKILL.md 显式 Read。

---

## 10.0.1 CLI 工具架构（`rustmigrate`）

Plugin 中的确定性计算由独立的 Rust CLI 工具 `rustmigrate` 承担，AI 判断留给 Plugin（SubAgent + SKILL.md）。

### CLI vs Plugin 边界

| 维度 | CLI（`rustmigrate`） | Plugin（SubAgent / SKILL.md） |
|------|---------------------|-------------------------------|
| 计算类型 | 确定性：AST 解析、图遍历、代码统计、状态校验 | 非确定性：翻译策略、代码生成、等价性判断 |
| 可测试性 | 单元测试 + 集成测试覆盖 | 需要人工或 LLM-as-judge 评估 |
| 输出格式 | 结构化 JSON `{status, data, warnings}` | 自然语言 + 文件产出物 |
| 执行速度 | 毫秒级 | 秒~分钟级（LLM 调用） |
| 典型操作 | `graph build`、`stats loc`、`validate state` | `/migrate run`（翻译）、对抗性审查 |

### CLI 命令概览

> 各命令 `data` 字段的完整 Schema 随 M1 CLI 实现落地，纳入 insta 快照回归测试（见 [08 § CLI 测试](./08-roadmap-and-reference.md)）。

**MVP（M1）— 14 个命令**：

| 子命令 | 说明 |
|--------|------|
| `rustmigrate init` | 初始化 `.rust-migration/` 目录 + 项目根目录 `.rustmigrate.toml` 配置文件 |
| `rustmigrate profile` | 分析源码项目画像（语言检测、框架识别、代码统计、外部工具可用性检测——按 `analysis-tools.json` 逐项验证安装与最低版本，缺失时输出 `ADAPTER_TOOL_MISSING` 警告；同时检测 Tier 0 Rust 外部二进制 `cargo-nextest` 可用性，缺失时输出 `RUST_TOOL_MISSING` 警告） |
| `rustmigrate graph build` | 使用 tree-sitter 解析源码，构建源码图（存储到 `source-graph.db`）；默认增量（检测 file_fingerprints 跳过未变更文件），`--full` 强制全量重建并重置 graph_integrity 为 full（用于熔断恢复，见 [04 § 5.7.5-5.7.6](./04-toolchain.md#575-图完整性与熔断)）；`--profile` 输出性能画像 JSON（见 [04 § 5.7.4.1](./04-toolchain.md#5741-性能基准与扩展性)） |
| `rustmigrate graph topo-sort` | 对依赖图执行拓扑排序，输出迁移顺序（纯排序原语，Kahn 算法，不支持有环图；检测到环则返回 E002 非零退出并列出环路径。**破环不在此命令**：源码环由 `state populate-modules` 缩点折叠为 composite 模块组，见 [04 § 5.7.6](./04-toolchain.md#576-图查询能力清单)、[MDR-004](../decisions/004-scc-fold-break-cycle.md)。完整 SCC 检测见 M2 `graph cycles`。`--reverse`：逆序输出（依赖在前），纯排序变体不破环，见 [MDR-012](../decisions/012-m3-debt-batch-a-deviations.md)） |
| `rustmigrate graph parallel-groups` | 输出可并行迁移的层：按 `SccGroup.sprint` 聚合 `migration_sequence()`，同 sprint 号的组拓扑独立可并行派发翻译（ORCH-01 编排器读此分层）。与 `topo-sort` 相反，**有环不报错**——环已折叠为 SCC 组（`is_cycle=true`），输出 `{layer_count, group_count, layers[{sprint, groups[{group_key, members, is_cycle}]}]}`。见 [MDR-018](../decisions/018-keep-parallel-migration.md) |
| `rustmigrate graph deps <module>` | 查询模块的正向依赖树 |
| `rustmigrate graph interfaces <module>` | 输出模块的导出接口签名文本（查询 source-graph.db 中 `is_exported=true` 节点，按 `line_range` 从 source-ref/ 提取）；`--deps-of <target>` 批量输出 target 的直接依赖模块（imports 边的 1-hop 邻居）的导出接口签名（区别于 `graph deps` 的 BFS 传递闭包）；含每条签名的 token 估算（bytes/4） |
| `rustmigrate graph stats` | 图统计信息（节点/边计数、度分布） |
| `rustmigrate graph decompose` | 拆解 dry-run（M3-DEC-01）：在 SCC 缩点 DAG 上做凸性拓扑 first-fit 装箱，把机械小文件按 footprint 预算（`--budget`，token≈bytes/4）合批，输出拆解计划 + §8 验收四维度报告（目标缩减 / 正确性不变量 / 内聚 MQ vs 随机基线 / 机械·危险分类分布）。**只读：不写 state、不产 active 合批组、不派翻译**，供 DEC-GATE 判定（方案权威 [decomposition-redesign.md](../decomposition-redesign.md)） |
| `rustmigrate validate state` | 校验 `migration-state.json` 的合法性（JSON Schema + 状态机约束） |
| `rustmigrate validate rules` | 校验各适配器 `porting-template.md` 的 `rule_version` 与权威规则清单（`references/rule-registry.json`）一致性（陈旧检测，M4-GOV-01）：`--registry` 指清单、`--adapters-dir` 扫 `<lang>/porting-template.md`；缺失/版本不符/未知规则逐条落 `data.checks[].issues`。严重度由 `[rules].enforce_rule_version_consistency`（默认 true）控制——为真时不一致返回错误（退出码 1、非静默），为假时降级 warning。详见 [MDR-014](../decisions/014-rule-version-registry.md) |
| `rustmigrate state get` | 查询指定模块的当前迁移状态 |
| `rustmigrate state transition` | 执行状态转换（带状态机合法性前置条件检查：校验当前状态→目标状态为合法转换路径）。`--module` 为模块级 ModuleStatus 转换；省略则为项目级 ProjectState 转换（`/migrate analyze` 把 state 从 `init` 推进到 `sprint_loop` 的接入点） |
| `rustmigrate state reset` | 幂等回退失败/中途模块到干净重译入口（M4-ROB-01a）：status→`translating`、清全部进度字段（substatus/phase_a_version/audit/通过率/coverage/known_differences/blocked 锚点），保留 `attempts`（追加 reset 审计）与结构冻结字段（tier/member_files/composite_kind/decomposition_*/danger）；已在干净入口时幂等空操作（`was_noop`，`cleanup.skip=true`）；`done`/`blocked`/`paused`/`degrade_*` 须 `--force`，项目 `graduate` 态一律拒绝。非 noop 输出 `cleanup.member_files` 源作用域驱动编排器删部分 `.rs` 产物（CLI 不猜路径删文件，见 [MDR-015](../decisions/015-reset-idempotent-retry-boundary.md)） |
| `rustmigrate state recover` | 从 watchdog stall（agent stdout 静默卡死）确定性、幂等恢复模块（M4-ROB-01b）。`--policy retry`：复用 reset 语义回退干净重译入口，输出 `recovery.member_files` 产物作用域；`--policy skip`：置 `paused` 决策点（headless 自动 `degrade_skip`），输出 `recovery.advice` + 无依赖模块推进指引。`--reason` 记入 `attempts` 审计。幂等（retry 已净 / skip 已 paused|degrade → `was_noop`）；`done`/`blocked`（非 stall 态）拒绝、项目 `graduate` 态拒绝。检测归编排器（CLI 不观测子进程 stdout），恢复归 CLI（见 [MDR-016](../decisions/016-watchdog-stall-recovery-boundary.md)） |
| `rustmigrate state resume` | 额度耗尽/中断后的**续跑断点计划**（M4-ROB-01c）——纯查询、无副作用、不加载 graph。读已 checkpoint 的 `migration-state.json`，按状态归桶：运行态（translating/compile_fixing/testing/reviewing）→ `interrupted`（各带 `recover_command`，编排器逐个 `state recover --policy retry` 幂等重入）；`paused` → `awaiting_decision`（待人类/降级决策，不复活）；`pending` → `next`（用 `state deps <M>` 判就绪）；`blocked` → `blocked`；终态（done/degrade_*）**不重跑**，仅计入 `progress`（done/degraded/in_progress/pending/blocked/awaiting_decision/total）。额度检测归编排器/harness（CLI 观测不到 token 预算），实际重入复用 `state recover`（见 [MDR-017](../decisions/017-quota-pause-resume-boundary.md)） |
| `rustmigrate state populate-modules` | 用源码图迁移序列填充 `migration-state.json` 的 `modules`/`sprint`（PLAN 操作）：读 `source-graph.db` → `migration_sequence()` 缩点为 SCC 模块组 → 每组写 `{status:pending, sprint:<缩点 DAG 层级>, member_files:<仅多文件组>}`（module key 用组代表 NodeId 原值；**M1 另写恒为 `Low` 的死字段 `risk`，M2-TIER-01a 删除 `risk`、改填复杂度分档 `tier`**，见 [03 § 4.3.2](./03-execution-model.md#432-复杂度自适应分档tier-01m2)）+ `sprint.current=1`。**破环（MDR-004）：循环依赖不再拒绝，整组折叠为 composite 模块组（编译门禁单元；翻译粒度=单文件，见 [MDR-006](../decisions/006-scc-per-file-stub-first.md)）**。是 `/migrate analyze`→`/migrate run` 衔接的 PLAN 落盘环节（见 PLAN.md §9.5） |
| `rustmigrate stats loc` | 统计源码和 Rust 代码行数（嵌入 tokei） |
| `rustmigrate stats compare` | 源码与 Rust 结构复杂度对比（函数数量比、代码行数比、控制流嵌套层级）——复用 tokei + tree-sitter 函数计数，作为 Phase A 结构校验门禁（见 03 § 4.3 Step 4.5） |
| `rustmigrate stats quality` | 迁移质量度量（per-module `final_score` + project-wide `degrade_rate` / `behavior_coverage`），对齐 03 § 7.5 评分卡（M4-QUAL-01） |
| `rustmigrate stats community` | 社区结构偏离度诊断：Leiden 社区检测 vs 目录分区 NMI/ARI → `deviation_score`（M4-QUAL-04） |
| `rustmigrate scaffold workspace` | 生成 Cargo workspace 基础骨架（委托 `cargo init`）；dev-dependencies 与 `deny.toml` 由 scaffolder SubAgent 按项目测试需求注入 |

**M2 扩展 — 5 个命令**：

| 子命令 | 说明 | 推迟理由 |
|--------|------|---------|
| `rustmigrate graph rdeps <module>` | 反向依赖查询 | MVP 阶段 deps 正向查询够用 |
| `rustmigrate graph cycles` | 循环依赖检测 | MVP 目标是 <5K 行单项目，环少见 |
| `rustmigrate graph export` | 导出为 JSON/DOT/Mermaid | MVP 阶段 stats 输出够用 |
| `rustmigrate validate config` | 校验 `.rustmigrate.toml` | MVP 阶段 TOML 解析时隐式校验即可 |
| `rustmigrate state update` | 乐观锁状态更新（`--cas-version` 比较并写入，版本不匹配返回冲突） | MVP 单写者串行无需 CAS |

> **注（MVP 的轻量环检测）**：完整 `rustmigrate graph cycles`（SCC）推迟到 M2，但 MVP 阶段 SKILL.md `/migrate run` 的 Step 0.5（blocked 模块自动恢复）内置了一次轻量 DFS 环检测，专门防止 blocked 模块互相等待造成的死锁——检测到环即报错中止并提示用户打破循环，详见 [09-appendix-schemas.md 附录 B Step 0.5](./09-appendix-schemas.md#附录-bmvp-skill-的-skillmd-骨架)。二者范围不同：`graph cycles` 检测源码全图依赖环，Step 0.5 仅检测 blocked 子图的恢复死锁。

### Workspace 结构

参考 ast-grep / oxc 的 workspace 组织方式，采用 crate 分层：

```
cli/
├── Cargo.toml              # workspace root (rust-version = "stable-2", 如 "1.75"；遵循 Rust 社区 MSRV 惯例)
├── crates/
│   ├── core/               # 核心逻辑（分析、图、状态机、校验）
│   │   ├── src/
│   │   │   ├── graph.rs    # 图引擎：petgraph StableGraph + SQLite 持久化 + Query→Resolve→Set API
│   │   │   ├── profile.rs  # 项目画像分析（tree-sitter + tokei）
│   │   │   ├── state.rs    # 状态机管理
│   │   │   ├── scaffold.rs # workspace 骨架生成
│   │   │   └── validate.rs # 配置/状态校验（jsonschema）
│   │   └── Cargo.toml
│   └── cli/                # CLI 入口（clap）
│       ├── src/main.rs     # clap 命令路由
│       └── Cargo.toml
```

### CLI 与 Plugin 交互

SKILL.md 通过 Bash tool 调用 CLI，所有输出为统一 JSON 格式：

```json
{
  "status": "ok",           // "ok" | "error" | "warning"
  "data": { ... },          // 命令特定的结构化数据
  "warnings": ["..."]       // 非致命警告列表
}
```

调用示例（SKILL.md 中的指令）：
```
使用 Bash tool 执行：rustmigrate graph build --root ./src --format json
解析 JSON 输出中的 data.modules 字段，获取模块列表。
```

### 关键嵌入 crate 列表

| Crate | 用途 | 对应子命令 |
|-------|------|-----------|
| tree-sitter + 语言绑定 | 多语言 AST 解析 | `graph build`, `profile` |
| ast-grep-core | 代码模式搜索/重写 | `profile`（惯用法检测）、`graph build`（calls 等边的模式补充解析） |
| tokei | 代码行数统计 | `stats loc`, `stats compare` |
| syn + quote | Rust 代码生成/分析（M2：自定义 lint crate） | M2 条件引入（MVP `scaffold workspace` 用 toml_edit 生成 TOML） |
| petgraph | 依赖图数据结构（StableGraph + newtype 索引） | `graph build/topo-sort/parallel-groups/deps/rdeps/cycles` |
| rusqlite | SQLite 图持久化（FTS5 全文搜索 M2 才启用，见 [04 § 5.7.3](./04-toolchain.md#573-持久化存储)） | `graph build`（写入）, `graph export`（M2 查询） |
| jsonschema | JSON Schema 校验 | `validate state`, `validate config` |

### 分发方式

1. **Plugin `bin/` 预编译**：Plugin 的 `bin/` 目录包含主流平台的预编译二进制（`rustmigrate-darwin-arm64`、`rustmigrate-linux-x86_64` 等）。`scripts/ensure-cli.sh` 在首次调用时检测 OS/Arch 并选择正确的二进制。
2. **`cargo install rustmigrate`**：用户也可通过 crates.io 安装最新版本，覆盖 Plugin 预编译版本。
3. **优先级**：`$PATH` 中的 `rustmigrate` > Plugin `bin/` 中的预编译版本。

**跨平台分发与版本管理（R2-BS2-03）**：
- **首发平台矩阵**：`{darwin-x86_64, darwin-arm64, linux-x86_64, linux-arm64}`；Windows 及其他平台标记 best-effort / 社区贡献。命名约定 `rustmigrate-{os}-{arch}`（版本由 GitHub Release 标签隔离）。
- **二进制更新规则**：Plugin patch release（仅 CHANGELOG、无代码变更）时 CLI 二进制可复用前版；minor/major release 时**所有平台二进制全量更新**（防版本漂移）。
- **构建与一致性**：GitHub Actions 矩阵构建（`os: [ubuntu-latest, macos-latest]` × 目标三元组，arm64 用 `cross`）；`release.yml` post-build 步骤对每个二进制执行 `./binary --version` 比对 `plugin.json` version，不匹配则 fail release（与 § 10.0.2 版本一致性检查同源）。矩阵 post-build 步骤额外记录各平台二进制体积并与 `DESIGN_ASSUMPTIONS.md` Spike 0 基线比对；超 2x 则 fail release（复用 Spike 0 回退触发条件）。矩阵额外包含一个 MSRV 构建作业（`cargo build --locked` 在声明的 `rust-version` 上执行），确保不引入高版本 API。
- **ensure-cli.sh 容错**：二进制选择/下载后验证其 SHA256 与已发布 `SHA256SUMS.txt` 一致，校验失败触发 `cargo install rustmigrate` 降级并输出显式警告；检测/下载本身失败时同样降级提示用户 `cargo install rustmigrate` 或手动下载链接，告知该平台为 best-effort。M2 可选引入 cosign/Sigstore 签名强化。

> 该补充按平台矩阵定义 0.5d + GitHub Actions 矩阵脚本 1d + 版本一致性 CI 步骤 0.5d（共 2d）并入 [08-roadmap-and-reference.md](./08-roadmap-and-reference.md) 的「Plugin 打包与发版」估算，不引入新业务复杂度。

### M1 CLI 工作量估算

| 模块 | 工作量 | 说明 |
|------|--------|------|
> CLI 细化工作量估算见 [08-roadmap-and-reference.md § M1 工作量分解](./08-roadmap-and-reference.md)（合计 18-25 人天，含 crate 集成和测试）。

---

## 10.0.2 版本控制与向后兼容策略

Plugin 内部接口（`plugin.json`、`agents/*.md`、Skill 命令签名、产出物 Schema）演进时遵循以下策略，应对 Claude Code Plugin API 破坏性变更（风险见 [07-pitfalls-and-risks.md § 12.1](./07-pitfalls-and-risks.md#121-风险矩阵)）：

| 变更对象 | 版本控制 | 兼容性规则 |
|---------|---------|-----------|
| `plugin.json` `version` | 语义化版本（SemVer） | 破坏性变更升 major；新增可选能力升 minor |
| `agents/*.md` 格式 | 随 Plugin major 版本 | 不兼容的字段/格式变更须升 major，并提供旧格式迁移说明 |
| Skill 命令签名（参数/输出） | 随 Plugin 版本 | 变更需经 **deprecation 期**（至少 1 个 minor 版本保留旧签名 + 警告），再于下个 major 移除 |
| `migration-state.json` / `source-graph.db` Schema | 顶层 `schema_version` 字段 | 不兼容变更须升 `schema_version` 并提供迁移工具（`rustmigrate` 升级命令读旧版本、写新版本） |
| SKILL.md 程序步骤（流程顺序、检查点、条件） | 随 Plugin 版本 | 步骤重排/删除/语义改变（如 Step 1.5 意图门禁从交互确认改为默认跳过）须升 **major**；新增可选步骤或检查点升 minor。CHANGELOG 须列出所有破坏性步骤变更及迁移指导 |

**关键规则**：
- **新增可选参数不触发 deprecation**：Claude Code 为 Skill tool 新增 *可选* 参数时，不视为破坏性变更，仅在 CHANGELOG 记录。
- **兼容性窗口**：除 deprecation 期外，明确支持范围为「当前 + 前 2 个 major 版本」。N-2 之外的 major 版本在新 major 发布后 **180 天**进入 deprecation 通知期（CHANGELOG + Plugin README 标记进入维护阶段），**365 天**后完全下线。
- **通知机制**：不兼容变更在 CHANGELOG + Plugin README 顶部「Breaking Changes」段落显式公告；运行时检测到旧 Schema 时输出升级提示。
- **自动化兼容测试**：M0 Spike 0 验证 Plugin 在最近 2 个 Claude Code 版本上的最小骨架加载路径；该测试纳入 CI，作为 API 破坏性变更的早期信号。

### 10.0.3 进行中的迁移项目与 Plugin 版本升级路径

§ 10.0.2 定义了版本控制规则；本节补充**用户在 Sprint 进行中升级 Plugin** 时的实操路径（R1-D7-05），避免旧 `.rustmigrate.toml` 因升级失效。

| 升级类型 | 对进行中项目的影响 | 动作 |
|---------|------------------|------|
| **major** | 可能破坏 Schema / 命令签名 | **暂停当前 Sprint** → 备份 `.rust-migration/` → 运行迁移工具升 `schema_version` → 复查关键产出物可编译/可运行 → 确认后继续 |
| **minor** | 新增可选能力，行为不变 | 可选升级；升级后运行 `rustmigrate validate config` 检查 warnings |
| **patch** | 仅修复，无接口变更 | 自动升级，无需干预 |

- **`.rustmigrate.toml` 向后兼容承诺**：新增字段一律可选且**不改变现有字段行为**，旧配置无需手动调整；新增字段若有推荐值，CLI 首次检测到缺失时自动注入（写回时遵守 § 10.8 原子写入）。即使「可选但推荐」的新参数，其默认值也须保持与旧行为一致，不得隐式改变已有迁移结果。
- **升级检查清单**：① 保留旧 Plugin 备份 → ② `rustmigrate validate config`（minor/major）→ ③ 审查 warnings → ④ 手动复查关键产出物是否仍可编译/运行 → ⑤ 确认后更新。major 升级时若 CHANGELOG 列出 SKILL.md 步骤变更，用户须在 `/migrate run` 前审查 Breaking Changes，确认进行中的 Sprint 是否需手动恢复（如某检查点被删除/语义改变）。
- **升级失败诊断检查点**：升级后行为异常时按序排查——(a) `plugin.json` version 与 `rustmigrate --version` 一致性（不一致 → `PLUGIN_VERSION_MISMATCH`）；(b) `migration-state.json` `schema_version` 兼容性（不支持 → `SCHEMA_VERSION_UNSUPPORTED`）；(c) `source-graph.db` 表结构完整性。错误码以 § 10.7 为权威。
- **升级失败降级**：恢复备份 Plugin → 检查产出物是否损坏 → 必要时从最后一个良好 checkpoint（`migration-state.json` + `.backup`，见 § 10.8）回滚。

### Release 流程与产出物版本化

**版本号同步**：`plugin.json` `version` 与 `rustmigrate` CLI 二进制版本**必须一致**；任一组件的破坏性变更触发两者协调升 major。CI 加一项版本一致性检查（比对 `plugin.json` 与 CLI `--version`）。

**CHANGELOG 规范**：采用 [Keep a Changelog](https://keepachangelog.com) 格式，单文件同时覆盖 Plugin 与 CLI 改动。开发期按类别维护 `CHANGELOG.unreleased.md`（Features / Bug Fixes / Breaking Changes / Deprecated）；发版时维护者将 unreleased 段归档为「版本号 + 日期」。

**GitHub Release artifact 标准**（哪些进 Release，哪些仅存长期项目记录）：

| artifact | 进 GitHub Release | 阶段 |
|----------|:----------------:|------|
| 本版本 CHANGELOG 段落 | 是 | M1 |
| PARITY.md / KNOWN_DIFFERENCES.md 快照（见 [05 § 6.4.1](./05-documentation-system.md#641-社区透明度与异议协议)） | 是 | M1 |
| `bin/` 主流平台预编译二进制 | 是 | M1 |
| `SHA256SUMS.txt`（per-binary 校验和，`release.yml` sha256sum 步骤生成） | 是 | M1 |
| `release-meta.json`：`{version, release_date, schema_versions{migration_state_json, source_graph_db, porting_rules}, breaking_changes[{component, description, migration_guide}], upgrade_path}` | 是 | **M2**（M1 跳过） |
| 各 `sprint-N-report.json`、DESIGN_ASSUMPTIONS.md | 否（仅长期项目记录 `.rust-migration/`） | — |

**发版前检查清单**（置于 `.github/RELEASE.md`，M2 接入 CI 作为 pre-release gate）：
- [ ] `cargo-semver-checks` 检查 CLI crate 的 API 兼容性
- [ ] Plugin schema 兼容性测试（`agents/*.md` 格式、Skill 命令签名向后兼容）
- [ ] `migration-state.json` schema 版本检查（确认 M1→M2 迁移脚本可用）
- [ ] 人工复核 CHANGELOG 准确性
- [ ] MSRV 验证（`cargo build --locked` 在 `Cargo.toml` 声明的 `rust-version` 上通过）
- [ ] `skills/migrate/references/` 无 `status: needs-review` 的过期 pattern（`grep -r "status: needs-review" skills/migrate/references/`）
- [ ] `cargo doc --no-deps -D warnings` passes (no broken intra-doc links or missing docs on public items)
- [ ] `cargo audit` on rustmigrate's own Cargo.lock (zero critical/high advisories)

依赖维护：Dependabot patch-level auto-merge enabled; security advisories patched within 7 days.

---

## 10.1 Skills（用户入口）

MVP 核心命令 3 个，后续迭代 +1 个。所有命令共享 `/migrate` 命名空间前缀。

| Skill | 触发 | 功能 | MVP? |
|-------|------|------|------|
| `/migrate analyze` | 手动 | 合并原 init + plan + test：分析源码仓库、生成项目画像、生成迁移规则、搭建测试基础设施 | 是 |
| `/migrate run` | 手动 | 执行指定模块的迁移（Phase A/B 双阶段内循环） | 是 |
| `/migrate review` | 手动 | 合并原 verify + status：运行完整验证管线（F3 集成验证）+ 查看迁移进度仪表板 | 是 |
| `/migrate graduate` | 手动 | 合并原 graduate + unsafe-audit：评估毕业标准 + unsafe 分类审计 + 知识固化 | 否 |

> **设计理由**：社区共识所有 skill 共享 25,000 token 预算，SKILL.md 有 500 行上限。8 个独立 SKILL.md 总量过大，且对用户而言命令数过多。参考 OpenSpec 等成功案例仅 3 个命令，将 8 个 Skill 合并为 3+1 个。
>
> `analyze` 内部通过 SKILL.md 分步指令实现原 init→plan→test 的串行流程，用户无需记住 3 个命令的执行顺序。

> **SKILL.md 行数预算与外部引用模式**：500 行是软约束（实践中 SKILL.md 可超出，如社区先例 UA 的 SKILL.md 达约 45KB）。为控制主 SKILL.md 体量，可复用的检查点校验模式（JSON Schema 校验、文件存在性、非空校验、重试逻辑模板）抽到 `skills/migrate/lib/checkpoint-patterns.md`，SKILL.md 按模式名引用而非全文展开。**Plan B 拆分阈值**：若某命令的 SKILL.md 在补全错误处理后内容超过 800 行，对其复杂步骤（3+ 步）触发 Plan B3（混合外部脚本）。MVP 检查点采用文件存在性确定性判断（L1），M2+ 视复杂度升级为内容校验检查点（L2/L3，见 § 10.5 产出物有效性分级）。

---

## 10.1.1 规则分层策略（方案 D：混合）

> 本节为「规则分层策略（核心/参考/项目专有）」的权威定义。26 类迁移规则的完整列表见 [05-documentation-system.md § 6.2 迁移规则体系](./05-documentation-system.md#62-迁移规则体系通用--项目专有)（权威来源）。

Plugin 不支持 `rules/` 目录分发，因此采用混合策略将规则按特征分层存放：

| 规则类型 | 特征 | 存放位置 | 加载方式 |
|---------|------|---------|---------|
| 核心规则（~50行/agent） | 短、必须遵守、高频 | `agents/*.md` 内嵌 | Agent 启动即生效 |
| 参考指南（~200-500行/个） | 长、按需、条件触发 | `skills/migrate/references/` | SKILL.md 条件 Read |
| 项目专有规则 | 项目特定 | `.rust-migration/porting/` | SKILL.md 显式 Read |

**核心规则**：每个 Agent 的 `.md` 文件中直接内嵌该角色必须遵守的核心规则（如 translator.md 内嵌类型映射规则、命名约定等）。这些规则简短（每个 Agent 约 50 行），Agent 启动时自动加载，无需额外 Read 操作。

**参考指南**：较长的模式指南（如 `async-to-tokio.md`、`express-to-axum.md`）和反模式文档放在 `skills/migrate/references/` 下。SKILL.md 根据当前迁移模块的特征条件性地 Read 相关指南（如检测到 Express 路由时 Read `express-to-axum.md`）。

**项目专有规则**：由 `/migrate analyze` 分析源码后生成到 `.rust-migration/porting/` 目录（如 `dependency-mapping.md`、`business-logic-rules.md`）。这些规则与具体项目绑定，不随 Plugin 分发，由 SKILL.md 在翻译阶段显式 Read。

---

## 10.2 SubAgents（4 个专职角色）

| Agent | 职责 | 核心工具 |
|-------|------|---------|
| `analyzer` | 源码分析、项目画像、依赖图语义增强、惯用法检查 | tree-sitter, dependency-cruiser, Mypy, tokei |
| `translator` | 迁移规则生成、Phase A 忠实翻译 + Phase B 惯用化优化、多候选生成 [M2+] | LLM, ast-grep |
| `verifier` | 等价性验证、**模块级测试生成**、Phase A→B 中间的对抗性审查、不等价证据收集、性能对比 | cargo-test, proptest, criterion, Miri |
| `scaffolder` | 测试基础设施搭建、行为录制、黄金测试集管理、Cargo workspace 骨架生成 | insta, cargo-fuzz, mitmproxy |

### SubAgent 输入/输出接口表

> 每个 SubAgent 通过 `.rust-migration/` 目录下的文件通信。以下表格定义各 SubAgent 的输入/输出契约。

| SubAgent | 输入文件 | 输出文件 | 前置条件 | 后置条件 | Schema 校验规则（产出物有效性） |
|----------|---------|---------|---------|---------|------------------------------|
| **analyzer** | 源码目录、`.rustmigrate.toml` | `source-graph.db`（语义增强）、项目画像摘要（stdout JSON） | CLI `rustmigrate graph build` 已完成基础图构建 | `source-graph.db` 含 calls/uses_type 边 | 必须含 calls/uses_type 边；节点数 ≥ 5；边数 ≥ 5（L3 语义检查，M1 可人工 sampling） |
| **translator**（规则生成） | `source-graph.db`、适配器 `porting-template.md` | `porting/` 目录（迁移规则文件） | analyzer 已完成 | `porting/` 至少含一个规则文件 | **L1 存在性**：`porting/` 存在、非空、至少一个 `.md` 规则文件大小 > 0、含关键标题（Markdown 产出物无 JSON Schema，仅做 L1） |
| **translator**（意图摘要） | `porting/` 规则、目标模块源码 | `{module}-intent.md`（意图摘要） | 模块 pending 或 translating（substatus=null） | 意图摘要非空且含 7 维度（9 required 属性） | **L2**（附录 E Schema） |
| **translator**（Phase A/B） | `{module}-intent.md` + 规则 + 依赖接口 | Rust 源文件（写入 `rust_root/` 目录，由 `.rustmigrate.toml` 配置）、Cargo.toml `[dependencies]` 更新（按 `dependency-mapping.md` 映射）、`_porting_manifest.json`（写入 `.rust-migration/context/module-learnings/{module}/`） | 模块 translating 且意图已确认（Step 1.5 人类确认门禁通过） | Rust 文件存在且通过 F1 编译；manifest 至少含一条 RULE 引用 | **L1 存在性**：Rust 文件存在且 F1 编译通过；manifest 存在且非空 |
| **verifier**（对抗审查） | `.rust-migration/intermediate/attempts/{module}-phase-a.rs`（Phase A 持久化代码）、原始源码（`source-ref/`）、迁移规则 | `{module}-review.md`（审查报告） | Phase A 翻译完成并已持久化 | 审查报告包含差异列表 | **L1 存在性**：`{module}-review.md` 存在、非空且含差异列表标题（Markdown 产出物，仅做 L1） |
| **verifier**（测试验证） | Phase B Rust 产出物、黄金文件 | 测试结果 JSON（stdout）、KNOWN_DIFFERENCES.md 追加条目 | Phase B 翻译完成 | 测试通过率 ≥ 预期；TODO(port) 计数 = 0（否则标记 incomplete，见 [03 § 4.3 Step 3](./03-execution-model.md#43-内循环模块级单会话内-phase-ab-双阶段翻译)） | **L2 结构校验**：测试结果 JSON 格式合法、通过率字段非空且在 [0,1] 范围（JSON 产出物，做 L2） |
| **scaffolder** | `source-graph.db`、模块接口信息 | `test-fixtures/golden/` 测试数据、Cargo.toml dev-deps 注入、`test-fixtures/ffi-bridge/`（若检测到 purity_confidence=high 的纯函数） | analyzer 已完成 | 测试基础设施可运行；FFI bridge round-trip 对 ≥1 纯函数验证通过（若适用） | **L1 存在性**：`test-fixtures/golden/` 非空、Cargo.toml 含注入的 dev-deps（文件/配置产出物，仅做 L1） |

> **verifier 对抗审查的 Phase A 代码来源**：Phase A 翻译完成后，translator 将 Phase A 的 Rust 代码持久化到 `.rust-migration/intermediate/attempts/{module}-phase-a.rs`；verifier 对抗审查从该路径读取 Phase A 代码执行审查（输入路径完全显式，不依赖"中间态"模糊概念）。Phase A 持久化步骤与 `/migrate run` 序列的对应关系见 [03-execution-model.md § 4.3](./03-execution-model.md)。

**行动指南**：每个 SubAgent 有独立的系统提示，包含其职责边界和可用工具列表。Agent 之间通过 `migration-state.json` 和产出物文件通信。

### 10.2.1 SubAgent 实现机制

MVP 中 SubAgent 的实现基于 Claude Code 的标准 agent 定义机制：

**文件形式**：
- 每个 SubAgent 是 Plugin 的 `agents/` 目录下的一个独立 `.md` 文件（如 `analyzer.md`、`translator.md`、`verifier.md`、`scaffolder.md`）
- 每个 `.md` 文件定义该 SubAgent 的系统提示，包含职责描述、可用工具列表、行为约束和输出格式要求

**调用方式**：
- Skill 的 SKILL.md 中通过 Claude Code 的 `Agent` tool 调用 SubAgent
- 调用时指定 agent 名称（实际工具参数为 `subagent_type`；本文档用 `agentType` 指代该概念）。**Plugin 内 SubAgent 须带插件命名空间前缀 `<plugin-name>:<agent>`**——本插件 name=`rust-migrate`，故为 `rust-migrate:analyzer` / `rust-migrate:translator` / `rust-migrate:scaffolder` / `rust-migrate:verifier`（M1-PLG-05 Live 验证实测确认）。Claude Code 据此加载对应 `agents/<agent>.md` 作为系统提示
- 示例：SKILL.md 中写"使用 Agent tool 调用 analyzer SubAgent，传入项目根目录路径"

**上下文隔离**：
- 每个 SubAgent 运行在独立的 agent 上下文中，不共享对话历史
- SubAgent 之间通过文件系统（`.rust-migration/` 目录）共享数据，不直接通信
- 这保证了每个 SubAgent 的上下文窗口不被其他 SubAgent 的输出污染

**错误传播**：
- SubAgent 的输出文本返回给 Skill（即主对话上下文中的 Claude）
- Skill **不解析 SubAgent 输出文本来判断成功**，仅检查关键产出物文件是否存在（L1）且通过 Schema 校验（L2，见上方接口表「Schema 校验规则」列与 § 10.5 产出物有效性分级）
- **L1/L2/L3 适用对象界定（R2-D3-03，权威定义）**：本节与 § 10.2 接口表「Schema 校验规则」列共同构成产出物分级的权威定义（§ 10.5「产出物有效性分级」给出 L1/L2/L3 三级语义，不重复逐产出物清单）。
  - **L1（存在性）**：Markdown 产出物（`{module}-intent.md`、`porting/` 规则、`{module}-review.md`）与代码/配置产出物无 JSON Schema，只做存在、非空、关键标题存在。
  - **L2（结构校验）**：JSON 产出物（`migration-state.json`、测试结果 JSON）做格式合法、关键字段非空校验；SQLite（`source-graph.db`）做表结构校验（必要表存在、版本号合法）。`{module}-intent.md` 在 M1 例外升 L2（9 required 属性 JSON Schema 见 [09 附录 E](./09-appendix-schemas.md#附录-e意图摘要-moduleintentmd-内容规范)）。
  - **引用一致性（L2 的延后子项）**：`blocked_by` 引用的模块名须存在于 `modules` 键中——此项**不在 SubAgent 完成后立即执行**，而延后到 `/migrate run` 的 Step 0.5（彼时有完整依赖上下文，见 [09 附录 B Step 0.5](./09-appendix-schemas.md#附录-bmvp-skill-的-skillmd-骨架)），失败错误码为 `BLOCKED_BY_VALIDATION_FAILED`（见 § 10.7）。
  - **MVP 边界**：上述 L2 项中，`migration-state.json` 与测试结果 JSON、`source-graph.db`、`{module}-intent.md` 即 MVP 的 L2 范围；其余 Markdown 产出物 MVP 停留 L1，M2 视复杂度再升级。语义校验 L3 见 § 10.5，MVP 为人工 sampling。`rustmigrate validate state` 以本界定为设计输入，MVP 不要求 100% 自动化。
- 失败时 Skill 根据 SKILL.md 的分步指令决定重试或降级（见 § 10.2.2）

### 10.2.2 失败恢复机制

每次 SubAgent 调用后、后续 SubAgent 启动前，Skill 显式校验产出物并按以下三步处理失败：

1. **记录调用**：每次 SubAgent 调用（含超时或产出物校验失败）记录到 `migration-state.json` 的 `subagent_calls` 数组（字段见 § 10.5），用于诊断卡死与统计重试次数。超时阈值由 `.rustmigrate.toml` `[orchestration].subagent_timeout_secs` 控制（默认 600s）。
2. **诊断 + 重试**：校验失败时生成诊断报告（缺失字段 / 无效 JSON / 数据范围错误 / 超时），若重试次数 < `max_retries_per_step`（默认 2）且总耗时未超全局预算，提示用户选择「重试单个 SubAgent」（按 `retry_backoff_secs` 退避）。**Step 4 Phase B 双计数器规则**：SubAgent 超时/产出物校验失败（无编译输出）计入 `max_retries_per_step`（2 次）；编译失败（有输出但 cargo check 不过）计入 `max_retry_rounds`（3 轮）——两个计数器独立；任一耗尽即进入 pause→degrade 路径。
3. **降级 / 回滚**：重试耗尽后，提示用户三选项——「重试」「部分跳过」（调用降级逻辑 Plan B2，将该模块标记 `degrade_*`）「完整回滚」（**按 [09-appendix-schemas.md 附录 B「关键检查点的失败恢复规则」表](./09-appendix-schemas.md#附录-bmvp-skill-的-skillmd-骨架)清理该步产出物，状态复位到 pre-run 状态，之后可重新执行该命令**）。该表按 3 个 SubAgent 调用点（analyzer / translator 规则生成 / scaffolder）逐行给出「步序号 → 失败时保留文件 → 删除文件 → 复位到何种状态」，本节不重复列举具体文件清单。

> **中间产物始终保留（R2-D5-01）**：无论选择哪种回滚，`.rust-migration/intermediate/attempts/*.json`（翻译尝试历史）**始终保留**，作为自动回滚失败原因的诊断证据，不在清理范围内。

> **诊断报告与错误码**：校验失败的诊断输出采用 CLI 标准 error JSON（见 § 10.7），便于工具解析与重试决策。

---

## 10.3 Hooks（自动化门禁）

**关键原则（Deterministic Assertion Enforcement）**：门禁用独立脚本确保确定性——Hook 级（fmt/file-guard）自动触发不可跳过；Skill 级（verify.sh/full-verify.sh）调用依赖指令跟随，脚本内部逻辑确定性执行。

> **v0.9 变更**：F1 编译反馈改为 rust-analyzer LSP 自动诊断，删除原 PostToolUse → cargo check 的 Hook（`check.sh`）。保留 cargo fmt 格式化 Hook + 文件保护 Hook + F2/F3 验证脚本。

> **F1 诊断假设的验收依赖（R2-D3-01）**：「无需 Hook」依赖 rust-analyzer 在迁移场景下的诊断时效性与隔离性，须由 M0 Spike 2 量化验收，标准记入 `DESIGN_ASSUMPTIONS.md`（M0 产出物）的「F1 rust-analyzer 诊断验收标准」节，含一张「F1 验证场景矩阵」：(a) 在 5K / 15K / 100K LOC 三档规模上，LSP 初始化 < 3s、增量编译诊断延迟 < 5s、诊断覆盖全部 Tier 0 错误（编译/类型/borrow）；(b) Plugin Context 隔离——rust-analyzer 须运行在独立 language server 进程（不与用户全局实例共享），经 `.rust-migration/.vscode/settings.json` 的 `rust-analyzer.server.path` 与 `rust-analyzer.checkOnSave.command` 指定隔离实例；(c) 失败回退——任一规模诊断延迟 > 5s / 初始化 > 3s 或检测到实例冲突时，强制将 F1 升级为 `PostToolUse` Hook（`verify.sh` 内增 `cargo check` 一阶段），并按 [07-pitfalls-and-risks.md § 12.2](./07-pitfalls-and-risks.md#122-plan-b-体系) Plan B2 调整工作量（+1 人天）。

### Hook 脚本

```
hooks/scripts/
├── fmt.sh            # PostToolUse: cargo fmt（仅 .rs 文件，自动格式化）
├── file-guard.sh     # PreToolUse: 防止 agent 修改源项目文件或关键产出物
├── verify.sh         # F2: cargo nextest run + cargo clippy
└── full-verify.sh    # F3: 完整验证管线
```

### Hook 配置

Hook 配置遵循 Claude Code 真实 API 格式（`hooks/hooks.json`）：

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Edit|Write",
        "hooks": [
          {
            "type": "command",
            "command": "hooks/scripts/fmt.sh"
          }
        ]
      }
    ],
    "PreToolUse": [
      {
        "matcher": "Edit|Write|Bash",
        "hooks": [
          {
            "type": "command",
            "command": "hooks/scripts/file-guard.sh"
          }
        ]
      }
    ]
  }
}
```

> **fmt.sh 文件过滤说明**：Claude Code 的 `matcher` 字段匹配的是**工具名**（如 `Edit`、`Write`），不支持文件路径 glob。因此 `fmt.sh` 会在所有 Edit/Write 操作后触发。脚本内部通过 stdin JSON payload 自行过滤文件扩展名——非 `.rs` 文件直接 `exit 0`。
>
> ```bash
> #!/bin/bash
> # fmt.sh — 自动格式化（PostToolUse Hook 触发）
> INPUT=$(cat)
> FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty')
> [[ "$FILE_PATH" != *.rs ]] && exit 0
> cd "$(cargo locate-project --message-format plain 2>/dev/null | xargs dirname 2>/dev/null || echo .)"
> cargo fmt 2>&1
> ```
>
> **Hook stdin JSON 格式**：PostToolUse Hook 从 stdin 接收 JSON payload，脚本据此过滤。预期关键字段如下：
>
> ```json
> {
>   "tool_name": "Edit",                       // 触发的工具名（Edit | Write | Bash）
>   "tool_input": { "file_path": "/abs/path/to/file.rs", "...": "..." },  // 工具入参，含被修改文件路径
>   "cwd": "/abs/path/to/project"              // 调用时的工作目录（用于定位 Cargo.toml）
> }
> ```
>
> **该格式需在 M0 Spike 0 中验证**——若与实际 Claude Code Hook API 不符（字段名/层级变化），需相应调整 `jq` 提取路径。`fmt.sh` 用 `cargo locate-project` 定位最近的 `Cargo.toml`（基于脚本 cwd），多工作区场景下 cwd 由 Hook payload 的 `cwd` 字段确定。

> **file-guard.sh 说明**：PreToolUse Hook，防止 agent 修改源项目文件（`source_root` 下的文件）或关键产出物（如 `KNOWN_DIFFERENCES.md` 中已审批的条目）。脚本通过检查目标文件路径是否在保护范围内来决定是否阻止操作。
>
> **并发写入防护**：file-guard.sh 仅做路径级保护，**不能阻止对 `source-graph.db` / `migration-state.json` 的并发写入**。MVP 阶段对这两个文件的写入串行化由 § 10.5「全局并发隔离」保障：(a) 脚本对这两个文件加 `flock` 排他锁（如 `flock -n .rust-migration/source-graph.db.lock`），若锁被占用则阻止 Edit/Write/Bash 操作，防止 SubAgent 与 Skill 主上下文经 CLI 同时写入；(b) SubAgent 间遵守锁协议——analyzer 完成并释放排他锁后，translator 才获取（SKILL.md 显式约定）。SQLite WAL checkpoint 策略与回收条件见 [04-toolchain.md § 5.7.3](./04-toolchain.md#573-持久化存储)。
>
> **该锁在 MVP 的真实目的**：对 `source-graph.db` 的 `flock` 排他锁是**防御性编程**——应对「指令跟随失败导致多个 SubAgent 意外并发」的小概率情况。正常 MVP 流程（analyzer→translator→scaffolder 严格串行，序列见 § 10.5）下此锁不会被竞争。M2 引入跨模块并行（见 § 10.5）后，该锁对 `source-graph.db` 仍为防御/回退用途——**D5 定稿：M2 翻译期 `source-graph.db` 只读、编排器集中 writer，无 DB 多 writer 并发**，真实冲突面转移到 Rust 代码装配（共享 `.rs`/Cargo.toml/lib.rs），由 D3 约束包（worktree 自检 + reconcile + 整组 check 真门）治理（见 § 10.5「跨模块并行的写隔离约束包」与 [04 § 5.7.3](./04-toolchain.md#573-持久化存储)）；WAL + busy_timeout 仅未来「SubAgent 直写 db」回退模式才配合。
>
> **威胁模型澄清（R2-D3-04）**：flock 作用于进程级互斥而非内容锁，仅防止 SubAgent/Skill 同时**打开**被保护文件；单进程内的写入安全由 SQLite `BEGIN IMMEDIATE TRANSACTION` + SKILL.md 序列化保障（见 § 10.5）。此外 file-guard.sh 对 **Bash 工具的有效性依赖 Claude Code Hook stdin JSON 为 Bash 工具提供结构化 `tool_input`**——若该字段缺失或格式不符，从命令字符串可靠提取目标路径不可行，`jq -r '.tool_input.file_path'` 返回空将使 flock 退化为无效空锁。该依赖须在 M0 Spike 0 验证（与上文「Hook stdin JSON 格式」同一验证项），作为 `DESIGN_ASSUMPTIONS.md` 的**预承诺决策点 F1**（两分支均预先写定，避免实现期再议）：**(成功)** Hook 为所有 Bash 调用提供结构化 `tool_input.file_path` → 按 file-guard.sh 设计执行路径级 flock，SKILL.md 不变；**(失败)** 字段缺失/格式不符 → file-guard.sh 对 Bash 退化为 no-op，SKILL.md **必须**改行「Bash 命令白名单」强制（仅放行 `rustmigrate` CLI 子集，其余用户自撰 Bash 报错拒绝），代价约 1-2 人天 CLI/SKILL.md 改写并升 M1 人工复审 UX 影响。

**F2 和 F3 的实现方式**：
- **F2（模块完成后验证）**：通过 `hooks/scripts/verify.sh` 独立脚本执行 `cargo nextest run` + `cargo clippy --all-targets -- -D warnings`（M3-VAL-03 更新：原 `nextest run --lib` 会漏跑 `tests/` 集成测试——verifier 生成的行为等价/golden 差异 harness 即在此，导致模块可在等价从未实跑时被签批 done；clippy 加 `--all-targets` 一并 lint 测试码）；`verify.sh` 在调用 `cargo clippy` 前须设置 `CLIPPY_CONF_DIR=$(git rev-parse --show-toplevel)/.rust-migration`（固定约定路径；`$MIGRATION_ROOT` 环境变量存在时作为 override 逃生口），因 `.rust-migration/` 不是 `rust_root` 祖先目录、Clippy 默认查找无法命中（见 [04 §5.2](./04-toolchain.md#52-tier-0硬性门禁)）。若 migration-state.json 标记模块为 async/concurrent，追加 `RUSTFLAGS='--cfg loom' cargo nextest run --test loom_*`（或 shuttle 等价调用）。Skill SKILL.md 中指令调用该脚本，但脚本本身是确定性的，agent 无法修改或跳过其内部逻辑。
- **F3（Sprint Review 集成验证）**：由 `/migrate review` Skill 触发 `hooks/scripts/full-verify.sh`，执行 `cargo deny check` + `cargo audit` 等完整验证管线。

**概念事件 → Claude Code 实际实现机制映射表**：

| 概念事件 | 反馈层级 | Claude Code 实现机制 | 说明 |
|---------|---------|---------------------|------|
| 写入 .rs 文件 | F1 | **rust-analyzer LSP 自动诊断**（无需 Hook） | 编译错误自动反馈，无延迟 |
| 写入 .rs 文件 | — | `PostToolUse` Hook（`matcher: "Edit\|Write"`）→ `fmt.sh`（脚本内过滤 `.rs`） | 自动格式化 |
| 修改受保护文件 | — | `PreToolUse` Hook（`matcher: "Edit\|Write\|Bash"`）→ `file-guard.sh` | 防止误修改 |
| 模块翻译完成 | F2 | Skill 调用 `hooks/scripts/verify.sh` | 脚本确定性执行 |
| Sprint Review | F3 | `/migrate review` → `hooks/scripts/full-verify.sh` | 用户显式调用 |
| 迁移状态变更 | — | `migration-state.json` 文件写入 | 编排器自行管理 |
| 编排检查点 | — | 确定性文件存在性检查（脚本） | 检查产出物文件是否存在且有效 |

**Tier 0 覆盖确认**：Tier 0 的三个工具（cargo check / clippy / cargo-nextest）——cargo check 由 rust-analyzer LSP 自动覆盖（F1），clippy 和 cargo-nextest 由 `verify.sh`（F2）执行。`verify.sh` 内部也包含 `cargo check` 作为确定性兜底。

---

## 10.4 unsafe 分类管理

不只是"加注释"，而是分级管理：

| 优先级 | 类别 | 说明 | 处理方式 | "清理"含义 |
|--------|------|------|---------|-----------|
| P0 | 可立即消除 | 有 safe 替代方案 | 本 Sprint 内替换为 safe 代码 | **消除**——unsafe 块不再存在 |
| P1 | FFI 边界 | 调用外部 C 库必需 | 封装在最小 unsafe 块 + SAFETY 注释 + 审计确认 | **封装审计完毕**——unsafe 仍存在，但已封装在安全抽象后，且审计通过 |
| P2 | 性能关键 | safe 版本有显著开销 | benchmark 证明后保留 + Miri 测试 | 保留（有性能证据） |
| P3 | 暂无 safe 方案 | 等待 Rust 语言演进 | 标记 TODO + 定期重评估 | 保留（等待上游） |
| P4 | 历史遗留 | 迁移过程中临时引入 | 毕业前必须**重新归类**到 P0-P3 | **重新归类**——不允许以"历史遗留"状态毕业 |

**行动指南**：`/migrate graduate` 中包含 unsafe 审计功能，自动扫描所有 unsafe 块，生成分类报告，标注清理优先级。毕业标准：P0 全部消除，P1 全部封装审计完毕，P4 全部重新归类到 P0-P3。

---

## 10.5 编排调度路径

每个 Skill 的执行并非单一 SubAgent 调用，而是按预定义序列调度多个 SubAgent 协作。MVP 阶段 SubAgent **串行执行**，通过文件通信 + 顺序约束实现协调。

### Skill 内部调度序列

| Skill | 调度序列 | 关键产出物 | 说明 |
|-------|---------|-----------|------|
| `/migrate analyze` | `analyzer` → `translator`(规则生成) → `scaffolder`(测试搭建) → 写入所有初始化产出物 | migration-state.json, source-graph.db, 迁移规则(porting/), PARITY.md, AGENTS.md, test-fixtures/ | 原 init+plan+test 合并，序列最长（3 次 SubAgent 调用） |
| `/migrate run` | **前置 blocked 检查点** → `translator`(语义解构/意图摘要) → 人类确认门禁 → `translator`(Phase A 忠实翻译) → `verifier`(对抗性审查) → `translator`(Phase B 优化) → `verifier`(测试验证) → 更新状态 | {module}-intent.md, Rust 代码, 审查报告, 测试, MDR | Phase A/B 双阶段翻译 |
| `/migrate review` | `verifier`(全量验证 + pattern 新鲜度扫描，按 05 §6.12 标记 needs-review) → 生成报告 → 更新 PARITY.md + 状态仪表板输出 | sprint-N-report.json, 终端仪表板 | 原 verify+status 合并 |
| `/migrate graduate` | `verifier`(毕业评估：覆盖率 + unsafe 审计 + 性能基准) → 生成毕业报告 | graduation-report.json, unsafe-audit.json | 原 graduate+unsafe-audit 合并 |

> **注意**：`/migrate analyze` 的 7 步序列（含 3 次 SubAgent 调用）是 M0 Spike 1 验证的主要对象——如果指令跟随不够可靠，此命令应拆为子步骤（Plan B1 微 Skill 链）。

> **`/migrate run` 前置 blocked 检查点**：执行翻译序列前先遍历 migration-state.json 中所有 `blocked` 模块，对每个模块检查 `blocked_by` 引用的模块是否已进入 `done`/`degrade_*`，若是则自动恢复到 `pre_blocked_status` 并记录日志；同时校验目标模块自身依赖是否就绪，未就绪则中止并将其标记为 blocked。具体伪码与责任边界（MVP 手动 / M2 `validate state` 自动）见 [09-appendix-schemas.md 附录 B § /migrate run 骨架 Step 0.5/0.6](./09-appendix-schemas.md#附录-bmvp-skill-的-skillmd-骨架) 及 [附录 A § 合法状态转换「检测与恢复的责任边界」](./09-appendix-schemas.md#合法状态转换)。

### MVP 阶段执行模型

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
- **M0 验证要求**：在 M0 Spike 1 中验证 Claude 能否可靠执行 `/migrate analyze` 的 7 步序列（含 3 次 SubAgent 串行调用）。验收标准与失败判定规则见 [07-pitfalls-and-risks.md § 12.2 Plan B 体系表](./07-pitfalls-and-risks.md#122-plan-b-体系)（5 次独立测试，>20% 失败率自动触发 Plan B；20%-50% 临界区间追加 5+ 样本重评）。**M0 Spike 1 成功率 < 80%（或 7 步完成率 < 95% / 超时次数 > 2）时强制采用 Plan B3 混合编排，不可选**。
- **检查点确定性（两级校验）**：SubAgent 间的编排检查点采用两级确定性校验，**Skill 不解析 SubAgent 输出文本来判断成功，只看产出物文件**：
  - **L1 文件存在性检查**（脚本，毫秒级）：仅检查关键产出物文件是否存在。proceed/halt 决策只依赖此项。
  - **L2 结构校验**（CLI `rustmigrate validate state` 等，秒级）：若文件存在，对 JSON 产出物执行 JSON Schema 校验、对 SQLite 检查表结构完整性，作为二级验证。

**产出物有效性分级**：
- **执行成功**（L1）：全部 7 步完成，所有关键产出物文件存在（文件存在性检查）。
- **结构有效**（L2）：产出物通过 JSON Schema 校验、SQLite 表结构完整（自动化校验）。
- **语义有效**（L3，M1+ 阶段）：产出物内容正确性由人工样本审视确认（非自动化），包括 source-graph.db 的节点/边关系准确性。

> Spike 1 验收标准仅关注「执行成功」（L1）与「结构有效」（L2）；「语义有效」（L3）在 M1 阶段可接受人工 sampling 而非 100% 自动化。编排可靠性假设的定义模板见 M0 产出的 `DESIGN_ASSUMPTIONS.md`。

**编排超时与失败重试策略（MVP）**：

`.rustmigrate.toml` 的 `[orchestration]` 段定义编排级超时与重试参数（默认值，可覆盖；详见 § 11.1）：

```toml
[orchestration]
subagent_timeout_secs = 600       # 单步 SubAgent 调用【总】超时（10 分钟，对应 LLM API 典型超时）
max_retries_per_step = 2          # 每步最大重试次数（不含初次尝试）
retry_backoff_secs = [5, 15]      # 梯级退避：第 1 次重试前等 5s，第 2 次等 15s
stall_timeout_secs = 600          # M4-ROB-01b watchdog：后台命令 stdout【静默】超此值判 stall（与总超时正交）
stall_recovery_policy = "retry_then_skip"  # stall 恢复策略：retry_then_skip | always_retry | always_skip
```

> **stall 检测与恢复（M4-ROB-01b）**：`subagent_timeout_secs` 是单次调用的**总**预算，`stall_timeout_secs` 是 **stdout 静默**阈值——持续产出的长命令不算 stall、但静默超阈即判卡死。检测由编排器轮询 `BashOutput` 完成（CLI 观测不到子进程 stdout），检出后调 `state recover --policy <retry|skip>` 做幂等恢复；`--policy` 由 `stall_recovery_policy` + 本模块重试轮次解析。分工与理由见 [MDR-016](../decisions/016-watchdog-stall-recovery-boundary.md)。

> **检查点失败处理**：每步检查点失败（L1 文件缺失或 L2 校验不通过）时，自动重试最多 2 次（间隔 5s/15s）；2 次后报错并输出 SubAgent 调用日志（记录于 `migration-state.json` 的 `subagent_calls` 数组，每条 `{step_index, subagent_name, started_at, ended_at, status, error_message}`，Schema 见 [09-appendix-schemas.md](./09-appendix-schemas.md#附录-amigration-statejson-schema)）供排查。**各产出物适用 L1 还是 L2** 以 § 10.2 接口表「Schema 校验规则」列与 § 10.2.1「L1/L2/L3 适用对象界定」为准（本节只定义三级语义，不重复逐产出物清单）。各步的具体失败行为见 09-appendix-schemas.md 附录 B 的「Checkpoint 失败处理」表。
>
> **前置条件检查点 ≠ 步骤执行失败**：`/migrate analyze` Step 2.5 的 `metadata.graph_build_completed` 检查属于**前置条件验证**，不走上述 `max_retries_per_step` 重试——读到 `false`/缺失即判定 `graph build` 进程异常退出（事务未提交、DB 已 commit 但标志未落盘的崩溃窗口同此处理），直接报错停止，提示用户检查 graph build 日志后手动重新执行 `rustmigrate graph build`，再重跑 `/migrate analyze`（见 [09 附录 B Step 2.5](./09-appendix-schemas.md#附录-bmvp-skill-的-skillmd-骨架)）。`graph_build_completed` 用 § 10.8 原子写入（非事务耦合：DB 写锁随 CLI 进程退出释放，标志独立原子落盘）。

**全局并发隔离（MVP）**：
- MVP 阶段保证对 `.rust-migration/` 的**单一进程/Skill 实例独占访问**：同一时刻只有一个 `/migrate` 命令在运行，SubAgent 串行执行。
- `source-graph.db`（SQLite）与 `migration-state.json` 的写入串行化由两条机制保障：(a) `file-guard.sh` 对这两个文件加排他锁（`flock`），确保 SubAgent 与 Skill 主上下文（经 CLI 写入）对 `source-graph.db` 的**独占访问**（互斥层面，非内容锁，见 § 10.3 威胁模型澄清）；(b) SKILL.md 显式约定锁协议——**后序 SubAgent 启动前显式检查 `migration-state.json` 的 `metadata.graph_build_completed=true`（确定的检查点，而非"已释放"的隐式约定）**。该标志由 CLI `graph build` 在 SQLite `COMMIT` 后同步写入，定义了"完成"的精确语义（进程退出 / WAL checkpoint / 状态写入三者中以此标志为准）；判定责任在 **Skill 而非 CLI**（避免 CLI 接口爆炸）。SQLite WAL 模式的 checkpoint 策略见 [04-toolchain.md § 5.7.3](./04-toolchain.md#573-持久化存储)。
- M2 并行编排的文件通信与并发安全协议见下文「M2 扩展：并发文件通信协议」；M2 并发时的版本一致性校验由 `migration-state.json` 的 `metadata` 版本字段（见附录 A）支持。
- **多终端并发冲突的强制隔离（R2-D5-02）**：「单一实例独占」由**全局命令锁**强制执行，而非依赖约定。每个 `/migrate` 命令的 SKILL.md 在 **Step 0「全局锁获取」**（一切操作之前）通过原子 `link()` 创建 `.rust-migration/.migration-lock` 文件（内容锁，不依赖 flock——flock 在 Claude Code 短命 Bash 进程模型下随进程退出释放，无法跨多次调用维持）；若锁文件已存在（link 失败），Skill 先做陈旧锁检测（读取锁文件 JSON 中的 `session_pid`=`$PPID` 并 `ps -p` 检活）；若确认是真实并发则报错退出：「检测到另一个 `/migrate` 命令正在运行；请等待其完成。如确认无进行中任务，可手动删除 `.rust-migration/.migration-lock`」——**不自动退避重试**。`[orchestration].lock_timeout_secs`（默认 300，覆盖 MVP 单模块预期时长）用于 `$PPID` 不可靠或跨机场景的兜底超时。完整 Step 0 锁协议骨架（原子创建、PPID 存活检测、陈旧锁恢复、释放时机、手动逃生口）见 [09-appendix-schemas.md 附录 B Step 0](./09-appendix-schemas.md#附录-bmvp-skill-的-skillmd-骨架)。

**Plan B 具体方案**（M0 Spike 1 失败时触发）：

| 方案 | 实现方式 | 代价 | 用户体验退化 |
|------|---------|------|-------------|
| **Plan B1: 微 Skill 链** | 将 `/migrate analyze` 拆为 `/migrate init`、`/migrate plan`、`/migrate test` 等微命令，每个 Skill 只做 1 步（1 次 SubAgent 调用）。状态通过 `migration-state.json` 在微 Skill 间传递。用户手动或脚本串联。 | 额外 2-3 人天开发 | 用户需手动执行更多命令，但每步更可控 |
| **Plan B2: 外部脚本编排** | 用 bash/Python 脚本调用 Claude Code CLI（`claude -p "执行 /migrate ..."`），脚本中做 if-else 分支、文件检查、重试逻辑。编排逻辑 100% 确定性。 | 额外 3-5 人天开发 | 依赖 Claude Code CLI API 的稳定性；需用户安装额外依赖 |
| **Plan B3: 混合方案** | 简单步骤（1-2 步）用 SKILL.md 指令，复杂编排（3+ 步循环/条件）用外部脚本。取两者优势。 | 额外 2-4 人天开发 | 最可能的实际落地方案 |

> **触发粒度（命令级 vs 步骤级）**：上述 M0 Spike 1 阈值（成功率 < 80% / 7 步完成率 < 95% / 超时 > 2，判定标准与决策终局性见 [07-pitfalls-and-risks.md § 12.2](./07-pitfalls-and-risks.md#122-plan-b-体系) 与 [08「M0→M1 决策检查点」](./08-roadmap-and-reference.md#m0--m1-决策检查点) 的协同约定）是**命令级**整体降级判据；当只有**单一步骤**反复失败（同一步连续触顶 § 「编排超时与失败重试策略」的 `max_retries_per_step`）时，可仅对该步做 **Plan B1 局部拆分**（把该步拆成独立微 Skill）而非整命令升级到 Plan B3，避免过度降级。**临界区间裁量**：成功率落在 80%-95% 之间时，依据「单步重试次数 + 总耗时」人工判定走 Plan B1 局部拆分还是 Plan B3 整命令升级，最终决策记入 `DESIGN_ASSUMPTIONS.md` 供 M1 参考。两者的验收数据均来自 M0 Spike 1，不另设新阈值。
>
> PORT-REVIEW 接受（R1-D8-05 medium，未新增独立「编排质量监控指标」表）：审查方案建议的量化表（成功率 ≥ 80% 保持 / 某步失败 2 次拆分 / 串行完成率 < 70% 升级）与本节及 [07 § 12.2](./07-pitfalls-and-risks.md#122-plan-b-体系) 既有阈值（> 20% 失败率、7 步完成率 < 95%、< 80% 强制 Plan B3）实质重叠，新增整张表会引入跨文件阈值冲突（违反 CLAUDE.md「文件权威来源」表与 YAGNI 护栏）。故只就「步骤级 vs 命令级触发粒度」这一真实缺口补一句，阈值仍以 07 § 12.2 为唯一权威。

**未来演进**：M2 阶段引入跨模块有限并行：不同模块的 SubAgent 可并行执行（同一模块内仍遵守 § 10.2 前置条件链 analyzer->translator->scaffolder）；并发上限由 `max_concurrent_agents` 控制（默认 3）。M4 阶段引入完整 DAG 调度。

### M2 扩展：并发文件通信协议（设计预留，不改变 MVP）

MVP（M1）阶段 SubAgent 串行执行，无并发风险。以下为 M2 引入并行时的并发安全设计预留，**不修改 MVP 行为**（M2「并发安全设计」为必做项，工作量见 [08-roadmap-and-reference.md M2 段落](./08-roadmap-and-reference.md)）：

| 共享资源 | M1（MVP）约束 | M2 设计预留 |
|---------|--------------|------------|
| `.rust-migration/porting/`（共享规则） | 串行写，无冲突 | CAS（比较并交换）更新；私有中间产物隔离到 `.rust-migration/intermediate/{agent_id}/` |
| `migration-state.json` | 单写者串行 | 顶层 `metadata` 增加 `version` + `last_modified_by` 字段支持乐观锁；CLI `rustmigrate state update --cas-version V1 ...`，版本不匹配返回冲突 |
| `source-graph.db`（SQLite） | 单写者；CLI `graph build` 与 analyzer 串行 | **D3/D5 定稿：编排器集中 writer（主架构）**——翻译期 SubAgent 经 `[workspace] shared_db_path`（绝对路径、只读）加载独立 petgraph 副本、**不直写 db**；`source-graph.db` 写者只有编排器：`graph build → save_to_db` 写图结构 + 模块终态回写 `migration_status`（与 `migration-state.json` 同序）。翻译期无多 writer 并发（SubAgent 只读、编排器单写），**WAL 并发写策略降为未来「SubAgent 直写 db」模式的可选回退**（见 [04 § 5.7.3](./04-toolchain.md#573-持久化存储) 与 [08 §M2 图写架构](./08-roadmap-and-reference.md#m2-质量提升8-12-周)） |
| 多 agent 工作区 | 单工作区 | 每个 agent 独立 worktree，final 阶段 merge 前检测 conflict |

> **MVP 序列约束的事务防御**：即使 MVP 串行执行，CLI `graph build` 仍用 `BEGIN IMMEDIATE TRANSACTION ... COMMIT` 包装写入，以防御指令跟随失败导致的意外并发并为 M2 并行预留。SKILL.md 的 `/migrate analyze` 序列在 Step 2.5 显式确认 CLI `graph build` 已**提交事务并释放 DB 写锁**后再启动 analyzer SubAgent——判定方式为单次读取 `migration-state.json` 的 `metadata.graph_build_completed`（该字段由 `graph build` 在 `COMMIT` 后同步写入，语义、读取与超时处理见 [09-appendix-schemas.md 附录 A「metadata 字段说明」与附录 B Step 2.5](./09-appendix-schemas.md#附录-amigration-statejson-schema)）。`migration-state.json` 的 `metadata.lock_token` 字段在 MVP 为 `null`，M2 用于分布式锁令牌（Schema 预留见 [09-appendix-schemas.md](./09-appendix-schemas.md)）。

### M2 扩展：跨模块并行的写隔离约束包（D3）

> 决策权威：[MDR-003](../decisions/003-m2-parallel-write-isolation.md)（含被否决方案 + 逃生口 + 四轮审查过程）。M2 跨模块并行（`max_concurrent_agents=3`）采用 **git worktree + 约束包**：每 agent 一个 worktree（含全部 done 代码），在内跑完整 M1 per-module 编译循环并**自检**，编排器结构化合并后**整组 `cargo check`/`test` 为唯一 done 真门**。worktree 是并行机制，与输出 crate 结构正交（M2 沿用单 crate 输出）。以下约束为实现必须落实项：

1. **完整 crate 真自检**：worktree 内做完整 crate 编译自检（含全部 done 依赖在场），**非模块副本假信心**——保留 M1 per-module 编译反馈环（`translate → cargo check → compile_fix → test`）。
2. **共享编辑策略 = D + A**（codex 第四轮验证排序 D > A > B > C；**否决** B 声明式 `shared_edits` schema 与 C interface-first 串行冻结）：
   - **D 最小化共享写面（porting 规则）**：并行模块**优先用既有共享 API + `Error::Other(String)`/`anyhow` 逃生口，不擅自扩展共享类型**；复杂共享扩展留**串行 cleanup**。减少共享写面比事后智能合并可靠（编排器是非确定性 LLM）。
   - **A 自由改 + 轻协议**：SubAgent 在 worktree 内**自由改任意文件（自检必需）**，回传 `{own_files, shared_touched}`（代码留盘）；**禁止删除/改签名既有共享 API，新增只允许 append**。⚠️ 语义冲突真实存在（孤儿规则逼 `From/Display/Serialize` impl 落进共享类型文件），叶子优先只能压低频率、不能消除。
3. **dependency-mapping 前置生态约束**：`dependency-mapping.md` 须前置规定唯一生态（anyhow xor thiserror、异步运行时唯一、序列化库唯一等），防并发模块各引不同 crate 产出 Frankenstein Cargo.toml。`new_deps` 合并须**并 feature 集 + 校验 default-feature 不冲突**（非仅版本取高）；`Cargo.lock` 合并后**重新 `cargo`-解析/验证**。
4. **两层 done**：worktree 自检过 = `agent_done`（substatus，**非终态**）；编排器结构化合并（新 `.rs` 复制 + `Cargo.toml`/`lib.rs(mod)` 程序化 union + 其余共享 `.rs` git merge）→ **整组 `cargo check`/`cargo test` 过才升最终 `done`**。兜住 orphan rule/coherence(E0119)/feature/宏/命名空间 等**只有整组编译才能暴露**的冲突——整组 check 是唯一 done 真门，不可省。
5. **reconcile 机制 + 轮次上限（防活锁）**：非结构化共享 `.rs` git 冲突 → **串行 reconcile**（冲突模块按依赖序逐个 rebase 到已合并主线后**重译**，**非 LLM 手解冲突块**）；reconcile 设**轮次上限**（默认同 `max_retry_rounds=3`），超限 → 该 sprint 降级串行翻译 / 转人工 review（杜绝「A 合并→B 重译又改共享→C 重译」活锁）。
6. **图缺陷回退**：整组 check 暴露「同层模块间本不该有的跨模块引用」（[03 § 4.2.1](./03-execution-model.md#421-执行模式分层) 同层 import 独立的前提被破坏，REFAC-10 档1 图不完整漏边所致）→ 判**图缺陷**，相关模块回退串行 + 记录待修图。
7. **target 目录策略**：各 worktree 先用**独立 `CARGO_TARGET_DIR`** 保证自检正确性（避免并发 cargo 锁争用）；**Sprint F 实测** worktree target 冷编/锁开销后，再据数据评估 sccache / 共享 target 优化（见下方进度保证「Sprint F 必测」）。

> **进度保证（无死锁，最坏退化串行）**：① 翻译期 agent 各在独立 worktree、不持共享锁、互不等待 → 无循环等待 ② reconcile 串行全序 + 轮次上限 → 无活锁 ③ 仅编排器写共享状态（`migration-state.json`/主 rust_root）→ 无写竞争。卡住的模块（自检/整组 check 3 轮不过）→ pause→degrade（headless 由 **M2-ADV-07** 自动 degrade 达终态，须改状态机见 [02 § 3.4](./02-architecture.md)）；下游由 **M2-CLI-06 auto-unblock** 解阻塞。**最坏情况 = 全 sprint 退化串行 = M1 速度，慢但不卡死。** worktree 优于轻量 staging 的核心论据「首轮普遍带编译错误」是合理推断而非实证，故 **Sprint F 必测**首轮编译通过率与 worktree target 成本；若数据 favor，「降级为轻量 staging」是已论证的简化路径。

---

## 10.6 产出物目录结构

```
.rust-migration/
├── PARITY.md                  # 迁移进度跟踪（Sprint 聚合）
├── KNOWN_DIFFERENCES.md       # 已知行为差异登记簿（即时写入）
├── AGENTS.md                  # AI 行为约束（含反合理化表）
├── SPRINT_LEARNINGS.md        # Sprint 级知识总结（每次 Review 追加）
├── DESIGN_ASSUMPTIONS.md      # M0 假设验证报告
├── migration-state.json       # 状态机 + Sprint 元数据
├── source-graph.db            # 源码图 SQLite 数据库（主存储）
├── source-ref/                # 源文件锁定副本（迁移期间保留，graduate 后清理）
├── porting/                   # 项目专有迁移规则
│   ├── dependency-mapping.md  # 项目特有的依赖映射
│   ├── business-logic-rules.md # 业务逻辑翻译策略
│   ├── known-workarounds.md   # 项目特有的 workaround
│   └── changelog.md           # 规则变更记录
├── context/                   # 项目知识（翻译经验沉淀）
│   ├── patterns/              # 项目特有的成功模式
│   ├── anti-patterns/         # 项目特有的失败教训
│   └── module-learnings/      # 模块级翻译经验
│       └── {module}/          # per-module 子目录
│           ├── learnings.md   # 模块翻译经验（L1，可选；格式见 05 §6.11）
│           └── _porting_manifest.json  # 规则版本清单（translator 写入，见 05 §6.2）
├── intermediate/              # 中间分析产物
│   ├── type-map.json          # 类型映射（M2 产出物，MVP 阶段类型映射信息在 porting/ 规则中）
│   └── attempts/              # 翻译尝试历史（断点续传用）
│       ├── {module}-phase-a.rs          # Phase A 持久化代码（供 verifier 对抗审查读取）
│       └── {module}-phase-b-partial.rs  # Phase B 失败时的部分状态（续传用，见 03 § 4.3 Step 5）
├── test-fixtures/             # 行为录制测试集
│   ├── golden/                # 黄金文件 (input/output 对)
│   ├── recordings/            # HTTP/CLI 录制
│   ├── proptest-regressions/  # proptest seed 记录
│   ├── fuzz-corpus/           # 模糊测试语料
│   └── benchmarks/            # 性能基线数据
├── decisions/                 # MDR 迁移决策记录（决策时立即写入）
│   ├── MDR-001-error-strategy.md
│   └── MDR-002-async-runtime.md
├── ci/                        # CI 辅助脚本（verify-reproducibility.sh，见 03 § 4.11.3）
└── reports/                   # 验证报告
    ├── coverage.json
    ├── complexity-comparison.json
    ├── unsafe-audit.json
    └── sprint-N-report.json
```

> **Phase 检查点与断点续传**：translator 除写 `attempts/{module}-phase-a.rs`（Phase A 完成）外，还在 `migration-state.json` 的 `attempts[].timestamp` 记录 Phase A 完成时间戳；Phase B 失败时写 `attempts/{module}-phase-b-partial.rs` 并置 substatus=`phase_b_failed_at_round_N`，`/migrate run --retry` 从该检查点重入 Phase B。恢复逻辑（基于 status/substatus 路由到对应入口步）见 [09 附录 B Step 0.3](./09-appendix-schemas.md#附录-bmvp-skill-的-skillmd-骨架)；契约见 [03 § 4.3 Step 5](./03-execution-model.md#43-内循环模块级单会话内-phase-ab-双阶段翻译) 与 [09 附录 A substatus 约定](./09-appendix-schemas.md#附录-amigration-statejson-schema)。

> **与 Plugin 目录的关系**：`.rust-migration/` 是项目本地产出物目录（每个迁移项目独立），Plugin 目录（`rust-migrate-plugin/`）是分发给所有用户的通用工具包。核心规则内嵌在 Plugin 的 `agents/*.md` 中，参考指南放在 Plugin 的 `skills/migrate/references/` 下（见 10.1.1 规则分层策略）。

---

## 10.7 错误信息与可调试性（MVP）

CLI 的成功输出为 `{status, data, warnings}`（见 § 10.0.1）。失败时输出标准化 error JSON，使错误可被工具/CI 解析并支撑重试决策：

```json
{
  "status": "error",
  "error_code": "PHASE_A_TRANSLATION_FAILED",   // 稳定错误码（见下方错误码表）
  "error_context": {
    "module": "src/auth/session.ts",
    "attempt_num": 2,
    "compiler_errors": ["E0382: borrow of moved value: `cfg`"],
    "suggested_fix": "将 cfg 改为按引用传递或 clone"      // 1-2 行修复建议模板
  }
}
```

**常见迁移错误码（与 SubAgent substatus 对应）**：下表（连同上文 § 10.7 的 `VALIDATION_*` 三码）即为 **MVP（M1）错误码表**，覆盖编排层主路径高频故障。完整编码体系（30-40 条，按「编译错误 / 翻译语义错误 / 验证失败 / 状态机错误 / SubAgent 错误」五类组织，每条含稳定码名、含义、触发条件、CI 推荐重试策略）在 **M2 开发期**补充，届时落地为 09-appendix-schemas.md 专节并在此回链（工作量见 [08-roadmap-and-reference.md](./08-roadmap-and-reference.md)）；MVP 不阻塞于完整枚举（R1-BS2-01）。

| error_code | 含义 | 典型修复建议 |
|-----------|------|------------|
| `LIFETIME_MISMATCH` | 生命周期不匹配 | 引入显式生命周期标注或调整借用范围 |
| `MUTEX_SEND_SYNC_VIOLATION` | 跨 `.await` 持有非 Send 类型 | 缩小锁持有范围或换用 `tokio::sync::Mutex` |
| `TYPE_INFERENCE_FAILED` | 类型推断失败 | 补充类型标注 |
| `PHASE_A_TRANSLATION_FAILED` | Phase A 忠实翻译编译失败 | 见 `error_context.compiler_errors` |
| `BLOCKED_BY_VALIDATION_FAILED` | `blocked_by` 引用了不存在的模块名（引用一致性，延后到 `/migrate run` Step 0.5 校验） | 若发生在 `/migrate analyze` 后，多为 SubAgent 写入的非法引用，重跑 `/migrate analyze`；若发生在 `/migrate run` Step 0.5，检查用户手填的 `blocked_by` |
| `VALIDATION_TIMEOUT` | L2 校验工具超时（> `[validation].timeout_secs`，默认 30s） | 工具故障，非产出物失效：检查环境/增大超时后重试一次 |
| `VALIDATION_OOM` | L2 校验工具内存不足 | 工具故障：检查环境内存后重试一次 |
| `VALIDATION_SCHEMA_CORRUPTED` | Schema 文件损坏 | 工具故障：重新运行 `rustmigrate init` 修复 Schema 后重试 |
| `PLUGIN_VERSION_MISMATCH` | `plugin.json` version 与 `rustmigrate --version` 不一致 | 重新安装匹配版本的 CLI/Plugin（见 § 10.0.3 升级失败诊断） |
| `SCHEMA_VERSION_UNSUPPORTED` | `migration-state.json` / `source-graph.db` 的 `schema_version` 超出当前支持范围 | 运行迁移工具升级 schema，或回退到兼容的 Plugin 版本 |
| `ADAPTER_TOOL_MISSING` | `rustmigrate profile` 检测到适配器所需外部工具（如 dependency-cruiser、mypy）未安装或版本不满足 | 按 `error_context.install_hint` 安装缺失工具后重新执行 |
| `RUST_TOOL_MISSING` | `rustmigrate profile` 检测到 Tier 0 Rust 外部二进制（`cargo-nextest`）未安装 | 按 `error_context.install_hint`（如 `cargo install cargo-nextest`）安装后重新执行 |

> **校验工具故障 vs 产出物失效的区分（R2-D5-04）**：L2 校验本身是 CLI 子命令 `rustmigrate validate state`，其执行可能因超时 / OOM / Schema 损坏而失败。SKILL.md 检查点须按 CLI 返回的 `error_code` 区分：若为 `VALIDATION_TIMEOUT` / `VALIDATION_OOM` / `VALIDATION_SCHEMA_CORRUPTED` 三者之一，记入 `subagent_calls` 的 substatus 为 `validation_tool_error_<type>`，**不进入 `max_retries_per_step` 重试循环**（重试无意义），而是向用户输出「校验工具故障，请检查环境（增大超时 / 内存 / 重新 init）后重试一次」；其余 error_code（JSON Schema 违反、SQLite 表结构缺失等产出物真失效）才进入正常重试循环。区分逻辑骨架见 [09-appendix-schemas.md 附录 B 检查点](./09-appendix-schemas.md#附录-bmvp-skill-的-skillmd-骨架)。

> **Hook 失败输出一致性**：`verify.sh` 和 `full-verify.sh` 失败时，输出格式须与上述 CLI error JSON 兼容——脚本内部调用的 cargo/clippy 诊断自动转换为 `{error_code, suggested_fix}` 结构，便于 Skill 与 CI 统一解析。
>
> **范围**：MVP 仅做「错误可被工具解析」（标准 error JSON + 错误码表）。诊断指南库（`skills/migrate/references/diagnostics/`）与 SubAgent 系统提示中嵌入「生成修复建议」要求为 M2+ 后置项，避免过度设计。

---

## 10.8 crash-safe state 持久化（MVP）

`migration-state.json` 在一次迁移中被多处写入（SKILL.md 的初始化、状态更新、失败记录、最终更新），若任一次写入中途进程崩溃（网络中断、LLM 超时、Plugin 重启），可能留下半完成的损坏文件，而 JSON 解析器无法恢复部分写入。`source-graph.db`（SQLite WAL）已具备原子性，`migration-state.json` 须对称地提供 crash-safe 保证（R2-D5-03）：

- **原子写入**：所有 `migration-state.json` 写入统一采用「写 `.migration-state.json.tmp` → `fsync` → 原子 `rename`」模式（POSIX `rename` 同目录覆盖为原子操作）。无论写入者是 CLI `graph build` 还是 Skill 脚本的各步骤，均须遵守此约定。
- **写前备份**：写入前先创建 `.migration-state.json.backup`（由 `.rustmigrate.toml` `[persistence].backup_on_write=true` 控制，默认开启；保留期 `retention_days=30`）。
- **读取恢复**：读取时若主文件 JSON 解析失败，自动尝试从 `.backup` 恢复；两者均失败则输出明确指引：「状态文件损坏，无法自动恢复。请手工操作：(a) 从版本控制恢复 `.rust-migration/` 目录；(b) 或从最近 `.backup` 恢复；(c) 或联系社区」。

> 故障恢复的三种路径、备份位置与手工编辑约束（修改后须 `rustmigrate validate state` 重新验证）在 Plugin README 的「故障恢复」节展开；crash-safe 写入的回归测试（模拟写入中途 SIGKILL/SIGTERM 后能从 `.backup` 恢复）纳入 `test/integration/`，参照 SQLite WAL 原子 rename 的同类工业实践。

---

# 十一、工作流灵活性与扩展

## 11.1 .rustmigrate.toml 配置文件

```toml
[project]
name = "my-project"
source_language = "typescript"       # typescript | python | c | cpp | go（M3 起可省略，省略时 init 按 source_root 自动探测）
source_root = "./src"
rust_root = "./rust-src"
source_commit = "abc123"             # 锁定源码版本

# 排除不参与迁移的路径（glob 模式）
# 注：非排除文件 import 排除路径时，graph build 跳过该边（无 FK 违反）并记入
# build 输出的 excluded_imports 数组（见 04 §5.7.4）；/migrate run Step 1 会对此输出警告
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
async_strategy = "boundary_async"    # core_sync | boundary_async | full_async（由 PLAN 阶段据 analyzer 检测结果确认，语义见 03 § 4.7）
max_concurrent_agents = 3
max_retry_rounds = 3                 # 翻译失败最大重试轮数
auto_confirm_intent = false          # 意图确认门禁（默认 false=须人类确认；各模式默认值见 03 §4.3.1）
degrade_strategy = "skip"            # skip | manual（MDR-007：FFI 桥接已取消）

[tools]
tier0 = true                         # 硬性门禁（不可关闭）
tier1 = true                         # 推荐工具（启用规则见 04 § 5.3：proptest/insta 由画像强制，cargo-llvm-cov 不可关闭）
tier2 = false                        # 高级工具（默认关闭）
# 禁用任一被画像强制的 Tier 1 工具须在此记录原因与置信度，会打印进验证画像（04 § 5.3）
# confidence 枚举：low（判定不确定）| medium（半验证）| high（完全验证）；low/medium 须经 verifier 复核（见 04 § 5.3）
tier1_exceptions = []                # 例：[{ tool = "proptest", reason = "no_pure_functions", confidence = "low" }]
petgraph_fallback_threshold = { critical_issues_open = 6, maintainer_inactive_months = 6 }  # 触阈则切自建邻接表回退（04 § 5.7.2）

[tools.tier2_override]
cargo_fuzz = false
cargo_mutants = false
miri = false
kani = false
loom = false                         # 默认 Tier 2；当模块含 std::sync 并发原语时提升为条件强制（见 04 §5.3）
shuttle = false                      # 默认 Tier 2；当模块含 tokio::sync/async 并发时提升为条件强制（见 04 §5.3）
criterion = false                    # 默认 Tier 2；当 migration_motives 含 performance 时自动提升为 Tier 1

[testing]
coverage_threshold = 80              # 覆盖率门槛（百分比）；与 03 §7.5 三级规则取并集；三级规则内的 40% 硬编码下限为不可配最低基线
proptest_cases = 256                 # 属性测试用例数（FFI 对比预计 > 30s 时 verifier 自动逐级下调 256→64→32，见 03 § 7.6.1）
fuzz_duration_secs = 60              # 模糊测试时长
benchmark_tolerance = 0.10           # 性能回归容忍度（10%）
ffi_sampling_threshold_ms = 10       # 单次 FFI 调用延迟阈值（以 M0 实测 p95 为单位）；超阈触发抽样，语义见 03 § 7.6.1
min_applicable_dimensions_per_function = "auto"  # auto（1-2 参数≥3 维 / 3+ 参数≥5 维）| N；§7.7 探测维度覆盖下限，verifier 自校验
chain_tolerance = { float_epsilon = 1e-9, collection_order_sensitive = false }  # 链式模块逐层比对容忍度，语义见 03 § 7.2 Step 2.1
nextest_threads = "auto"             # auto | 1 | N；CPU 数为默认。跨模块迁移或 FFI 测试推荐 1（串行，确保测试隔离，见 04 § 5.2）
ffi_coverage_mode = "rust_only"      # rust_only（默认，LLVM 只统计 Rust 侧）| include_calls（需 instrumented source 支持）；FFI 模块覆盖率采样限制见 04 § 5.3

[quality]                            # 质量评分（§ 7.5）；M1 仅 per-module 评分，趋势检测推至 M2
evaluation_method_version = "0.1"    # 评分规则版本，跨 Sprint 一致性校验（规则变更时递增）
baseline_sprint = 1                  # 趋势/回归对比的基线 Sprint（M2 用）
# score_regression_threshold 见 [stop_loss]（§ 4.9），不在此重复

[parser]
ast_engine = "tree-sitter"           # tree-sitter | ts-compiler-api（Spike 3 Plan B 回退，见 04 § 5.7.4）
ast_engine_fallback_threshold = 0.01 # 解析错误率超此值触发 Plan B（默认 1%，M0 Spike 3 可据实测覆写；分档降级见 04 § 5.7.4）

[analysis]
batch_size = "auto"                  # auto（按 ≤100K token 预算推导，约 35 文件/批）| 15-50（固定值）；社区检测分批，依据见 04 § 5.7.4.1
batch_reuse_rate_threshold = 0.30    # M1 运行时监控跨批重复分析率，超此值告警并建议启用自适应分批/目录降级；依据见 04 § 5.7.4.1

[reproducibility]                    # CI 确定性（语义见 03 § 4.11.3）
tree_sitter_version = "crates-io:0.20.x"  # 必填，"git:<commit-hash>" 或 "crates-io:0.20.x"；rustmigrate init 设为当前 tree-sitter-rust crate 最新小版本
cargo_lock_vendored = true           # M1 默认 true，强制 vendored Cargo.lock 锁定依赖版本
llm_determinism_seed = ""            # 可选；非空时作为 seed 传 Claude API；API 不支持 seed 时改为固定 temperature=0.2（见 03 § 4.11.3）

[context]
max_tokens_per_translation = 100000  # 每次翻译上下文预算（拆分/降级策略见 02 § 3.5.1）
module_summary_strategy = "interface_only"  # interface_only | full
budget_check_mode = "warn"           # strict（超预算即暂停）| warn（>0.95×max_tokens 提示确认）| ignore
enable_auto_split = true             # 超预算时是否按 02 § 3.5.1 决策树自动拆分

[workspace]
cargo_workspace = true               # 使用 Cargo workspace
crate_naming = "kebab-case"          # 子 crate 命名风格

[orchestration]                      # 编排级超时/重试（见 § 10.5）
subagent_timeout_secs = 600          # 单步 SubAgent 调用超时（10 分钟）
max_retries_per_step = 2             # 每步最大重试次数（不含初次尝试）
retry_backoff_secs = [5, 15]         # 梯级退避秒数
lock_timeout_secs = 300              # 全局命令锁陈旧兜底阈值：$PPID 不可靠或跨机场景的 now-started_at 判定（见 § 10.5 与 09 附录 B Step 0）
stall_timeout_secs = 600             # M4-ROB-01b watchdog：后台命令 stdout 静默超此值判 stall（与 subagent 总超时正交，见 § 10.5）
stall_recovery_policy = "retry_then_skip"  # M4-ROB-01b stall 恢复策略：retry_then_skip | always_retry | always_skip

[persistence]                        # state 持久化与崩溃恢复（见 § 10.8）
backup_on_write = true               # 每次写 migration-state.json 前创建 .backup
retention_days = 30                  # 备份保留期限

[validation]                         # L2 校验工具的故障边界（见 § 10.5 / § 10.7）
timeout_secs = 30                    # validate state 校验超时；超时视为工具故障而非产出物失效

[rules]                              # 规则版本追踪（见 05 § 6.2 代码回溯机制）
version_tracking = true              # 在 _porting_manifest.json 记录各模块/函数依据的规则版本
auto_regenerate_on_rule_upgrade = "review"  # skip | review | retranslate：breaking change 后对受影响模块的默认动作
enforce_rule_version_consistency = true     # 规则版本一致性强制（M4-GOV-01 已落地）：CLI `validate rules` 检出适配器模板 rule_version 与权威清单不一致时返回错误（false 降级 warning）；verifier 拒绝混用不兼容规则版本

# [storage]                          # 可选，SQLite 调优（默认值，通常无需设置）
# sqlite_busy_timeout_ms = 5000      # 锁等待超时
# wal_autocheckpoint_frames = 1000   # WAL 自动 checkpoint 阈值（帧）

# [adapter_validation]               # 可选，新语言适配器验收阈值（见 § 11.2）
# type_precision_threshold = 0.95    # 类型提取精度下限
# dep_coverage_threshold = 0.90      # 依赖覆盖率下限
```

**行动指南**：`/migrate analyze` 自动根据项目画像生成初版配置，用户可手动调整。

---

## 11.2 语言扩展架构

设计为**目录约定 + 两文件契约**的适配器模式（MDR-009）。每种源语言对应一个适配器目录，**只含两个契约文件**：`analysis-tools.json`（语言专用外部工具列表，CLI `profile --adapter-tools` 消费）+ `porting-template.md`（语言 → Rust 迁移规则模板，translator 消费）。适配器位于 Plugin 的 `skills/migrate/adapters/` 下。

**与早期设计的差异（MDR-009）**：本节早期版本曾把适配器定义为一组每语言 shell 脚本（`adapter.json` / `detect.sh` / `extract-types.sh` / `extract-deps.sh` / `ffi-bridge.sh`）+ JSON Schema 契约。该 shell 脚本模式从未落地、已正式取消，其职责分别下沉到 LLM 流程与 CLI：

- **语言检测**：由 `analyze.md` 流程读目录特征文件（`package.json` / `pyproject.toml` / `go.mod` 等）判定，不再有 `detect.sh`（信号与权重见 § 11.3）。
- **依赖/类型提取**：由 CLI `rustmigrate graph build`（内置 tree-sitter）完成，import 解析作为语言内部事务下沉到 CLI、图引擎保持语言无关，不再有 `extract-types.sh` / `extract-deps.sh`。
- **工具可用性检测**：由 CLI `rustmigrate profile --adapter-tools <analysis-tools.json>` 完成，只消费 `analysis-tools.json`。
- **FFI 桥接**：`ffi-bridge.sh` 的取消见 MDR-007。

因此 `adapter.json` 不再存在（无脚本路径需登记），也无对应的 JSON Schema CI 门禁。

### 目录约定

```
skills/migrate/adapters/
├── typescript/
│   ├── analysis-tools.json     # 语言专用外部工具列表（profile --adapter-tools 消费）
│   ├── porting-template.md     # 迁移规则模板（语言专用，生成到 .rust-migration/porting/）
│   └── tests/                  # 可选：验收基准数据（端到端样本 + ground-truth）
├── python/
│   ├── analysis-tools.json
│   └── porting-template.md
├── go/
│   ├── analysis-tools.json
│   └── porting-template.md
└── c_cpp/
    └── ...                     # 同上两文件结构（推迟，见 PLAN-M4 D-M4-03）
```

> **适配器与核心规则的版本同步（R3-D7-03）**：`porting-template.md` 的 frontmatter 已有 `rule_version` 字段（见 § 11.2.1）；约定其值记录生成/更新该模板时依据的核心规则版本（如 `RULE-3:v2.0.0, RULE-8:v1.5.0`）。核心规则发生破坏性升级（如 RULE-3 v1→v2）时，已生成模板的 `rule_version` 与当前核心规则比对即可识别过期，避免新旧规则版本混用（防止 [05 § 6.2](./05-documentation-system.md#62-迁移规则体系通用--项目专有)「项目专有规则优先」约束被打破）。陈旧检测的程序化执行（verifier 在 `/migrate analyze`/`run` 时比对并提示复审）为 M2 项，复用既有 `[rules].enforce_rule_version_consistency` 开关，不新增配置字段。

### 适配器职责分配

适配器两文件契约 + LLM 流程 + CLI 共同覆盖以下逻辑职责（不再映射到每语言 shell 脚本，MDR-009）：

| 逻辑职责 | 承担方 | 说明 |
|---------|--------|------|
| 语言标识 | `porting-template.md` frontmatter `language_id` | 语言标识 |
| 语言检测 | `analyze.md` 读特征文件 | 由 analyze 流程读 `package.json` / `pyproject.toml` 等判定（信号与权重见 § 11.3） |
| 类型 / 依赖提取 | CLI `rustmigrate graph build`（tree-sitter） | 语言无关图引擎，import 解析下沉到 CLI；输出映射到 source-graph 节点/边 |
| 工具可用性检测 | CLI `profile --adapter-tools` + `analysis-tools.json` | 检测语言专用外部工具是否预装 |
| PORTING 规则 | `porting-template.md` | 该语言的迁移规则预置，translator 消费 |

> **M3 变更（MDR 见 008）**：`source_language` 由必填改为可省略（`Option<SourceLang>`，默认 `None`）。省略时 `init` 按 `source_root` 调 `detect_language` 自动探测、回退 TypeScript，并据语言写入 `default_excludes_for_lang`（TS: `node_modules`/`dist`；Python: `__pycache__`/`.venv` 等）。

### 11.2.1 两文件契约

`porting-template.md`（translator 消费，由 analyze 规则生成步 Read 注入 translator 上下文）契约（YAML frontmatter + 规则正文）：

- frontmatter 格式复用 [05-documentation-system.md § 6.1](./05-documentation-system.md#61-核心产出物总览)；对适配器的特殊约定：**必含 `language_id`、`rule_version` 字段**。
- 正文至少含一条 `## 类型映射` 标题；除必含 `## 类型映射` 外，应含所有与源语言存在惯用法差异的 MVP 通用规则类对应节（参考 [05 § 6.2.1](./05-documentation-system.md#621-跨适配器特化示例) 跨适配器特化示例）。

`analysis-tools.json`（CLI `profile --adapter-tools` 消费）为 JSON 数组，每项 `{tool_id, display_name, min_version, install_hint, required, version_args}`，由 CLI 读取并检测语言专用外部工具可用性。其中 `version_args`（可选，默认 `["--version"]`）覆盖版本探测参数——少数工具不接受 `--version`（如 Go 用 `go version`），用此字段指定。

> **类型/依赖提取的输出契约**：由 CLI `rustmigrate graph build`（tree-sitter）统一产出，不再是适配器脚本职责。其输出映射到 source-graph 节点/边的权威格式见 [09-appendix-schemas.md § source-graph 导出格式](./09-appendix-schemas.md#附录-d关键中间产物-schema简化版)（每条依赖映射为 `imports`/`uses_type` 边，源/目标对应节点 `id`）。

**MVP 支持**：TypeScript 适配器。
**后续迭代**：Python 适配器 → C/C++ 适配器 → Go 适配器。

### 新语言适配器的工作量拆解

> 工作量总数以 [08-roadmap-and-reference.md](./08-roadmap-and-reference.md) 为唯一权威（TS 适配器 M1 合计 3-5 人天）。下表为该总数的组件级拆解，供社区贡献者评估。**适配器目录本体只产出两个文件**——类型/依赖提取由 CLI tree-sitter 统一承担（不在适配器目录内），语言检测复用 analyze 特征文件读取：

| 适配器组件 | TS 基准工作量 | 置信度 | 多语言风险因子 | 说明 |
|----------------|--------------|--------|--------------|------|
| `porting-template.md` | 1-1.5 人天 | 80% | 语言惯用法差异（C 宏 / Python 动态） | 语言专用迁移规则预置（适配器目录文件） |
| `analysis-tools.json` | 0.5 人天 | 90% | 工具生态成熟度（Mypy / libclang） | 列出语言专用外部工具供 profile 检测（适配器目录文件） |
| CLI tree-sitter 语言支持 | 1-2 人天 | 65% | grammar 成熟度 / import 解析复杂度 | 在 CLI 接入该语言 tree-sitter grammar 与 import 解析（一次性，非适配器目录文件） |
| 测试 | 1 人天 | 85% | 端到端样本可得性 | 适配器端到端验证 |

> **推广系数与成本重校触发器（R1-D7-03）**：上述基准的 TS 数据点经 M1 验证，但在 Python / C++ 上的成立度仅为表中「置信度」列所示（CLI tree-sitter 语言支持因依赖 grammar 与 import 解析成熟度，置信度最低）。社区贡献新适配器的预期周期为 **3-5 个工作日**（贡献流程承诺与 PR SLA——如「贡献超 7 天可拆为 M2 子任务」——见 Plugin 仓库 CONTRIBUTING.md）。**触发条件**：M2 任一 Python/C++ 适配器实际工作量超估 > 20% 时，须在进入下一语言前发起一次紧急成本复审。该拆解的真实成本回顾以 [08-roadmap-and-reference.md § 13.3 多语言适配器成本回顾](./08-roadmap-and-reference.md#m1-mvp-6-8-周)（**M1 强制交付 gate，而非 M1 后置项**）为权威数据来源。

### 适配器验收标准（可配置阈值）

适配器精度/覆盖率阈值在 `.rustmigrate.toml` 中可配置（见 § 11.1 `[adapter_validation]` 段），便于 CI 工具化校验。提取精度/覆盖率校验的是 **CLI tree-sitter 在该语言上的提取质量**（适配器目录无脚本，提取统一由 CLI 承担，MDR-009）：

| 验收项 | 默认阈值 | 校验方式 |
|--------|---------|---------|
| CLI tree-sitter 类型提取精度 | `type_precision_threshold = 0.95` | 该语言下 `rustmigrate graph build` 输出与 `tests/expected/type-map.json` 人工标注对比 |
| CLI tree-sitter 依赖覆盖率 | `dep_coverage_threshold = 0.90` | 该语言下 `rustmigrate graph build` 输出与 `tests/expected/deps.json` 已知依赖清单对比 |
| `porting-template.md` 必含字段 | 必须通过 | CI grep 校验 frontmatter 含 `language_id` + `rule_version` |
| `porting-template.md` 规则类覆盖 | 至少覆盖 [05 § 6.2](./05-documentation-system.md#62-迁移规则体系通用--项目专有) 中 MVP=是 AND 层级=通用 的必覆盖基线（RULE-2 类型映射、RULE-3 错误处理、RULE-8 命名约定；其余通用 MVP 规则按语言差异度自评覆盖） | CI grep 统计 template 中 `## RULE-N` 或等价标题数 ≥ 基线数 |

---

## 11.3 智能项目类型检测

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

---

## 11.4 四级渐进式用户旅程

| 级别 | 用户动作 | 工具介入深度 | 适用场景 |
|------|---------|-------------|---------|
| L1 探索 | `/migrate analyze` | 分析源码、生成项目画像、生成迁移规则、搭建测试基础设施 | 评估可行性 + 准备阶段 |
| L2 执行 | `/migrate run` | 逐模块迁移（Phase A/B 双阶段） + 验证 | 实际迁移 |
| L3 审查 | `/migrate review` | 运行完整验证管线 + 查看迁移进度仪表板 | Sprint Review |
| L4 毕业 | `/migrate graduate` | 评估毕业标准 + unsafe 审计 + 知识固化 | 迁移收尾 |

**行动指南**：用户可以在任意级别停留，不强制推进。L1 的画像报告本身就有价值（评估迁移可行性和成本）。当所有模块为 done/degrade 时，`/migrate review` 提示执行 L4 毕业。

---

## 11.5 验证管线 DAG 自定义

验证管线的逐工具启用/禁用统一通过 § 11.1 的 `[tools]` 段控制（Tier 0 不可关闭，Tier 1 由 `tier1_exceptions` 逐工具覆盖，Tier 2 由 `[tools.tier2_override]` 逐工具覆盖）。DAG 拓扑上 Tier 间天然存在依赖：Tier 0 通过 → Tier 1 可并行 → Tier 2 按资源顺序执行；同 Tier 内各工具可并行。
