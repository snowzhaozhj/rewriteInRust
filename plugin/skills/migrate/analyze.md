# /migrate analyze — 分析源码、生成规则、搭建测试基础设施

合并原 init + plan + test：分析源仓库、生成项目画像、生成迁移规则、搭建测试基础设施。这是迁移的起点，按下面的分步指令**顺序执行**——每步的检查点是确定性门禁，未通过不要进入下一步。

> 权威骨架：`docs/design/09-appendix-schemas.md` 附录 B；编排序列：`06-plugin-structure.md` §10.5（`analyzer → translator(规则) → scaffolder`，3 次串行 SubAgent 调用）。共享约定（CLI JSON 解析、SubAgent 校验、失败恢复）见 [SKILL.md](./SKILL.md)。

## 前置条件
- 当前目录是源项目根目录，源项目可构建、可测试。

## 分步指令

### Step -1: Bootstrap（幂等初始化）
若 `.rust-migration/` 不存在，执行 `rustmigrate init`——创建 `.rust-migration/` 目录、初始 `migration-state.json`（state=init）、项目根 `.rustmigrate.toml`。目录已存在则跳过（`init` 本身幂等，重复执行安全）。

### Step 0: 全局锁获取与陈旧锁恢复（所有 /migrate 命令通用，本处为权威定义）
> 为何用内容锁而非 flock：Claude Code 每次 Bash 调用是独立短命进程，flock 的 advisory lock 随进程退出即释放，无法跨多次调用维持。改用磁盘上持续存在的**内容锁**，记录会话宿主 PID 来判断有效性。

锁文件 `.rust-migration/.migration-lock` 内容 = 单行 JSON `{session_pid, started_at(ISO8601), hostname}`，其中 `session_pid = $PPID`（Claude Code 宿主进程 PID，生命周期覆盖整个会话）。

**获取**（原子创建，不依赖 flock）：
1. 写 JSON 到临时文件 `.rust-migration/.migration-lock.tmp.$$`
2. `link(.tmp.$$, .migration-lock)`——link 失败即锁已存在（等效 O_EXCL 且保证有内容）
3. `unlink(.tmp.$$)`
- link 成功 → 获锁完成，继续。
- link 失败 → 进入**陈旧检测**。

**陈旧检测**（仅 link 失败时）：读锁文件 JSON——
- 同机（hostname 匹配）且 `ps -p <session_pid>` 显示进程已死 → 陈旧锁，删除后重试一次获取。
- 同机、进程仍活、`lock.session_pid == 当前 $PPID`（同会话）→ 视为陈旧（同会话命令严格串行，当前能执行即证明前命令已结束），删锁并警告「检测到同会话残留锁，已自动清除」后重试一次。
- 同机、进程仍活、`lock.session_pid != 当前 $PPID`（不同会话）→ 真实并发，报错退出，不删锁、不退避；兜底：`now - started_at > lock_timeout_secs` 时视为陈旧并警告后删除。
- 跨机或 PID 不可判定：`now - started_at > lock_timeout_secs`（默认 300）→ 视为陈旧，提示用户确认无进行中任务后手动删除；否则按真实并发报错退出。

**释放**：Step 7 完成或命令异常退出时 `unlink .rust-migration/.migration-lock`。

> 逃生口：卡死时用户可手动删除 `.rust-migration/.migration-lock`。报错信息须含这一提示。

### Step 1: 检测项目类型
读取目录文件结构识别源语言与框架。检查这些标志文件是否存在：`package.json`、`tsconfig.json`、`pyproject.toml`、`go.mod`、`CMakeLists.txt`。检测不到时标 `unknown`，不要猜测。

### Step 2: 构建源码图
执行 `rustmigrate graph build --root ./src`（源码根按实际调整）。CLI 用 tree-sitter 解析，构建基础图（contains/imports 边）写入 `.rust-migration/source-graph.db`（SQLite）。

**检查点**：执行 `rustmigrate graph stats` 确认 `data` 中节点/边数量合理（非空、非全零）。失败则报告错误并停止——后续步骤都依赖这张图。

### Step 2.5: 确认 graph build 已释放数据库（前置门禁）
启动 analyzer 前，确定性确认 CLI `graph build` 已提交事务并释放 DB 写锁，避免 analyzer 与 CLI 同时持连接。用 Bash + jq 读 `migration-state.json`：

