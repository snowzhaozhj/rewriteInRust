---
language_id: typescript
rule_version: RULE-2:v1.0.0, RULE-3:v1.0.0, RULE-7:v1.0.0, RULE-8:v1.0.0, RULE-20:v1.0.0
target_languages: [ts, tsx]
category: porting-template
created: 2026-06-14
sprint: S1
confidence: high
---

# TypeScript → Rust 迁移规则模板

供 translator SubAgent 在 `/migrate analyze` Step 4 读取，作为生成项目专有规则（`.rust-migration/porting/`）的初始基线。translator 须结合 `source-graph.db` 中的实际类型与调用，把下面的通用规则**特化**为本项目的具体映射。

> **覆盖范围（M1 Phase 3 范围声明）**：设计 §11.2.1 要求模板「应含所有与源语言存在惯用法差异的 MVP 通用规则类对应节」（层级=通用 AND MVP=是 共 13 类：RULE-2/3/4/5/7/8/10/11/12/15/17/19/20）。本模板 M1 Phase 3 先落 **5 条对 TS→Rust 惯用法差异最显著的核心规则**：RULE-2（类型映射）/ RULE-3（错误处理）/ RULE-7（字符串）/ RULE-8（命名）/ RULE-20（不确定性）。**推迟 Phase 4 / M2 补全**：RULE-4 内存/所有权、RULE-5 指针/引用、RULE-10 标准库映射、RULE-11 禁止模式、RULE-12 unsafe 策略、RULE-15 全局状态、RULE-17 测试模式、RULE-19 惯用法映射。

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

## 不确定性处理

RULE-20。**这是硬规则，因为貌似合理但语义错误的代码比显式未完成更危险——后者能被发现，前者会静默带病上线。**

- 任何无法确定的映射、动态行为（`eval`、动态属性访问、运行时类型分发）、缺失的上下文 → 留 `TODO(port): <具体原因>`，不要猜测填充。
- 宁可让模块停在 incomplete，也不要输出未经验证的"看起来对"的实现。
- 不确定项汇总进生成的规则文件，供人类在 `/migrate run` 阶段决策。

## 生成产物要求

translator 据本模板写入 `.rust-migration/porting/` 时，至少产出：
- `dependency-mapping.md`：源项目外部依赖 → Rust crate 映射（基于实际 `imports` 边）。
- `core-rules.md`：上述规则在本项目的具体化（含项目真实出现的类型/错误/命名实例）。

每个生成文件须带 YAML frontmatter（含 `language_id`、`rule_version`）并至少含 `## 类型映射` 标题，以通过 analyze Step 4 的 L1 校验。
