//! 源: src/shared.ts（组外，sprint 1 预翻译依赖；签名供组内引用）
use std::collections::HashMap;

/// `export type EventName = string`
pub type EventName = String;

/// `export interface EventPayload { [key: string]: unknown }`
/// `unknown` 忠实阶段占位为 `String`。
pub type EventPayload = HashMap<String, String>;
