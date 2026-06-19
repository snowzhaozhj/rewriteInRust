//! Fuzz target: 随机字节作为 TS 源码输入 TypeScriptAdapter::analyze_file。
//!
//! 验证 tree-sitter TS 解析器面对畸形/随机输入不会 panic，
//! 应正常返回 Error 或解析出（可能为空的）结果。
//!
//! 手动跑 24h 全量 fuzz：
//!   cd cli/fuzz
//!   cargo +nightly fuzz run build_graph_fuzz -- -max_total_time=86400
//!
//! 快速冒烟（10 秒）：
//!   cargo +nightly fuzz run build_graph_fuzz -- -max_total_time=10

#![no_main]

use libfuzzer_sys::fuzz_target;
use rustmigrate_core::lang::typescript::TypeScriptAdapter;
use rustmigrate_core::lang::LanguageAdapter;

fuzz_target!(|data: &[u8]| {
    // 将随机字节解释为 UTF-8 字符串（无效字节替换为 U+FFFD）
    let source = String::from_utf8_lossy(data);

    // 创建 TypeScript 适配器
    let mut adapter = match TypeScriptAdapter::new() {
        Ok(a) => a,
        // 适配器初始化失败不算 crash，直接跳过
        Err(_) => return,
    };

    // 对随机输入调用 analyze_file，不应 panic
    // 返回 Ok（空或部分结果）或 Err 均可接受
    let _ = adapter.analyze_file(&source, "fuzz_input.ts");
});
