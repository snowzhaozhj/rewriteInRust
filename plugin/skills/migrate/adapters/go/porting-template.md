---
language_id: go
rule_version: RULE-2:v1.0.0, RULE-3:v1.0.0, RULE-6:v1.0.0, RULE-7:v1.0.0, RULE-8:v1.0.0, RULE-10:v1.0.0, RULE-12:v1.0.0, RULE-15:v1.0.0, RULE-20:v1.0.0
target_languages: [go]
category: porting-template
created: 2026-07-04
sprint: M4-D
confidence: high
---

# Go → Rust 迁移规则模板

translator 在 `/migrate analyze` 的规则生成步骤读取本模板，作为项目专有规则（`.rust-migration/porting/`）的基线，须结合 `source-graph.db` 的实际类型与调用**特化**为本项目映射。

> **覆盖范围**：核心规则——RULE-2（类型映射）/ RULE-3（错误处理）/ RULE-6（并发）/ RULE-7（字符串）/ RULE-8（命名）/ RULE-10（标准库 IO 映射）/ RULE-12（unsafe/cgo）/ RULE-15（全局状态）/ RULE-20（不确定性），并附 Go 专有惯用法差异（GC→所有权 / defer / goroutine / 多返回值 / 零值 / receiver / interface 隐式实现）。

## 类型映射

RULE-2。源类型 → Rust 类型；按用途选择拥有所有权的类型还是借用。Go 静态类型，映射多数直接；重点在**零值语义**与**接口/泛型**的转换。

| Go | Rust | 说明 |
|----|------|------|
| `string` | `String` / `&str` | 拥有用 `String`，借用参数用 `&str`。**Go string 与 Rust 同为 UTF-8 字节序列**（见字符串处理节）|
| `int` / `int64` | `i64` | Go `int` 平台相关（64 位平台为 64 位）；**Go 整数溢出静默回绕**，Rust debug 下 panic、release 回绕——溢出敏感处显式用 `wrapping_*`/`checked_*` 并注释 |
| `int32` / `rune` | `i32` / `char` | `rune` 是 Unicode 码点（`int32` 别名）→ 语义上是字符用 `char`，纯数值用 `i32` |
| `uint` / `uint64` / `byte` | `u64` / `u8` | `byte` 是 `uint8` 别名 → `u8` |
| `float64` / `float32` | `f64` / `f32` | IEEE 754 |
| `bool` | `bool` | |
| `[]byte` | `Vec<u8>` / `&[u8]` | 拥有用 `Vec<u8>`，借用用 `&[u8]` |
| `[]T` | `Vec<T>` | 只读切片参数用 `&[T]`；Go slice 是引用类型（共享底层数组），迁移时确认是否需要 clone |
| `[N]T`（数组） | `[T; N]` | 定长数组 |
| `map[K]V` | `HashMap<K, V>` / `BTreeMap` | **Go map 迭代顺序随机**（运行时刻意打乱）——原代码不能依赖顺序；需排序输出用 `BTreeMap`，需保留插入序用 `indexmap::IndexMap` |
| `struct` | `struct` | 字段命名按 RULE-8；导出字段（首字母大写）→ `pub` |
| `interface{...}`（带方法） | `trait` | 方法集 → trait；具体类型 `impl Trait for Struct`（Go 隐式实现 → Rust 显式 impl）|
| `interface{}` / `any` | 泛型 `<T>` / `Box<dyn Trait>` / `enum` | 空接口无约束——优先按实际用法收敛为泛型或具体 enum；真正异构容器才 `Box<dyn Any>`（留 `TODO(port)`）|
| `error` | `Result<T, E>` | 见错误处理节；库用 `thiserror`，应用边界用 `anyhow` |
| `chan T` | `std::sync::mpsc` / `tokio::sync::mpsc` / `crossbeam::channel` | 见并发节；按同步/异步选择 |
| 指针 `*T` | `&T` / `&mut T` / `Box<T>` / `Option<&T>` | 可空指针 → `Option`；所有权转移 → `Box`；借用 → 引用 |
| `nil`（指针/接口/slice/map） | `Option::None` / 空集合 | 可空一律 `Option`；`nil` slice/map 通常映射为空 `Vec`/`HashMap` |
| `func(A) B` | `Fn`/`FnMut`/`FnOnce` / `fn` 指针 | 按是否捕获/可变捕获/消费选择 |
| 泛型 `[T any]` / `[T constraint]` | `<T>` / `<T: Bound>` | Go 类型参数 → Rust 泛型 + trait bound |
| `const` / `iota` | `const` / `enum` | `iota` 枚举序列 → Rust `enum`（可 `#[repr(i64)]`）；单值常量 → `const` |
| `time.Duration` / `time.Time` | `std::time::Duration` / `std::time::Instant`/`chrono` | 时间类型按用途选 std 或 `chrono` |

