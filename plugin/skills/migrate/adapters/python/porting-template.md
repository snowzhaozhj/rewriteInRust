---
language_id: python
rule_version: RULE-2:v1.0.0, RULE-3:v1.0.0, RULE-6:v1.0.0, RULE-7:v1.0.0, RULE-8:v1.0.0, RULE-10:v1.0.0, RULE-12:v1.0.0, RULE-15:v1.0.0, RULE-20:v1.0.0
target_languages: [py]
category: porting-template
created: 2026-06-27
sprint: M3-C
confidence: high
---

# Python → Rust 迁移规则模板

translator 在 `/migrate analyze` 的规则生成步骤读取本模板，作为项目专有规则（`.rust-migration/porting/`）的基线，须结合 `source-graph.db` 的实际类型与调用**特化**为本项目映射。

> **覆盖范围**：核心规则——RULE-2（类型映射）/ RULE-3（错误处理）/ RULE-6（并发，M4 补）/ RULE-7（字符串）/ RULE-8（命名）/ RULE-10（标准库 IO 映射，M4 补）/ RULE-12（unsafe，M4 补）/ RULE-15（全局状态，M4 补）/ RULE-20（不确定性），并附 Python 专有惯用法差异（self / 动态类型 / decorator / GIL）。

## 类型映射

RULE-2。源类型 → Rust 类型；按用途选择拥有所有权的类型还是借用。Python 类型来自注解（PEP 484/526）；无注解时按实际使用推断，推断不出留 `TODO(port)`。

| Python | Rust | 说明 |
|--------|------|------|
| `str` | `String` / `&str` | 拥有用 `String`，借用参数用 `&str` |
| `int` | `i64` / `i32` / `usize` | Python int 任意精度；按实际取值范围选精确整型，溢出风险大时用 `i128` 或 `num-bigint` |
| `float` | `f64` | Python float 即 IEEE 754 双精度 |
| `decimal.Decimal` | `rust_decimal::Decimal` | 任意精度十进制；**禁止降级为 `f64`**（金融/精确计算场景会丢精度） |
| `bool` | `bool` | |
| `bytes` | `Vec<u8>` / `&[u8]` | 拥有用 `Vec<u8>`，借用用 `&[u8]` |
| `list[T]` / `List[T]` | `Vec<T>` | 只读切片参数用 `&[T]` |
| `tuple[A, B]` / `Tuple[A, B]` | `(A, B)` | 同构定长元组也可 `[T; N]` |
| `dict[K, V]` / `Dict[K, V]` | `HashMap` / `BTreeMap` / `IndexMap` | **Python dict（3.7+）保证插入顺序**；`HashMap`（随机）/`BTreeMap`（按键排序）均不保留插入序——代码依赖遍历/序列化顺序时用 `indexmap::IndexMap`，无法判断是否依赖顺序留 `TODO(port)`；需键排序才用 `BTreeMap` |
| `set[T]` / `Set[T]` | `HashSet<T>` / `BTreeSet<T>` | |
| `Optional[T]` / `T \| None` | `Option<T>` | 可空一律 `Option`，不要用哨兵值或 `None` 魔法 |
| `Union[A, B]` / `A \| B` | `enum`（带数据） | 异构联合 → Rust `enum`，比运行时 `isinstance` 分发更安全 |
| `dataclass` / `NamedTuple` / `TypedDict` | `struct` | 字段命名按 RULE-8；不可变 dataclass 不需要 `mut` |
| `Enum` / `IntEnum` | `enum` | Python 枚举值 → Rust `enum` 变体 |
| `abc.ABC` / `Protocol` | `trait` | 抽象基类/协议 → trait；具体子类 `impl Trait for Struct`；**带具体实现/实例字段的 ABC 需拆为 trait + struct**（trait 不能持有字段），多继承/mixin 无直接对应，复杂情形留 `TODO(port)` |
| `Callable[..., T]` | `Fn`/`FnMut`/`FnOnce` trait 或 `fn` 指针 | 按是否捕获/可变捕获/消费选择 |
| `Generator` / `yield` | `impl Iterator<Item = T>` | 生成器 → 迭代器；保留惰性求值语义。复杂的 send/throw 协程语义留 `TODO(port)` |
| `Any` | —— | **不可直译**：推断具体类型；无法推断留 `TODO(port)`（RULE-20） |
| `Awaitable[T]` / `async def` | `Future<Output = T>` / `async fn -> T` | 异步运行时映射（tokio/async-std）按项目选定 |

## 错误处理

RULE-3。把异常控制流改写成显式 `Result`。

