#!/usr/bin/env bash
set -euo pipefail
# Sprint 级全量验证
# 设计文档参考：06-plugin-structure.md § 10.3；skills/migrate/review.md Step 1
#
# 全量验证 = 供应链审计（cargo deny + cargo audit）+ 复用 verify.sh 的模块级检查
# （nextest + clippy）。cargo-deny / cargo-audit 未安装时优雅降级（warning + 跳过，
# 不整体 fail）——Live 实测环境这俩工具常缺失。

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# 供应链审计：工具缺失时降级为 warning 跳过，存在则严格执行（失败仍 fail）。
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

# 复用模块级检查（nextest + clippy）。verify.sh 内部已处理 CLIPPY_CONF_DIR 回退（见 #7）。
echo "==> 模块级检查（verify.sh：nextest + clippy）"
bash "$SCRIPT_DIR/verify.sh"
