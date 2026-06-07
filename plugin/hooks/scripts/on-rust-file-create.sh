#!/usr/bin/env bash
set -euo pipefail
# PostToolUse: 新 .rs 文件创建时自动运行 cargo clippy
# matcher 为 Write（仅创建/覆写文件时触发，Edit 不触发）

INPUT=$(cat)
FILE_PATH=$(printf '%s' "$INPUT" | jq -r '.tool_input.file_path // empty')

# 仅对 .rs 文件触发
if [[ "$FILE_PATH" != *.rs ]]; then
  exit 0
fi

# 定位 Cargo.toml 所在目录
CARGO_DIR=$(cargo locate-project --message-format plain 2>/dev/null | xargs dirname 2>/dev/null || echo ".")
cd "$CARGO_DIR"

# 运行 clippy，输出诊断结果
# 使用 || true 避免 clippy 发现 warning 时脚本以非零退出
echo "--- cargo clippy (triggered by: ${FILE_PATH}) ---"
cargo clippy -- -D warnings 2>&1 || true