- `raise` / `try`/`except` → 返回 `Result<T, E>`，用 `?` 传播错误。
- 库代码：用 `thiserror` 定义具体错误枚举，把不同异常类型映射为不同变体，保留分类信息。
- 应用边界 / `main`：用 `anyhow::Result` 聚合，附 `.context(...)`。
- `try/except/else/finally`：`finally` 的清理逻辑改用 RAII（`Drop`）或显式作用域，不要硬翻成 try-finally 结构。
- `except` 捕获后吞掉（`pass`）→ 在 Rust 显式处理或 `let _ = ...` 并注释为何忽略，不要静默丢错。
- 不要用 `unwrap()` / `expect()` / `panic!` 掩盖可恢复错误——只有逻辑不变量被破坏（不可恢复）才 panic，并在注释说明理由。

## 命名约定

RULE-8。Python（PEP 8）与 Rust 命名大多一致，差异点显式标注。

- `snake_case` 函数/变量/方法 → 保持 `snake_case`（无需转换，与 TS 不同）。
- `PascalCase` 类 → 保持 `PascalCase`（struct/enum/trait）。
- 常量 `UPPER_SNAKE_CASE` → 保持。
- 模块文件 `snake_case.py` → `snake_case` 模块；`__init__.py` 包 → `mod.rs` 或 `<pkg>/mod.rs` 模块树。
- 私有约定 `_name`（单下划线）→ Rust 用模块可见性（`pub(crate)` / 私有），不保留下划线前缀。
- dunder `__name__`（双下划线）→ 按语义映射到 trait 方法（如 `__eq__`→`PartialEq`、`__len__`→自定义 `len()`、`__repr__`→`Debug`），不直译名字。
- 缩写按 Rust 惯例：`HttpClient` 而非 `HTTPClient`。

## 字符串处理

RULE-7。Python 3 `str` 是 Unicode 码点序列，Rust `String`/`str` 是 UTF-8 字节——索引语义不同。

- Python `s[i]` 按码点取字符；Rust 不支持按索引取 char。按"字符"遍历用 `.chars()`，按字节用 `.bytes()`/`.as_bytes()`。
- `len(s)`（码点数）≠ Rust `.len()`（字节数）≠ `.chars().count()`（Unicode 标量数）——迁移涉及长度/切片逻辑时显式确认语义，必要时留 `TODO(port)` 标注差异。
- Python 切片 `s[a:b]` 按码点；Rust `&s[a..b]` 按字节且必须落在字符边界上，否则 panic——切片逻辑需重新按字节边界推导或用 `.char_indices()`。
- `str` vs `bytes`：Python 显式区分，迁移时 `str`→`String`、`bytes`→`Vec<u8>`，编解码用 `.encode()`/`.decode()` 对应 `String::from_utf8`/`.as_bytes()`。
- 正则：Python `re` → `regex` crate，但 **`regex` 不支持反向引用 `\1` 与环视 `(?=...)`/`(?<=...)`**（crate 设计为保证线性时间）——含这些特性的模式在 Rust 会直接编译失败，须改用 `fancy-regex` crate 或重构正则，**迁移前必须先检查模式是否用到这些特性**。另注意 `re.UNICODE` 等 flag 的默认行为差异。

## Python 专有惯用法差异

Python 的动态特性在 Rust 无直接对应，是迁移的主要风险点。

- **`self` 参数**：Python 方法显式 `self` → Rust `&self`/`&mut self`/`self`（按是否读写/消费选择）。`@classmethod` 的 `cls` → 关联函数（不带 self 的 `impl` 方法）；`@staticmethod` → 关联函数。
- **构造**：`__init__` → `Self::new()` 关联函数返回 `Self`；有默认值的参数 → `Default` impl 或 builder。`__new__` 极少需要直译，按语义处理。
- **decorator**：`@property` → getter 方法（`fn x(&self) -> T`）；`@staticmethod`/`@classmethod` 见上；自定义装饰器（包装函数）→ 通常内联或用高阶函数/包装 struct 实现，不要假装有等价语法，复杂的留 `TODO(port)`。
- **动态类型 / duck typing**：`isinstance` 运行时分发 → `enum` + match 或 trait object（`dyn Trait`）。`getattr`/`setattr`/`__getattr__` 动态属性 → **不可直译**，留 `TODO(port)` 并要求人类决策（RULE-20）。
- **`*args` / `**kwargs`**：可变位置参数 → slice/`Vec` 或具体参数列表；关键字参数 → 显式 struct（options pattern）。没有真正等价物，按调用点实际用法收敛为固定签名。
- **上下文管理器**：`with` / `__enter__`/`__exit__` / `contextlib.contextmanager` → RAII guard（实现 `Drop` 做清理）；与错误处理节的 `finally`→`Drop` 同源。
- **GIL 与并发**：Python 有 GIL，多线程不真并行（CPU 密集用多进程）。Rust 无 GIL，`threading`→`std::thread`、`asyncio`→async runtime、`multiprocessing`→线程或 `rayon`。**注意 `multiprocessing` 是进程隔离（独立内存、pickle 传值），映射到线程/rayon 变成共享内存**——原代码若依赖"无共享"假设会出问题。迁移并发代码时显式确认共享状态的同步（`Arc<Mutex<T>>`），不要假设原代码的"看似线程安全"在真并行下成立——留 `TODO(port)` 标注需要复审的共享状态。
- **全局可变状态 / 模块级单例**：Python 模块级变量 → Rust `static`（需 `OnceLock`/`LazyLock` 或显式传参），避免全局可变；优先改为显式依赖注入。
- **魔法方法运算符重载**：`__add__`/`__eq__`/`__lt__`/`__iter__` 等 → 对应 trait（`Add`/`PartialEq`/`PartialOrd`/`Iterator`）；`__repr__`→`Debug`、`__str__`→`Display`、`__len__`→自定义 `len()`、`__hash__`→`Hash`，按 trait 契约实现。

