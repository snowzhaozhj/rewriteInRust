# SCC 组契约：circular-deps `event-bus` 组

> 组级一次产出，逐文件翻译 agent 据此填空、签名锁定不许改。
> 源：`fixtures/circular-deps/src/`。CLI 实测：`graph cycles` 报 1 个真环
> `{handler.ts, event-bus.ts, emitter.ts}`，`graph interfaces event-bus.ts --members`
> 给出组成员与导出符号（class 签名 `class X`，方法签名本契约手写补全）。

- **group_key**：`file:src/emitter.ts`（SCC 缩点代表）
- **member_files**：`src/emitter.ts`、`src/event-bus.ts`、`src/handler.ts`
- **sprint**：2（`src/shared.ts` 无环，sprint 1 先译，组外依赖）

---

## 1. module_map（源文件 → Rust mod 名 + 路径）

| 源文件 | Rust mod | 路径 | 组内/组外 |
|--------|----------|------|-----------|
| `src/shared.ts` | `shared` | `src/shared.rs` | 组外（sprint 1 预翻译依赖） |
| `src/emitter.ts` | `emitter` | `src/emitter.rs` | 组内 |
| `src/event-bus.ts` | `event_bus` | `src/event_bus.rs` | 组内 |
| `src/handler.ts` | `handler` | `src/handler.rs` | 组内 |

`mod.rs`（本样例为 `lib.rs`）声明：`pub mod shared; pub mod emitter; pub mod event_bus; pub mod handler;`
**冻结**：逐文件 agent 不得新增/改动 mod 声明。

## 2. exported_symbols（跨引用符号完整 Rust 签名 — 锁定）

```rust
// shared（组外依赖，签名供组内引用）
pub type EventName = String;                       // export type EventName = string
pub type EventPayload = HashMap<String, String>;   // export interface EventPayload { [k]: unknown }
                                                   // unknown → String 占位（忠实阶段）

// emitter
pub struct Emitter { /* 字段见 ownership_graph */ }
impl Emitter {
    pub fn new(bus: Rc<RefCell<EventBus>>) -> Emitter;
    pub fn forward(&self, payload: &EventPayload);
}

// event_bus
pub struct EventBus { /* 字段见 ownership_graph */ }
impl EventBus {
    pub fn new() -> EventBus;
    pub fn register(&mut self, event: &str, handler: Rc<RefCell<Handler>>);
    pub fn emit(&self, event: &str, payload: &EventPayload);
}

// handler
pub struct Handler { /* 字段见 ownership_graph */ }
impl Handler {
    pub fn new(emitter: Weak<RefCell<Emitter>>) -> Handler;
    pub fn handle(&self, payload: &EventPayload);
}
```

## 3. ownership_graph（对象引用环边表 + Rc/Weak/Box 决策 — 图级决策，单文件视角看不到）

对象引用环：`Emitter --bus--> EventBus --handlers--> Handler --emitter--> Emitter`（与 import 环同向）。
共享可变 + 多持有者 → `Rc<RefCell<T>>`；环必须有且仅有一条 `Weak` 回边，否则 `Rc` 强引用环泄漏。

| 持有者字段 | 指向 | 决策 | 理由 |
|-----------|------|------|------|
| `Emitter.bus` | `EventBus` | `Rc<RefCell<EventBus>>`（强） | Emitter 拥有 bus，转发需可变借用 |
| `EventBus.handlers` | `Vec<Handler>` | `HashMap<EventName, Vec<Rc<RefCell<Handler>>>>`（强） | bus 注册并拥有多个 handler |
| **`Handler.emitter`** | `Emitter` | **`Weak<RefCell<Emitter>>`（弱，破环回边）** | **图级决策：此边设 Weak 打破强引用环；handle 时 `upgrade()` 临时提升** |

字段定义（锁定）：
```rust
pub struct Emitter   { bus: Rc<RefCell<EventBus>> }
pub struct EventBus  { handlers: HashMap<EventName, Vec<Rc<RefCell<Handler>>>> }
pub struct Handler   { emitter: Weak<RefCell<Emitter>> }
```

破环断言（实现门）：构图后 `Rc::strong_count(&emitter) == 1`（仅根 binding 持强引用，Handler 持 Weak 不计入）。

## 4. error_model

本组无 fallible 操作：TS 方法均 `: void`，无 throw。**组共享 Error：无**。
所有方法返回 `()`，不返回 `Result`（与 TS 一致）。

`handle` 中 `Weak::upgrade()` 失败（emitter 已释放）→ `if let Some(..)` 静默跳过。
**这是 Weak 破环引入的、TS 中不存在的分支**：TS GC 下 handler 可达即 emitter 强可达，
不会出现「持有的 emitter 已回收」；Rust 用 Weak 打破强环后，emitter 可能先于 handler 释放，
故须显式处理悬垂。属忠实翻译为保证内存安全所做的**所有权模型差异**（见 §3），非语义 bug。

## 5. visibility

- 三个 struct + `new`/`forward`/`register`/`emit`/`handle`：`pub`（对应 TS `export class` + public 方法）。
- 字段 `bus`/`handlers`/`emitter`：私有（对应 TS `private`）。

## 6. cross_file_calls（依赖索引 — 三向调用环，镜像 import 环）

| 调用方 | 被调 | 签名 |
|--------|------|------|
| `emitter::Emitter::forward` | `event_bus::EventBus::emit` | `emit(&self, event: &str, payload: &EventPayload)` |
| `event_bus::EventBus::emit` | `handler::Handler::handle` | `handle(&self, payload: &EventPayload)` |
| `handler::Handler::handle` | `emitter::Emitter::forward`（经 `Weak::upgrade`） | `forward(&self, payload: &EventPayload)` |

逐文件 agent 译某文件时，跨文件被调符号一律按本表签名调用，**不重新推断类型**。
