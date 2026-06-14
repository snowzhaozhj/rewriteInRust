#!/usr/bin/env bash
set -euo pipefail
# Sprint 级全量验证
# 设计文档参考：06-plugin-structure.md § 10.3；skills/migrate/review.md Step 1
#
# 全量验证 = 供应链审计（cargo deny + cargo audit）+ 复用 verify.sh 的模块级检查
# （nextest + clippy）。**须在 rust_root（cargo workspace 根，含 Cargo.toml）下调用**。
# 工具未安装时优雅降级（warning + 跳过，不整体 fail）——把”工具缺失”与”审计真失败”
# 区分开（Live 实测这俩工具常缺失）。CWD 无 Cargo.toml 则跳过供应链审计（warning），
# 但仍执行 verify.sh（nextest/clippy）——若 verify.sh 也因无清单失败则整体 fail（正确行为：
# 调用方须确保在 rust_root 下调用）。

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# 供应链审计须在含 Cargo.toml 的目录运行；CWD 无清单则跳过（warning），不误判为审计失败。
if [[ ! -f Cargo.toml ]]; then
  echo "WARNING: 当前目录无 Cargo.toml（full-verify.sh 须在 rust_root 下调用），跳过供应链审计（cargo deny/audit）" >&2
else
  if command -v cargo-deny >/dev/null 2>&1; then
    echo "==> cargo deny check"
    cargo deny check
  else
    echo "WARNING: cargo-deny 未安装，跳过依赖许可证/来源审计（cargo deny check）" >&2
  fi

  if command -v cargo-audit >/dev/null 2>&1; then
    echo "==> cargo audit"
    cargo audit
  else
    echo "WARNING: cargo-audit 未安装，跳过已知漏洞审计（cargo audit）" >&2
  fi
fi

# 复用模块级检查（nextest + clippy）。verify.sh 内部已处理 CLIPPY_CONF_DIR 回退（见 #7）。
echo "==> 模块级检查（verify.sh：nextest + clippy）"
bash "$SCRIPT_DIR/verify.sh"
