//! 源: src/emitter.ts（组内）
use std::cell::RefCell;
use std::rc::Rc;

use crate::event_bus::EventBus;
use crate::shared::EventPayload;

/// `export class Emitter`
pub struct Emitter {
    /// `constructor(private bus: EventBus)` —— Rc 强引用（owns）
    bus: Rc<RefCell<EventBus>>,
}

impl Emitter {
    pub fn new(bus: Rc<RefCell<EventBus>>) -> Emitter {
        Emitter { bus }
    }

    /// `forward(payload: EventPayload): void` —— 调 event_bus::EventBus::emit
    pub fn forward(&self, payload: &EventPayload) {
        self.bus.borrow().emit("forwarded", payload);
    }
}
