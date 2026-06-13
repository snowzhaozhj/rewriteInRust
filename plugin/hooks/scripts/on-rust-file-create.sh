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

# 从被编辑文件所在目录向上定位 Cargo 工程
# （hook cwd 通常是仓库根，而 Cargo 工程位于子目录或迁移目标工程，
#  必须相对文件定位；否则 locate-project 落空导致 clippy 永不实际运行）
FILE_DIR=$(dirname "$FILE_PATH")
[[ -d "$FILE_DIR" ]] || exit 0
cd "$FILE_DIR"

CARGO_TOML=$(cargo locate-project --message-format plain 2>/dev/null || true)
# 文件不在任何 Cargo 工程内则跳过
[[ -n "$CARGO_TOML" ]] || exit 0
cd "$(dirname "$CARGO_TOML")"

# 运行 clippy，输出诊断结果
# 使用 || true 避免 clippy 发现 warning 时脚本以非零退出
echo "--- cargo clippy (triggered by: ${FILE_PATH}) ---"
cargo clippy -- -D warnings 2>&1 || true