```
COMPLETED = .metadata.graph_build_completed
- COMPLETED == true  → 通过门禁，进入语义增强。
- COMPLETED != true  → graph build 未正常提交（CLI 在事务 COMMIT 后才原子写入该字段），
                       按 graph build 失败处理：报错停止，提示用户检查日志后重新执行
                       `rustmigrate graph build`（不进入 analyzer，不轮询等待）。
```

> 为何无需轮询：该字段由 CLI 在 COMMIT 后原子写入，MVP 串行下 Step 2 的 Bash 调用返回后必然已落盘，单次读取判定即可。`graph build` 是覆盖式重建，重跑安全。这属于**前置条件验证**，不走 `max_retries_per_step` 重试。

门禁通过后，用 Agent tool 调用 **analyzer** SubAgent 做语义增强（补 calls/uses_type 边、复杂度标注）。
**检查点**：analyzer 返回后，确认 `source-graph.db` 含 calls/uses_type 边（L3 语义校验，M1 可人工 sampling）。

### Step 2.8: 拓扑排序探测（循环依赖前置门禁）
生成初始状态前执行 `rustmigrate graph topo-sort`，按退出码分支：
- 退出码 0 → 排序成功，继续 Step 3。
- 非零退出 → 解析诊断 JSON：
  - `error == "graph_truncated"`（增量构建触发断路器，图不完整）→ 提示用户执行 `rustmigrate graph build --full` 后重跑 `/migrate analyze`，不进入 Step 3。
  - 输出含 `cycle_path`（检测到环）→ 输出环路径并**暂停**，提示用户二选一打破循环：(a) 在源项目临时注释环上某条 import 后重跑 `/migrate analyze`；(b) 将环内模块标记 `requires_manual_review` 降级，人工拆环后再迁移。

> 完整 SCC 检测见 M2 `graph cycles`；此处只做 MVP 轻量门禁，防止有环图进入后续翻译。

### Step 3: 更新迁移状态
基于 analyzer 产出，更新 `migration-state.json`：state 从 init 转为 profile，填充 project/modules/sprint 字段；保留 Step 2 已写入的 `metadata.graph_build_completed`。
**检查点**：`migration-state.json` 的 state 字段为 `"profile"`。

### Step 4: 调用 translator SubAgent（规则生成）
用 Agent tool 调用 **translator** 生成项目专有迁移规则。
- 输入：`source-graph.db` 图数据 + 语言适配器 `porting-template.md`（Read `adapters/{language}/porting-template.md` 注入 translator 上下文）。
- 产出：`.rust-migration/porting/` 目录（dependency-mapping.md、core-rules.md 等）。

**检查点（L1）**：`porting/` 存在、非空、至少一个 `.md` 规则文件大小 > 0 且含关键标题（如 `## 类型映射`）。

### Step 5: 生成辅助产出物
基于 analyzer 与 translator 产出，生成：
- `.rust-migration/PARITY.md`（初始进度表：Sprint 聚合视图 + 模块详情表，等价深度用 strong/stub）。
- `.rust-migration/AGENTS.md`（AI 行为约束，从模板生成，注入项目特有约束）。

### Step 6: 调用 scaffolder SubAgent（测试基础设施）
用 Agent tool 调用 **scaffolder** 搭建黄金文件测试基础设施。
- 输入：`source-graph.db` + 模块接口信息（`rustmigrate graph interfaces <module>`）。
- 产出：`.rust-migration/test-fixtures/golden/` 下的测试数据 + Cargo.toml dev-deps 注入。

**检查点（L1）**：`test-fixtures/golden/` 存在且非空。

### Step 7: 输出摘要
向用户展示项目画像摘要：源语言、代码行数、模块数、依赖数、建议迁移策略。释放全局锁（Step 0），提示用户下一步执行 `/migrate run`。

## 失败处理
任一 SubAgent 调用（Step 2.5/4/6）校验失败时，按 [SKILL.md](./SKILL.md) 的失败恢复三步处理（记录→诊断重试→降级/回滚）。回滚清理范围见 `09-appendix-schemas.md` 附录 B「关键检查点的失败恢复规则」表；`intermediate/attempts/*` 始终保留。
