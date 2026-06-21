# MDR-005: 符号签名持久化进图（signature 进图）

> 状态：已定稿（2026-06-21，Phase 2 Level 0 落地时确立）。收口「契约 agent 的签名输入从哪来」的架构选择，并登记 §5.7.1 `NodeData` 新增 `signature` 字段。

## 背景

Phase 2「SCC 逐文件翻译 + 整组编译门」中，契约 agent 需要整组导出符号的**准确声明签名**作为输入（产 Rust 契约 + stub）。`graph interfaces --members` 负责一次性输出整组签名。

签名从哪来，初版走「选项 A」：CLI 在 query 时按 `line_range` 回读源文件 + 手写括号扫描剥函数体。经审查（用户质疑 + codex 异构确认）判为**反模式**：

1. **重复 IO + 重造 lexer**：build 时 tree-sitter AST 信息齐全却被丢弃，query 时回读源文件 + 在 CLI 用字符串扫描重造 mini-lexer。
2. **一致性风险**：build 后源文件可能改/删、行号错位，签名读取静默失真。
3. **分层污染**：「哪些种类有函数体、`<>` 是泛型」是 TypeScript 知识，却写在语言无关的 CLI 层（M3 接 Python/C 必错）。

## 决策

`signature` 由 **build 时 lang adapter 用 tree-sitter AST 提取**，作为 `SourceNode` 的符号专属属性**持久化进图**（存 `nodes.extra` JSON 列），query 直读、不回读源文件。

- 提取（`lang/typescript.rs` `decl_signature`）：function/class 取 `node` 起始到 `body` 子节点起始（剥函数/类体）；interface/enum 整节点（body 即类型定义本身）。AST 子节点边界天然处理泛型/对象参数/箭头，无字符串扫描歧义。
- 存储：`SourceNode.signature: Option<String>` → `NodeExtra.signature` → `nodes.extra` JSON 列。**零 schema 改动、零 migration**（`extra` 是 §5.7.1 line 266 明文预留的稀疏属性扩展位，`#[serde(default)]` 前向兼容）。
- 增量一致性：`signature` 纳入 `structure_hash`（否则仅改返回类型时 content_hash 变而 structure_hash 不变 → 增量判 COSMETIC → 不重写节点 → DB signature 过期）。
- query：CLI `collect_exported_interfaces` 直读 `node.signature`，零源文件回读、零语言相关逻辑。

## 核心权衡（为什么进图，而非按需提取）

`signature` 是**源码文本内容**，不同于图其余属性（`line_range` 定位指针、`is_exported`/`is_async` 布尔枚举、`decorators` 短标记——皆为 AST 派生的结构化元数据，非内容）。把内容快照放进「轻量结构索引」与 §5.7.3「轻量、存储敏感」哲学有张力。这是真实代价，明示。

仍选进图，因相对「按需提取」（query 时 adapter 现场 parse）有三项实质优势：

| | 进图（本决策） | 按需提取（备选） |
|---|---|---|
| query 回读源文件 | 否（用 build 快照） | 是（源文件定位/一致性问题回归——build root 未持久化） |
| 一致性机制 | 复用图既有（build 快照 + structure_hash + 增量重建），不引入新模型 | 每次现读现 parse，依赖 query 时源码与图同步 |
| 语言逻辑分层 | adapter 层（正确） | adapter 层（正确） |
| 图体积 | 增加（signature 文本，mobx 量级 ~50KB） | 不变（图纯净） |

进图复用图已有的一致性班车（signature 与 `line_range`/`is_exported` 同为 build 快照），并绕开 build-root 未持久化导致的源文件定位难题；代价（图变重）量级可控。

## 边界约束（护栏，防图退化为内容库）

图可承载**翻译决策所需的、符号级的源码快照**（当前仅 `signature`：声明头/类型定义，剥函数体）。**不得**借此无节制地把源码内容塞入图：

- ✅ 允许：声明签名（剥体后的声明头、interface/enum 类型定义）——契约/翻译决策直接消费、量级受控。
- ❌ 禁止：函数体/实现体、完整文件内容、注释/文档块、任意源码片段缓存。需要源码正文的消费者应读源文件，不走图。

新增「内容类」属性进图须经 MDR 评估，对照本边界。

## 连带影响（已评估）

