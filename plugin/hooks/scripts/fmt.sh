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

# 从被编辑文件所在目录向上定位 Cargo 工程
# （hook cwd 通常是仓库根，而 Cargo 工程可能位于子目录如 cli/，
#  或为迁移生成的目标工程，故必须相对文件定位而非相对 cwd）
FILE_DIR=$(dirname "$FILE_PATH")
[[ -d "$FILE_DIR" ]] || exit 0
cd "$FILE_DIR"

CARGO_TOML=$(cargo locate-project --message-format plain 2>/dev/null || true)
# 文件不在任何 Cargo 工程内则跳过（不阻塞，不报错）
[[ -n "$CARGO_TOML" ]] || exit 0
cd "$(dirname "$CARGO_TOML")"

cargo fmt 2>&1
