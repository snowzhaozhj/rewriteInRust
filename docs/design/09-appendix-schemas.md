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
- 当模块 A 依赖的模块 B 降级或阻塞时，A 自动进入 `blocked` 状态
- `migration-state.json` 中记录 `blocked_by` 字段（阻塞来源模块）和 `pre_blocked_status` 字段（进入 blocked 前的状态）
- 阻塞解除后（B 完成迁移或降级决策确定），A 恢复到 `pre_blocked_status`

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

### Step 2: 调用 analyzer SubAgent（项目画像）
使用 analyzer SubAgent 执行项目画像分析。
输入：项目根目录路径。
等待产出物：`.rust-migration/intermediate/source-graph.json`。

**检查点**：验证 source-graph.json 存在且包含 `modules` 和 `dependencies` 字段。
如果验证失败，报告错误并停止。

### Step 3: 生成初始状态
基于 analyzer 的产出物，生成以下文件：
- `.rust-migration/migration-state.json`（初始状态：PROFILE）
- `.rust-migration/.rustmigrate.toml`（默认配置）

**检查点**：验证 migration-state.json 的 state 字段为 "profile"。

### Step 4: 调用 translator SubAgent（规则生成）
使用 translator SubAgent 生成 PORTING.md 初始规则。
输入：source-graph.json + 语言适配器的 porting-template.md。
等待产出物：`.rust-migration/PORTING.md`。

**检查点**：验证 PORTING.md 存在且包含核心规则类（类型映射、错误处理等）。

### Step 5: 生成辅助产出物
基于 analyzer 和 translator 的产出物，生成以下文件：
- `.rust-migration/PARITY.md`（初始进度表）
- `.rust-migration/AGENTS.md`（AI 行为约束，从模板生成）

### Step 6: 调用 scaffolder SubAgent（测试基础设施搭建）
使用 scaffolder SubAgent 搭建黄金文件测试基础设施。
输入：source-graph.json + 模块接口信息。
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
- `.rust-migration/PORTING.md` 存在
- 目标模块已在 Sprint 计划中

## 上下文加载
1. 读取 `migration-state.json`，确认当前 Sprint 和目标模块
2. 读取 `PORTING.md` 中与目标模块相关的规则（按模块类型筛选）
3. 读取目标模块源码
4. 读取依赖模块的接口签名（仅接口，不含实现）

## 分步指令

### Step 1: 语义解构（调用 translator SubAgent）
调用 translator SubAgent，要求其生成目标模块的意图摘要。
输入：源码 + 相关 PORTING.md 规则。
产出物：`.rust-migration/intermediate/{module}-intent.md`。

**检查点**：意图摘要文件存在且非空。

### Step 2: Phase A — 忠实翻译（调用 translator SubAgent）
调用 translator SubAgent，基于意图摘要生成 Rust 代码。
优先保持与源码的 1:1 对应（便于 diff 对照审查）。
Private 方法默认翻译（不省略），保持结构完整性。
标记系统：TODO(port) 标记未完成项，PERF(port) 标记已知性能问题，PORT NOTE 标记翻译决策。
输入：意图摘要 + PORTING.md 规则 + 依赖接口。
产出物：Rust 源文件写入 `rust_root` 对应路径。

**检查点**：Rust 文件存在。
注意：写入 .rs 文件后 rust-analyzer LSP 会自动提供编译诊断（F1 反馈）。

### Step 3: 对抗性审查（调用 verifier SubAgent）
调用 verifier SubAgent，对 Phase A 产出物执行对抗性审查。
逐维度比对源码与翻译结果（使用 7.7 节探测维度清单）。
产出物：`.rust-migration/intermediate/{module}-review.md`（差异列表 + 修正建议）。

**检查点**：审查报告文件存在且非空。

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

### source-graph.json（源码依赖图）

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

### type-map.json（类型映射表）

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
      "rule_ref": "PORTING.md#R07"
    },
    {
      "source_type": "number",
      "source_language": "typescript",
      "rust_type": "f64",
      "notes": "JS number 统一为 f64；整数场景可优化为 i64/u64",
      "rule_ref": "PORTING.md#R02"
    },
    {
      "source_type": "Map<string, T>",
      "source_language": "typescript",
      "rust_type": "HashMap<String, T>",
      "notes": "注意迭代顺序差异（JS Map 保持插入序，Rust HashMap 不保证）",
      "rule_ref": "PORTING.md#R02"
    }
  ]
}
```

### call-graph.json（调用图）

> 独立示例文件：[`schemas/call-graph.example.json`](./schemas/call-graph.example.json)

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
