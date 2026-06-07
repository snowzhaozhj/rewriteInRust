#!/bin/bash
# F2: 模块完成后验证
set -e
cargo nextest run --lib
cargo clippy -- -D warnings
