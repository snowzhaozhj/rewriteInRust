> [返回主索引](./README.md)

# 十、Claude Code 插件结构

## 10.0 Plugin 概览

本项目从第一天起设计为 Claude Code Plugin，遵循标准 Plugin 打包格式。

### .claude-plugin/plugin.json

```json
// .claude-plugin/plugin.json
{
  "name": "rust-migrate",
  "version": "0.1.0",
  "description": "Rust 迁移验证工作台 — AI 辅助的代码迁移 + 验证管线",
  "author": "rewriteInRust",
  "skills": "./skills/"
}
```

> **注意**：`agents/`、`hooks/` 等目录通过 Plugin 目录约定自动发现，不需要在 plugin.json 中显式声明。Plugin 不支持 `rules/` 目录分发（见 10.1.1 规则分层策略）。

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

**MVP（M1）— 11 个命令**：

| 子命令 | 说明 |
|--------|------|
| `rustmigrate init` | 初始化 `.rust-migration/` 目录 + 项目根目录 `.rustmigrate.toml` 配置文件 |
| `rustmigrate profile` | 分析源码项目画像（语言检测、框架识别、代码统计） |
| `rustmigrate graph build` | 使用 tree-sitter 解析源码，构建源码图（存储到 `source-graph.db`） |
| `rustmigrate graph topo-sort` | 对依赖图执行拓扑排序，输出迁移顺序 |
| `rustmigrate graph deps <module>` | 查询模块的正向依赖树 |
| `rustmigrate graph stats` | 图统计信息（节点/边计数、度分布） |
| `rustmigrate validate state` | 校验 `migration-state.json` 的合法性（JSON Schema + 状态机约束） |
| `rustmigrate state get` | 查询指定模块的当前迁移状态 |
| `rustmigrate state transition` | 执行状态转换（带前置条件检查） |
| `rustmigrate stats loc` | 统计源码和 Rust 代码行数（嵌入 tokei） |
| `rustmigrate scaffold workspace` | 生成 Cargo workspace 骨架（注入 dev-dependencies） |

**M2 扩展 — 5 个命令**：

| 子命令 | 说明 | 推迟理由 |
|--------|------|---------|
| `rustmigrate graph rdeps <module>` | 反向依赖查询 | MVP 阶段 deps 正向查询够用 |
| `rustmigrate graph cycles` | 循环依赖检测 | MVP 目标是 <5K 行单项目，环少见 |
| `rustmigrate graph export` | 导出为 JSON/DOT/Mermaid | MVP 阶段 stats 输出够用 |
| `rustmigrate stats compare` | 源码与 Rust 复杂度对比 | MVP 阶段手动 tokei 对比即可 |
| `rustmigrate validate config` | 校验 `.rustmigrate.toml` | MVP 阶段 TOML 解析时隐式校验即可 |

### Workspace 结构

参考 ast-grep / oxc 的 workspace 组织方式，采用 crate 分层：

