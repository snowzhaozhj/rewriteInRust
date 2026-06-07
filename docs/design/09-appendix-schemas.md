# 附录：Schema 与 SKILL.md 骨架

> [返回主索引](./README.md)

---

## 附录 A：migration-state.json Schema

> 完整 JSON Schema 在 M1 阶段补充。以下为结构示例和状态枚举定义。
> 独立 JSON 示例文件见 [`schemas/migration-state.example.json`](./schemas/migration-state.example.json)

```json
{
  "version": "0.9",
  "state": "sprint_loop",
  "state_history": [
    {
      "state": "init",
      "entered_at": "2026-06-06T10:00:00Z",
      "exited_at": "2026-06-06T10:05:00Z"
    },
    {
      "state": "profile",
      "entered_at": "2026-06-06T10:05:00Z",
      "exited_at": "2026-06-06T11:00:00Z"
    },
    {
      "state": "plan",
      "entered_at": "2026-06-06T11:00:00Z",
      "exited_at": "2026-06-06T14:00:00Z"
    },
    {
      "state": "scaffold",
      "entered_at": "2026-06-06T14:00:00Z",
      "exited_at": "2026-06-06T16:00:00Z"
    },
    {
      "state": "sprint_loop",
      "entered_at": "2026-06-06T16:00:00Z",
      "exited_at": null
    }
  ],
  "project": {
    "name": "my-project",
    "source_language": "typescript",
    "source_commit": "abc123",
    "source_loc": 15000,
    "created_at": "2026-06-06T10:00:00Z"
  },
  "sprint": {
    "current": 2,
    "history": [
      {
        "id": 1,
        "started_at": "2026-06-06T10:00:00Z",
        "completed_at": "2026-06-13T18:00:00Z",
        "target_modules": ["utils/string", "utils/math"],
        "completed_modules": ["utils/string", "utils/math"],
        "porting_md_version": "1.2",
        "notes": "首个 Sprint，规则追加了整数溢出处理"
      }
    ]
  },
  "modules": {
    "utils/string": {
      "status": "done",
      "substatus": null,
      "sprint": 1,
      "attempts": [
        {
          "timestamp": "2026-06-07T14:00:00Z",
          "result": "success",
          "retry_count": 1,
          "checkpoint": "intermediate/attempts/utils-string-001.json"
        }
      ],
      "test_pass_rate": "24/24",
      "coverage": 91,
      "known_differences": 0,
      "risk": "low"
    },
    "core/parser": {
      "status": "testing",
      "substatus": "proptest_failing",
      "sprint": 2,
      "attempts": [
        {
          "timestamp": "2026-06-14T09:00:00Z",
          "result": "partial",
          "retry_count": 2,
          "checkpoint": "intermediate/attempts/core-parser-002.json"
        }
      ],
      "test_pass_rate": "18/22",
      "coverage": 76,
      "known_differences": 1,
      "risk": "medium",
      "phase_a_version": "sha256:a1b2c3d4",
      "phase_a_audit_passed": true
    },
    "core/runtime": {
      "status": "blocked",
      "substatus": "waiting_for_core/parser_testing_complete",
      "sprint": 2,
      "blocked_by": ["core/parser"],
      "pre_blocked_status": "pending",
      "attempts": [],
      "test_pass_rate": null,
      "coverage": null,
      "known_differences": 0,
      "risk": "high"
    }
  },
  "config_ref": ".rustmigrate.toml",
  "subagent_calls": [
    { "step_index": 1, "subagent_name": "translator", "started_at": "2026-06-14T09:05:00Z", "ended_at": "2026-06-14T09:08:30Z", "status": "success", "error_message": null }
  ],
  "metadata": {
    "graph_build_completed": true,
    "graph_build_completed_at": "2026-06-06T16:05:00Z",
    "last_error": null,
    "lock_token": null
  }
}
```

**`metadata` 字段说明**：