## 标准库 IO 映射

RULE-10（IO 子节）。IO 标准库模块（`os`/`sys`/`subprocess`/`socket`/`shutil`/`io`/`pathlib`/`requests`/`urllib`/`http`/`tempfile` 等）→ Rust 对应 crate/标准库映射。当 `danger` 含 `io_side_effect` 时本节强制生效。

| Python 模块 | Rust 映射 | 说明 |
|------------|----------|------|
| `open()` / `io` | `std::fs::File` / `std::io::BufReader` | `open(f)`→`File::open(f)?`，`with open` 的自动关闭→`Drop` |
| `os` / `os.path` | `std::fs` / `std::env` / `std::path` | `os.remove`→`fs::remove_file`，`os.makedirs`→`fs::create_dir_all` |
| `pathlib.Path` | `std::path::PathBuf` | `Path("a")/"b"`→`PathBuf::from("a").join("b")` |
| `subprocess` | `std::process::Command` | `subprocess.run()`→`Command::new(...).output()?` |
| `shutil` | `std::fs` / `fs_extra` | `shutil.copy`→`fs::copy`，`shutil.rmtree`→`fs::remove_dir_all` |
| `socket` | `std::net` / `tokio::net` | TCP/UDP socket |
| `requests` / `urllib` | `reqwest` | `requests.get()`→`reqwest::get().await?` |
| `http.server` | `hyper` / `axum` | HTTP 服务端 |
| `tempfile` | `tempfile` crate | `NamedTemporaryFile`→`tempfile::NamedTempFile` |

- **副作用顺序/可见性**：保留原代码 IO 操作的执行顺序；在意图摘要 `observable_side_effects` 如实登记所有 IO 操作点。
- **资源清理**：Python `with` 语句（上下文管理器）→ RAII guard（`Drop`）/ `scopeguard`；`try/finally` 同理。不要丢弃资源清理语义。
- **错误路径**（联动 RULE-3）：IO 错误用 `std::io::Result` 或 `anyhow` 包裹；Python 的 `FileNotFoundError`/`PermissionError` 等细分异常→ Rust `std::io::ErrorKind` 匹配或 `thiserror` 枚举。
- **`input()` 内置函数**：`input()`→`std::io::stdin().read_line()`，注意 Rust 版本不自动 strip 换行符。

## 并发模式

RULE-6。danger 含 `concurrency` 时强制生效。Python 有 GIL（多线程不真并行），Rust 无 GIL——并发模型差异是高风险点（与「Python 专有惯用法差异」的 GIL 条联动）。

| Python | Rust | 说明 |
|--------|------|------|
| `threading.Thread` | `std::thread` | Rust 线程真并行，无 GIL——原依赖 GIL 隐式原子性的代码会出竞态 |
| `asyncio` / `async def` / `await` | `Future` / `async fn` + tokio | 需选定运行时；Rust Future 惰性需 `.await` 驱动 |
| `asyncio.gather(*tasks)` | `tokio::join!` / `join_all` | 并发等待 |
| `multiprocessing` | `std::thread` / `rayon` | **关键陷阱**：`multiprocessing` 是进程隔离（独立内存、pickle 传值），映射到线程/rayon 变共享内存——原代码若依赖"无共享"假设会出问题 |
| `queue.Queue` | `std::sync::mpsc` / `crossbeam::channel` | 线程间通信 |
| `threading.Lock` / `RLock` | `std::sync::Mutex` / `parking_lot::Mutex` | 显式锁 |
| GIL 隐式保护的共享状态 | `Arc<Mutex<T>>` / 原子类型 | **必须显式同步**：Python 单 GIL 下 `+=` 看似原子，Rust 多线程下需 `AtomicU64`/`Mutex` |