## 错误处理

RULE-3。把 Go 的 `(T, error)` 多返回值惯用法改写成 `Result`。

- `func f() (T, error)` → `fn f() -> Result<T, E>`；`if err != nil { return ..., err }` → `?` 传播。
- **禁止 `panic()` 掩盖可恢复错误**——只有逻辑不变量被破坏（不可恢复）才 `panic!`，并注释理由。Go 库代码惯用返回 error 而非 panic，迁移后保持此边界。
- 库代码：用 `thiserror` 定义具体错误枚举，把不同 error 值/`errors.Is`/`errors.As` 分支映射为不同变体，保留分类信息。
- 应用边界 / `main`：用 `anyhow::Result` 聚合，`fmt.Errorf("...: %w", err)` 的 wrap → `.context(...)`（`%w` 错误链 → `anyhow` 的 source 链或 `thiserror` 的 `#[source]`）。
- `defer` 中的清理（含 `defer file.Close()`）→ RAII（`Drop`），不要硬翻成 defer 结构。
- `panic`/`recover` 对：Go 用 recover 在 goroutine 边界拦截 panic → Rust 用 `Result` 显式传播；跨线程 panic 用 `std::thread::JoinHandle::join` 的 `Err` 处理。**不要用 `catch_unwind` 模拟 recover**，除非确实是不可恢复的边界隔离。
- 忽略 error（`_ = f()` 或裸调用不检查返回的 error）→ Rust 显式 `let _ = ...` 并注释为何忽略，或改为处理，不要静默丢错。

## 命名约定

RULE-8。Go 用 MixedCaps，Rust 用 snake_case——函数/变量/字段需转换。

- `MixedCaps` / `mixedCaps` 函数/变量/方法 → `snake_case`。
- 类型 `MixedCaps`（struct/interface）→ 保持 `PascalCase`（Rust struct/enum/trait 同用 PascalCase）。
- **导出规则**：Go 首字母大写 = 包外可见 → Rust `pub`；首字母小写 = 包内私有 → Rust 模块私有（不加 `pub`）。这是 Go→Rust 可见性的核心映射。
- 常量：Go `MaxSize` → Rust `MAX_SIZE`（Rust 常量惯例 `UPPER_SNAKE_CASE`）。
- 包名 `package foo`（目录）→ Rust `mod foo` / `foo/mod.rs`（一个 Go 包 = 一个 Rust 模块，同包多文件合并）。
- 缩写按 Rust 惯例：`HttpClient` 而非 `HTTPClient`（Go 惯用全大写缩写 `URL`/`ID`/`HTTP` → Rust `Url`/`Id`/`Http`）。
- getter：Go 惯例 `func (p *P) Name() string`（无 Get 前缀）→ Rust `fn name(&self) -> &str`。

## 字符串处理

RULE-7。**Go `string` 与 Rust `String`/`str` 同为 UTF-8 字节序列**——这点比 Python（码点序列）更契合 Rust，但索引/遍历语义仍需注意。

