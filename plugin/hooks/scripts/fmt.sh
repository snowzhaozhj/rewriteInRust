#!/usr/bin/env bash
set -euo pipefail
# PostToolUse: 自动格式化 .rs 文件
# matcher 为 Edit|Write，脚本内部过滤 .rs 后缀

INPUT=$(cat)
FILE_PATH=$(printf '%s' "$INPUT" | jq -r '.tool_input.file_path // empty')

# 非 .rs 文件直接放行
if [[ "$FILE_PATH" != *.rs ]]; then
  exit 0
fi

# 定位最近的 Cargo.toml 所在目录
CARGO_DIR=$(cargo locate-project --message-format plain 2>/dev/null | xargs dirname 2>/dev/null || echo ".")
cd "$CARGO_DIR"

cargo fmt 2>&1
