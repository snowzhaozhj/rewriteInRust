#!/usr/bin/env bash
set -euo pipefail
# F2: 模块完成后验证
# 设计文档参考：06-plugin-structure.md § 10.3 F2

# 定位 Clippy 配置目录（.rust-migration/ 不是 rust_root 的祖先目录）
CLIPPY_CONF_DIR="${MIGRATION_ROOT:-$(git rev-parse --show-toplevel)/.rust-migration}"
export CLIPPY_CONF_DIR

cargo nextest run --lib
cargo clippy -- -D warnings
