> [返回主索引](./README.md)

# 五、工具链选型

## 5.1 三档分级

| Tier | 含义 | 触发方式 | 失败影响 |
|------|------|---------|---------|
| **Tier 0 硬性门禁** | 每次写入/提交必须通过 | LSP 自动（F1）+ 脚本门禁（F2） | 阻塞继续 |
| **Tier 1 推荐** | 画像自动启用，可按需关闭 | Sprint Review 触发 | 警告但不阻塞 |
| **Tier 2 高级** | 用户显式启用 | 手动触发 | 可选 |

## 5.2 Tier 0：硬性门禁

每次代码变更必须通过，无例外。

| 类别 | 工具 | 用途 | 生产验证 |
|------|------|------|---------|
| 编译 | **cargo check** | 编译通过 | Rust 标准工具 |
| Lint | **cargo clippy** | 惯用性检查 + **迁移规则执行** | Rust 标准工具 |
| 测试 | **cargo-nextest** | 测试执行 | cargo test 全面升级 |

**Clippy 作为迁移规则执行器**（借鉴 Bun 的 `clippy.toml` 实践）：

MVP 阶段在 scaffold 生成的 `clippy.toml` 中配置 3-5 条高确定性规则，将 PORTING 规则的"禁止模式"（规则 #11）硬编码为 lint 门禁。这比 prompt 约束更硬性——AI 无法绕过编译器检查。

```toml
# clippy.toml — MVP 最小规则集
# 禁止裸 unwrap/expect（迁移代码必须使用 Result 传播）
disallowed-methods = [
    { path = "core::result::Result::unwrap", reason = "迁移代码禁止 unwrap，使用 ? 或 .unwrap_or" },
    { path = "core::option::Option::unwrap", reason = "迁移代码禁止 unwrap，使用 ? 或 .unwrap_or" },
]
```

项目专有规则（如禁用特定 std API、强制使用项目内部封装）在 `/migrate analyze` 阶段根据项目画像动态生成，追加到 `clippy.toml`。M2 阶段扩展到完整的 `disallowed_methods` / `disallowed_types` / `disallowed_macros` 配置。

**clippy.toml 的表达力边界与演化路径**：

`clippy.toml` 只支持 `disallowed_methods` / `disallowed_types` / `disallowed_macros` 三类「禁止清单」式约束，无法表达语义规则（如「本项目所有 `Arc<Mutex<T>>` 必须改用无锁结构」）。这一边界决定了两件事：

