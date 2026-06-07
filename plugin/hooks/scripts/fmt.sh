#!/bin/bash
# PostToolUse: 自动格式化 .rs 文件
INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty')
[[ "$FILE_PATH" != *.rs ]] && exit 0
cd "$(cargo locate-project --message-format plain 2>/dev/null | xargs dirname 2>/dev/null || echo .)"
cargo fmt 2>&1