```
cli/
├── Cargo.toml              # workspace root
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
| ast-grep-core | 代码模式搜索/重写 | `profile`（惯用法检测） |
| tokei | 代码行数统计 | `stats loc`, `stats compare` |
| syn + quote | Rust 代码生成/分析 | `scaffold workspace` |
| petgraph | 依赖图数据结构（StableGraph + newtype 索引） | `graph build/topo-sort/deps/rdeps/cycles` |
| rusqlite | SQLite 图持久化 + FTS5 全文搜索 | `graph build`（写入）, `graph export`（查询） |
| jsonschema | JSON Schema 校验 | `validate state`, `validate config` |

### 分发方式

1. **Plugin `bin/` 预编译**：Plugin 的 `bin/` 目录包含主流平台的预编译二进制（`rustmigrate-darwin-arm64`、`rustmigrate-linux-x86_64` 等）。`scripts/ensure-cli.sh` 在首次调用时检测 OS/Arch 并选择正确的二进制。
2. **`cargo install rustmigrate`**：用户也可通过 crates.io 安装最新版本，覆盖 Plugin 预编译版本。
3. **优先级**：`$PATH` 中的 `rustmigrate` > Plugin `bin/` 中的预编译版本。

### M1 CLI 工作量估算

| 模块 | 工作量 | 说明 |
|------|--------|------|
| CLI 骨架（clap + JSON 输出） | 1 人天 | workspace 搭建 + clap 路由 + 统一输出格式 |
| `profile`（tree-sitter + tokei） | 2 人天 | 语言检测 + 框架识别 + 代码统计 |
| `graph build` + 查询命令（petgraph + rusqlite） | 3 人天 | AST 提取 + 图存储 + topo-sort/deps/rdeps/cycles/stats/export |
| `state` + `validate`（jsonschema） | 1.5 人天 | 状态机 + JSON Schema 校验 |
| `stats loc` + `compare` | 1 人天 | tokei 封装 + 复杂度对比 |
| `scaffold workspace` | 1 人天 | Cargo.toml 生成 + dev-deps 注入 |
| 测试 + CI | 1.5 人天 | 单元测试 + 集成测试 + GitHub Actions |
| **合计** | **~11 人天** | M1 阶段完成 |

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

---

## 10.1.1 规则分层策略（方案 D：混合）

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
| `translator` | 迁移规则生成、Phase A 忠实翻译 + Phase B 惯用化优化、多候选生成 | LLM, syn+quote, ast-grep |
| `verifier` | 等价性验证、**模块级测试生成**、Phase A→B 中间的对抗性审查、不等价证据收集、性能对比 | cargo-test, proptest, criterion, Miri |
| `scaffolder` | 测试基础设施搭建、行为录制、黄金测试集管理、Cargo workspace 骨架生成 | insta, cargo-fuzz, mitmproxy |

### SubAgent 输入/输出接口表

> 每个 SubAgent 通过 `.rust-migration/` 目录下的文件通信。以下表格定义各 SubAgent 的输入/输出契约。

| SubAgent | 输入文件 | 输出文件 | 前置条件 | 后置条件 |
|----------|---------|---------|---------|---------|
| **analyzer** | 源码目录、`.rustmigrate.toml` | `source-graph.db`（语义增强）、项目画像摘要（stdout JSON） | CLI `rustmigrate graph build` 已完成基础图构建 | `source-graph.db` 含 calls/uses_type 边 |
| **translator**（规则生成） | `source-graph.db`、适配器 `porting-template.md` | `porting/` 目录（迁移规则文件） | analyzer 已完成 | `porting/` 至少含一个规则文件 |
| **translator**（代码翻译） | `porting/` 规则、目标模块源码、依赖接口 | Rust 源文件（写入目标目录）、`{module}-intent.md`（意图摘要） | 模块状态为 translating | Rust 文件存在且通过 F1 编译 |
| **verifier**（对抗审查） | Phase A Rust 产出物、原始源码、迁移规则 | `{module}-review.md`（审查报告） | Phase A 翻译完成 | 审查报告包含差异列表 |
| **verifier**（测试验证） | Phase B Rust 产出物、黄金文件 | 测试结果 JSON（stdout）、KNOWN_DIFFERENCES.md 追加条目 | Phase B 翻译完成 | 测试通过率 ≥ 预期 |
| **scaffolder** | `source-graph.db`、模块接口信息 | `test-fixtures/golden/` 测试数据、Cargo.toml dev-deps 注入 | analyzer 已完成 | 测试基础设施可运行 |

**行动指南**：每个 SubAgent 有独立的系统提示，包含其职责边界和可用工具列表。Agent 之间通过 `migration-state.json` 和产出物文件通信。

### 10.2.1 SubAgent 实现机制

MVP 中 SubAgent 的实现基于 Claude Code 的标准 agent 定义机制：

**文件形式**：
- 每个 SubAgent 是 Plugin 的 `agents/` 目录下的一个独立 `.md` 文件（如 `analyzer.md`、`translator.md`、`verifier.md`、`scaffolder.md`）
- 每个 `.md` 文件定义该 SubAgent 的系统提示，包含职责描述、可用工具列表、行为约束和输出格式要求

**调用方式**：
- Skill 的 SKILL.md 中通过 Claude Code 的 `Agent` tool 调用 SubAgent
- 调用时指定 `agentType` 为对应的 agent 名称（如 `analyzer`），Claude Code 自动加载对应的 `agents/analyzer.md` 作为系统提示
- 示例：SKILL.md 中写"使用 Agent tool 调用 analyzer SubAgent，传入项目根目录路径"

**上下文隔离**：
- 每个 SubAgent 运行在独立的 agent 上下文中，不共享对话历史
- SubAgent 之间通过文件系统（`.rust-migration/` 目录）共享数据，不直接通信
- 这保证了每个 SubAgent 的上下文窗口不被其他 SubAgent 的输出污染

**错误传播**：
- SubAgent 的输出文本返回给 Skill（即主对话上下文中的 Claude）
- Skill 根据 SubAgent 的输出文本判断成功/失败——检查关键产出物文件是否存在且有效
- 失败时 Skill 根据 SKILL.md 的分步指令决定重试或降级

---

## 10.3 Hooks（自动化门禁）

**关键原则（Deterministic Assertion Enforcement）**：门禁用独立脚本，agent 无法说服自己跳过。

> **v0.9 变更**：F1 编译反馈改为 rust-analyzer LSP 自动诊断，删除原 PostToolUse → cargo check 的 Hook（`check.sh`）。保留 cargo fmt 格式化 Hook + 文件保护 Hook + F2/F3 验证脚本。

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

> **file-guard.sh 说明**：PreToolUse Hook，防止 agent 修改源项目文件（`source_root` 下的文件）或关键产出物（如 `KNOWN_DIFFERENCES.md` 中已审批的条目）。脚本通过检查目标文件路径是否在保护范围内来决定是否阻止操作。

**F2 和 F3 的实现方式**：
- **F2（模块完成后验证）**：通过 `hooks/scripts/verify.sh` 独立脚本执行 `cargo nextest run --lib` + `cargo clippy -- -D warnings`。Skill SKILL.md 中指令调用该脚本，但脚本本身是确定性的，agent 无法修改或跳过其内部逻辑。
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
| `/migrate run` | `translator`(Phase A 忠实翻译) → `verifier`(对抗性审查) → `translator`(Phase B 优化) → `verifier`(测试验证) → 更新状态 | Rust 代码, 审查报告, 测试, MDR | Phase A/B 双阶段翻译 |
| `/migrate review` | `verifier`(全量验证) → 生成报告 → 更新 PARITY.md + 状态仪表板输出 | sprint-N-report.json, 终端仪表板 | 原 verify+status 合并 |
| `/migrate graduate` | `verifier`(毕业评估：覆盖率 + unsafe 审计 + 性能基准) → 生成毕业报告 | graduation-report.json, unsafe-audit.json | 原 graduate+unsafe-audit 合并 |

> **注意**：`/migrate analyze` 的 7 步序列（含 3 次 SubAgent 调用）是 M0 Spike 1 验证的主要对象——如果指令跟随不够可靠，此命令应拆为子步骤（Plan B1 微 Skill 链）。

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
- **M0 验证要求**：在 M0 Spike 1 中验证 Claude 能否可靠执行 `/migrate analyze` 的 7 步序列（含 3 次 SubAgent 串行调用）。如果指令跟随不够可靠，触发 Plan B。
- **检查点确定性**：SubAgent 间的编排检查点使用确定性文件存在性检查（脚本），不依赖 AI 判断产出物是否"有效"——由校验脚本负责。

**Plan B 具体方案**（M0 Spike 1 失败时触发）：

| 方案 | 实现方式 | 代价 | 用户体验退化 |
|------|---------|------|-------------|
| **Plan B1: 微 Skill 链** | 将 `/migrate analyze` 拆为 `/migrate init`、`/migrate plan`、`/migrate test` 等微命令，每个 Skill 只做 1 步（1 次 SubAgent 调用）。状态通过 `migration-state.json` 在微 Skill 间传递。用户手动或脚本串联。 | 额外 2-3 人天开发 | 用户需手动执行更多命令，但每步更可控 |
| **Plan B2: 外部脚本编排** | 用 bash/Python 脚本调用 Claude Code CLI（`claude -p "执行 /migrate ..."`），脚本中做 if-else 分支、文件检查、重试逻辑。编排逻辑 100% 确定性。 | 额外 3-5 人天开发 | 依赖 Claude Code CLI API 的稳定性；需用户安装额外依赖 |
| **Plan B3: 混合方案** | 简单步骤（1-2 步）用 SKILL.md 指令，复杂编排（3+ 步循环/条件）用外部脚本。取两者优势。 | 额外 2-4 人天开发 | 最可能的实际落地方案 |

**未来演进**：M2 阶段引入有限并行（analyzer + scaffolder 可并行），M4 阶段引入完整 DAG 调度。

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
├── porting/                   # 项目专有迁移规则
│   ├── dependency-mapping.md  # 项目特有的依赖映射
│   ├── business-logic-rules.md # 业务逻辑翻译策略
│   ├── known-workarounds.md   # 项目特有的 workaround
│   └── changelog.md           # 规则变更记录
├── context/                   # 项目知识（翻译经验沉淀）
│   ├── patterns/              # 项目特有的成功模式
│   ├── anti-patterns/         # 项目特有的失败教训
│   └── module-learnings/      # 模块级翻译经验
├── intermediate/              # 中间分析产物
│   ├── type-map.json          # 类型映射（M2 产出物，MVP 阶段类型映射信息在 porting/ 规则中）
│   └── attempts/              # 翻译尝试历史（断点续传用）
├── test-fixtures/             # 行为录制测试集
│   ├── golden/                # 黄金文件 (input/output 对)
│   ├── recordings/            # HTTP/CLI 录制
│   ├── proptest-regressions/  # proptest seed 记录
│   ├── fuzz-corpus/           # 模糊测试语料
│   └── benchmarks/            # 性能基线数据
├── decisions/                 # MDR 迁移决策记录（决策时立即写入）
│   ├── MDR-001-error-strategy.md
│   └── MDR-002-async-runtime.md
└── reports/                   # 验证报告
    ├── coverage.json
    ├── complexity-comparison.json
    ├── unsafe-audit.json
    └── sprint-N-report.json
```