1. **「AI 无法绕过」的真实含义**：clippy 门禁能确保被禁方法的**直接调用**被拦截，但 AI 仍可能通过 wrapper 函数、`unsafe` 块或宏间接规避。因此 Tier 0 的 clippy 门禁需与 `#![deny(unsafe_code)]` 类的全局属性、以及 AGENTS.md 的反合理化约束配合，单靠 clippy.toml 不构成绝对防线。
2. **规则规模阈值与自定义 lint 回退**（决策树，权威定义见 [AGENTS.md（05 § 6.6）](./05-documentation-system.md#66-agentsmd--ai-行为约束)）：
   - 规则 ≤ 10 条 **且** 全部可用 `disallowed_*` 表达 → 留在 `clippy.toml`；
   - 规则 > 15 条 **或** > 30% 需要语义判断（无法用禁止清单表达）→ 升级为自定义 lint crate，置于 `.rust-migration/lint-rules/`（轻量 lint 用全局 `deny` 属性 + 编译期检查即可，无需 proc-macro）。
   - verifier 在 `/migrate analyze` 末尾须复核生成的 `clippy.toml`；规则数 > 20 时升级规则编码策略评审（见 AGENTS.md 约束）。translator 每条规则输出须标注「是否可用 `disallowed_*` 表达（Y/N）」（见 AGENTS.md 约束），verifier 汇总时统计「需语义判断（N）」占比，供 M1 结束时的升级决策。

   M0 [Spike 1](./08-roadmap-and-reference.md#m0-假设验证周2-3-周)（SubAgent 编排可靠性）顺带采集规则可行性指标：可用 `clippy.toml` 表达的规则占比、误报/漏报率、规则维护成本——该占比由上述 translator 标注（Y/N）自动统计，若需语义判断的规则 > 30% 则在 M2 规划中升级为自定义 lint crate。M2 阶段若 M1 确认需要 > 15 条规则，再实现自定义 lint crate 架构（工作量见 [08 § M2](./08-roadmap-and-reference.md#m2-质量提升8-12-周)）。

**clippy 规则与目标 Cargo.toml 解耦**：生成的 `clippy.toml` 应放在 `.rust-migration/`（不直接注入用户的 `Cargo.toml`），保持迁移规则与目标项目构建配置分离。`verify.sh` 通过 `cargo clippy` 在目标项目根目录读取该配置（Rust 工具链默认从工作目录向上查找 `clippy.toml`，或经 `CLIPPY_CONF_DIR` 指定目录）。

**Tier 0 执行机制：双层递送**（F1 + F2，互补而非二选一）：

Tier 0 三个工具通过两条递送路径落地，二者互补——

- **F1（自动，LSP 驱动）**：`cargo check` 由 rust-analyzer LSP 在写入 `.rs` 文件后秒级自动反馈，无需 Hook。
- **F2（按需，脚本驱动）**：`clippy` + `cargo-nextest` 由 `verify.sh` 在模块翻译完成时执行；`verify.sh` 内部也包含 `cargo check` 作为**确定性兜底**（rust-analyzer 在超大项目可能有性能问题或不可用）。

> 各工具的递送机制与回退方案，以 [03 § 4.4 三层反馈循环](./03-execution-model.md#44-三层反馈循环) 的统一映射表为准，Hook 概念事件表见 [06 § 10.3](./06-plugin-structure.md#103-hooks自动化门禁)。F1（自动）与 F2（按需）是 Tier 0 覆盖的两条路径，不是互斥选项。

**cargo-nextest 测试隔离**：

nextest 默认按 CPU 数并发执行测试，可能在两类场景产生验证假阴：(a) 测试间共享全局状态/临时文件，即使 Rust 借用检查通过仍可能有竞态；(b) 跨模块并行验证时若共享 `source-graph.db` 或临时目录会产生竞态。隔离级别由 `.rustmigrate.toml` 的 `[testing] nextest_threads` 控制（schema 权威见 [06 § 11.1](./06-plugin-structure.md#111-rustmigratetoml-配置文件)）：默认 `"auto"`（CPU 数）；**跨模块迁移或 FFI 测试推荐设为 `1`（串行）**。降级为 FFI 的模块（经 napi-rs/PyO3 等）测试涉及跨语言调用，应强制串行以确保隔离。`verify.sh` 从配置读取该值并以 `cargo nextest run --test-threads=<N>` 传递（实现约定见 [06 § 10.3 verify.sh](./06-plugin-structure.md#103-hooks自动化门禁)）。

## 5.3 Tier 1：推荐（画像自动启用）

| 类别 | 工具 | 用途 | 何时启用 |
|------|------|------|---------|
| 覆盖率 | **cargo-llvm-cov** | LLVM 原生覆盖率 | 始终 |
| 快照 | **insta** | 快照测试（锁定输出） | 有 CLI/API 输出时 |
| 属性 | **proptest** | 属性测试（等价性验证） | 有纯函数时 |
| 许可证 | **cargo-deny** | 许可证合规 + 依赖审计 | Sprint Review 触发 |
| CVE | **cargo-audit** | 已知漏洞扫描 | Sprint Review 触发 |
| 搜索/重写 | **ast-grep** | 模式匹配 + 代码重写 | 始终 |
| 统计 | **tokei** | 代码行数对比 | 始终 |
| 多语言 AST | **tree-sitter** | 源码结构分析 | 始终 |
| Rust 代码生成 | **syn + quote** | 宏/过程宏 | M2：自定义 lint crate 需要时 |
| 性能基准 | **criterion** | 性能回归检测 | 默认 Tier 2；当 `migration_motives` 含 `performance` 时自动提升为 Tier 1 |
| unsafe 审计 | **cargo-geiger** | unsafe 使用统计 | 始终 |
| 任务运行 | **just** | 任务自动化 | 始终 |
| 文件监控 | **bacon** | 持续编译反馈 | 本地开发 |

**语言专用工具**：

| 源语言 | 工具 | 用途 |
|--------|------|------|
| JS/TS | **dependency-cruiser** | 依赖图分析 |
| Python | **Mypy** | 类型提取 |
| Python | **import-linter + grimp** | 依赖图分析 |

**FFI 桥接**（按需启用）：

| 目标 | 工具 | 生产验证 |
|------|------|---------|
| Node.js | **napi-rs** | SWC/Next.js |
| Python | **PyO3 + maturin** | OpenAI, Hugging Face |
| C/C++ | **bindgen + cbindgen** | Rust 官方标准 |

**Tier 1 启用规则与「验证体系可靠性」的调和**：

「可按需关闭」与「验证可信」之间存在张力——若关闭 proptest 却仍声称完成纯函数等价性验证，就是门面可靠性。为消除这一错位，Tier 1 工具的启用**由画像条件强制决定**，而非简单的全局开关：

- **有纯函数 → 强制启用 proptest**（属性测试是纯函数等价性的核心证据；§ 7.7 的 8 个探测维度依赖它，关闭即纯函数验证失效）。
- **有 CLI/API 输出 → 强制启用 insta**（快照锁定输出）。
- **有有状态模块 + 跨语言 FFI → M2 行为录制框架启用**（MVP 内默认跳过——行为录制框架属 M2 交付，见 [08 § MVP 不包含](./08-roadmap-and-reference.md#m1-mvp6-8-周)；MVP 阶段在 `tier1_exceptions` 记录 `behavior_recording: m2_deferred`，有状态验证暂以 L1 insta 快照 + 手工审阅承担）。此规则与上方纯函数/CLI 规则对称，明确「Tier 1 始终启用」不与「MVP 不含行为录制」矛盾：录制框架的强制启用本身是 M2 范围。
- **有异步/并发模式 → 条件强制启用 loom**（analyzer 检测到 async/await 语法、tokio::spawn/Arc<Mutex> 并发原语时，loom 从 Tier 2 提升为条件强制 Tier 1，提供基础并发状态空间验证）。
- **cargo-llvm-cov 始终启用，不可关闭**（覆盖率是 ≥源 等价代理的下限）。但 **FFI 降级模块的覆盖率采样有边界**：LLVM 插桩只统计 Rust 侧代码，FFI 边界处被调用的源语言实现（经 napi-rs/PyO3）不计入，覆盖率会虚高。因此：(a) 覆盖率报告按模块状态标注 `coverage_rust_only`（FFI 模块）vs `coverage_full`（原生 Rust），PARITY.md 对应行增加该列；(b) FFI 模块**不能仅凭「覆盖率 ≥ 源」判定 `done`**——见 [03 § 7.1.1](./03-execution-model.md#711-模块类型--测试层要求矩阵) FFI 行的条件强制（必须 L1 快照 + 手工审阅）；(c) 采样模式经 `.rustmigrate.toml` 的 `[testing] ffi_coverage_mode` 配置（`rust_only` 默认 / `include_calls`，schema 权威见 [06 § 11.1](./06-plugin-structure.md#111-rustmigratetoml-配置文件)）。
- 许可证审计（cargo-deny）/ CVE（cargo-audit）仅 Sprint Review 触发，模块级不强制。

`.rustmigrate.toml` 保留 `tier1 = true/false` 全局开关，但**禁用任一被强制工具必须在 `tier1_exceptions` 中记录原因与置信度**（schema 权威见 [06 § 11.1](./06-plugin-structure.md#111-rustmigratetoml-配置文件)），例如 `tier1_exceptions = [{ tool = "proptest", reason = "no_pure_functions", confidence = "low" }]`。

**「无纯函数」判定的追溯与复核闭环**（消除 analyzer 误判导致 proptest 静默跳过的风险）：

- **保守偏置**：analyzer SubAgent 的系统提示须含「若对函数纯性有疑虑（如调用链不完整、I/O 静态检查不确定），一律判定为『有纯函数』」——把「跳过 proptest」的门槛抬高，宁可多测不可漏测。
- **置信度标注**（`confidence` 枚举，非自由文本）：`low`（调用图不完整或 I/O 静态检查不确定）/ `medium`（半验证）/ `high`（完全静态验证无纯函数）。该值随 `tier1_exceptions` 持久化。
- **verifier 复核（自动闭环，替代手工 Sprint 回顾）**：`/migrate review` SKILL 须将所有 `low`/`medium` 置信度的 exception 在验证报告中以 ⚠️ 标注，并要求 verifier SubAgent 在最终批准前重新核查这些条目（verifier 系统提示中纳入此规则）。verifier 有权推翻 `low`/`medium` 判定：若复核后确认确无纯函数，将 `confidence` 升级为 `high`；若发现确有纯函数，则撤销该 exception 并触发 proptest 补测（重新生成属性测试，而非标记为「已接受降级」）。高风险项目可附加人类确认门禁。

> AGENTS.md（[05 § 6.6](./05-documentation-system.md#66-agentsmd--ai-行为约束)）的纯函数判定约束须冠以一句：「若存在 `confidence = low|medium` 的纯函数 exception，复核前先查阅本指南」。

**验证透明度（verification profile）**：`/migrate review` 生成的最终报告（`.rust-migration/reports/`）须包含一节「验证画像」，列出本次迁移**实际生效的 Tier 1 工具**及 `tier1_exceptions` 中被禁用的工具与理由；并以表格列出所有 `degrade_ffi` 模块及其覆盖率采样范围（`coverage_rust_only` 标注 + 是否已过 L1 快照 + 手工审阅），让外部审阅者看清 FFI 模块的覆盖率不代表跨边界源实现已被覆盖。这样外部审阅者能直接看到验证声明的实际覆盖面，无需改动核心架构即让「关闭了什么、为什么」可追溯。

## 5.4 Tier 2：高级（用户显式启用）

| 类别 | 工具 | 用途 | 风险/注意 |
|------|------|------|---------|
| 模糊测试 | **cargo-fuzz** | 随机输入差异对比 | 需要 corpus 管理 |
| 变异测试 | **cargo-mutants** | 验证测试质量 | 大项目耗时长 |
| UB 检测 | **Miri** | unsafe 代码 UB 检测 | 不支持所有 FFI |
| 形式化 | **Kani** | 关键路径验证 | 474 issues，有限制 |
| 并发 | **loom / shuttle** | 并发正确性验证 | 需要专门编写测试 |
| 安全扫描 | **Semgrep/OpenGrep** | 安全模式检测 | Rust 规则较少 |
| 精细编译 | **cargo-careful** | 额外 UB 检测 | 编译慢 |

## 5.5 谨慎使用（TRIAL）

| 工具 | 风险 | 缓解措施 |
|------|------|---------|
| **petgraph** | bus factor=1，279 issues | 已预估回退方案（自建 adjacency list，3-5 人天）+ 触发阈值，详见 § 5.7.2 |
| **cxx** | 作者称 MVP | 复杂 C++ 考虑 autocxx + bindgen |
| **OXC** (oxc_parser) | 0.x API 不稳定 | 主路径用 tree-sitter；TS 兼容率经 Spike 3 量化（见 § 5.7.4 注），回退选项为 TS Compiler API |
| **Semgrep** (Rust 规则) | Rust 规则太少 | 仅作为补充，不作为主要安全工具 |

## 5.6 明确不用（AVOID）

| 工具 | 原因 | 替代 |
|------|------|------|
| GoReplay | 停滞，仅 HTTP/1.1 | mitmproxy |
| madge | 2024 年 8 月后无更新 | dependency-cruiser |
| Pyright (作为管道工具) | 无 Python API | Mypy |
| pydeps | 无法处理动态导入 | import-linter + grimp |
| cargo-tarpaulin | 被 cargo-llvm-cov 超越 | cargo-llvm-cov |
| bolero | 维护不够 | cargo-fuzz |
| D2 | pre-1.0 | Mermaid |
| FalkorDB | 过度设计 | petgraph + SQLite |

## 5.7 图存储与查询架构

> **设计依据**：深度分析了 6 个成熟开源项目的图设计——GitHub stack-graphs（Arena + Handle 零成本抽象）、Meta guppy（Query→Resolve→Set 三段式 API）、CodeGraph（SQLite + FTS5 + 边出处标记）、GitNexus（嵌入式图数据库 + Leiden 社区检测 + 置信度证据链）、Understand-Anything（结构指纹增量 + Louvain 社区检测）、tree-sitter-graph + cargo-modules（声明式图构建 DSL + petgraph StableGraph）。以下方案融合了各项目的最佳实践。

### 5.7.1 图数据模型

**节点类型**（**12 种：MVP 9 + M2 3**，下表逐项标注阶段）：

> 数字口径（消除三处数字歧义）：schema 共 **12 种**节点（MVP 标注 9 种 + M2 标注 3 种），见下表。MVP 部署版本含全部 9 种 MVP 节点；典型 <5K 行项目运行时实际只**触发**其中部分（见 [§ 5.7.1 末注](#571-图数据模型)）。全文统一以「12 种（MVP 9 + M2 3）」为准。


| 类别 | 节点类型 | 说明 | 阶段 |
|------|---------|------|------|
| 结构 | `File` | 源文件 | MVP |
| 结构 | `Module` | 逻辑模块（TS namespace / Python package） | MVP |
| 结构 | `Package` | 顶层包 | MVP |
| 符号 | `Function` | 函数/方法（通过 `contains` 边拓扑区分：被 Class 包含 = method） | MVP |
| 符号 | `Class` | 类/结构体 | MVP |
| 符号 | `Interface` | 接口/trait | MVP |
| 符号 | `Enum` | 枚举 | MVP |
| 符号 | `TypeAlias` | 类型别名 | M2 |
| 符号 | `Variable` | 模块级常量/变量 | M2 |
| 语义 | `Community` | 功能聚类（Leiden 算法产出） | M2 |
| 迁移 | `RustTarget` | 对应的 Rust 目标实体（struct/enum/trait/function/module） | MVP |
| 迁移 | `TestFixture` | 黄金文件测试锚点 | MVP |

> **MVP/M2 划分原则**：一个节点类型是否 MVP，取决于它是否影响核心迁移循环（analyze → run → review）。TypeAlias 不影响翻译顺序（不引入新的模块间依赖）；Variable 跟随所在 File/Module 一起翻译；Community 需要 Leiden 算法支持。
>
> **设计决策**：
> - **Function 不拆分为 Method** — 通过 `contains` 边拓扑推导：被 Class 包含的 Function 就是 method。少一个节点类型，查询时多一步推导（微秒级，可忽略）。
> - **不新增 Decorator/Annotation 节点** — 装饰器是附加元数据，不是独立代码实体，作为 Class/Function 的 `decorators` 属性处理。
> - **不新增 Struct/Trait 节点** — Rust 侧实体通过 `RustTarget` + `rust_kind` 属性表达，支持 N:M 映射（一个大 class 拆成多个 struct，多个工具函数合并到一个 module）。
> - **RustTarget 保留为独立节点而非属性** — 因为源→目标是多对多映射，节点属性只能表达 1:1。
> - **图只建模源语言代码** — RustTarget 作为桥接锚点指向目标侧，不建模 Rust 侧完整 AST（Rust 代码质量由 cargo check/clippy/nextest 验证）。

**边类型**（12 种，MVP 8 + M2 4）：

| 边类型 | 含义 | 方向 | 阶段 |
|--------|------|------|------|
| `contains` | 父子包含 | File/Class → Function/Class | MVP |
| `imports` | 导入依赖 | File → File | MVP |
| `calls` | 函数调用 | Function → Function | MVP |
| `extends` | 继承/实现 | Class → Class/Interface | MVP |
| `uses_type` | 类型引用 | Function → Class/Interface/Enum | MVP |
| `exports` | 对外导出 | Module → Function/Class | MVP |
| `maps_to` | 迁移映射 | 源节点 → RustTarget | MVP |
| `tested_by` | 测试覆盖 | Function → TestFixture | MVP |
| `member_of` | 社区归属 | 源节点 → Community | M2 |
| `depends_on` | 包级依赖 | Package → Package | M2 |
| `wraps` | FFI 桥接 | RustTarget → 源节点 | M2 |
| `implements` | 接口实现（从 extends 拆出） | Class → Interface | M2（可选） |

> **M2 边的理由**：`member_of` 依赖 Community 节点；`depends_on` 面向 monorepo（MVP 目标是 <5K 行单项目）；`wraps` 用于降级路径分析（M2 交付物）；`implements` MVP 阶段通过 `extends` 边的 `sub_kind` 属性区分。

**边属性**（借鉴 CodeGraph 的 provenance + GitNexus 的 confidence）：

```rust
struct EdgeData {
    edge_type: EdgeType,
    provenance: Provenance,       // 边的来源
    weight: f32,                  // 0.0-1.0，连接强度
    sub_kind: Option<String>,     // extends 边: "inherits" | "implements"
    mapping_notes: Option<String>, // maps_to 边: 映射上下文说明
}

enum Provenance {
    TreeSitter,    // tree-sitter AST 确定性解析
    ToolAssisted,  // 确定性辅助工具（ast-grep / dependency-cruiser 等）
    LLM,           // LLM 推断（需人工确认）
    Manual,        // 用户手动标注
}
```

> **借鉴 CodeGraph**：`provenance` 字段让下游消费者（SubAgent、CLI 报告）能区分确定性关系和推测性关系，避免 LLM 推断的边被当作事实。
> **`sub_kind`**：MVP 阶段 `extends` 边通过此属性区分继承和实现，避免多一种边类型。M2 阶段如果查询需求频繁，再拆分为独立 `implements` 边类型。
> **`mapping_notes`**：`maps_to` 边携带映射上下文（如 "Array<T> → Vec<T>，注意引用语义差异"），避免额外查找 type-map.json。

**节点属性**：

```rust
struct NodeData {
    id: NodeId,              // 格式：type:file_path:name
    node_type: NodeType,
    name: String,
    file_path: PathBuf,
    line_range: (u32, u32),  // (start, end)
    is_exported: bool,
    complexity: Complexity,  // Simple / Moderate / Complex
    // 符号专属（Function/Class）
    is_async: bool,                     // 是否异步函数（影响 tokio 映射）
    visibility: Option<Visibility>,     // pub / crate / private
    is_abstract: bool,                  // 是否抽象类
    decorators: Vec<String>,            // ["@Controller", "@Injectable"]
    // RustTarget 专属
    rust_kind: Option<RustKind>,        // Struct / Enum / Trait / Function / Module / Crate
    rust_path: Option<String>,          // "my_crate::utils::string::capitalize"
    crate_name: Option<String>,         // "my-utils"
    // 迁移追踪
    migration_status: Option<MigrationStatus>,
    migration_priority: Option<u8>,     // 拓扑排序后的翻译优先级
}
```

> **类型特有属性的存储**：Rust 代码中可用枚举区分；SQLite 持久化时，类型特有属性存储在 `extra JSON` 字段中（schema 已预留），避免节点表列过多。

**节点 ID 命名规范**（借鉴 UA）：
- 文件节点：`file:src/utils/string.ts`
- 函数节点：`function:src/utils/string.ts:formatDate`
- 类节点：`class:src/models/user.ts:UserModel`
- RustTarget 节点：`rust_target:my_crate::utils::string::capitalize`

> **MVP 实际触发的子集**（schema 是「一套支持两个产品阶段」的前瞻设计，但 <5K 行项目只会产生其中一部分）：节点限于 `File`/`Module`/`Package`/`Function`/`Class`/`Interface`/`Enum`/`RustTarget`/`TestFixture` 共 **9 类**；边限于 `contains`/`imports`/`calls`/`extends`/`uses_type`/`exports`/`maps_to`/`tested_by` 共 **8 类**。`Community`/`TypeAlias`/`Variable` 等 M2 节点与 `member_of`/`depends_on`/`wraps`/`implements` 等 M2 边虽在 schema 预留，但 MVP <5K 项目不会产生（典型图规模 20-100 节点、100-500 边，不触发社区检测，见 § 5.7.3）。能力首次需要的时机对照见 [08 § M1](./08-roadmap-and-reference.md#m1-mvp6-8-周)。

### 5.7.2 内存图引擎

**选型：petgraph + newtype 索引**（借鉴 Guppy 的封装模式）：

```rust
// 自定义索引类型防止混用（借鉴 Guppy 的 graph_ix! 宏）
struct SourceIx(u32);
struct MigrationIx(u32);

// 源码图（核心）
type SourceGraph = StableGraph<NodeData, EdgeData, Directed, SourceIx>;

// 元数据旁路存储（借鉴 Guppy：petgraph 节点只存轻量数据，重元数据存 HashMap）
struct GraphStore {
    graph: SourceGraph,
    nodes_by_file: HashMap<PathBuf, Vec<NodeIndex<SourceIx>>>,  // 按文件索引（增量更新用）
    nodes_by_id: HashMap<NodeId, NodeIndex<SourceIx>>,          // 按 ID 索引（O(1) 查找）
    sccs: OnceCell<Vec<Vec<NodeIndex<SourceIx>>>>,              // 延迟计算强连通分量
}
```

> **借鉴 Guppy 的关键模式**：
> - **newtype 索引**：`SourceIx` 防止不同图的索引被意外混用
> - **StableGraph**：删除节点后索引不变（增量更新友好）
> - **OnceCell 延迟计算**：SCC / 拓扑排序等昂贵计算按需触发
> - **FixedBitSet 结果集**：集合运算用位操作完成，O(n/64)

**Query → Resolve → Set 三段式 API**（借鉴 Guppy）：

```rust
// Step 1: 构建查询
let query = graph.query_forward(&["file:src/core/parser.ts"]);

// Step 2: 执行遍历（可自定义过滤器）
let resolved = query.resolve_with(|edge| edge.edge_type != EdgeType::TestedBy);

// Step 3: 操作结果集
let leaf_modules = resolved.roots(Direction::Outgoing);  // 叶子模块
let topo_order = resolved.topo_sorted();                  // 拓扑排序（翻译顺序）
let sub = resolved.filter(|n| n.is_exported);            // 子集过滤
```

**petgraph bus factor 风险与预置回退方案**：

petgraph 是核心数据结构（12 节点 + 12 边，见 § 5.7.1），但 bus factor=1，是 MVP 的关键依赖。为把「未文档化风险」转化为「已预估的可管理决策」：

- **承载操作（load-bearing）**：MVP 真正依赖的 petgraph 能力有限——Kahn 拓扑排序、`FixedBitSet` 的并/交（查询结果集）、`node_weight` 的 O(1) 查找、BFS 遍历。M0 [Spike 3](./08-roadmap-and-reference.md#m0-假设验证周2-3-周)（tree-sitter 精度）顺带核验这些 `StableGraph` / `FixedBitSet` 操作的 API 稳定性（可与图规模对标合并，见 § 5.7.3 注）。
- **回退设计与成本**：`Vec<Vec<NodeIndex>>` 邻接表 + `HashMap<NodeId, usize>`（O(1) ID 查找）可覆盖 MVP ~95% 需求（拓扑排序、BFS、边遍历）。自建成本预估 **3-5 人天**（不含 Guppy 式三段式 API 的完整复刻）。
- **触发阈值与执行时机**：在 `.rustmigrate.toml` 记录 `petgraph_fallback_threshold = { critical_issues_open = 6, maintainer_inactive_months = 6 }`（schema 权威见 [06 § 11.1](./06-plugin-structure.md#111-rustmigratetoml-配置文件)）。该阈值**不做持续自动监控**（无人值守的 issue 计数脚本属过度设计）：MVP 阶段的稳定性由 M0 [Spike 3](./08-roadmap-and-reference.md#m0-假设验证周2-3-周) 顺带核验的 `StableGraph`/`FixedBitSet` API 验收兜底，故 petgraph 风险只在 **M2 启动时一次性评估**该阈值。M0 验收结论按以下格式写入 `DESIGN_ASSUMPTIONS.md`：「petgraph API 验收（Spike 3）：通过/未通过。通过 → 阈值降级为 M2 启动时一次性评估；未通过 → M1 启动自建邻接表回退（成本 3-5 人天，计入 M1 工作量）」。即使 Spike 3 通过，若 M2 真实项目暴露 petgraph 缺陷（命中 MVP 用到的拓扑排序/BFS/FixedBitSet API），改按**项目级回退评估**处理，不再走全局阈值。

### 5.7.3 持久化存储

> **MVP 与 M2 存储策略分层**（CodeGraph 验证了 SQLite 方案，但其规模未知）：MVP 目标 <5K 行单项目，实际图规模通常 **20-100 节点、100-500 边**（见 § 5.7.1 末注），运行时主结构是内存 petgraph。持久化主存储仍为 SQLite（`.db`，符合断点续传与未来增量更新需求），但 **FTS5 全文搜索、社区检测等高级能力一律延迟到 M2**——MVP 不创建 `nodes_fts` 虚拟表，节省 ~50 行 petgraph→SQLite 同步逻辑 + <5K 项目约 5-10% 的存储开销。**升级触发条件**：项目 > 20K 行、需要跨字段全文检索能力（M2 才出现的用户故事「按 name/type/file 查找相关模块」，CLI 命令清单以 [06 § 10.0.1](./06-plugin-structure.md#1001-cli-工具架构rustmigrate) 为准）、或需要跨项目增量重建。M0 [Spike 3](./08-roadmap-and-reference.md#m0-假设验证周2-3-周) 顺带对 3 个 <5K 行 TS 项目测量节点/边规模与构建耗时，复核此分层假设。MVP/M2 能力对照见 [08 § M1](./08-roadmap-and-reference.md#m1-mvp6-8-周)。
>
> **前向兼容权衡（明示）**：对 MVP 的 20-100 节点规模，petgraph + rusqlite 的组合确实**超出最小必需**——MVP 阶段图构建后只读，未用到 StableGraph 的动态增删能力。选择该三层架构是**为 M2 多项目 Workflow 与增量更新预留地基的前向兼容权衡，而非 MVP 功能必需**。代价是 rusqlite 嵌入增加编译时间与二进制体积。因此引入 **crate 集成风险门控**：M0 [Spike 0](./08-roadmap-and-reference.md#m0-假设验证周2-3-周) 增设「crate 集成风险评估」检查点，实测最终 CLI 的三项指标——编译时间（冷启）、二进制体积、首次 `graph build` 冷启动感知时间（门禁见 [08 § M0 Spike 0](./08-roadmap-and-reference.md#m0-假设验证周2-3-周)）。**决策规则（三选一超限即回退）**：Spike 0 在 M0 启动时先实测一组基线值并记入 `DESIGN_ASSUMPTIONS.md`，三项指标中**任一项超出 Spike 0 记录的可接受上限**（具体阈值由 Spike 0 据目标平台敲定、写入验收纪要，而非预先臆造数值），则 Spike 0 判失败，M1 自动启用 JSON 持久化回退（去掉 rusqlite，断点续传改用 JSON + atomic rename），SQLite 推迟到 M2 再引入；三项全通过则 Spike 0 通过、继续后续 Spike。M1 工作量按 [08 § M0→M1 决策检查点](./08-roadmap-and-reference.md#m0--m1-决策检查点)「Spike 0 crate 集成超限」行调整。这把「前向兼容假设」转为「带可测触发条件的门控决策」。

**选型：SQLite（M2 增配 FTS5）**（CodeGraph 验证了此方案的可行性）：

```sql
-- 节点表
CREATE TABLE nodes (
    id TEXT PRIMARY KEY,
    node_type TEXT NOT NULL,
    name TEXT NOT NULL,
    file_path TEXT NOT NULL,
    start_line INTEGER,
    end_line INTEGER,
    is_exported BOOLEAN DEFAULT FALSE,
    complexity TEXT DEFAULT 'moderate',
    migration_status TEXT,
    migration_priority INTEGER,
    extra JSON  -- 可扩展属性
);

-- 边表
CREATE TABLE edges (
    source TEXT NOT NULL REFERENCES nodes(id),
    target TEXT NOT NULL REFERENCES nodes(id),
    edge_type TEXT NOT NULL,
    provenance TEXT NOT NULL DEFAULT 'tree-sitter',
    weight REAL DEFAULT 1.0,
    PRIMARY KEY (source, target, edge_type)
);

-- 文件指纹表（增量更新用）
CREATE TABLE file_fingerprints (
    file_path TEXT PRIMARY KEY,
    content_hash TEXT NOT NULL,
    structure_hash TEXT NOT NULL,  -- AST 签名哈希（借鉴 UA 的结构指纹）
    analyzed_at TEXT NOT NULL
);

-- 全文搜索索引（借鉴 CodeGraph 的 FTS5 + BM25 权重）—— M2 延迟创建，MVP 不建
-- MVP 的 `graph deps` 是精确匹配 BFS（已是 O(V+E) 遍历），FTS5 的 O(log n) 索引检索
-- 对 MVP 无性能收益、只增复杂度；待 M2 出现「按 name/type/file 跨字段查找模块」的
-- 用户故事（如 `graph search <keyword>`）再启用：
-- CREATE VIRTUAL TABLE nodes_fts USING fts5(
--     id, name, file_path, content='nodes', content_rowid='rowid'
-- );

-- 关键索引
CREATE INDEX idx_nodes_file ON nodes(file_path);
CREATE INDEX idx_nodes_type ON nodes(node_type);
CREATE INDEX idx_edges_source_type ON edges(source, edge_type);
CREATE INDEX idx_edges_target_type ON edges(target, edge_type);
```

**连接配置**（借鉴 CodeGraph）：WAL 模式、64MB 页缓存、mmap I/O、`PRAGMA synchronous=NORMAL`（WAL 下平衡性能与崩溃安全）。

**并发写入策略**（M2+ Workflow 批量模式：多 agent 在独立 worktree 并行迁移、共享单一 `source-graph.db`，见 [03 § 4.2.1](./03-execution-model.md#421-执行模式分层)）：

> **MVP 不需要本节策略**：MVP 单模块串行，所有写操作由单个 CLI writer（如 `graph build`）或单个 SubAgent 顺序执行，无并发锁争夺。MVP 期对 `source-graph.db` 的写串行化**不依赖** WAL + `busy_timeout`，而由两层既有机制保障：(a) 每个 `/migrate` 命令在 Step 0 以 `flock -n .rust-migration/.migration-lock` 获取**全局命令锁**，禁止两个 `/migrate` 实例并发（含 Sprint Planning 中调用 `graph build` 增量重建与分析并发，见 [06 § 10.5 多终端并发隔离](./06-plugin-structure.md#105-编排调度路径)）；(b) `file-guard.sh` 对 `source-graph.db` 加 `flock` 排他锁，防御指令跟随失败导致的意外并发（见 [06 § 10.3](./06-plugin-structure.md#103-hooks自动化门禁)）。analyzer→translator→scaffolder 严格串行（序列见 [06 § 10.5](./06-plugin-structure.md#105-编排调度路径)）。下方的 WAL + `busy_timeout` 策略仅 M2+ 并行模式需要。

- **隔离与重试**：SQLite WAL 支持「单 writer + 多 reader」并发。每个连接设 `PRAGMA busy_timeout=5000`；写操作遇 `SQLITE_BUSY` 时按指数退避重试（最多 3 次）后上报。rusqlite 经 libsqlite3 透传上述 PRAGMA 与 busy handler。
- **写入序列化**：`graph build` / 增量更新的写入由 CLI 侧 writer 排他锁串行化（reader 信号量 ≤ N，writer 独占），避免多 agent 同时改图。M2 多 agent 并发时，analyzer/scaffolder 对 `nodes`/`edges` 表的写入通过 SQLite WAL 串行化（rusqlite 的 `busy_timeout` + 指数退避重试保证一致性）；汇总 agent 在所有并行 agent 完成后负责 WAL checkpoint。
- **petgraph 副本隔离（M2 无内存竞态）**：petgraph 是进程内内存结构、无 WAL，故 M2 并发**不共享内存图**。各 SubAgent 启动时从 SQLite 加载**独立**的 petgraph 内存副本（`rustmigrate graph build --load-from-db`），进程内只读该副本。新增节点/边先写入 SQLite（经上述 WAL 串行化），后续 agent 启动时从 SQLite 重新加载——以「SQLite 为单一真相源 + 各 agent 内存副本隔离」规避内存竞态，代价是启动加载延迟（可接受，纳入 M2 性能门禁）。这同时回答了「中间态持久化（写 SQLite）/ 副本合并（不合并，各自从 DB 重载）/ 冲突检测（由 SQLite 写串行化兜底，无需独立算法）」三问。
- **增量更新的原子性**（MVP 与 M2 通用，与并发无关）：§ 5.7.5 的 STRUCTURAL 变更需「删除旧节点+边 → 重新解析插入」，必须包在**单个 `BEGIN IMMEDIATE TRANSACTION ... COMMIT` 事务**内完成，避免中间态触发外键约束违反（`edges.source/target` 引用 `nodes.id`）。删除按「先边后节点」顺序，插入按「先节点后边」顺序。MVP 期该事务由单个 `graph build` 进程持有、提交后才写 `metadata.graph_build_completed`（见 [09 附录 A](./09-appendix-schemas.md#附录-amigration-statejson-schema)），保证下游 analyzer 不会读到半成品图。
- **跨文件一致性**：`migration-state.json` 与 `source-graph.db` 间无分布式事务，采用「先提交 DB 事务并 WAL checkpoint，再用 atomic rename（写临时文件后 `rename`）落 state.json」的顺序，保证 state.json 永不引用尚未持久化的图状态（all-or-nothing 语义）。

> 多 agent 写竞争属于 12.1 风险矩阵「多 agent 冲突」的缓解面；MVP（单模块串行）不涉及并发写，以上为 M2 实现约束。

**存储位置**：`.rust-migration/source-graph.db`（与 `migration-state.json` 同目录）

### 5.7.4 图构建管线

**三阶段确定性 + LLM 混合构建**（借鉴 UA 的"确定性脚本 + LLM Agent 分离"模式）：

| 阶段 | 工具 | 输入 | 输出 | 确定性? |
|------|------|------|------|---------|
| 1. 文件扫描 | `rustmigrate profile` | 项目目录 | 文件列表 + 语言统计 | 是 |
| 2. AST 解析 | tree-sitter（+ 适配器补充） | 源文件 | 节点 + `contains`/`imports`/`calls`/`extends` 边 | 是 |
| 3. 语义增强 | analyzer SubAgent | AST 图 + 源码 | `uses_type` 边补充 + 复杂度标注 + 社区检测 | 否（LLM） |

> **借鉴 UA 的关键原则**：阶段 1-2 必须是确定性脚本（tree-sitter AST 解析），仅阶段 3 的语义增强使用 LLM。这降低了不确定性和成本。

**批次划分**（借鉴 UA 的 Louvain 社区分组）：

阶段 2 对大项目（>200 文件）按 import 关系做社区检测分批，每批上限 35 文件。这让每批内的文件有上下文关联，提升 LLM 分析质量。退化方案：社区检测失败时按目录分组。性能基准、35 文件依据与退化算法见 § 5.7.4.1。

**CLI `graph build` 与适配器 `extract-deps.sh` 的职责关系**：

- **`rustmigrate graph build`**（阶段 2）：使用 tree-sitter 做确定性 AST 解析，产出基础图（contains、imports、exports 边）。这是图的骨架，解析过程确定性（同一输入产出同一图）。

> **「确定性」≠「零解析错误」**：tree-sitter 的确定性指**可复现**，不代表对现代 TS 语法零误差。MVP 仅覆盖 TypeScript（见 [06 § 11.2](./06-plugin-structure.md#112-语言扩展架构)），故 OXC 的 0.x 不稳定只影响单一备选解析器、不影响主路径——OXC 对比非 MVP 必需。但 tree-sitter 对 TS 的实际兼容率须在 M0 [Spike 3](./08-roadmap-and-reference.md#m0-假设验证周2-3-周) 量化：在 50-100 个含现代语法（装饰器、泛型约束、联合类型、const 断言、复杂泛型）的真实 TS 文件语料上跑 `tree-sitter-typescript`，记录**按语法类别分桶**的解析错误率（装饰器/联合类型/泛型/const 断言等各占比）并写入 `DESIGN_ASSUMPTIONS.md`。**兼容率降级决策流程**（以实测数据驱动，触发条件自动化，避免 M1 期决策卡顿）：
>
> - **error_rate ≤ 0.3%** → 继续用 tree-sitter，结论记入 `DESIGN_ASSUMPTIONS.md`，无需进一步动作。
> - **0.3% < error_rate ≤ 1%** → 继续用 tree-sitter，但在 `DESIGN_ASSUMPTIONS.md` 列出已知失败语法类别，并将这些类别列入 M1 定点改进项。
> - **error_rate > 1%（默认阈值 `ast_engine_fallback_threshold`，可经 Spike 3 实测覆写）** → Spike 3 产出「回退激活报告」（错误率按语法类别 + 文件数 + 影响面分析），由 M0 验收门禁决定是否切 Plan B（将生成的 `.rustmigrate.toml` 默认 `ast_engine` 设为 `ts-compiler-api`）。
>
> 混合策略（部分文件 tree-sitter、部分 TS Compiler API）对 MVP 属过度设计，推迟到 M2。AST 引擎经 `.rustmigrate.toml` 的 `[parser] ast_engine`（`tree-sitter` | `ts-compiler-api`，schema 权威见 [06 § 11.1](./06-plugin-structure.md#111-rustmigratetoml-配置文件)）切换为 TS Compiler API 回退（对应 Spike 3 的 Plan B）。
- **适配器 `extract-deps.sh`**（阶段 2 补充）：使用语言专用工具（如 dependency-cruiser）做精细依赖分析，能发现 tree-sitter 无法覆盖的动态 import、re-export 等场景。
- **合并策略**：CLI 构建基础图后，适配器输出作为补充合并入图。同一条边如果两者都产出，保留 `provenance: TreeSitter` 版本（确定性优先）；仅适配器发现的边标注 `provenance: ToolAssisted`。

#### 5.7.4.1 性能基准与扩展性

> 对应 D6 审查清单项「图构建性能与增量更新」。下表多数实测值在 M0 [Spike 3](./08-roadmap-and-reference.md#m0-假设验证周2-3-周)（顺带测图规模与构建耗时，见 § 5.7.3 注）填入。

**性能基准表**（待 Spike 实测；当前为预期量级，未测项标 `[M0 Spike 3 TBD]`）：

| 语言 | 文件数 | 平均文件大小 | tree-sitter 解析 | 社区检测 | 总耗时 | 内存峰值 |
|------|-------|------------|-----------------|---------|-------|---------|
| TS | 100 | ~150 行 | [M0 Spike 3 TBD] | [M0 Spike 3 TBD] | [M0 Spike 3 TBD] | [M0 Spike 3 TBD] |
| TS | 300 | ~150 行 | [M0 Spike 3 TBD] | [M0 Spike 3 TBD] | [M0 Spike 3 TBD] | [M0 Spike 3 TBD] |
| TS | 500 | ~150 行 | [M0 Spike 3 TBD] | [M0 Spike 3 TBD] | [M0 Spike 3 TBD] | [M0 Spike 3 TBD] |
| Python / C | — | — | [M3 多语言阶段 TBD] | [M3 TBD] | [M3 TBD] | [M3 TBD] |

> MVP <5K 行项目通常 ~100 文件以内、不触发社区检测（见 § 5.7.1 末注），故社区检测耗时仅在 >200 文件大项目（M2+）才成为成本项。

**算法复杂度**：Louvain 社区检测时间复杂度约 **O(m log n)**（m=边数、n=节点数），见 Blondel et al. (2008)。import 图是接近 DAG 的稀疏图（边数与节点数同量级，m ≈ O(n)），故实际接近 O(n log n)，对 <500 文件规模可接受。

**35 文件/批的依据**：该数字主要来自**上下文预算约束**而非纯经验值——按 [02 § 3.5](./02-architecture.md#35-llm-上下文窗口管理) 的 ≤100K token 预算，每文件约 2.8K token 时 35 文件 ≈ 98K token，正好贴近上限。UA 经验给出同量级参考。但「35 文件能否达成批内高内聚、低跨批重复」与「批大小 vs LLM 分析质量」两个假设未实测，须在 M0 [Spike 3](./08-roadmap-and-reference.md#m0-假设验证周2-3-周) 的「批大小优化验收」子项中量化（3-5 个中型项目 × 批大小 20/35/50），**指标用与批大小因果强的三项**：批内符号引用覆盖度、同符号跨批重复分析次数、跨批依赖链准确度（不用 proptest 等价率 / clippy 告警数，二者与批大小因果弱）。该验收为 M1 阻塞性交付物，不得因时间压力推迟到 M2；验收失败时的分档决策（批大小调整 / 目录降级 / 自适应分批）见 [08 § Spike 3 补充 a 决策树](./08-roadmap-and-reference.md#m0-假设验证周2-3-周)。批大小可经 `.rustmigrate.toml` 的 `[analysis] batch_size = "auto"`（或 15-50 固定值，schema 权威见 [06 § 11.1](./06-plugin-structure.md#111-rustmigratetoml-配置文件)）配置；M1 运行时监控实际跨批重复率，超过 `[analysis] batch_reuse_rate_threshold`（默认 0.30）时告警并建议启用自适应分批或目录降级。

**退化算法（社区检测失败）**：按顶层目录分组；单组 > 35 文件则递归按子目录细分，最大递归深度 3；仍超限则按文件名排序切片。退化代价：批内内聚性下降，可能使 LLM 跨批重复分析增加约 10-20%。

**CLI 插桩**：`rustmigrate graph build --profile` 输出 JSON 性能画像：`{ parse_time_ms, detection_time_ms, batch_count, batch_sizes[], fallback_triggered, memory_peak_mb }`，供基准回填与回归监控（`--profile` 标志的命令归属以 [06 § 10.0.1](./06-plugin-structure.md#1001-cli-工具架构rustmigrate) 为准）。

### 5.7.5 增量更新策略

**三级变更检测**（借鉴 UA 的结构指纹）：

| 级别 | 含义 | 触发 | 操作 |
|------|------|------|------|
| `NONE` | 内容哈希相同 | — | 跳过 |
| `COSMETIC` | 内容变了但 AST 签名不变 | 仅函数体内部修改 | 更新哈希，不重建图 |
| `STRUCTURAL` | AST 签名变了 | 新增/删除函数、参数变化、导出状态变化 | 删除该文件旧节点+边，重新解析 |

**结构指纹**：提取函数签名（名称+参数类型+返回类型+导出状态）+ 类签名（方法列表+属性列表）+ import 列表的 hash。区分 COSMETIC/STRUCTURAL 变更。

**传递性更新**（借鉴 GitNexus 的 BFS importer expansion）：STRUCTURAL 变更时，通过 `imports` 边反向 BFS（最大深度 3）找到所有导入该文件的文件，纳入重分析范围。边界条件：

- **深度 ≤ 3 的依据**：成熟 OSS 项目（rust-analyzer、tokio、serde 等）的 import 链多为浅链，超过 3 层的反向传播对结构变更影响通常已衰减；深度 3 在「覆盖真实影响面」与「避免全图重扫」间取平衡（实测分布待 M0 Spike 校准）。
- **环检测**：反向 BFS 维护 `visited` 集合，已处理节点跳过，杜绝循环导入（lint 应禁止，但翻译生成的 Rust 代码可能意外引入环）导致的死循环。
- **规模熔断**：若反向 BFS 将处理 > N 个文件（默认 N=50），截断为「仅直接导入者」并记 warning，避免大项目触发 O(全图) 重分析。
- **复杂度**：最坏 O(V+E)（V=文件、E=import 边）；import 图稀疏，典型 ~O(文件数)。
- **与社区检测退化的交互**：当社区检测失败改用目录分组时，传递性更新仍在 import 图上进行（与分批无关），但重分析批次按目录而非社区对齐。

```text
fn transitive_update(file, max_depth = 3):
    visited = Set()
    bfs_backward(file, visited, depth = 0)   # 沿 imports 边反向，depth>max_depth 即停
    if len(visited) > 50:
        warn_and_fallback()                  # 截断为直接导入者
    return visited
```

**实测校准**（深度 3 / 熔断 50 为经验值，须经 M0 Spike 校准——见 [08 § M0 Spike 3 补充](./08-roadmap-and-reference.md#m0-假设验证周2-3-周)，结果回填下表并将最终值/调整规则写入 `DESIGN_ASSUMPTIONS.md`）：

| 校准项 | 测量内容 | 当前假设 | M0 实测 | 调整决策 |
|--------|---------|---------|---------|---------|
| import 链深度分布 | 反向 import 链「深度 > 3」文件占比 | 多数 < 3 | [Spike TBD] | 占比高则上调 max_depth |
| 反向 BFS 耗时 | @100 / 300 / 500 文件项目 | O(V+E)，~O(文件数) | [Spike TBD] | >预期则评估增量缓存 |
| 熔断命中率 | 「> 50 直接导入者」节点占比 | 罕见（浅链） | [Spike TBD] | 命中频繁则上调 N 或改分层截断 |

> 熔断截断为「仅直接导入者」会丢失间接导入者的影响面——对长链项目可能造成图不一致，故熔断触发须记 warning 并提示用户对受影响子图做一次全量 `graph build`。M2+ 多 agent 并行时，增量更新写锁竞争纳入 [§ 5.7.3 并发写策略](#573-持久化存储) 与 [08 § M2 SQLite 并发写门禁](./08-roadmap-and-reference.md#m2-质量提升8-12-周) 评估。

### 5.7.6 图查询能力清单

CLI `rustmigrate graph` 子命令提供以下查询（M1 实现标注「是」的 3 项查询）：

| 查询 | 用途 | 算法 | MVP? |
|------|------|------|------|
| `topo-sort` | 确定翻译顺序 | Kahn 拓扑排序（见下注：MVP 不支持有环图） | 是 |
| `deps <module>` | 某模块的依赖树 | 正向 BFS | 是 |
| `stats` | 图统计信息 | 节点/边计数、度分布、连通分量 | 是 |
| `rdeps <module>` | 谁依赖此模块 | 反向 BFS | M2 |
| `cycles` | 循环依赖检测 | Kosaraju SCC | M2 |
| `impact <file>` | 文件变更影响半径 | 反向 BFS + 深度分层 | 否 |
| `community` | 功能聚类分析 | Louvain 社区检测 | 否 |
| `path <A> <B>` | 两节点间最短路径 | BFS 最短路径 | 否 |

> **MVP topo-sort 有环降级**：MVP 用标准 Kahn 算法，**不支持有环图**。若源码存在循环依赖（含 re-export 间接环），`topo-sort` 返回非零退出码并列出环路径（而非产出无效排序）。`/migrate analyze` 在生成初始状态前会调用 `topo-sort` 探测（见 [09 附录 B Step 2.8](./09-appendix-schemas.md#附录-bmvp-skill-的-skillmd-骨架)），检测到环则暂停并提示用户二选一降级：(a) 在源项目临时打破某条循环导入后重跑 `/migrate analyze`；(b) 跳过环内模块（标记 `requires_manual_review` 降级），人工拆环后再迁移。完整 SCC 环检测见 M2 的 `graph cycles`（流程见 [03 § 4.2 循环依赖处理](./03-execution-model.md#42-外循环sprint-级跨会话天周)）。

### 5.7.7 可视化

| 输出格式 | 用途 | 工具 |
|---------|------|------|
| **Mermaid** | 文档内嵌（PARITY.md、MDR） | CLI 内置模板 |
| **Graphviz DOT** | 详细交互式查看 | CLI 输出 `.dot` 文件 |
| **JSON** | SubAgent/LLM 消费 | CLI 标准输出 |
| **ASCII 树** | 终端快速查看模块结构 | CLI 内置（借鉴 cargo-modules 的树形输出） |

**过滤与聚焦**（借鉴 cargo-modules 的多维度正交过滤 + 边重定向）：

- `--focus <path>` — 聚焦到某模块的 1-hop/2-hop 邻域
- `--depth <n>` — 限制展示深度
- `--no-tests` — 排除测试节点
- `--edge-type <type>` — 按边类型过滤
- 被过滤节点的边重定向到最近的未被过滤祖先（而非简单删除），保持图连通性

### 5.7.8 可观测性与诊断

> **审查指出的遗漏**：CLI 和 Plugin 自身如何记录操作日志、如何排查编排失败。

**CLI 日志**：

| 日志级别 | 输出目标 | 内容 |
|---------|---------|------|
| `error` | stderr | 致命错误（文件不存在、Schema 校验失败） |
| `warn` | stderr | 非致命警告（可选工具缺失、过期指纹） |
| `info` | stderr | 命令执行摘要（节点/边数量、耗时） |
| `debug` | 日志文件 `.rust-migration/logs/rustmigrate.log` | 详细执行步骤（AST 解析耗时、SQL 查询详情） |

通过 `RUST_LOG` 环境变量或 `--verbose` / `-v` 标志控制级别。

**SubAgent 编排诊断**：

当 SubAgent 编排失败时，用户需要知道"在哪一步失败了"和"失败原因是什么"。诊断信息通过以下方式提供：
- SKILL.md 每个 Step 的检查点失败时，输出明确的错误消息和建议（"source-graph.db 不存在——请先运行 `rustmigrate graph build`"）
- `migration-state.json` 的 `state_history` 记录每个状态转换的时间戳，可用于定位卡在哪个阶段
- `--dry-run` 标志（M2）：预演 Skill 的执行步骤而不实际执行，用于调试编排逻辑

---

## 5.8 工具集成方式分类

所有工具按集成方式分为三类，决定了安装方式、调用路径和用户体验。

### 类别 A：嵌入 CLI（`rustmigrate` Cargo 依赖）

纯 Rust crate，编译进 `rustmigrate` 二进制。用户不需要单独安装，CLI 提供统一的 JSON 输出接口。

| 工具 | 输入 | 输出 | 集成理由 |
|------|------|------|---------|
| **tree-sitter** + 语言绑定（tree-sitter-typescript / tree-sitter-python 等） | 源文件路径 | AST 节点 JSON（类型、位置、子节点） | 多语言 AST 解析是核心能力，必须零依赖可用 |
| **ast-grep-core** | AST + 模式规则 | 匹配结果 JSON（位置、捕获） | 代码搜索/重写是高频操作，嵌入避免 CLI 调用开销 |
| **tokei** | 目录路径 | 语言统计 JSON（行数、文件数、空行/注释占比） | 代码量对比是基础分析，嵌入保证跨平台一致性 |
| **syn + quote** | Rust 源码字符串 | Rust TokenStream / 格式化代码 | M2 条件嵌入：自定义 lint crate 的 proc-macro 生成需要时引入；MVP 不需要 |
| **petgraph** | 节点+边列表 | 拓扑排序、路径查询、子图提取 JSON | 依赖图是核心数据结构，内存操作性能敏感 |
| **jsonschema** | JSON 数据 + Schema 文件 | 校验结果（通过/失败+错误详情） | 检查点校验必须确定性执行，Schema 编译期内嵌 |

> **scc 与 tokei 的取舍**：5.3 节列出 `tokei + scc` 并用。v0.9.2 决定**仅嵌入 tokei**——tokei 是纯 Rust crate 可直接嵌入，覆盖核心 LOC 统计需求；scc 是 Go 编写的外部二进制，其额外的复杂度/COCOMO 估算能力可通过 tree-sitter AST 分析自行实现。如需 scc 的性能优势（大仓场景），可作为可选外部调用。

### 类别 B：外部调用（子进程 + JSON 解析）

独立工具链或非 Rust 语言编写，只能通过子进程调用。CLI 封装 `ToolRunner` trait 统一处理调用、超时、JSON 解析和错误上报。

| 工具 | 输入 | 输出 | 集成理由 |
|------|------|------|---------|
| **cargo check** | Cargo 项目路径 | 编译诊断 JSON（`--message-format=json`） | Rust 编译器本身是外部工具链 |
| **cargo clippy** | Cargo 项目路径 | Lint 诊断 JSON | 需要完整 rustc 工具链 |
| **cargo-nextest** | Cargo 项目路径 + 测试过滤 | 测试结果 JSON（JUnit XML 或 libtest JSON） | 独立二进制，替代 cargo test |
| **cargo-llvm-cov** | Cargo 项目路径 | 覆盖率 JSON（lcov 格式） | 依赖 LLVM 覆盖率工具链 |
| **cargo-deny** | Cargo.toml | 许可证/依赖审计 JSON | 独立工具，需单独安装 |
| **cargo-audit** | Cargo.lock | CVE 报告 JSON | 依赖 RustSec 数据库 |
| **cargo-geiger** | Cargo 项目路径 | unsafe 统计 JSON | 独立工具，需单独安装 |
| **cargo-fuzz** | 目标 + 语料目录 | 崩溃报告 | 依赖 libFuzzer |
| **cargo-mutants** | Cargo 项目路径 | 变异测试报告 JSON | 独立工具，耗时长 |
| **Miri** | Cargo 项目路径 | UB 检测报告 | rustup 组件，需单独安装 |
| **dependency-cruiser** | JS/TS 项目路径 | 依赖图 JSON | Node.js 工具，需 `npx` |
| **Mypy** | Python 项目路径 | 类型信息 JSON（`--output=json`） | Python 工具，需 `pip` |
| **import-linter + grimp** | Python 项目路径 | 依赖图 JSON | Python 工具，需 `pip` |
| **just** | Justfile | 任务执行结果 | 任务运行器，替代 Makefile |
| **bacon** | Cargo 项目路径 | 持续编译反馈 | 文件监控，本地开发辅助 |
| **Kani** | Cargo 项目路径 + 验证目标 | 形式化验证结果 | Tier 2，需 nightly + 额外安装 |
| **Semgrep/OpenGrep** | 源码 + 规则文件 | 安全扫描结果 JSON | 安全检测，Rust 规则较少 |
| **cargo-careful** | Cargo 项目路径 | UB 检测结果 | 编译慢，Tier 2 补充 |

### 类别 C：目标项目依赖（scaffold 注入）

被迁移项目的 `dev-dependencies` 或 `dependencies`，由 `rustmigrate scaffold` 命令注入到目标项目的 `Cargo.toml`。

| 工具 | 输入 | 输出 | 集成理由 |
|------|------|------|---------|
| **insta** | 测试函数中的值 | 快照文件（`.snap`） | 快照测试框架，需作为目标项目的 dev-dependency |
| **proptest** | 属性策略定义 | 测试结果 + 回归种子文件 | 属性测试框架，需编译进目标测试二进制 |
| **criterion** | 基准测试函数 | 性能报告 HTML/JSON | 基准测试框架，需作为目标项目的 dev-dependency |
| **loom / shuttle** | 并发测试代码 | 状态空间探索结果 | 并发测试框架，需替换标准库原语 |
| **napi-rs** | Rust 函数 + `#[napi]` 宏 | Node.js 可调用的 `.node` 二进制 | FFI 桥接，需作为目标项目的 dependency |
| **PyO3** | Rust 函数 + `#[pyfunction]` 宏 | Python 可调用的 `.so/.pyd` | FFI 桥接，需作为目标项目的 dependency |
| **bindgen / cbindgen** | C/C++ 头文件 / Rust 源码 | Rust FFI 绑定 / C 头文件 | FFI 桥接，需在目标项目的 build.rs 中配置 |
