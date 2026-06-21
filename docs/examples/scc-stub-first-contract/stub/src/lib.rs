//! SCC 组 stub 骨架（契约门）。
//!
//! 由「契约步骤」整组一次产出：struct/fn 签名齐全、所有权类型已定、函数体全 `todo!()`。
//! `cargo check` 通过即证明跨文件签名一致、所有权类型可解析 —— 契约 valid。
//! 逐文件翻译 agent 的输入即此骨架 + contract.md + 单文件源码，任务仅填 `todo!()`，签名锁定。
//!
//! stub 阶段函数体未填，字段/参数尚未被读取，故允许相关告警。
#![allow(dead_code, unused_variables)]

pub mod emitter;
pub mod event_bus;
pub mod handler;
pub mod shared;