> **与 Plugin 目录的关系**：`.rust-migration/` 是项目本地产出物目录（每个迁移项目独立），Plugin 目录（`rust-migrate-plugin/`）是分发给所有用户的通用工具包。核心规则内嵌在 Plugin 的 `agents/*.md` 中，参考指南放在 Plugin 的 `skills/migrate/references/` 下（见 10.1.1 规则分层策略）。

---

# 十一、工作流灵活性与扩展

## 11.1 .rustmigrate.toml 配置文件

```toml
[project]
name = "my-project"
source_language = "typescript"       # typescript | python | c | cpp | go
source_root = "./src"
rust_root = "./rust-src"
source_commit = "abc123"             # 锁定源码版本

# 排除不参与迁移的路径（glob 模式）
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
max_concurrent_agents = 3
max_retry_rounds = 3                 # 翻译失败最大重试轮数
degrade_strategy = "ffi"             # ffi | manual | skip

[tools]
tier0 = true                         # 硬性门禁（不可关闭）
tier1 = true                         # 推荐工具
tier2 = false                        # 高级工具（默认关闭）

[tools.tier2_override]
cargo_fuzz = false
cargo_mutants = false
miri = false
kani = false
loom = false
criterion = false                    # 默认 Tier 2；当 migration_motives 含 performance 时自动提升为 Tier 1

[testing]
coverage_threshold = 80              # 覆盖率门槛（百分比）
proptest_cases = 256                 # 属性测试用例数
fuzz_duration_secs = 60              # 模糊测试时长
benchmark_tolerance = 0.10           # 性能回归容忍度（10%）

[context]
max_tokens_per_translation = 100000  # 每次翻译上下文预算
module_summary_strategy = "interface_only"  # interface_only | full

[workspace]
cargo_workspace = true               # 使用 Cargo workspace
crate_naming = "kebab-case"          # 子 crate 命名风格
```

