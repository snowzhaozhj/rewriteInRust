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
      "risk": "medium"
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
  "config_ref": ".rustmigrate.toml"
}
```

### 状态机概念名 → JSON 字段值映射

| 状态机图（[见架构设计 > 状态机](./02-architecture.md#34-编排器状态机设计)） | migration-state.json `status` 值 | 说明 |
|-------------------|----------------------------------|------|
| TRANSLATE | `translating` | 翻译中 |
| CHECK | `compile_fixing` | 编译修复中 |
| VERIFY | `testing` + `reviewing` | 测试 → 对抗审查两个子步骤 |
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
| `reviewing` | 对抗审查中 | verifier SubAgent 正在执行对抗性审查（不等价证据探测） |
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
| `done` | `null` | 无需额外说明 |

`substatus` 无枚举约束，由 SubAgent 或人工自行填写，仅用于辅助沟通，不参与状态机流转判断。

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

### Step 1: 检测项目类型
读取当前目录的文件结构，识别源语言和框架。
检查以下文件是否存在：package.json, tsconfig.json, pyproject.toml, go.mod, CMakeLists.txt。

### Step 2: 构建源码图
使用 Bash tool 执行：`rustmigrate graph build --root ./src --format json`
CLI 构建基础图（contains/imports 边），存储到 `.rust-migration/source-graph.db`（SQLite）。
然后调用 analyzer SubAgent 做语义增强（补充 calls/uses_type 边、复杂度标注）。

**检查点**：验证 source-graph.db 存在。执行 `rustmigrate graph stats` 确认节点/边数量合理。
如果验证失败，报告错误并停止。

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
对每个 blocked 模块 M：
  读取 M.blocked_by（字符串数组）
  查询 blocked_by 中每个模块在 migration-state.json 中的当前 status
  若所有阻塞源都已进入 'done' 或 'degrade_*'：
    - 将 M.status 恢复为 M.pre_blocked_status
    - 写入日志："解除对 module M 的阻塞：所有阻塞源已解决"
    - 写回 migration-state.json
  否则：
    - 保持 blocked 状态，将当前未完成的阻塞源记入 M.substatus
```

> MVP 不做自动拓扑排序：A→B→C 多层 blocked 时，本步骤只解除「blocked_by 全部完成」的一层，连续运行会逐层推进（见 [附录 A § 合法状态转换「多模块同时 blocked」注](#合法状态转换)）。

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

**检查点**：测试通过率 ≥ 预期，clippy 无 warning。

### Step 6: 状态更新
更新 `migration-state.json` 中该模块的状态。
更新 `PARITY.md` 中该模块的进度行。
如有架构决策，写入 MDR。
```

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
      "provenance": "ast-grep",
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
