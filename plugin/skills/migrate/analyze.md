# /migrate analyze — 分析源码、生成规则、搭建测试基础设施

分析源仓库 → 生成项目画像 → 生成迁移规则 → 搭建黄金文件测试。这是迁移的起点。**顺序执行**下面每一步，检查点是确定性门禁，未通过不要进入下一步。共享约定（CLI 解析、SubAgent 校验、全局锁、失败恢复）见 [SKILL.md](./SKILL.md)。

## 前置条件
- 当前目录是源项目根，源项目可构建、可测试。
- 开始时取全局锁，第 9 步完成或异常退出时释放（见 SKILL.md「全局锁」）。

## 流程

### 1. 初始化
`.rust-migration/` 不存在则 `rustmigrate init`，创建该目录、`migration-state.json`（state=init）、项目根 `.rustmigrate.toml`。已存在则跳过（`init` 幂等）。

### 2. 检测项目类型
读目录结构识别源语言：检查 `package.json` / `tsconfig.json` / `pyproject.toml` / `go.mod` / `CMakeLists.txt`。检测不到标 `unknown`，不要猜。

### 3. 构建源码图
`rustmigrate graph build --root ./src`（源码根按实际调整），tree-sitter 解析写入 `source-graph.db`。

**检查点**：`rustmigrate graph stats` 的节点/边非空；`migration-state.json` 的 `metadata.graph_build_completed == true`（CLI 在事务提交后才原子写入该字段）。任一不满足按 build 失败停止——后续步骤全部依赖这张图。

### 4. 语义分析（analyzer）
用 Agent tool 调 `rust-migrate:analyzer`：验证 `graph build` 产出的 calls 边、做复杂度与惯用法标注，产出画像摘要。MVP 不直写图（跨文件补边推迟 M2）。

**检查点**：`rustmigrate graph stats` 的 calls 边计数 > 0。

### 5. 拓扑排序 + 填充迁移序列
`rustmigrate graph topo-sort`：
- 退出码 0 → 继续。
- 有环（退出码 2、`data.kind == "cyclic_dependency"`）→ 输出 `data.cycle_path` 并**暂停**，请用户拆环后重跑：(a) 临时注释环上某条 import；(b) 人工把环内模块标降级。

无环则落盘迁移序列并推进状态机（**写状态统一走 CLI**）：
1. `rustmigrate state transition --to profile`
2. `rustmigrate state transition --to plan`
3. `rustmigrate state populate-modules`——把拓扑序每个文件模块写成 `modules[<NodeId>] = {status:pending, sprint:1, risk:low}`，并设 `sprint.current=1`。**module key 用 NodeId 原值**（如 `file:src/utils.ts`），与 `graph deps` 输出一致，run 阶段依赖门禁据此查 `modules[dep].status`，不可改写。

**检查点**：state == `plan`、`data.module_count > 0`。

### 6. 生成迁移规则（translator）
用 Agent tool 调 `rust-migrate:translator`，输入 `source-graph.db` + `adapters/<language>/porting-template.md`（Read 后注入上下文），产出 `.rust-migration/porting/`（dependency-mapping.md、core-rules.md 等）。

**检查点**：`porting/` 非空，至少一个 `.md` 大小 > 0 且含关键标题（如 `## 类型映射`）。

### 7. 生成辅助产出物
基于 analyzer 与 translator 产出，生成：
- `.rust-migration/PARITY.md`——初始进度表（Sprint 聚合视图 + 模块详情，等价深度 strong/stub）。
- `.rust-migration/AGENTS.md`——AI 行为约束，从模板生成并注入项目特有约束。

### 8. 搭建测试基础设施（scaffolder）
用 Agent tool 调 `rust-migrate:scaffolder`，输入 `source-graph.db` + 模块接口（`rustmigrate graph interfaces <module>`），产出 `test-fixtures/golden/` 下的测试数据 + Cargo dev-dependencies。

**检查点**：`test-fixtures/golden/` 非空。

通过后推进到翻译循环就绪：
1. `rustmigrate state transition --to scaffold`
2. `rustmigrate state transition --to sprint_loop`

**检查点**：state == `sprint_loop`——`/migrate run` 的前置条件就绪。

### 9. 输出摘要
向用户展示项目画像：源语言、代码行数、模块数、依赖数、建议迁移策略。释放全局锁。

## 失败处理
任一 SubAgent 步骤（4 / 6 / 8）校验失败时，按 SKILL.md「失败恢复」三步处理（记录 → 诊断重试 → 降级/回滚）。`intermediate/attempts/*` 始终保留。