**行动指南**：`/migrate analyze` 自动根据项目画像生成初版配置，用户可手动调整。

---

## 11.2 语言扩展架构

设计为**目录约定 + JSON Schema 契约**的适配器模式。每种源语言对应一个适配器目录，包含检测、分析、模板等脚本和配置文件。适配器位于 Plugin 的 `skills/migrate/adapters/` 下。

### 目录约定

```
skills/migrate/adapters/
├── typescript/
│   ├── adapter.json            # 适配器元数据（JSON Schema 契约）
│   ├── detect.sh               # 检测项目是否使用此语言
│   ├── extract-types.sh        # 类型提取（调用 TS Compiler API）
│   ├── extract-deps.sh         # 依赖图提取（调用 dependency-cruiser）
│   ├── porting-template.md     # 迁移规则模板（语言专用，生成到 .rust-migration/porting/）
│   ├── ffi-bridge.sh           # FFI 桥接配置（napi-rs）
│   └── analysis-tools.json     # 语言专用工具列表
├── python/
│   ├── adapter.json
│   ├── detect.sh
│   ├── extract-types.sh        # 调用 Mypy
│   ├── extract-deps.sh         # 调用 import-linter + grimp
│   ├── porting-template.md
│   ├── ffi-bridge.sh           # PyO3 + maturin
│   └── analysis-tools.json
└── c_cpp/
    └── ...                     # bindgen + cbindgen
```

