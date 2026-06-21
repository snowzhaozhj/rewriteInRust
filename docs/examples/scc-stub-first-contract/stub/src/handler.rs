//! 源: src/handler.ts（组内）
use std::cell::RefCell;
use std::rc::Weak;

use crate::emitter::Emitter;
use crate::shared::EventPayload;

/// `export class Handler`
pub struct Handler {
    /// `constructor(private emitter: Emitter)` —— **Weak 破环回边**（见 contract §3 ownership_graph）
    emitter: Weak<RefCell<Emitter>>,
}

impl Handler {
    pub fn new(emitter: Weak<RefCell<Emitter>>) -> Handler {
        todo!()
    }

    /// `handle(payload: EventPayload): void` —— upgrade Weak 后调 emitter::Emitter::forward
    pub fn handle(&self, payload: &EventPayload) {
        todo!()
    }
}