- Go `s[i]` 取**字节**（`byte`），Rust 也不支持按索引取 char——按字节用 `.as_bytes()[i]`，按字符遍历用 `.chars()`。
- Go `for i, r := range s` 按**rune（码点）**迭代（`i` 是字节偏移、`r` 是 `rune`）→ Rust `.char_indices()`（同样给字节偏移 + `char`），语义高度对应。
- `len(s)` 是**字节数**（与 Rust `.len()` 一致，均非码点数）；码点数 Go 用 `utf8.RuneCountInString` → Rust `.chars().count()`。
- Go 切片 `s[a:b]` 按**字节**且不校验字符边界（可能切出非法 UTF-8）；Rust `&s[a..b]` 按字节但**必须落在字符边界**否则 panic——迁移切片逻辑需确认边界，必要时用 `.char_indices()` 或 `TODO(port)` 标注。
- `[]byte(s)` / `string(b)` 转换 → `.as_bytes()` / `String::from_utf8`（注意后者返 `Result`，Go 的 `string([]byte)` 不校验）。
- `[]rune(s)` → `.chars().collect::<Vec<char>>()`。
- 正则：Go `regexp`（RE2，线性时间，**同样不支持反向引用与环视**）→ Rust `regex` crate（设计一致，多数模式可直接迁移）；若原 Go 代码已绕开这些特性，Rust `regex` 无缝对应。
- 字符串拼接：Go `strings.Builder` → Rust `String::push_str` / `write!`；`fmt.Sprintf` → `format!`。

## Go 专有惯用法差异

Go 的 GC、隐式接口、goroutine 等在 Rust 无直接对应，是迁移的主要风险点。

- **GC → 所有权**：Go 靠 GC 管理内存，Rust 靠所有权/借用。迁移时确定每个值的所有者与生命周期；共享只读用 `&`/`Rc`/`Arc`，共享可变用 `RefCell`/`Mutex`。Go 的自由别名（多处持有同一指针）需重新设计为明确的所有权 + 借用。
- **`defer` → `Drop` / scopeguard**：`defer f.Close()` / `defer mu.Unlock()` → RAII guard（`Drop` 自动释放）或 `scopeguard::defer!`。多个 defer 的 LIFO 顺序对应 Rust 变量逆序析构。
- **多返回值**：Go `(T, error)` → `Result<T, E>`；`(A, B)` 非错误多返回 → 元组 `(A, B)`；`(v, ok)`（map 查找/类型断言）→ `Option<V>` / `Result`。
- **零值语义**：Go 每个类型有零值（`0`/`""`/`nil`/`false`/零 struct），变量声明即可用；Rust 无隐式零值——用 `Default` trait、`Option`（可空场景）或显式初始化。**依赖零值的逻辑**（如未初始化 map 判空）需显式处理。
- **receiver 方法**：`func (s *T) M()` → `impl T { fn m(&mut self) }`；`func (s T) M()`（值 receiver）→ `fn m(&self)`（或按是否需拷贝语义决定）。指针 receiver → `&mut self`/`&self`，值 receiver → `&self`。
- **struct 嵌入（组合）**：Go 嵌入 `struct { Base; ... }` 提供方法提升（伪继承）→ Rust 用组合 + 显式委托，或 `Deref`（谨慎，仅当确是"is-a"）。图层已把嵌入标为 `Extends` 边供参考，但 **Rust 无继承，须转为组合**。
- **interface 隐式实现**：Go 类型只要方法集匹配即自动实现 interface（无 `implements` 声明）→ Rust 需**显式 `impl Trait for Type`**。图层对隐式实现**不强连 Implements 边**（静态推不出，D-M4-02），translator 须依据 interface 方法集与类型方法集自行判定并显式 impl。
- **`nil` 的多义性**：Go `nil` 可以是空指针/空接口/空 slice/空 map/空 chan/空 func——**语义各不同**。空指针 → `Option::None`；空 slice/map → 空集合（`Vec::new()`/`HashMap::new()`）；nil 接口 → `Option<Box<dyn Trait>>`。**注意 Go "nil 接口 vs 装了 nil 指针的非 nil 接口"陷阱**——迁移到 Rust 的 `Option` 时该陷阱自然消失，但需确认原逻辑意图。
- **变长参数**：`func f(args ...T)` → `fn f(args: &[T])` 或 `impl IntoIterator`；调用点 `f(slice...)` → 直接传切片。
- **空结构体 / `struct{}`**：`map[K]struct{}`（集合惯用法）→ `HashSet<K>`；`chan struct{}`（信号）→ `mpsc::channel::<()>`。
- **`init()` 函数**：Go 包级 `init()` 自动执行 → Rust 无对应，改为显式初始化函数（`OnceLock`/`LazyLock` 惰性初始化或调用点显式调用）。

## 标准库 IO 映射