### adapter.json 契约（JSON Schema）

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "required": ["language_id", "display_name", "detect", "extract_types", "extract_deps"],
  "properties": {
    "language_id":    { "type": "string", "description": "语言标识，如 typescript / python / c_cpp" },
    "display_name":   { "type": "string", "description": "显示名称" },
    "detect":         { "type": "string", "description": "检测脚本路径（相对于适配器目录）" },
    "extract_types":  { "type": "string", "description": "类型提取脚本路径" },
    "extract_deps":   { "type": "string", "description": "依赖图提取脚本路径" },
    "porting_template": { "type": "string", "description": "PORTING 模板文件路径" },
    "ffi_bridge":     { "type": ["string", "null"], "description": "FFI 桥接脚本路径（可选）" },
    "analysis_tools": { "type": "string", "description": "分析工具配置文件路径" }
  }
}
```

### 逻辑接口参考

适配器的脚本需覆盖以下逻辑接口（对应原概念设计中的方法）：

| 逻辑接口 | 对应脚本 | 职责 |
|---------|---------|------|
| `language_id` | adapter.json 字段 | 语言标识 |
| `detect` | detect.sh | 检测项目是否使用此语言，返回 0/1 |
| `extract_types` | extract-types.sh | 提取类型信息（语言专用，不走统一 IR） |
| `extract_dependencies` | extract-deps.sh | 提取依赖图，输出统一 JSON 格式 |
| `porting_template` | porting-template.md | 该语言的 PORTING 预置规则 |
| `ffi_bridge` | ffi-bridge.sh | FFI 桥接工具配置 |
| `analysis_tools` | analysis-tools.json | 语言专用分析工具列表 |

### 适配器脚本调用链路

Skill 的 SKILL.md 通过 Claude Code 的 Bash tool 执行适配器目录下的 shell 脚本。调用链路如下：

```
Skill SKILL.md（分步指令）
  │
  ├── Step: "使用 Bash tool 执行 adapters/{language}/detect.sh"
  │     └── Bash tool → detect.sh → 返回 0（匹配）/ 1（不匹配）
  │
  ├── Step: "使用 Bash tool 执行 adapters/{language}/extract-types.sh <source_root>"
  │     └── Bash tool → extract-types.sh → 输出 type-map.json 到 intermediate/
  │
  ├── Step: "使用 Bash tool 执行 adapters/{language}/extract-deps.sh <source_root>"
  │     └── Bash tool → extract-deps.sh → 输出依赖 JSON → 合并到 source-graph.db
  │
  └── Step: "读取 adapters/{language}/porting-template.md 作为 PORTING 规则初始模板"
        └── Read tool → porting-template.md → 注入 translator SubAgent 上下文
```

脚本约定：
- 所有脚本接收项目根目录作为第一个参数（`$1`）
- 输出文件写入 `.rust-migration/intermediate/` 目录
- 脚本退出码 0 表示成功，非 0 表示失败（Skill 据此决定是否重试或报错）
- 脚本内部调用语言专用工具（如 `npx tsc`、`mypy`、`dependency-cruiser`），这些工具需在项目环境中预装

**MVP 支持**：TypeScript 适配器。
**后续迭代**：Python 适配器 → C/C++ 适配器 → Go 适配器。

每个适配器实现：
- 类型提取（TS Compiler API / Mypy / libclang）
- 依赖分析（dependency-cruiser / import-linter / 自建）
- PORTING 模板（语言专用规则预置）
- FFI 桥接（napi-rs / PyO3 / bindgen）

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

`.rustmigrate.toml` 中可以自定义验证管线的启用节点：

```toml
[pipeline]
# Tier 0（不可关闭）
cargo_check = true
clippy = true
cargo_test = true

# Tier 1（可关闭）
coverage = true
snapshot = true
property_test = true
complexity_check = true

# Tier 2（默认关闭）
fuzz = false
mutation = false
miri = false
formal = false
concurrency = false
```
