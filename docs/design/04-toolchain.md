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

## 5.3 Tier 1：推荐（画像自动启用）

| 类别 | 工具 | 用途 | 何时启用 |
|------|------|------|---------|
| 覆盖率 | **cargo-llvm-cov** | LLVM 原生覆盖率 | 始终 |
| 快照 | **insta** | 快照测试（锁定输出） | 有 CLI/API 输出时 |
| 属性 | **proptest** | 属性测试（等价性验证） | 有纯函数时 |
| 许可证 | **cargo-deny** | 许可证合规 + 依赖审计 | Sprint Review 触发 |
| CVE | **cargo-audit** | 已知漏洞扫描 | Sprint Review 触发 |
| 搜索/重写 | **ast-grep** | 模式匹配 + 代码重写 | 始终 |
| 统计 | **tokei + scc** | 代码复杂度对比 | 始终 |
| 多语言 AST | **tree-sitter** | 源码结构分析 | 始终 |
| Rust 代码生成 | **syn + quote** | 宏/过程宏 | 需要代码生成时 |
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
| **petgraph** | bus factor=1，279 issues | 轻量场景可自建 adjacency list |
| **cxx** | 作者称 MVP | 复杂 C++ 考虑 autocxx + bindgen |
| **OXC** (oxc_parser) | 0.x API 不稳定 | 备选 tree-sitter-typescript |
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

**节点类型**（13 种，MVP 10 + M2 3）：

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
    TreeSitter,   // tree-sitter AST 确定性解析
    AstGrep,      // ast-grep 模式匹配
    LLM,          // LLM 推断（需人工确认）
    Manual,       // 用户手动标注
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

### 5.7.3 持久化存储

**选型：SQLite + FTS5**（CodeGraph 验证了此方案的可行性）：

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

-- 全文搜索索引（借鉴 CodeGraph 的 FTS5 + BM25 权重）
CREATE VIRTUAL TABLE nodes_fts USING fts5(
    id, name, file_path,
    content='nodes',
    content_rowid='rowid'
);

-- 关键索引
CREATE INDEX idx_nodes_file ON nodes(file_path);
CREATE INDEX idx_nodes_type ON nodes(node_type);
CREATE INDEX idx_edges_source_type ON edges(source, edge_type);
CREATE INDEX idx_edges_target_type ON edges(target, edge_type);
```

**连接配置**（借鉴 CodeGraph）：WAL 模式、64MB 页缓存、mmap I/O。

**存储位置**：`.rust-migration/source-graph.db`（与 `migration-state.json` 同目录）

### 5.7.4 图构建管线

**三阶段确定性 + LLM 混合构建**（借鉴 UA 的"确定性脚本 + LLM Agent 分离"模式）：

| 阶段 | 工具 | 输入 | 输出 | 确定性? |
|------|------|------|------|---------|
| 1. 文件扫描 | `rustmigrate profile` | 项目目录 | 文件列表 + 语言统计 | 是 |
| 2. AST 解析 | tree-sitter + ast-grep | 源文件 | 节点 + `contains`/`imports`/`calls`/`extends` 边 | 是 |
| 3. 语义增强 | analyzer SubAgent | AST 图 + 源码 | `uses_type` 边补充 + 复杂度标注 + 社区检测 | 否（LLM） |

> **借鉴 UA 的关键原则**：阶段 1-2 必须是确定性脚本（tree-sitter AST 解析），仅阶段 3 的语义增强使用 LLM。这降低了不确定性和成本。

**批次划分**（借鉴 UA 的 Louvain 社区分组）：

阶段 2 对大项目（>200 文件）按 import 关系做社区检测分批，每批上限 35 文件。这让每批内的文件有上下文关联，提升 LLM 分析质量。退化方案：社区检测失败时按目录分组。

**CLI `graph build` 与适配器 `extract-deps.sh` 的职责关系**：

- **`rustmigrate graph build`**（阶段 2）：使用 tree-sitter 做确定性 AST 解析，产出基础图（contains、imports、exports 边）。这是图的骨架，100% 确定性。
- **适配器 `extract-deps.sh`**（阶段 2 补充）：使用语言专用工具（如 dependency-cruiser）做精细依赖分析，能发现 tree-sitter 无法覆盖的动态 import、re-export 等场景。
- **合并策略**：CLI 构建基础图后，适配器输出作为补充合并入图。同一条边如果两者都产出，保留 `provenance: TreeSitter` 版本（确定性优先）；仅适配器发现的边标注 `provenance: AstGrep`。

### 5.7.5 增量更新策略

**三级变更检测**（借鉴 UA 的结构指纹）：

| 级别 | 含义 | 触发 | 操作 |
|------|------|------|------|
| `NONE` | 内容哈希相同 | — | 跳过 |
| `COSMETIC` | 内容变了但 AST 签名不变 | 仅函数体内部修改 | 更新哈希，不重建图 |
| `STRUCTURAL` | AST 签名变了 | 新增/删除函数、参数变化、导出状态变化 | 删除该文件旧节点+边，重新解析 |

**结构指纹**：提取函数签名（名称+参数类型+返回类型+导出状态）+ 类签名（方法列表+属性列表）+ import 列表的 hash。区分 COSMETIC/STRUCTURAL 变更。

**传递性更新**（借鉴 GitNexus 的 BFS importer expansion）：STRUCTURAL 变更时，通过 `imports` 边反向 BFS（最大深度 3）找到所有导入该文件的文件，纳入重分析范围。

### 5.7.6 图查询能力清单

CLI `rustmigrate graph` 子命令提供以下查询（M1 实现前 4 项）：

| 查询 | 用途 | 算法 | MVP? |
|------|------|------|------|
| `topo-sort` | 确定翻译顺序 | Kahn/DFS 拓扑排序（借鉴 Guppy 的 cycle-aware TopoWithCycles） | 是 |
| `deps <module>` | 某模块的依赖树 | 正向 BFS | 是 |
| `rdeps <module>` | 谁依赖此模块 | 反向 BFS | M2 |
| `cycles` | 循环依赖检测 | Kosaraju SCC | M2 |
| `impact <file>` | 文件变更影响半径 | 反向 BFS + 深度分层 | 否 |
| `community` | 功能聚类分析 | Louvain 社区检测 | 否 |
| `path <A> <B>` | 两节点间最短路径 | BFS 最短路径 | 否 |
| `stats` | 图统计信息 | 节点/边计数、度分布、连通分量 | 是 |

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
| **tokei** | 目录路径 | 语言统计 JSON（行数、文件数、复杂度） | 代码量对比是基础分析，嵌入保证跨平台一致性 |
| **syn + quote** | Rust 源码字符串 | Rust TokenStream / 格式化代码 | 代码生成和 AST 操作是翻译阶段核心依赖 |
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