RULE-10（IO 子节）。IO 标准库包（`os`/`io`/`bufio`/`net`/`net/http`/`path/filepath`/`os/exec` 等）→ Rust 对应 crate/标准库映射。当 `danger` 含 `io_side_effect` 时本节强制生效。

| Go 包 | Rust 映射 | 说明 |
|-------|----------|------|
| `os.Open` / `os.Create` | `std::fs::File` | `os.Open(f)`→`File::open(f)?`；用 `defer f.Close()` 自动关闭 → `Drop` |
| `io` / `bufio` | `std::io::{Read, Write, BufReader, BufWriter}` | `bufio.NewReader`→`BufReader::new` |
| `os` / `path/filepath` | `std::fs` / `std::env` / `std::path` | `os.Remove`→`fs::remove_file`，`os.MkdirAll`→`fs::create_dir_all`，`filepath.Join`→`Path::join` |
| `os/exec` | `std::process::Command` | `exec.Command(...).Output()`→`Command::new(...).output()?` |
| `net` | `std::net` / `tokio::net` | TCP/UDP socket |
| `net/http`（client） | `reqwest` | `http.Get(url)`→`reqwest::get(url).await?` |
| `net/http`（server） | `hyper` / `axum` | HTTP 服务端 |
| `encoding/json` | `serde_json` | `json.Marshal`/`Unmarshal`→`serde_json::to_string`/`from_str`（配 `#[derive(Serialize, Deserialize)]`）|
| `os.Getenv` / `flag` | `std::env::var` / `clap` | 环境变量 / 命令行参数 |

- **副作用顺序/可见性**：保留原代码 IO 操作的执行顺序；在意图摘要 `observable_side_effects` 如实登记所有 IO 操作点。
- **资源清理**（联动惯用法节）：`defer Close()` → RAII（`Drop`）；不要丢弃清理语义。
- **错误路径**（联动 RULE-3）：IO 错误用 `std::io::Result` 或 `anyhow`；Go 的 `os.IsNotExist(err)` 等 → Rust `std::io::ErrorKind` 匹配。

## 并发模式

RULE-6。danger 含 `concurrency` 时强制生效。Go 以 goroutine + channel 为核心并发模型，与 Rust 的 async/线程差异是**高风险点**——**默认走 degrade_skip**（见 degrade 边界），确有把握再逐条映射。

| Go | Rust | 说明 |
|----|------|------|
| `go f()` | `tokio::spawn` / `std::thread::spawn` | IO 密集用 tokio async task，CPU 密集用线程/`rayon` |
| `chan T`（无缓冲/有缓冲） | `tokio::sync::mpsc` / `std::sync::mpsc` / `crossbeam::channel` | 无缓冲 chan（同步握手）语义特殊——mpsc 是异步队列，需确认背压/同步语义是否需 `sync_channel(0)` |
| `select { case ... }` | `tokio::select!` | 多路复用；**Go select 的随机公平选择语义** Rust 需注意 |
| `sync.Mutex` / `sync.RWMutex` | `std::sync::Mutex` / `RwLock` / `parking_lot` | 显式锁 |
| `sync.WaitGroup` | `tokio::task::JoinSet` / `JoinHandle` 收集 | 等待一组任务完成 |
| `sync.Once` | `std::sync::Once` / `OnceLock` | 一次性初始化 |
| `context.Context`（取消/超时） | `tokio_util::sync::CancellationToken` / `tokio::time::timeout` / `select!` | Go 的 context 传播取消 → Rust 取消令牌或 select 竞速 |
| `atomic.*` | `std::sync::atomic::*` | 原子操作直接对应 |

- **goroutine 泄漏**：Go 无主子 goroutine 生命周期绑定，易泄漏；Rust task/thread 需显式 join 或用结构化并发（`JoinSet`）。迁移时确认每个 goroutine 的退出路径。
- **共享状态**：Go "不要用共享内存通信，用通信共享内存"——若原代码走 channel 传递所有权，Rust 用 `mpsc` 传 `move` 值；若走共享内存 + 锁，用 `Arc<Mutex<T>>`。
- **无缓冲 channel 的同步语义**易被 mpsc 破坏——留 `TODO(port)` 显式确认。
- 并发构造难以 1:1 映射时，回报编排器走 degrade_skip（`--degrade=concurrency`）。

