---
language_id: python
rule_version: RULE-2:v1.0.0, RULE-3:v1.0.0, RULE-7:v1.0.0, RULE-8:v1.0.0, RULE-20:v1.0.0
target_languages: [py]
category: porting-template
created: 2026-06-27
sprint: M3-C
confidence: high
---

# Python → Rust 迁移规则模板

translator 在 `/migrate analyze` 的规则生成步骤读取本模板，作为项目专有规则（`.rust-migration/porting/`）的基线，须结合 `source-graph.db` 的实际类型与调用**特化**为本项目映射。

> **覆盖范围**：落 Python→Rust 惯用法差异最显著的 5 条核心规则——RULE-2（类型映射）/ RULE-3（错误处理）/ RULE-7（字符串）/ RULE-8（命名）/ RULE-20（不确定性），并附 Python 专有惯用法差异（self / 动态类型 / decorator / GIL）。

## 类型映射

RULE-2。源类型 → Rust 类型；按用途选择拥有所有权的类型还是借用。Python 类型来自注解（PEP 484/526）；无注解时按实际使用推断，推断不出留 `TODO(port)`。

| Python | Rust | 说明 |
|--------|------|------|
| `str` | `String` / `&str` | 拥有用 `String`，借用参数用 `&str` |
| `int` | `i64` / `i32` / `usize` | Python int 任意精度；按实际取值范围选精确整型，溢出风险大时用 `i128` 或 `num-bigint` |
| `float` | `f64` | Python float 即 IEEE 754 双精度 |
| `bool` | `bool` | |
| `bytes` | `Vec<u8>` / `&[u8]` | 拥有用 `Vec<u8>`，借用用 `&[u8]` |
| `list[T]` / `List[T]` | `Vec<T>` | 只读切片参数用 `&[T]` |
| `tuple[A, B]` / `Tuple[A, B]` | `(A, B)` | 同构定长元组也可 `[T; N]` |
| `dict[K, V]` / `Dict[K, V]` | `HashMap<K, V>` / `BTreeMap<K, V>` | 需稳定有序/可比较键时用 `BTreeMap` |
| `set[T]` / `Set[T]` | `HashSet<T>` / `BTreeSet<T>` | |
| `Optional[T]` / `T \| None` | `Option<T>` | 可空一律 `Option`，不要用哨兵值或 `None` 魔法 |
| `Union[A, B]` / `A \| B` | `enum`（带数据） | 异构联合 → Rust `enum`，比运行时 `isinstance` 分发更安全 |
| `dataclass` / `NamedTuple` / `TypedDict` | `struct` | 字段命名按 RULE-8；不可变 dataclass 不需要 `mut` |
| `Enum` / `IntEnum` | `enum` | Python 枚举值 → Rust `enum` 变体 |
| `abc.ABC` / `Protocol` | `trait` | 抽象基类/协议 → trait；具体子类 `impl Trait for Struct` |
| `Callable[..., T]` | `Fn`/`FnMut`/`FnOnce` trait 或 `fn` 指针 | 按是否捕获/可变捕获/消费选择 |
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
- 正则：Python `re` → `regex` crate；注意 `re` 默认行为与 flag（`re.UNICODE` 等）差异。

## Python 专有惯用法差异

Python 的动态特性在 Rust 无直接对应，是迁移的主要风险点。

- **`self` 参数**：Python 方法显式 `self` → Rust `&self`/`&mut self`/`self`（按是否读写/消费选择）。`@classmethod` 的 `cls` → 关联函数（不带 self 的 `impl` 方法）；`@staticmethod` → 关联函数。
- **构造**：`__init__` → `Self::new()` 关联函数返回 `Self`；有默认值的参数 → `Default` impl 或 builder。`__new__` 极少需要直译，按语义处理。
- **decorator**：`@property` → getter 方法（`fn x(&self) -> T`）；`@staticmethod`/`@classmethod` 见上；自定义装饰器（包装函数）→ 通常内联或用高阶函数/包装 struct 实现，不要假装有等价语法，复杂的留 `TODO(port)`。
- **动态类型 / duck typing**：`isinstance` 运行时分发 → `enum` + match 或 trait object（`dyn Trait`）。`getattr`/`setattr`/`__getattr__` 动态属性 → **不可直译**，留 `TODO(port)` 并要求人类决策（RULE-20）。
- **`*args` / `**kwargs`**：可变位置参数 → slice/`Vec` 或具体参数列表；关键字参数 → 显式 struct（options pattern）。没有真正等价物，按调用点实际用法收敛为固定签名。
- **GIL 与并发**：Python 有 GIL，多线程不真并行（CPU 密集用多进程）。Rust 无 GIL，`threading`→`std::thread`、`asyncio`→async runtime、`multiprocessing`→线程或 `rayon`。迁移并发代码时显式确认共享状态的同步（`Arc<Mutex<T>>`），不要假设原代码的"看似线程安全"在真并行下成立——留 `TODO(port)` 标注需要复审的共享状态。
- **全局可变状态 / 模块级单例**：Python 模块级变量 → Rust `static`（需 `OnceLock`/`LazyLock` 或显式传参），避免全局可变；优先改为显式依赖注入。
- **魔法方法运算符重载**：`__add__`/`__eq__`/`__lt__`/`__iter__` 等 → 对应 trait（`Add`/`PartialEq`/`PartialOrd`/`Iterator`），按 trait 契约实现。

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
