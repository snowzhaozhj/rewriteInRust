#!/usr/bin/env bash
set -euo pipefail
# PreToolUse: 防止 agent 修改源项目文件
# 仅允许修改 .rust-migration/ 和 rust-src/ 下的文件
# 设计文档参考：06-plugin-structure.md § 10.3

INPUT=$(cat)
FILE_PATH=$(printf '%s' "$INPUT" | jq -r '.tool_input.file_path // empty')

# 无法获取文件路径时放行（Bash 工具可能无结构化路径，见设计文档 R2-D3-04）
if [[ -z "$FILE_PATH" ]]; then
  exit 0
fi

# 允许的目录：.rust-migration/ 和 rust-src/
if [[ "$FILE_PATH" == */.rust-migration/* ]] || [[ "$FILE_PATH" == */rust-src/* ]]; then
  exit 0
fi

# 允许修改 plugin 自身的文件（开发阶段）
if [[ "$FILE_PATH" == */plugin/* ]]; then
  exit 0
fi

# 其他路径：阻止修改源项目文件
echo "BLOCKED: 文件 ${FILE_PATH} 不在迁移工作目录内（.rust-migration/ 或 rust-src/），禁止修改源项目文件"
exit 2
