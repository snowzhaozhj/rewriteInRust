---
language_id: typescript
rule_version: RULE-2:v1.0.0, RULE-3:v1.0.0, RULE-6:v1.0.0, RULE-7:v1.0.0, RULE-8:v1.0.0, RULE-10:v1.0.0, RULE-12:v1.0.0, RULE-15:v1.0.0, RULE-20:v1.0.0
target_languages: [ts, tsx]
category: porting-template
created: 2026-06-14
sprint: S1
confidence: high
---

# TypeScript → Rust 迁移规则模板

translator 在 `/migrate analyze` 的规则生成步骤读取本模板，作为项目专有规则（`.rust-migration/porting/`）的基线，须结合 `source-graph.db` 的实际类型与调用**特化**为本项目映射。

> **覆盖范围**：核心规则——RULE-2（类型映射）/ RULE-3（错误处理）/ RULE-6（并发，M4 补）/ RULE-7（字符串）/ RULE-8（命名）/ RULE-10（标准库 IO 映射，M4 补）/ RULE-12（unsafe，M4 补）/ RULE-15（全局状态，M4 补）/ RULE-20（不确定性）。其余推迟：RULE-4 所有权、RULE-5 引用、RULE-11 禁止模式、RULE-17 测试、RULE-19 惯用法。

## 类型映射

RULE-2。源类型 → Rust 类型；按用途选择拥有所有权的类型还是借用。

| TypeScript | Rust | 说明 |
|------------|------|------|
| `string` | `String` / `&str` | 拥有用 `String`，借用参数用 `&str` |
| `number` | `i64` / `f64` | 整数语义优先精确整型（`i32`/`u32`/`usize` 按范围）；浮点才用 `f64`，不要无脑 `f64` |
| `bigint` | `i128` / `i64` | 按实际取值范围 |
| `boolean` | `bool` | |
| `T[]` / `Array<T>` | `Vec<T>` | 只读切片参数用 `&[T]` |
| `[A, B]`（元组） | `(A, B)` | |
| `T \| undefined` / `T \| null` / `T?` | `Option<T>` | 可空一律 `Option`，不要用哨兵值 |
| `Record<K, V>` / `Map<K, V>` | `HashMap<K, V>` / `BTreeMap<K, V>` | 需稳定有序/可比较键时用 `BTreeMap` |
| `Set<T>` | `HashSet<T>` / `BTreeSet<T>` | |
| `interface` / `type`（对象） | `struct` | 字段命名按 RULE-8 |
| `enum` / 联合字面量 | `enum`（可带数据） | TS 字符串联合 → Rust `enum`，比字符串更安全 |
| `Promise<T>` | `Future<Output = T>` / `async fn -> T` | 异步运行时映射见 RULE-6（M2） |
| `any` / `unknown` | —— | **不可直译**：推断具体类型；无法推断则留 `TODO(port)`（RULE-20） |
| `Function` / 回调 | `Fn`/`FnMut`/`FnOnce` trait 或 `fn` 指针 | 按是否捕获/可变捕获/消费选择 |

## 错误处理

RULE-3。把异常控制流改写成显式 `Result`。

- `throw` / `try`/`catch` → 返回 `Result<T, E>`，用 `?` 传播错误。
- 库代码：用 `thiserror` 定义具体错误枚举，保留错误分类信息。
- 应用边界 / `main`：用 `anyhow::Result` 聚合，附 `.context(...)`。
- `Promise.reject` → `Err(...)`；`async` 函数返回 `Result`。
- 不要用 `unwrap()` / `expect()` / `panic!` 掩盖可恢复错误——只有逻辑不变量被破坏（不可恢复）才 panic，并在 SAFETY/理由注释说明。

## 命名约定

RULE-8。

- `camelCase` 函数/变量/方法 → `snake_case`。
- `PascalCase` 类/接口/类型 → 保持 `PascalCase`（struct/enum/trait）。
- 常量 `UPPER_SNAKE_CASE`。
- 文件/模块名 → `snake_case`（TS 的 `kebab-case` 文件名也转 `snake_case` 模块）。
- 缩写按 Rust 惯例：`HttpClient` 而非 `HTTPClient`。

## 字符串处理

RULE-7。TS string 是 UTF-16，Rust `String`/`str` 是 UTF-8——索引语义不同。

- 按"字符"遍历用 `.chars()`；按字节用 `.bytes()`/`.as_bytes()`；不要假设 `s[i]` 直接可用（Rust 不支持按索引取 char）。
- `.length`（UTF-16 code unit 数）≠ Rust `.len()`（字节数）≠ `.chars().count()`（Unicode 标量数）——迁移涉及长度/截断逻辑时显式确认语义，必要时留 `TODO(port)` 标注差异。
- 正则：JS `RegExp` → `regex` crate；注意 JS 默认 Unicode 行为与 flag 差异。

## 标准库 IO 映射

RULE-10（IO 子节）。IO 标准库模块（`fs`/`path`/`http`/`net`/`child_process`/`os`/`stream` 等 Node.js 内置）→ Rust 对应 crate/标准库映射。当 `danger` 含 `io_side_effect` 时本节强制生效。

| Node.js 模块 | Rust 映射 | 说明 |
|-------------|----------|------|
| `fs` (sync) | `std::fs` | `readFileSync`→`fs::read_to_string`，`writeFileSync`→`fs::write` |
| `fs` (async) | `tokio::fs` | `readFile`→`tokio::fs::read_to_string`，返回 `Result` |
| `path` | `std::path::Path`/`PathBuf` | `path.join`→`Path::join`，`path.resolve`→`fs::canonicalize` |
| `http`/`https` | `reqwest` / `hyper` | HTTP 客户端→`reqwest`，服务端→`hyper`/`axum` |
| `net` | `std::net`/`tokio::net` | TCP/UDP socket |
| `child_process` | `std::process::Command` | `exec`/`spawn`→`Command::new(...).output()` |
| `os` | `std::env` | `os.platform()`→`std::env::consts::OS` |