- **CPU 密集 vs IO 密集**：Python 惯例 CPU 密集用 `multiprocessing`、IO 密集用 `asyncio`——映射时 CPU 密集优先 `rayon`（数据并行），IO 密集用 tokio async。
- **共享状态复审**：迁移并发代码时显式确认所有共享状态的同步，不要假设原代码"看似线程安全"在真并行下成立——留 `TODO(port)` 标注需复审的共享状态。
- 并发构造难以 1:1 映射时，回报编排器走 degrade_skip（`--degrade=concurrency`）。

## unsafe 使用策略

RULE-12。danger 含 `ffi` 时强制生效。Python 项目的 FFI 主要是 C 扩展（`ctypes`/`cffi`/Cython）或原生模块（`.so`/`.pyd`）。

- **C 扩展模块**：Python↔C 桥接（`ctypes.CDLL`/`cffi`）→ 若 C 库有 Rust 等价 crate 则直接替代；否则用 `bindgen` 生成 FFI 绑定 + 安全包装。
- **Cython 加速代码**：通常是纯算法的性能优化 → Rust 原生重写即可（Rust 本身已快），不需保留 FFI。
- **最小化 unsafe 面**：FFI 调用、原始指针、`transmute` 必需的 `unsafe` 须用安全抽象包裹，每处 `unsafe` 块注明 `// SAFETY: <前提>`。
- **NumPy/科学计算 buffer**：`numpy` 数组的 C buffer 协议 → `ndarray` crate；避免直接操作原始指针。
- 无法安全表达的 FFI → 回报编排器走 `--degrade=ffi` 路径，不要强行 `unsafe` 掩盖。

## 全局状态处理

RULE-15。danger 含 `shared_mutable_global` 时强制生效。Python 模块级可变变量 → Rust 需显式同步（与「Python 专有惯用法差异」的全局可变状态条联动）。

| Python | Rust | 说明 |
|--------|------|------|
| 模块级 `counter = 0`（可变） | `static COUNTER: AtomicU64` / `OnceLock<Mutex<T>>` | 简单计数用原子；复杂状态用 `Mutex` |
| 模块级缓存 `_cache = {}` | `static CACHE: LazyLock<Mutex<HashMap<...>>>` | 惰性初始化 + 同步 |
| 单例模式 `_instance = None` | `OnceLock<T>` | 一次性初始化 |
| 模块级常量 `MAX = 100` | `const MAX: i64` / `static` | 不可变全局直接 `const`，无需同步 |
| `functools.lru_cache` 装饰器 | `once_cell`/`LazyLock` + `Mutex<HashMap>` 或 `cached` crate | 函数级记忆化缓存 |

- **优先依赖注入**：能改为显式传参的全局状态，优先重构为函数参数/struct 字段，避免全局可变。
- **禁 `static mut`**：用 `OnceLock`/`LazyLock`/`Mutex`/原子类型，不要 `static mut`（UB 风险）。
- **GIL 假象**：Python 模块级 `+=` 在 GIL 下看似安全，Rust 多线程下必须原子/锁——迁移时显式同步并标注。

## 不确定性处理

RULE-20。**这是硬规则，因为貌似合理但语义错误的代码比显式未完成更危险——后者能被发现，前者会静默带病上线。** Python 动态特性多，此规则尤其关键。

- 任何无法确定的映射、动态行为（`eval`/`exec`、`getattr` 动态属性、`isinstance` 运行时分发、metaclass、monkey patching）、缺失的类型注解 → 留 `TODO(port): <具体原因>`，不要猜测填充。
- 宁可让模块停在 incomplete，也不要输出未经验证的"看起来对"的实现。
- 不确定项汇总进生成的规则文件，供人类在 `/migrate run` 阶段决策。

## 生成产物要求

translator 据本模板写入 `.rust-migration/porting/` 时，至少产出：
- `dependency-mapping.md`：源项目外部依赖（PyPI 包）→ Rust crate 映射（基于实际 `imports` 边）。
- `core-rules.md`：上述规则在本项目的具体化（含项目真实出现的类型/错误/命名/动态特性实例）。

每个生成文件须带 YAML frontmatter（含 `language_id`、`rule_version`）并至少含 `## 类型映射` 标题，以通过规则生成步骤的 L1 校验。
