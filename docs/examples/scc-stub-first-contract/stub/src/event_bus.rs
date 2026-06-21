//! 源: src/event-bus.ts（组内）
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::handler::Handler;
use crate::shared::{EventName, EventPayload};

/// `export class EventBus`
#[derive(Default)]
pub struct EventBus {
    /// `handlers: Map<EventName, Handler[]>` —— Rc 强引用，bus 拥有 handler
    handlers: HashMap<EventName, Vec<Rc<RefCell<Handler>>>>,
}

impl EventBus {
    pub fn new() -> EventBus {
        todo!()
    }

    /// `register(event: EventName, handler: Handler): void`
    pub fn register(&mut self, event: &str, handler: Rc<RefCell<Handler>>) {
        todo!()
    }

    /// `emit(event: EventName, payload: EventPayload): void` —— 调 handler::Handler::handle
    pub fn emit(&self, event: &str, payload: &EventPayload) {
        todo!()
    }
}