- **副作用顺序/可见性**：保留原代码 IO 操作的执行顺序；在意图摘要 `observable_side_effects` 如实登记所有 IO 操作点。
- **资源清理**：`try/finally` 保障的资源释放→`Drop` / RAII guard / `scopeguard`；不要用裸 `unsafe` 释放资源。
- **错误路径**（联动 RULE-3）：IO 错误用 `std::io::Result` 或 `anyhow` 包裹；`fs.readFileSync` 可能抛异常→Rust 必须显式处理 `Result`。

## 并发模式

RULE-6。danger 含 `concurrency` 时强制生效。TS 单线程事件循环模型 → Rust 多线程/async 运行时，并发语义差异是高风险点。

| TypeScript | Rust | 说明 |
|------------|------|------|
| `Promise<T>` / `async`/`await` | `Future` / `async fn` + tokio | 需选定运行时（tokio 默认）；`await` 语义相近但 Rust Future 惰性、需 `.await` 驱动 |
| `Promise.all([...])` | `tokio::join!` / `futures::future::join_all` | 并发等待多个 Future |
| `Promise.race` | `tokio::select!` | 取首个完成 |
| `setTimeout` / `setInterval` | `tokio::time::sleep` / `interval` | 定时器需在 async 上下文 |
| 共享可变状态（单线程隐式安全） | `Arc<Mutex<T>>` / `Arc<RwLock<T>>` | **关键陷阱**：TS 单线程下回调间共享状态无需锁；Rust 多线程必须显式同步 |
| `EventEmitter` | `tokio::sync::broadcast` / `mpsc` channel | 事件订阅→channel |
| Worker threads | `std::thread` / `rayon` | CPU 密集任务 |

- **取消语义**：TS `AbortController` → Rust `tokio_util::sync::CancellationToken` 或 drop Future；注意 Rust drop Future 即取消，清理须在 `Drop` 中。
- **执行顺序**：TS 微任务队列保证的执行顺序在 Rust 多线程下不成立——依赖隐式顺序的代码留 `TODO(port)` 复审。
- 并发构造难以 1:1 映射时，回报编排器走 degrade_skip（`--degrade=concurrency`）。

## unsafe 使用策略

RULE-12。danger 含 `ffi` 时强制生效。TS 项目的 FFI 主要是 N-API（`napi`/`node-gyp`）原生模块或 WASM 边界。

- **N-API 原生模块**：JS↔C/C++ 桥接 → Rust 侧用 `napi-rs` 重写绑定，或若原生模块是纯算法则直接用 Rust crate 替代。
- **最小化 unsafe 面**：跨边界必需的 `unsafe`（原始指针、`transmute`、FFI 调用）须用安全抽象包裹，并在每处 `unsafe` 块上注明 `// SAFETY: <前提>`。
- **WASM 边界**：`wasm-bindgen` 暴露的接口 → Rust 侧 `#[wasm_bindgen]`，注意 JS↔Rust 类型编组（字符串/数组拷贝开销）。
- 无法安全表达的 FFI → 回报编排器走 `--degrade=ffi` 路径，不要强行 `unsafe` 掩盖。

## 全局状态处理

RULE-15。danger 含 `shared_mutable_global` 时强制生效。TS 模块级 `let`/`var` 可变绑定 → Rust 需显式同步。

| TypeScript | Rust | 说明 |
|------------|------|------|
| `export let counter = 0`（可变） | `static COUNTER: AtomicU64` / `OnceLock<Mutex<T>>` | 简单计数用原子；复杂状态用 `Mutex` |
| 模块级缓存 `const cache = new Map()` | `static CACHE: LazyLock<Mutex<HashMap<...>>>` | 惰性初始化 + 同步 |
| 单例 `let instance` | `OnceLock<T>` | 一次性初始化 |
| 模块级 `const`（不可变） | `const` / `static` | 不可变全局直接 `const`，无需同步 |

- **优先依赖注入**：能改为显式传参的全局状态，优先重构为函数参数/struct 字段，避免全局可变。
- **禁 `static mut`**：用 `OnceLock`/`LazyLock`/`Mutex`/原子类型，不要 `static mut`（UB 风险）。
- 初始化时机敏感的全局（依赖加载顺序）→ 显式 `OnceLock::get_or_init`，留 `TODO(port)` 标注原初始化假设。

## 不确定性处理

RULE-20。**这是硬规则，因为貌似合理但语义错误的代码比显式未完成更危险——后者能被发现，前者会静默带病上线。**

- 任何无法确定的映射、动态行为（`eval`、动态属性访问、运行时类型分发）、缺失的上下文 → 留 `TODO(port): <具体原因>`，不要猜测填充。
- 宁可让模块停在 incomplete，也不要输出未经验证的"看起来对"的实现。
- 不确定项汇总进生成的规则文件，供人类在 `/migrate run` 阶段决策。

## 生成产物要求

translator 据本模板写入 `.rust-migration/porting/` 时，至少产出：
- `dependency-mapping.md`：源项目外部依赖 → Rust crate 映射（基于实际 `imports` 边）。
- `core-rules.md`：上述规则在本项目的具体化（含项目真实出现的类型/错误/命名实例）。

每个生成文件须带 YAML frontmatter（含 `language_id`、`rule_version`）并至少含 `## 类型映射` 标题，以通过规则生成步骤的 L1 校验。
