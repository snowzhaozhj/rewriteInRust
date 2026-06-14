#!/usr/bin/env bash
set -euo pipefail
# 运行时定位 rustmigrate 二进制（供 skill 调用）
# 设计参考：plugin 通过 $PATH > 预编译/本地构建产物的优先级解析 CLI。
# 用法：BIN=$(hooks/scripts/ensure-cli.sh) && "$BIN" <子命令>
# 找到则打印绝对路径到 stdout 并退出 0；找不到打印中文安装指引到 stderr 并退出 1。

# 本脚本位于 plugin/hooks/scripts/，仓库根为其上三级（scripts → hooks → plugin → 根）
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "${SCRIPT_DIR}/../../.." && pwd)

# 优先级：① PATH ② $RUSTMIGRATE_BIN ③ release 构建产物 ④ debug 构建产物
if PATH_BIN=$(command -v rustmigrate 2>/dev/null); then
  echo "$PATH_BIN"
  exit 0
fi

if [[ -n "${RUSTMIGRATE_BIN:-}" && -x "${RUSTMIGRATE_BIN}" ]]; then
  echo "$RUSTMIGRATE_BIN"
  exit 0
fi

RELEASE_BIN="${REPO_ROOT}/cli/target/release/rustmigrate"
if [[ -x "$RELEASE_BIN" ]]; then
  echo "$RELEASE_BIN"
  exit 0
fi

DEBUG_BIN="${REPO_ROOT}/cli/target/debug/rustmigrate"
if [[ -x "$DEBUG_BIN" ]]; then
  echo "$DEBUG_BIN"
  exit 0
fi

# 都找不到：输出中文安装指引到 stderr
cat >&2 <<EOF
未找到 rustmigrate CLI。请任选其一安装后重试：
  1. 本地构建（推荐开发）：cargo build --release --manifest-path "${REPO_ROOT}/cli/Cargo.toml"
  2. 安装到 PATH：cargo install rustmigrate
  3. 已有二进制：export RUSTMIGRATE_BIN=/path/to/rustmigrate
解析优先级：PATH > \$RUSTMIGRATE_BIN > cli/target/release > cli/target/debug
EOF
exit 1