## unsafe / cgo 使用策略

RULE-12。danger 含 `ffi` 时强制生效。Go 的 unsafe 面主要是 cgo（`import "C"`）与 `unsafe.Pointer`。

- **cgo（`import "C"`）**：Go↔C 桥接 → 若 C 库有 Rust 等价 crate 则直接替代；否则用 `bindgen` 生成 FFI 绑定 + 安全包装。**cgo 归 `DangerCategory::Ffi`（→ RULE-12）**，无法安全表达时走 `--degrade=ffi`。
- **`unsafe.Pointer` / `uintptr`**：指针运算/类型双关 → **钉死归 `DangerCategory::Ffi`（→ RULE-12），口径与 detect_tier/gaps/degrade 三处统一**。用安全抽象包裹，每处 `unsafe` 块注明 `// SAFETY: <前提>`；无法安全表达则 degrade。
- **`reflect`**：运行时反射（`reflect.TypeOf`/`ValueOf`/字段动态访问）→ **不可直译**，归不确定性处理（RULE-20），留 `TODO(port)` 要求人类决策。
- **最小化 unsafe 面**：FFI 调用、`transmute` 必需的 `unsafe` 须用安全抽象包裹，不要用 `unsafe` 掩盖可安全表达的逻辑。
- 无法安全表达的 FFI/unsafe → 回报编排器走 `--degrade=ffi` 路径，不要强行 `unsafe` 掩盖。

## 全局状态处理

RULE-15。danger 含 `shared_mutable_global` 时强制生效。Go 包级可变变量 → Rust 需显式同步。

| Go | Rust | 说明 |
|----|------|------|
| 包级 `var counter int`（可变） | `static COUNTER: AtomicI64` / `OnceLock<Mutex<T>>` | 简单计数用原子；复杂状态用 `Mutex` |
| 包级缓存 `var cache = map[K]V{}` | `static CACHE: LazyLock<Mutex<HashMap<...>>>` | 惰性初始化 + 同步 |
| 单例（`var instance *T` + `sync.Once`） | `OnceLock<T>` | 一次性初始化 |
| 包级常量 `const Max = 100` | `const MAX: i64` | 不可变全局直接 `const`，无需同步 |
| `init()` 初始化的全局 | `LazyLock<T>` | 把 `init()` 逻辑移入惰性初始化闭包 |

- **优先依赖注入**：能改为显式传参的全局状态，优先重构为函数参数/struct 字段，避免全局可变。
- **禁 `static mut`**：用 `OnceLock`/`LazyLock`/`Mutex`/原子类型，不要 `static mut`（UB 风险）。
- **Go 无 GIL**：包级变量并发访问在 Go 中也需显式同步（`sync.Mutex`/`atomic`），Rust 同理但由类型系统强制（`Send`/`Sync`）——迁移时原有的锁需保留映射。

## 不确定性处理

RULE-20。**这是硬规则，因为貌似合理但语义错误的代码比显式未完成更危险——后者能被发现，前者会静默带病上线。**

- 任何无法确定的映射、动态行为（`reflect` 反射、`interface{}`/`any` 无约束容器、类型断言 `x.(T)`/type switch 的动态分发、`unsafe`、cgo）、缺失的语义信息 → 留 `TODO(port): <具体原因>`，不要猜测填充。
- Go interface 隐式实现的 impl 判定（图层不强连 Implements 边）：若方法集匹配关系不明确，留 `TODO(port)` 而非臆断 impl。
- 宁可让模块停在 incomplete，也不要输出未经验证的"看起来对"的实现。
- 不确定项汇总进生成的规则文件，供人类在 `/migrate run` 阶段决策。

## 生成产物要求

translator 据本模板写入 `.rust-migration/porting/` 时，至少产出：
- `dependency-mapping.md`：源项目外部依赖（Go module，`go.mod` require）→ Rust crate 映射（基于实际 `imports` 边）。
- `core-rules.md`：上述规则在本项目的具体化（含项目真实出现的类型/错误/命名/并发/接口实例）。

每个生成文件须带 YAML frontmatter（含 `language_id`、`rule_version`）并至少含 `## 类型映射` 标题，以通过规则生成步骤的 L1 校验。
