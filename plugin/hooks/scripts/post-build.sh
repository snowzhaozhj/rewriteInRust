#!/usr/bin/env bash
set -euo pipefail
# 运行 cargo check 并将结果格式化为 JSON 输出
# 输出格式：{"status":"ok|error","data":{...}}

# 定位 Cargo.toml 所在目录
CARGO_DIR=$(cargo locate-project --message-format plain 2>/dev/null | xargs dirname 2>/dev/null || echo ".")
cd "$CARGO_DIR"

# 运行 cargo check，捕获 JSON 格式输出和退出码
# if 结构避免 set -e 提前退出
if CHECK_OUTPUT=$(cargo check --message-format=json 2>/dev/null); then
  printf '{"status":"ok","data":{"message":"cargo check passed"}}\n'
else
  # 从 cargo JSON 输出中提取编译错误
  ERRORS=$(printf '%s' "$CHECK_OUTPUT" | \
    jq -s '[.[] | select(.reason == "compiler-message") | .message | select(.level == "error") | {code: (.code.code // null), message: .message}]' 2>/dev/null || echo '[]')
  printf '{"status":"error","data":{"errors":%s}}\n' "$ERRORS"
fi