| 字段 | 类型 | 含义 |
|------|------|------|
| `graph_build_completed` | `boolean` | CLI `rustmigrate graph build` 是否已**提交事务并释放 DB**。语义明确为「`graph build` 进程的 `BEGIN IMMEDIATE TRANSACTION ... COMMIT` 已提交、SQLite 连接已关闭」——即 DB 写锁已释放，下游 analyzer SubAgent 可安全获取只读连接。该字段由 `graph build` 在事务提交后写入（CLI 本身持有事务，写入是确定性的，不依赖外部轮询）。 |
| `graph_build_completed_at` | `string \| null` | 上述提交完成时间（ISO 8601），用于诊断时序问题 |
| `last_error` | `string \| null` | 最近一次 run 中止的结构化错误原因（如 Step 0.5 检测到循环依赖时记录环路径），便于用户排查；正常运行为 `null` |
| `lock_token` | `string \| null` | MVP 恒为 `null`；M2 用于分布式锁令牌（见 [06 § 10.5](./06-plugin-structure.md#105-编排调度路径)） |

**`subagent_calls` 字段说明**：顶层 append-only 数组，每次 SubAgent 调用（含重试）追加一条 `{step_index, subagent_name, started_at, ended_at, status, error_message}`，用于诊断卡死与统计重试次数（见 [06 § 10.5](./06-plugin-structure.md#105-编排调度路径)）。

### 状态机概念名 → JSON 字段值映射

| 状态机图（[见架构设计 > 状态机](./02-architecture.md#34-编排器状态机设计)） | migration-state.json `status` 值 | 说明 |
|-------------------|----------------------------------|------|
| TRANSLATE | `translating` | 翻译中 |
| CHECK | `compile_fixing` | 编译修复中 |
| VERIFY | `testing` + `reviewing` | 测试 → 最终签批两个子步骤 |
| PAUSE | `paused` | 暂停等待人类降级决策 |
| DEGRADE | `degrade_ffi` / `degrade_manual` / `degrade_skip` | 三种降级方式 |
| GRADUATE | `done` | 完成（项目级毕业由 `/migrate graduate` 评估） |

### 模块级状态枚举

每个模块在 `migration-state.json` 的 `modules[].status` 字段使用以下状态值：

| 状态 | 含义 | 说明 |
|------|------|------|
| `pending` | 未开始 | 模块已识别但尚未开始迁移 |
| `translating` | 翻译中 | translator SubAgent 正在生成 Rust 代码 |
| `compile_fixing` | 编译修复中 | F1 反馈循环中，正在修复编译错误 |
| `testing` | 测试验证中 | F2 阶段，verifier SubAgent 正在生成和运行测试 |
| `reviewing` | 最终签批中 | 测试通过后执行 TODO(port) 清零检查与最终验收签批 |
| `done` | 完成 | 翻译和验证全部通过 |
| `degrade_ffi` | 降级为 FFI 桥接 | 翻译失败，保持原实现，Rust 端通过 FFI 调用 |
| `degrade_manual` | 降级为人工处理 | 翻译失败，标记 TODO 等待人工处理 |
| `degrade_skip` | 降级为功能裁剪 | 协商后移除该功能 |
| `paused` | 暂停等待人类决策 | 翻译/测试多轮失败，暂停等待人类确认降级方式 |
| `blocked` | 被依赖阻塞 | 依赖的模块尚未完成迁移，无法开始 |

**substatus 字段说明**：

每个模块除 `status` 外，还有一个可选的 `substatus` 字段（自由文本，`string | null`）。`substatus` 用于描述当前模块在该状态下的具体阻塞原因或进展细节，方便排查和状态报告。示例值：

| status | substatus 示例 | 含义 |
|--------|---------------|------|
| `testing` | `"proptest_failing"` | proptest 用例未通过 |
| `compile_fixing` | `"lifetime_error_in_parse_fn"` | `parse` 函数存在生命周期错误 |
| `paused` | `"3_rounds_failed_awaiting_degrade_decision"` | 3 轮翻译失败，等待人类选择降级方式 |
| `blocked` | `"waiting_for_core/config_ffi_decision"` | 等待 core/config 模块 FFI 方案落定 |
| `degrade_manual` | `"async_cancellation_too_complex"` | 异步取消逻辑过于复杂，需人工处理 |
| `translating` | `"phase_a_complete_awaiting_review"` | Phase A 忠实翻译完成，待 verifier 对抗审查（[03 § 4.3](./03-execution-model.md#43-内循环模块级单会话内-phase-ab-双阶段翻译) Step 4） |
| `translating` | `"phase_b_optimization_in_progress"` | Phase B 惯用化优化进行中（Step 5） |
| `translating` | `"phase_b_failed_at_round_N"` | Phase B 第 N 轮编译修正失败，已持久化部分状态供续传（见下方断点续传） |
| `done` | `null` | 无需额外说明 |

`substatus` 无枚举约束，由 SubAgent 或人工自行填写，仅用于辅助沟通，不参与状态机流转判断。

**Phase A 解耦字段**（用于 verifier 责任归因，见 [02 § 3.2.4](./02-architecture.md#324-subagent-合并7--4)）：

| 字段 | 类型 | 含义 |
|------|------|------|
| `phase_a_version` | `string \| null` | 当前 Phase A 持久化文件（`intermediate/attempts/{module}-phase-a.rs`）的内容 hash；未进入翻译时为 `null` |
| `phase_a_audit_passed` | `boolean \| null` | Step 3.5 结构校验结果（`true`/`false`）；未校验时为 `null` |

> **Phase A/B 子步骤的 substatus 约定**：02-architecture.md § 3.4 注明「Phase A/B 是 TRANSLATE 状态内的内部子步骤，不占用状态机节点」。MVP 通过上述 `translating` 的 substatus（`phase_a_complete_awaiting_review` / `phase_b_optimization_in_progress` / `phase_b_failed_at_round_N`）在 `migration-state.json` 中表达 Phase 级进度，使中间崩溃可定位到具体 Phase 而无需新增状态机节点。Phase A 完成时间戳记录在模块的 `attempts[].timestamp`，恢复逻辑据此判断哪个 Phase 失败。

### 合法状态转换

```
pending → translating → compile_fixing → testing → reviewing → done
                                    ↓                         → degrade_ffi
                                    ↓                         → degrade_manual
                                    ↓                         → degrade_skip
                              compile_fixing（3轮失败）→ paused → degrade_*（人类确认）
                              testing（不可修复）→ paused → degrade_*（人类确认）
                                                  paused → translating（人类选择重试）

blocked 可从任何状态进入（依赖模块降级或阻塞时触发）
blocked → {原状态}（阻塞解除后恢复到进入 blocked 前的状态）

degrade_* → translating（通过 /migrate run --module=X --force 恢复）
```

**到达 `done` 的前置条件**：除测试通过率 ≥ 预期、clippy 无 warning 外，该模块的 `TODO(port)` 计数须 = 0（由 verifier 在 [附录 B § /migrate run Step 5 TODO(port) 检查点](#附录-bmvp-skill-的-skillmd-骨架)保证）；不满足则标记 incomplete，停留在 `reviewing`/`testing` 而非进入 `done`。

**blocked 状态处理**：
- 当模块 A 依赖的模块 B 降级或阻塞时，A 进入 `blocked` 状态
- `migration-state.json` 中记录 `blocked_by` 字段和 `pre_blocked_status` 字段（进入 blocked 前的状态）
  - **`blocked_by` 是字符串数组**（一个模块可能同时被多个上游模块阻塞），见上文示例 `"blocked_by": ["core/parser"]`
- 阻塞解除后（B 全部进入 `done` 或 `degrade_*`），A 恢复到 `pre_blocked_status`

**检测与恢复的责任边界（避免永久阻塞）**：

| 事件 | MVP（M0-M1）由谁负责 | M2+ 由谁负责 |
|------|----------------------|--------------|
| 进入 blocked / 填充 `blocked_by` | `/migrate run` 的前置检查（见 [附录 B](#附录-bmvp-skill-的-skillmd-骨架) Step 0.5），手动填充，**MVP 不自动持续扫描** | `rustmigrate validate state` 程序化检测并更新 |
| 解除 blocked / 恢复 `pre_blocked_status` | `/migrate run` 执行前的 Step 0.5 检查点：遍历所有 `blocked` 模块，若其 `blocked_by` 全部进入 `done`/`degrade_*` 则恢复 | `rustmigrate validate state` 自动拓扑解除 |

> **注（多模块同时 blocked）**：MVP 不做自动拓扑排序。若 A→B→C 三者均 blocked，用户应按依赖关系**逐层手动恢复**（先解 C，再解 B，最后 A）——每次 `/migrate run` 的 Step 0.5 只解除「`blocked_by` 已全部完成」的那一层，故连续运行会自然逐层推进。完整自动拓扑排序在 M2 阶段实现。

> **注（与降级级联的区别）**：本节描述的是「上游完成 → 下游解除 blocked」的**恢复**问题。另有「上游降级为 FFI → 下游需感知接口类型变化」的**级联**问题，二者不同，后者见 [02-architecture.md § 3.4「降级后依赖级联影响」](./02-architecture.md#34-编排器状态机设计)。

> **注（`pre_blocked_status` 失效场景）**：若 B 降级后代码发生重构、B→A 依赖关系本身变更，则 `pre_blocked_status` 可能不再准确。MVP 的处理是：恢复后该模块仍会走完整的 `/migrate run` 流程重新验证，故即使 `pre_blocked_status` 偏旧也不会产出错误结果（最坏情况是多跑一次翻译/验证）。

---

## 附录 B：MVP Skill 的 SKILL.md 骨架

以下为 `/migrate analyze` 和 `/migrate run` 的 SKILL.md 骨架结构示例，展示分步指令格式、上下文加载、SubAgent 调用和检查点的编写方式。

### /migrate analyze SKILL.md 骨架

```markdown
# /migrate analyze — 分析源码、生成规则、搭建测试基础设施

## 前置条件
- 当前目录是源项目根目录
- 源项目可构建、可测试

## 分步指令

### Step 0: 全局锁获取与陈旧锁恢复（所有 /migrate 命令通用）
> 本 Step 0 为 `/migrate analyze` 与 `/migrate run` 共用；`/migrate run` 骨架不重复，引用此处。被 [06 § 10.5「多终端并发冲突的强制隔离」](./06-plugin-structure.md#105-编排调度路径) 引用为权威骨架。

```
锁文件 .rust-migration/.migration-lock 内容 = 单行 JSON {pid, started_at(ISO8601), hostname}

[获取] flock -n .rust-migration/.migration-lock --  # 非阻塞，进程退出时 OS 自动释放 FD
  - 成功 → 写入本进程 {pid, started_at, hostname}，继续
  - 失败（锁被占）→ 进入[陈旧检测]

[陈旧检测]（仅在非阻塞获取失败时执行）
  读取锁文件 JSON：
  - 同机（hostname 匹配）且 ps -p <pid> 显示进程已死 → 陈旧锁，删除后重试一次获取
  - 进程仍活 → 真实并发，报错退出（见下方错误信息），不删锁、不退避重试
  - 跨机 或 PID 不可判定：若 now - started_at > lock_timeout_secs（默认 3600）→ 视为陈旧，
    报错并提示用户确认无进行中任务后手动删除；否则按真实并发报错退出
```

> **平台差异与逃生口**：`flock` 在 macOS/Linux 上进程崩溃时由 OS 关闭 FD 自动释放，故正常崩溃**无需**人工清理；`lock_timeout_secs` 仅用于「锁文件残留但 flock 已释放」或跨机场景的兜底判定。用户卡死时的逃生口为**手动删除** `.rust-migration/.migration-lock`（MVP 不新增 CLI 清理子命令以守 11 命令边界）。错误信息须含「如确认无进行中任务，可手动删除 .rust-migration/.migration-lock」。

### Step 1: 检测项目类型
读取当前目录的文件结构，识别源语言和框架。
检查以下文件是否存在：package.json, tsconfig.json, pyproject.toml, go.mod, CMakeLists.txt。

### Step 2: 构建源码图
使用 Bash tool 执行：`rustmigrate graph build --root ./src --format json`
CLI 构建基础图（contains/imports 边），存储到 `.rust-migration/source-graph.db`（SQLite）。

**检查点**：验证 source-graph.db 存在。执行 `rustmigrate graph stats` 确认节点/边数量合理。
如果验证失败，报告错误并停止。

### Step 2.5: 确认 graph build 已释放数据库（前置门禁）
在启动 analyzer SubAgent 前，**确定性**确认 CLI `graph build` 已提交事务并释放 DB 写锁，避免 analyzer 与 CLI 同时持有连接：

```
使用 Bash + jq 读取 migration-state.json：
  COMPLETED = .metadata.graph_build_completed

判定：
  - COMPLETED == true  → 通过门禁，进入下一指令段
  - COMPLETED != true  → graph build 未正常提交（CLI 在事务提交后才会写入该字段，
                          见附录 A「metadata 字段说明」），属于 graph build 失败，
                          报告错误并停止，提示用户检查 graph build 日志后重新执行
                          `rustmigrate graph build`（不进入 analyzer，不轮询等待）
```

> **为何无需轮询/超时**：`graph_build_completed` 由 CLI `graph build` 在 `COMMIT` 后、进程退出前用 § 10.8 原子写入（CLI 自身持有事务，写入是确定性的）。MVP 串行执行下，Step 2 的 `graph build` Bash 调用返回后该字段必然已落盘，故此处只需**单次读取判定**，不存在 busy-loop 或死锁等待。若读到 `false/缺失`，说明 `graph build` 进程异常退出（事务未提交，或罕见的「DB 已 COMMIT 但标志写入前崩溃」窗口）——两种情况都按错误处理：直接重新执行 `rustmigrate graph build`（其 `BEGIN IMMEDIATE...COMMIT` 写入是覆盖式重建，重跑安全，无需新增 reset 子命令）。graph build 在「已提交后重跑」的幂等性须在 M0 Spike 0 验证并记入 `DESIGN_ASSUMPTIONS.md`。M2 引入真正并行后，若 Spike 1 暴露时序问题，再升级为 `rustmigrate validate state --check-graph-consistency` 显式校验命令（见 [08-roadmap-and-reference.md](./08-roadmap-and-reference.md)）。

然后调用 analyzer SubAgent 做语义增强（补充 calls/uses_type 边、复杂度标注）。

### Step 2.8: 拓扑排序探测（循环依赖前置门禁）
在生成初始状态前执行 `rustmigrate graph topo-sort`，检查返回值：
- 退出码 0 → 排序成功，继续 Step 3。
- 非零退出（检测到循环依赖）→ 输出环路径并**暂停**，提示用户二选一打破循环：
  (a) 在源项目中临时注释掉环上的某条 import，重新执行 `/migrate analyze`；
  (b) 跳过环内模块——将其标记为 `requires_manual_review` 降级（见 [附录 A § 合法状态转换](#合法状态转换)），由人工拆环后再迁移。
完整 SCC 环检测见 M2 `rustmigrate graph cycles`（降级策略见 [03 § 4.2 循环依赖处理](./03-execution-model.md#42-外循环sprint-级跨会话天周)）。

### Step 3: 生成初始状态
基于 analyzer 的产出物，生成以下文件：
- `.rust-migration/migration-state.json`（初始状态：PROFILE）
- `.rustmigrate.toml`（默认配置，项目根目录）

**检查点**：验证 migration-state.json 的 state 字段为 "profile"。

### Step 4: 调用 translator SubAgent（规则生成）
使用 translator SubAgent 生成迁移规则初始内容。
输入：source-graph.db 图数据 + 语言适配器的 porting-template.md。
等待产出物：`.rust-migration/porting/` 目录（含 dependency-mapping.md、core-rules.md 等规则文件）。

**检查点**：验证 `.rust-migration/porting/` 目录存在且包含至少一个规则文件。

### Step 5: 生成辅助产出物
基于 analyzer 和 translator 的产出物，生成以下文件：
- `.rust-migration/PARITY.md`（初始进度表）
- `.rust-migration/AGENTS.md`（AI 行为约束，从模板生成）

### Step 6: 调用 scaffolder SubAgent（测试基础设施搭建）
使用 scaffolder SubAgent 搭建黄金文件测试基础设施。
输入：source-graph.db 图数据 + 模块接口信息。
等待产出物：`.rust-migration/test-fixtures/golden/` 目录下的测试数据。

**检查点**：验证 test-fixtures/golden/ 目录存在且非空。

### Step 7: 输出摘要
向用户展示项目画像摘要：源语言、代码行数、模块数、依赖数、建议的迁移策略。
提示用户下一步执行 `/migrate run`。
```

### /migrate run SKILL.md 骨架

```markdown
# /migrate run — 执行模块迁移

## 前置条件
- `.rust-migration/migration-state.json` 存在且 state 为 "sprint_loop"
- `.rust-migration/porting/` 目录存在且包含规则文件
- 目标模块已在 Sprint 计划中

## 上下文加载
1. 读取 `migration-state.json`，确认当前 Sprint 和目标模块
2. 读取 `.rust-migration/porting/` 目录中与目标模块相关的迁移规则（按模块类型筛选）
3. 读取目标模块源码
4. 读取依赖模块的接口签名（仅接口，不含实现）

> **单 Skill 可行性 / token 成本预估**：调用 translator 前先按「源码大小 + 相关规则 + 依赖接口（interface_only）→ ≤ 100K tokens」预估单次 Work Unit 的 context 预算，> 95K 提示拆分、> 100K 走降级路径——映射公式、各成分 token 量级与降级行为以 [02-architecture.md § 3.5.1 上下文预算运行时检查与拆分策略](./02-architecture.md#351-上下文预算运行时检查与拆分策略) 为权威。本骨架的每个 `**检查点**` 即为「指令段 → 文件存在性检查点 → 下一指令段」的实例编码：单步检查点失败按 [06 § 10.5](./06-plugin-structure.md#105-编排调度路径) 的 `max_retries_per_step` 重试，不回滚已通过的前序步骤。

## 分步指令

### Step 0.5: 自动解除可解除的 blocked 模块
读取 migration-state.json 中所有 status='blocked' 的模块，逐个检查其阻塞源是否已解决：

```
[前置] 循环依赖检测（防止 blocked 模块互相等待导致死锁）：
  构建有向子图 G：仅含当前 status='blocked' 的模块为节点，
                 对每个 blocked 模块 M，为 M → (M.blocked_by 中仍为 'blocked' 的模块) 连边
  对 G 执行环检测（MVP 用一次 DFS 着色法即可，无需完整 Kosaraju/Tarjan SCC）
  若检测到环（含自依赖 A→A）：
    - 报错并中止本次 run，输出具体环路径，例如：
      "循环依赖检测：A blocked_by B, B blocked_by C, C blocked_by A
       — 这些模块互相阻塞，无法自动恢复。请修正 migration-state.json 的
       blocked_by 字段（删除误配的依赖），或用
       `/migrate run --module=X --degrade=skip` 将环中某模块降级为 skip 以打破循环。"
    - 将错误原因记入 migration-state.json 的 metadata.last_error 字段
    - 不进入下方逐个恢复逻辑

对每个 blocked 模块 M：
  读取 M.blocked_by（字符串数组）
  查询 blocked_by 中每个模块在 migration-state.json 中的当前 status
  若所有阻塞源都已进入 'done' 或 'degrade_*'：
    - 将 M.status 恢复为 M.pre_blocked_status
    - 写入日志："解除对 module M 的阻塞：所有阻塞源已解决"
    - 通过 `rustmigrate state transition --module <M> --to <pre_blocked_status> --reason 'blocked_by resolved'` 写回状态（确保 tmp-fsync-rename 原子写入）
  否则：
    - 保持 blocked 状态，将当前未完成的阻塞源记入 M.substatus
```

> MVP 不做自动拓扑排序：A→B→C 多层 blocked 时，本步骤只解除「blocked_by 全部完成」的一层，连续运行会逐层推进（见 [附录 A § 合法状态转换「多模块同时 blocked」注](#合法状态转换)）。但**环形**阻塞（A→B→C→A 或自依赖）无法靠逐层推进解除，故 Step 0.5 在恢复前先做一次 DFS 环检测并报错中止，避免静默死锁。`metadata.last_error` 字段见 [附录 A「metadata 字段说明」](#附录-amigration-statejson-schema)。

> **MVP 实现归属与确定性边界**：上述伪码在 MVP 期由 SKILL.md 通过指令跟随执行（非独立确定性脚本），与 L1/L2 校验的确定性存在割裂——这是 MVP 的已知约束，**不在 M2 之前补 CLI 化**。完整自动化（含 DFS 环检测 + 拓扑排序的程序化实现）推迟到 M2，抽取为 `rustmigrate validate state --check-blocked --auto-unblock`，详见 [08 § M2 状态机程序化实现](./08-roadmap-and-reference.md#m2-质量提升8-12-周)。因此 MVP 验收时，Verifier **必须在测试中实证环检测确实触发并中止**（构造 A↔B 互锁与 A→A 自依赖用例），不得依赖 Skill 的指令跟随行为推定其生效。

### Step 0.6: 目标模块依赖就绪检查（前置门禁）
查询目标模块的依赖是否全部完成（通过 `rustmigrate graph deps <module>` 或 migration-state.json）。
若存在依赖未进入 `done`/`degrade_*`：
- **中止本次 run**，输出阻塞原因（列出哪些依赖未完成）
- 将目标模块标记为 `blocked`，填充 `blocked_by`（未完成的依赖数组）和 `pre_blocked_status`

### Step 1: 语义解构（调用 translator SubAgent）
调用 translator SubAgent，要求其生成目标模块的意图摘要。
输入：源码 + 相关迁移规则（`.rust-migration/porting/` 目录下的规则文件）。
产出物：`.rust-migration/intermediate/{module}-intent.md`。

**检查点**：意图摘要文件存在且非空。

### Step 1.5: 意图确认门禁（人类决策点，MVP 默认开启）
向用户展示 Step 1 生成的意图摘要全文，**暂停**等待人类确认后才进入 Step 2。
这是与 [03 § 7.4 安全护栏](./03-execution-model.md#74-安全护栏借鉴-rustlift)（Approval Token / 不自动宣布成功）一致的人类决策点：意图摘要是后续翻译的语义契约，错误的意图会污染整个内循环，因此必须在翻译前拦截。
确认方式（沿用 [03 § 4.2.1](./03-execution-model.md#421-执行模式分层) Skill 交互式模式，不新增 CLI 命令）：交互式询问"意图摘要是否准确？(确认/修订)"。
- 确认 → 进入 Step 2。
- 修订 → 用户补充约束后重新执行 Step 1。
power-user 可在 `.rustmigrate.toml` 设 `auto_confirm_intent = true` 跳过本门禁（/goal 自主循环与 Workflow 批量模式默认跳过）；首次迁移和高风险模块建议保持开启。

**检查点**：用户已确认意图摘要（或配置显式跳过）。

### Step 2: Phase A — 忠实翻译（调用 translator SubAgent）
调用 translator SubAgent，基于意图摘要生成 Rust 代码。
**Phase A 优先 1:1 结构对应，不做优化**：保持与源码的 1:1 对应（便于 diff 对照审查）。不得删除死代码、辅助函数、冗余字段或内联未使用常量——即使看似无用也须保留源码结构，惯用化优化留到 Phase B。
Private 方法默认翻译（不省略），保持结构完整性。
非平凡函数须加 PORT NOTE 注释，标注源码行号范围或等价锚点（便于 diff 对照与 Step 3.5 结构校验）。
标记系统：TODO(port) 标记未完成项，PERF(port) 标记已知性能问题，PORT NOTE 标记翻译决策。
输入：意图摘要 + 迁移规则（porting/ 目录）+ 依赖接口。
产出物：Rust 源文件写入 `rust_root` 对应路径。

**检查点**：Rust 文件存在。
注意：写入 .rs 文件后 rust-analyzer LSP 会自动提供编译诊断（F1 反馈）。

### Step 3: 对抗性审查（调用 verifier SubAgent）
调用 verifier SubAgent，对 Phase A 产出物执行对抗性审查。
逐维度比对源码与翻译结果（使用 7.7 节探测维度清单）。
产出物：`.rust-migration/intermediate/{module}-review.md`（差异列表 + 修正建议）。

**检查点**：审查报告文件存在且非空。

### Step 3.5: Phase A 结构校验门禁（在进入行为对抗审查后、Phase B 前）
verifier 在对抗性审查时附带校验 Phase A 是否保持了 1:1 结构（确认翻译器未提前优化）：
- 函数数量比 0.9x–2.0x（与 [03 § 7.5 质量记分卡](./03-execution-model.md#75-质量评估分层评分卡)告警阈值一致）
- 代码行数比 1.2x–3.0x（同上）
- 主控制流（循环、条件分支）数量/嵌套层级按源码 AST 结构大致对应

若结构比例越界 → 标记为「Phase A 疑似已优化」，**要求 translator 以忠实保留模式重做 Phase A**，再进入 Step 4。这是一道门禁而非软提示。

**状态写入**：校验通过/失败后写入 `modules[module].phase_a_audit_passed = true/false` 及 `phase_a_version = 当前 Phase A 文件 content hash`。

**检查点**：结构比例在界内，或已重做 Phase A 通过校验。

### Step 4: Phase B — 编译修正 + 惯用化优化（调用 translator SubAgent）
基于审查报告修正语义偏差。
并发/内存管理部分允许重写（非直译），须记录 MDR。
惯用 Rust 优化（消除翻译腔）。

如果 cargo check 失败：
1. 先执行 `cargo fix --allow-dirty`（确定性自动修复）
2. 剩余错误反馈给 translator SubAgent 修复
3. 最多重试 3 轮（由 .rustmigrate.toml 的 max_retry_rounds 控制）
4. 3 轮后仍失败 → **暂停**，生成降级分析报告，等待人类通过 `/migrate run --degrade=ffi` 确认

**检查点**：编译通过（cargo check 成功）。

### Step 5: F2 测试验证
调用 verifier SubAgent 生成当前模块的 Rust 测试（基于意图摘要、接口契约和已有黄金文件）。
产出物：Rust 测试文件写入对应模块的 `tests/` 目录或 `#[cfg(test)]` 内联模块。

翻译步骤完成后，执行以下验证命令：
- `cargo nextest run --lib` — 运行单元测试
- `cargo clippy -- -D warnings` — lint 检查

如果测试失败：调用 verifier SubAgent 分析失败原因。
可修复 → 修复后重新执行 Step 5。
不可修复 → 记录到 KNOWN_DIFFERENCES.md。

**TODO(port) 检查点（verifier）**：扫描生成的 Rust 代码中 `TODO(port)` 匹配数。若 > 0，在审查报告中标记该模块为 incomplete，将等价深度（PARITY.md）置为 `stub` 而非 `strong`，并阻塞 → `done` 状态转移（清理纪律见 [03 § 4.3 Step 3](./03-execution-model.md#43-内循环模块级单会话内-phase-ab-双阶段翻译)）。

**检查点**：测试通过率 ≥ 预期，clippy 无 warning，TODO(port) 计数 = 0（否则标记 incomplete）。

### Step 6: 状态更新
通过 `rustmigrate state transition --module <M> --to done` 更新该模块状态（确保 tmp-fsync-rename 原子写入，见 [06 § 10.8](./06-plugin-structure.md#108-持久化与崩溃安全mvp)）。
更新 `PARITY.md` 中该模块的进度行。
如有架构决策，写入 MDR。
```

### 关键检查点的失败恢复规则（Checkpoint 失败处理）

> 被 [06 § 10.2.2](./06-plugin-structure.md#1022-失败恢复机制) 与 [06 § 10.5](./06-plugin-structure.md#105-编排调度路径) 引用的权威表。各检查点的校验级别（L1/L2）以 [06 § 10.2 接口表](./06-plugin-structure.md#102-subagents4-个专职角色) 为准；下表给出失败后的重试与回滚动作。

| 检查点（SubAgent 调用点 / 门禁） | 校验级 | 失败时保留 | 失败时删除 | 重试 | 复位到 | 备注 |
|------|------|---------|---------|------|--------|------|
| analyze Step 2.5 graph build 释放门禁 | 前置 | source-ref/ | 不删（DB 已 commit 则保留） | **否** | — | 非步骤失败：报错并提示重跑 `rustmigrate graph build`（见 Step 2.5 注） |
| analyze Step 2.5→ analyzer 调用 | L1 | source-graph.db | 无 | ≤2 | pre-run | 语义增强失败可重试 |
| analyze Step 4 translator 规则生成 | L1 | source-graph.db、migration-state.json | porting/ 内本次半成品 | ≤2 | profile | 重试仍败则回滚到 analyzer 完成态 |
| analyze Step 6 scaffolder 测试搭建 | L1 | 前序全部 | test-fixtures/ 内本次半成品 | ≤2 | translator 完成态 | — |
| run Step 0.5 引用一致性 | L2（延后） | 全部 | 无 | **否** | — | `BLOCKED_BY_VALIDATION_FAILED`，见 [06 § 10.7](./06-plugin-structure.md#107-错误信息与可调试性mvp) |
| run Step 1 translator 意图摘要 | L2 | — | 本次 `{module}-intent.md` | ≤2 | 模块 pending | L2 = 6 字段非空（附录 E） |
| run Step 2 Phase A translator 翻译 | L1 | `{module}-intent.md` + `intermediate/attempts/*` | `rust_root/{module}.rs`（部分写入） | ≤2 | translating（substatus=null，即意图已确认、Phase A 未开始） | 回滚后重入 Step 2 |
| run Step 3 verifier 对抗审查 | L1 | Phase A `.rs` 文件 | `intermediate/{module}-review.md` | ≤2 | translating/phase_a_complete_awaiting_review | 回滚后重入 Step 3 |
| run Step 4 Phase B translator 惯用化 | L1 | Phase A `.rs` + review.md + `intermediate/attempts/*-phase-b-*.rs` | `rust_root/{module}.rs`（Phase B 覆写） | 按 max_retry_rounds（3）然后 pause→degrade | translating/phase_a_complete_awaiting_review | 3 轮耗尽走 pause→degrade 路径 |
| run Step 5 verifier 测试验证 | L1+L2 | Phase A/B 产物 | 测试结果 JSON | ≤2 | reviewing | JSON 产出物做 L2，测试代码做 L1 |

> 通用规则：`intermediate/attempts/*`（含 `*.json` 与 `*-phase-*.rs`）在任何回滚下**始终保留**作诊断证据（见 [06 § 10.2.2](./06-plugin-structure.md#1022-失败恢复机制)）；`validation_tool_error_*`（超时/OOM/Schema 损坏）不计入重试（见 [06 § 10.7](./06-plugin-structure.md#107-错误信息与可调试性mvp)）。

---

注：原附录 C（证据等级说明）已合并到 [README.md](./README.md) 中。

---

## 附录 D：关键中间产物 Schema（简化版）

> 以下为 `.rust-migration/intermediate/` 目录下关键中间产物的简化 JSON 结构示例。完整 JSON Schema 在 M1 阶段补充。
> 独立 JSON 示例文件见 [`schemas/`](./schemas/) 目录。

### source-graph 导出格式（JSON）

> **主存储**为 SQLite（`.rust-migration/source-graph.db`），以下为 `rustmigrate graph export --format json` 的导出格式。
> 独立示例文件：[`schemas/source-graph.example.json`](./schemas/source-graph.example.json)
> 图数据模型详见 [04-toolchain.md § 5.7.1](./04-toolchain.md#571-图数据模型)

```json
{
  "version": "0.2",
  "generated_at": "2026-06-06T10:05:00Z",
  "storage": "sqlite",
  "db_path": ".rust-migration/source-graph.db",
  "nodes": [
    {
      "id": "file:src/utils/string.ts",
      "node_type": "File",
      "name": "string.ts",
      "file_path": "src/utils/string.ts",
      "line_range": [1, 320],
      "is_exported": false,
      "complexity": "simple"
    },
    {
      "id": "function:src/utils/string.ts:capitalize",
      "node_type": "Function",
      "name": "capitalize",
      "file_path": "src/utils/string.ts",
      "line_range": [15, 28],
      "is_exported": true,
      "complexity": "simple",
      "migration_status": "done",
      "migration_priority": 1
    }
  ],
  "edges": [
    {
      "source": "file:src/utils/string.ts",
      "target": "function:src/utils/string.ts:capitalize",
      "edge_type": "contains",
      "provenance": "tree-sitter",
      "weight": 1.0
    },
    {
      "source": "file:src/core/parser.ts",
      "target": "file:src/utils/string.ts",
      "edge_type": "imports",
      "provenance": "tree-sitter",
      "weight": 1.0
    },
    {
      "source": "function:src/core/parser.ts:parseTitle",
      "target": "function:src/utils/string.ts:capitalize",
      "edge_type": "calls",
      "provenance": "tool-assisted",
      "weight": 0.95
    }
  ],
  "topological_order": ["utils/string", "utils/math", "core/parser", "core/runtime"],
  "file_fingerprints": {
    "src/utils/string.ts": {
      "content_hash": "sha256:a1b2c3...",
      "structure_hash": "sha256:d4e5f6...",
      "analyzed_at": "2026-06-06T10:05:00Z"
    }
  }
}
```

> **注意**：实际存储使用 SQLite 数据库（`.rust-migration/source-graph.db`），上述 JSON 为导出/调试格式。CLI 子命令 `rustmigrate graph export --format json` 可导出此格式。

### type-map.json（类型映射表）【M2 参考，MVP 不使用】

> 独立示例文件：[`schemas/type-map.example.json`](./schemas/type-map.example.json)

```json
{
  "version": "0.1",
  "generated_at": "2026-06-06T11:00:00Z",
  "mappings": [
    {
      "source_type": "string",
      "source_language": "typescript",
      "rust_type": "String",
      "notes": "UTF-16 → UTF-8，注意 length 语义差异",
      "rule_ref": "porting/core-rules.md#R07"
    },
    {
      "source_type": "number",
      "source_language": "typescript",
      "rust_type": "f64",
      "notes": "JS number 统一为 f64；整数场景可优化为 i64/u64",
      "rule_ref": "porting/core-rules.md#R02"
    },
    {
      "source_type": "Map<string, T>",
      "source_language": "typescript",
      "rust_type": "HashMap<String, T>",
      "notes": "注意迭代顺序差异（JS Map 保持插入序，Rust HashMap 不保证）",
      "rule_ref": "porting/core-rules.md#R02"
    }
  ]
}
```

### call-graph.json（调用图）【M2 参考，MVP 不使用】

> 独立示例文件：[`schemas/call-graph.example.json`](./schemas/call-graph.example.json)
> **与 source-graph 的关系**：source-graph.db 中的 `calls` 边已包含调用关系数据。call-graph.json 是调用关系的扁平化视图，由 `rustmigrate graph export --view calls` 导出（M2 命令），用于人类阅读和外部工具消费。MVP 阶段不需要独立维护此文件。

```json
{
  "version": "0.1",
  "generated_at": "2026-06-06T10:05:00Z",
  "functions": [
    {
      "id": "utils/string::capitalize",
      "module": "utils/string",
      "calls": ["utils/string::isEmptyString"],
      "called_by": ["core/parser::parseTitle", "cli/format::formatOutput"]
    },
    {
      "id": "core/parser::parseTitle",
      "module": "core/parser",
      "calls": ["utils/string::capitalize", "utils/string::truncate"],
      "called_by": ["core/runtime::processDocument"]
    }
  ]
}
```

---

## 附录 E：意图摘要（`{module}-intent.md`）内容规范

> 意图摘要是 Phase A 翻译的语义契约（见 [03 § 4.3 Step 2](./03-execution-model.md#43-内循环模块级单会话内-phase-ab-双阶段翻译)）。translator 生成时必须逐项覆盖以下 6 个核心字段；verifier 对抗审查（[03 § 7.7](./03-execution-model.md#77-不等价证据探测维度清单) 维度 9）逐字段核对 Phase A/B 代码与本摘要的一致性。

**Markdown 模板**：

```markdown
# 意图摘要：{module}

## 1. 标题/目的
{该模块做什么、为什么存在；纯语义描述，不含源语言语法}

## 2. 公开接口签名
{逐个列出对外函数：名称 + 入参类型 + 返回类型；语言无关的契约描述}

## 3. 前置/后置条件
- 前置：{调用前必须成立的不变式}
- 后置：{调用后保证成立的不变式}

## 4. 错误处理方案
{哪些输入/状态会失败、如何失败（异常/错误码/panic）、错误如何传播}

## 5. 并发模型
{是否共享可变状态、是否异步、取消语义；无并发则写「纯同步，无共享状态」}

## 6. 关键边界值处理
{整数溢出策略（RULE-3）、空集合/null、Unicode、浮点精度等的明确处理}
```

**工具化校验 JSON Schema（M1 用于产出物有效性检查，L2）**：

```json
{
  "version": "0.1",
  "type": "object",
  "required": ["module", "purpose", "interfaces", "preconditions",
               "postconditions", "error_model", "concurrency_model", "boundary_handling"],
  "properties": {
    "module": { "type": "string" },
    "purpose": { "type": "string", "minLength": 1 },
    "interfaces": { "type": "array", "minItems": 1,
      "items": { "type": "object", "required": ["name", "params", "returns"] } },
    "preconditions": { "type": "array", "items": { "type": "string" } },
    "postconditions": { "type": "array", "items": { "type": "string" } },
    "error_model": { "type": "string", "minLength": 1 },
    "concurrency_model": { "type": "string", "minLength": 1 },
    "boundary_handling": { "type": "string", "minLength": 1 }
  }
}
```

> verifier 产出物有效性检查（L2）：6 字段全部非空且 `interfaces` 至少一项；缺字段视为意图摘要不完整，要求 translator 重新生成。完整语义形式化验证为 M2+ 扩展。

---

## 附录 F：评分报告 `sprint-N-report.json` Schema

> 由 verifier 在 `/migrate review` 时产出（评分公式与 M1/M2 时序见 [03 § 7.5](./03-execution-model.md#75-质量评估分层评分卡)）。M1 仅产出 `quality_scores`（per-module）与基础结构；`quality_trends`（跨 Sprint 递进）在 M2 由 `rustmigrate stats quality-trends` 填充。

```json
{
  "version": "0.1",
  "sprint": 2,
  "quality_scores": {
    "modules": {
      "utils/string": {
        "deterministic_avg": 92,
        "ai_avg": 85,
        "final_score": 89.9,
        "deterministic_details": {
          "compile_pass": 100, "test_pass_rate": 100,
          "loc_ratio": 1.6, "cyclomatic_ratio": 1.05,
          "fn_count_ratio": 1.1, "clippy_warnings": 0, "unsafe_blocks": 0
        },
        "ai_details": { "idiomaticity": 88, "semantic_fidelity": 85, "maintainability": 82 },
        "confidence": 90
      }
    }
  },
  "quality_trends": {
    "sprint_1": { "final_score": 87.0, "deterministic_avg": 90 },
    "sprint_2": { "final_score": 89.9, "deterministic_avg": 92 }
  },
  "evaluation_method_version": "0.1"
}
```

> `quality_trends` 为各 Sprint 聚合值的**递进序列**（非单一对象），供 M2 回归检测对比。`evaluation_method_version` 用于跨 Sprint 一致性校验（评分规则变更时递增），配置见 [06 § 11.1](./06-plugin-structure.md#111-rustmigratetoml-配置文件) `[quality]`。
