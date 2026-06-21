//! SCC 组实现（实现门）。
//!
//! 由 `../stub/` 骨架逐文件填 `todo!()` 得到 —— 三个成员文件各由独立 agent 填空，
//! **签名与 stub 逐字节一致**（逐文件 agent 禁改签名、禁碰 mod.rs/Cargo.toml）。
//! 整组 `cargo check && cargo test` 通过即实现门成立；`break_cycle` 测试断言
//! `Rc::strong_count(&emitter) == 1` 证明 Weak 回边正确破除强引用环。

pub mod emitter;
pub mod event_bus;
pub mod handler;
pub mod shared;

#[cfg(test)]
mod break_cycle {
    use crate::{emitter::Emitter, event_bus::EventBus, handler::Handler, shared::EventPayload};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// 构图 → 断言 Handler 的 Weak 回边不增加 Emitter 强引用 → 强引用环已破。
    #[test]
    fn weak_back_edge_breaks_strong_cycle() {
        let bus = Rc::new(RefCell::new(EventBus::new()));
        let emitter = Rc::new(RefCell::new(Emitter::new(bus.clone())));
        let handler = Rc::new(RefCell::new(Handler::new(Rc::downgrade(&emitter))));
        // 注册到非 "forwarded" 事件，避免 forward→emit("forwarded") 自递归（与 TS 同隐患）。
        bus.borrow_mut().register("ping", handler.clone());

        // 破环断言：Handler 持 Weak<Emitter>，不计入强引用 → 仅根 binding 持有。
        assert_eq!(Rc::strong_count(&emitter), 1);
        // 对照：bus / handler 各被「根 binding + 拥有者」双持 → 2。
        assert_eq!(Rc::strong_count(&bus), 2);
        assert_eq!(Rc::strong_count(&handler), 2);

        // 三向跨文件调用链可达：emit(ping)→handle→upgrade→forward→emit(forwarded)。
        // 终止性来自「未向 "forwarded" 注册 handler」（与破环是两个独立机制：终止靠不注册、
        // 破环靠 Weak）。若把本 handler 注册到 "forwarded"，Rust 与 TS 同样无限递归。
        let payload = EventPayload::new();
        bus.borrow().emit("ping", &payload);
    }

    /// Weak 在 emitter 释放后 upgrade 失败 → handle 静默跳过，无悬垂、无 panic。
    #[test]
    fn dropped_emitter_yields_dangling_weak() {
        // 独立探针 Weak，与 Handler 内部字段同指 emitter；用于离块后显式断言悬垂。
        let probe;
        let handler = {
            let bus = Rc::new(RefCell::new(EventBus::new()));
            let emitter = Rc::new(RefCell::new(Emitter::new(bus)));
            probe = Rc::downgrade(&emitter);
            // handler 仅持 Weak<emitter>；离开块时 emitter / bus 强引用归零被 drop。
            Rc::new(RefCell::new(Handler::new(Rc::downgrade(&emitter))))
        };
        // 显式断言：emitter 已 drop → 探针悬垂。若 Handler.emitter 误改回 Rc（强持有），
        // emitter 不会在离块时释放，此断言失败 → 本测试即具备捕捉破环回归的判别力。
        assert!(probe.upgrade().is_none());
        // upgrade 失败 → handle 内 `if let Some` 不进入，无 panic。
        handler.borrow().handle(&EventPayload::new());
    }
}
