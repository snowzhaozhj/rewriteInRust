#!/usr/bin/env bash
set -euo pipefail
# 运行时定位 rustmigrate 二进制（供 skill 调用）
# 设计参考：plugin 通过 $PATH > 显式路径 > 预编译/本地构建产物的优先级解析 CLI。
# 用法：BIN=$(hooks/scripts/ensure-cli.sh) && "$BIN" <子命令>
# 找到具备当前 Plugin 所需能力的二进制则打印绝对路径；旧版同版本号二进制会被跳过。

# 本脚本位于 plugin/hooks/scripts/，仓库根为其上三级（scripts → hooks → plugin → 根）
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "${SCRIPT_DIR}/../../.." && pwd)

supports_required_capabilities() {
  local candidate=$1
  [[ -x "$candidate" ]] && "$candidate" state record-metrics --help >/dev/null 2>&1
}

# 优先级：① PATH ② $RUSTMIGRATE_BIN ③ release 构建产物 ④ debug 构建产物
if PATH_BIN=$(command -v rustmigrate 2>/dev/null); then
  if supports_required_capabilities "$PATH_BIN"; then
    echo "$PATH_BIN"
    exit 0
  fi
fi

if [[ -n "${RUSTMIGRATE_BIN:-}" ]] && supports_required_capabilities "${RUSTMIGRATE_BIN}"; then
  echo "$RUSTMIGRATE_BIN"
  exit 0
fi

RELEASE_BIN="${REPO_ROOT}/cli/target/release/rustmigrate"
if supports_required_capabilities "$RELEASE_BIN"; then
  echo "$RELEASE_BIN"
  exit 0
fi

DEBUG_BIN="${REPO_ROOT}/cli/target/debug/rustmigrate"
if supports_required_capabilities "$DEBUG_BIN"; then
  echo "$DEBUG_BIN"
  exit 0
fi

cat >&2 <<EOF
未找到支持当前 Plugin 所需能力（state record-metrics）的 rustmigrate CLI。
PATH 或指定路径中的旧二进制会被跳过。请任选其一重建/安装后重试：
  1. 本地构建（推荐开发）：cargo build --release --manifest-path "${REPO_ROOT}/cli/Cargo.toml"
  2. 安装到 PATH：cargo install rustmigrate
  3. 已有新二进制：export RUSTMIGRATE_BIN=/path/to/rustmigrate
解析优先级：PATH > \$RUSTMIGRATE_BIN > cli/target/release > cli/target/debug
EOF
exit 1
