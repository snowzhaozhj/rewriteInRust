# /migrate analyze — 分析源码、生成规则、搭建测试基础设施

分析源仓库 → 生成项目画像 → 生成迁移规则 → 搭建黄金文件测试。这是迁移的起点。**顺序执行**下面每一步，检查点是确定性门禁，未通过不要进入下一步。共享约定（CLI 解析、SubAgent 校验、全局锁、失败恢复）见 [SKILL.md](./SKILL.md)。

## 前置条件
- 当前目录是源项目根，源项目可构建、可测试。
- 开始时取全局锁，第 10 步完成或异常退出时释放（见 SKILL.md「全局锁」）。

## 流程

### 1. 初始化
`.rust-migration/` 不存在则 `rustmigrate init`，创建该目录、`migration-state.json`（state=init）、项目根 `.rustmigrate.toml`。已存在则跳过（`init` 幂等）。

### 2. 检测项目类型
读目录结构识别源语言：检查 `package.json` / `tsconfig.json` / `pyproject.toml` / `go.mod` / `CMakeLists.txt`。检测不到标 `unknown`，不要猜。记录检测结果为 `<language_id>`（如 `typescript`、`python`、`c`），后续步骤依赖此值。

### 3. 项目画像（profile + 工具可用性检测）

运行 `rustmigrate profile --root . --adapter-tools <adapter_tools_path>`，其中 `<adapter_tools_path>` 指向语言适配器的 `analysis-tools.json`。

**自动定位 `analysis-tools.json`**（按优先级依次尝试，命中即停）：

1. **配置文件**：读 `.rustmigrate.toml`，若 `[project]` 段含 `adapter_path`，用 `<adapter_path>/analysis-tools.json`。
2. **环境变量**：`$CLAUDE_PLUGIN_ROOT` 非空时，用 `${CLAUDE_PLUGIN_ROOT}/skills/migrate/adapters/<language_id>/analysis-tools.json`。
3. **plugin 相对路径**：用 `plugin/skills/migrate/adapters/<language_id>/analysis-tools.json`（相对于 rustmigrate 仓库根，适用于 plugin 与 CLI 同仓部署）。
4. **全部未命中**：省略 `--adapter-tools`，`profile` 仍会检测 `cargo-nextest` 等 Tier 0 Rust 工具，仅跳过适配器工具检测（降级为 warning）。

**检查点**：`profile` 输出的 `data.tool_checks` 中必需工具（`required=true`）全部 `satisfies_min=true`；有 `ADAPTER_TOOL_MISSING` 或 `RUST_TOOL_MISSING` 警告时如实转达用户，不静默。

### 4. 构建源码图
`rustmigrate graph build --root ./src`（源码根按实际调整），tree-sitter 解析写入 `source-graph.db`。

**检查点**：`rustmigrate graph stats` 的节点/边非空；`migration-state.json` 的 `metadata.graph_build_completed == true`（CLI 在事务提交后才原子写入该字段）。任一不满足按 build 失败停止——后续步骤全部依赖这张图。

### 5. 语义分析（analyzer）
调 `rust-migrate:analyzer`（**前/后记 subagent_call**，见 SKILL.md「SubAgent 编排」，本步 step_index=5）：验证 `graph build` 产出的 calls 边、做复杂度与惯用法标注，产出画像摘要。MVP 不直写图（跨文件补边推迟 M2）。

**检查点**（按是否含函数节点分流）：读 `rustmigrate graph stats` 的 `nodes_by_type`。
- `nodes_by_type.function > 0`（项目含函数）→ 要求 `edges_by_type.calls > 0`，否则视为 calls 漏报、按失败处理。
- `nodes_by_type` 无 `function` 或 `function == 0`（纯类型定义 / 空文件，如 edge-cases fixture）→ `calls == 0` 是正确值而非漏报，**跳过本检查**，直接进下一步。

### 6. 拓扑排序 + 填充迁移序列
`rustmigrate graph topo-sort`：查看迁移排序与环路径（纯排序原语，有环返回退出码 2 + `data.cycle_path`）。**破环（MDR-004）：有环不再暂停拆环**——源码循环依赖由 populate 自动折叠为 composite 模块组（编译门禁单元；翻译粒度=单文件、契约+stub→逐文件填空→整组编译门，见 [translator.md](../../agents/translator.md)「SCC 模块组翻译」），无论有无环都直接继续落盘。

落盘迁移序列并推进状态机（**写状态统一走 CLI**）：
1. `rustmigrate state transition --to profile`
2. `rustmigrate state transition --to plan`
3. `rustmigrate state populate-modules [--root <源码根>]`——把 SCC 缩点后每个迁移单位写成 `modules[<组代表 NodeId>] = {status:pending, sprint:<缩点 DAG 层级>, tier:auto, member_files:<仅多文件组>}`，并设 `sprint.current=1`。`tier` 由 AST 语义特征自动评估（`trivial`/`standard`/`full`，composite 组取组内最高档），传 `--root` 与 `graph build` 一致以启用分档检测。单文件模块 key 用 NodeId 原值（如 `file:src/utils.ts`），composite 组 key 用组内字典序最小成员。**run 阶段依赖门禁用 `state deps <module>`（组感知，把组内成员的文件级依赖映射回组代表），不用 `graph deps`**。

**检查点**：state == `plan`、`data.module_count > 0`。

### 7. 生成迁移规则（translator）
调 `rust-migrate:translator`（**前/后记 subagent_call**，step_index=7），输入 `source-graph.db` + `adapters/<language_id>/porting-template.md`（Read 后注入上下文；`<language_id>` 来自步骤 2 的检测结果），产出 `.rust-migration/porting/`（dependency-mapping.md、core-rules.md 等）。

**定位 `porting-template.md`**：与步骤 3 定位 `analysis-tools.json` 相同的优先级策略，只是文件名换为 `porting-template.md`。

**检查点**：`porting/` 非空，至少一个 `.md` 大小 > 0 且含关键标题（如 `## 类型映射`）。

### 8. 生成辅助产出物
基于 analyzer 与 translator 产出，生成：
- `.rust-migration/PARITY.md`——初始进度表（Sprint 聚合视图 + 模块详情，等价深度 strong/stub）。
- `.rust-migration/AGENTS.md`——AI 行为约束，从模板生成并注入项目特有约束。

### 9. 搭建测试基础设施（scaffolder）
调 `rust-migrate:scaffolder`（**前/后记 subagent_call**，step_index=9），输入 `source-graph.db` + 模块接口（`rustmigrate graph interfaces <module>`），产出 `test-fixtures/golden/` 下的测试数据 + Cargo dev-dependencies。

**检查点**：`test-fixtures/golden/` 非空。

通过后推进到翻译循环就绪：
1. `rustmigrate state transition --to scaffold`
2. `rustmigrate state transition --to sprint_loop`

**检查点**：state == `sprint_loop`——`/migrate run` 的前置条件就绪。

### 10. 输出摘要
向用户展示项目画像：源语言、代码行数、模块数、依赖数、建议迁移策略。释放全局锁。

## 失败处理
任一 SubAgent 步骤（5 / 7 / 9）校验失败时，按 SKILL.md「失败恢复」三步处理（记录 → 诊断重试 → 降级/回滚）。`intermediate/attempts/*` 始终保留。