1. **首次增量构建全量重解析**：`structure_hash` 公式纳入 signature → 旧库（无 signature 或旧 hash）首次增量时全部文件判 STRUCTURAL、全量重建一次。一次性、正确（补齐 signature），非 bug。
2. **DB 体积增加**：每符号 signature 文本（interface 整节点可能多行）。mobx 51 文件量级 ~50KB，可接受；超大项目需关注（与 §5.7.3「<5K 行 20-100 节点」目标的张力，体积敏感场景可回退按需提取）。
3. **`graph export` JSON 暂不暴露 signature**：§09 附录 `source-graph.json` 导出样例字段止于 `migration_priority`，export.rs 未输出 signature——导出格式与图主存储不一致。当前契约 agent 走 `graph interfaces --members`（专命令），不依赖通用导出。**TODO**：若 export 需作完整契约，再同步 export.rs + §09 附录样例 + 测试。
4. **scope 边界（已有解析 gap，非本决策范围，记 TODO）**：函数重载（无 body 的 `function_signature` 当前 walk_ast 不提取）、匿名 `export default`（无 name 当前不入图）、class 方法签名未单列（方法是独立节点、通常非 exported）。

## 与设计正文的同步

- **04 § 5.7.1 `NodeData`**：新增 `signature: Option<String>`（符号专属，本 MDR）。已同步。
- **存储**：复用 `extra JSON`（§5.7.1 line 266 / §5.7.3 schema 已预留），无 schema 变更。
- **09 附录**：导出样例暂不含 signature（见连带影响 3）。

## 异构确认与演进方向（codex 对抗评估）

codex 独立评估判定方案 A 为「合理的工程取舍，但应承认它是**受控的源码快照缓存**，不是纯结构图字段」。两点修正/补充已纳入：

1. **类比修正**：「signature 与 `line_range` 同类」论据有破绽——`line_range` 是定位指针、signature 是下游直接消费的内容。准确表述应为「signature 是**纳入图增量机制的物化派生内容**」，本 MDR 以此为准。
2. **第三条路（演进方向）= 同库旁路存储**：signature 不进 `SourceNode`（petgraph 节点 weight），改存 `node_id → signature` 单独 SQLite 表 / 旁路 `HashMap`，`graph interfaces` 按节点集合批量读。这更贴合设计 §04 line 296-304「petgraph 节点只存轻量数据、重元数据旁路 HashMap」哲学，仍复用 build 快照 + structure_hash、不回读源码。**当前不采用**：MVP 20-100 节点规模下「进 extra」成本可忽略，旁路需新增表（破「零 migration」）+ 旁路加载 API + persist 拆分，属为当前不存在的规模问题过度工程。

   **演进触发条件**（满足任一则迁旁路存储，重评本决策）：① 单库 signature 总量 > ~1 MB 或显著拖慢图加载；② petgraph 内存副本因 signature 文本明显膨胀影响并行 worktree 加载；③ 出现「signature 之外的第二类内容载荷」需进图（届时旁路表更划算）。

## 边界护栏的可执行化（codex：缺硬约束）

护栏当前靠文档纪律 + AST 节点种类限定（`decl_signature` 剥体、interface/enum 整节点），但 interface/enum「整节点」可能是多行大块。为防「翻译决策所需」成为过宽口子、图退化为内容库，护栏须可执行化（TODO，优先级随规模上升）：

- **已有**：`signature_extraction_by_kind` 断言 function/class 剥体（signature 不含函数体 `{...}`）——这是护栏的第一道。
- **待补**：(a) build 期对 signature 长度设软上限（超限告警，提示疑似含实现体/被滥用）；(b) 测试断言 signature 不含 `statement_block`（函数体）文本特征。

## 落地与验证

- 实现：`types/graph.rs`（字段）、`lang/typescript.rs`（`decl_signature` + 5 提取点）、`graph/persist.rs`（`NodeExtra` round-trip）、`graph/fingerprint.rs`（structure_hash）、`cli/src/lib.rs`（直读 + 删回读 lexer）。
- 测试：`signature_extraction_by_kind`、`structure_hash_sensitive_to_signature`、`persist_round_trip_preserves_signature`、`smoke_graph_interfaces_members_whole_scc_group`。412 测试全过。
- Level 0：mobx 51 文件 SCC 签名 ~4,297 token（>40x 余量），证「契约 agent 装得下」。
