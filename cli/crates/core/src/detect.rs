//! 模块复杂度分档——文件 I/O + 语言分发。
//!
//! 本模块**不含任何语言特定逻辑**。分档判据（什么算"危险信号"）
//! 完全由各 `LanguageAdapter::detect_tier()` 实现决定。
//! 本层只负责：读文件 → 选 adapter → 调 `detect_tier()`。

use std::path::Path;

use crate::error::{MigrateError, Result};
use crate::lang::typescript::TypeScriptAdapter;
use crate::lang::LanguageAdapter;
use crate::types::state::ModuleTier;

/// 对单个源文件进行复杂度分档。
pub fn detect_tier(file_path: &Path) -> Result<ModuleTier> {
    let source = std::fs::read_to_string(file_path).map_err(MigrateError::Io)?;
    let mut adapter = TypeScriptAdapter::new()?;
    if !adapter.can_handle(file_path) {
        return Ok(ModuleTier::Full);
    }
    Ok(adapter.detect_tier(&source))
}

/// 从源码字符串分档（供测试使用，默认 TypeScript）。
pub fn detect_tier_from_source(source: &str) -> Result<ModuleTier> {
    let mut adapter = TypeScriptAdapter::new()?;
    Ok(adapter.detect_tier(source))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trivial_pure_types() {
        let source = r#"
export interface User {
    id: string;
    name: string;
}

export type UserId = string;

export enum Role {
    Admin,
    User,
    Guest,
}
"#;
        assert_eq!(
            detect_tier_from_source(source).unwrap(),
            ModuleTier::Trivial
        );
    }

    #[test]
    fn trivial_barrel_reexport() {
        let source = r#"
export { User, UserId } from './types';
export * from './constants';
"#;
        assert_eq!(
            detect_tier_from_source(source).unwrap(),
            ModuleTier::Trivial
        );
    }

    #[test]
    fn trivial_const_literals() {
        let source = r#"
export const MAX_RETRIES = 3;
export const API_URL = "https://api.example.com";
export const ENABLED = true;
"#;
        assert_eq!(
            detect_tier_from_source(source).unwrap(),
            ModuleTier::Trivial
        );
    }

    #[test]
    fn standard_simple_function() {
        let source = r#"
export function add(a: number, b: number): number {
    return a + b;
}
"#;
        assert_eq!(
            detect_tier_from_source(source).unwrap(),
            ModuleTier::Standard
        );
    }

    #[test]
    fn standard_class_no_async() {
        let source = r#"
export class Calculator {
    add(a: number, b: number): number {
        return a + b;
    }
}
"#;
        assert_eq!(
            detect_tier_from_source(source).unwrap(),
            ModuleTier::Standard
        );
    }

    #[test]
    fn full_async_function() {
        let source = r#"
export async function fetchData(url: string): Promise<string> {
    const response = await fetch(url);
    return response.text();
}
"#;
        assert_eq!(detect_tier_from_source(source).unwrap(), ModuleTier::Full);
    }

    #[test]
    fn full_try_catch() {
        let source = r#"
export function safeParse(json: string): unknown {
    try {
        return JSON.parse(json);
    } catch (e) {
        return null;
    }
}
"#;
        assert_eq!(detect_tier_from_source(source).unwrap(), ModuleTier::Full);
    }

    #[test]
    fn full_promise_all() {
        let source = r#"
export function fetchAll(urls: string[]) {
    return Promise.all(urls.map(u => fetch(u)));
}
"#;
        assert_eq!(detect_tier_from_source(source).unwrap(), ModuleTier::Full);
    }

    #[test]
    fn full_io_import() {
        let source = r#"
import * as fs from 'fs';

export function readConfig(): string {
    return fs.readFileSync('config.json', 'utf-8');
}
"#;
        assert_eq!(detect_tier_from_source(source).unwrap(), ModuleTier::Full);
    }

    #[test]
    fn full_global_mutable_state() {
        let source = r#"
let counter = 0;

export function increment(): number {
    counter += 1;
    return counter;
}
"#;
        assert_eq!(detect_tier_from_source(source).unwrap(), ModuleTier::Full);
    }

    #[test]
    fn full_throw_statement() {
        let source = r#"
export function validate(x: number): void {
    if (x < 0) {
        throw new Error("negative");
    }
}
"#;
        assert_eq!(detect_tier_from_source(source).unwrap(), ModuleTier::Full);
    }

    #[test]
    fn full_conditional_type() {
        let source = r#"
export type IsString<T> = T extends string ? true : false;

export function check<T>(value: T): IsString<T> {
    return (typeof value === 'string') as any;
}
"#;
        assert_eq!(detect_tier_from_source(source).unwrap(), ModuleTier::Full);
    }

    #[test]
    fn trivial_non_exported_types() {
        let source = r#"
interface Internal {
    x: number;
}

type Id = string;
"#;
        assert_eq!(
            detect_tier_from_source(source).unwrap(),
            ModuleTier::Trivial
        );
    }

    #[test]
    fn standard_arrow_function_export() {
        let source = r#"
export const greet = (name: string): string => {
    return `Hello, ${name}`;
};
"#;
        assert_eq!(
            detect_tier_from_source(source).unwrap(),
            ModuleTier::Standard
        );
    }

    #[test]
    fn full_set_timeout() {
        let source = r#"
export function delay(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
}
"#;
        assert_eq!(detect_tier_from_source(source).unwrap(), ModuleTier::Full);
    }

    #[test]
    fn empty_file_is_trivial() {
        assert_eq!(detect_tier_from_source("").unwrap(), ModuleTier::Trivial);
    }

    #[test]
    fn full_top_level_expression() {
        let source = r#"
console.log("side effect");
"#;
        assert_eq!(detect_tier_from_source(source).unwrap(), ModuleTier::Full);
    }

    #[test]
    fn full_math_operations() {
        let source = r#"
export function clamp(x: number, min: number, max: number): number {
    return Math.max(min, Math.min(max, x));
}
"#;
        assert_eq!(detect_tier_from_source(source).unwrap(), ModuleTier::Full);
    }

    #[test]
    fn full_parse_float() {
        let source = r#"
export function parse(s: string): number {
    return parseFloat(s);
}
"#;
        assert_eq!(detect_tier_from_source(source).unwrap(), ModuleTier::Full);
    }

    #[test]
    fn full_typeof_guard() {
        let source = r#"
export function isString(x: unknown): x is string {
    return typeof x === "string";
}
"#;
        assert_eq!(detect_tier_from_source(source).unwrap(), ModuleTier::Full);
    }

    #[test]
    fn full_as_any_cast() {
        let source = r#"
export function unsafe_cast(x: number): string {
    return x as any;
}
"#;
        assert_eq!(detect_tier_from_source(source).unwrap(), ModuleTier::Full);
    }
}
