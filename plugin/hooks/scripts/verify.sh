#!/usr/bin/env bash
set -euo pipefail
# F2: 模块完成后验证
# 设计文档参考：06-plugin-structure.md § 10.3 F2

# 定位 Clippy 配置目录（.rust-migration/ 不是 rust_root 祖先目录，Clippy 默认查找无法命中）。
# 优先级：MIGRATION_ROOT 显式 override > git 顶层/.rust-migration > 从 $PWD 向上搜最近含 .rust-migration 的祖先。
# 注意 verify.sh 在 rust_root（如 rust-src/）下执行，而 .rust-migration/ 位于项目根，
# 故非 git 场景不能简单用 $PWD（会错算成 rust-src/.rust-migration），须向上回溯。
# 从 $PWD 向上搜最近含 .rust-migration 的祖先；可选 $1=搜索上界（含），不越过——
# 防止 git 仓库内根无 .rust-migration 时越过仓库边界误命中仓库外祖先的 .rust-migration。
find_migration_root() {
  local boundary="${1:-/}"
  local dir="$PWD"
  while true; do
    if [[ -d "$dir/.rust-migration" ]]; then
      printf '%s\n' "$dir/.rust-migration"
      return 0
    fi
    [[ "$dir" == "$boundary" || "$dir" == "/" ]] && break
    dir="$(dirname "$dir")"
  done
  # 兜底：有边界（git 内）退回 <boundary>/.rust-migration，否则 $PWD/.rust-migration（保持原约定路径形态）
  if [[ "$boundary" != "/" ]]; then
    printf '%s\n' "$boundary/.rust-migration"
  else
    printf '%s\n' "$PWD/.rust-migration"
  fi
}

if [[ -n "${MIGRATION_ROOT:-}" ]]; then
  CLIPPY_CONF_DIR="$MIGRATION_ROOT"
else
  # git rev-parse 在非 git 仓库会以 128 退出，set -e 下会中止脚本，故吞掉退出码后走向上回溯。
  git_top="$(git rev-parse --show-toplevel 2>/dev/null || true)"
  if [[ -n "$git_top" && -d "$git_top/.rust-migration" ]]; then
    CLIPPY_CONF_DIR="$git_top/.rust-migration"
  elif [[ -n "$git_top" ]]; then
    # git 仓库内但根尚无 .rust-migration：限制在仓库边界内向上搜，不越过 git_top
    CLIPPY_CONF_DIR="$(find_migration_root "$git_top")"
  else
    CLIPPY_CONF_DIR="$(find_migration_root)"
  fi
fi
export CLIPPY_CONF_DIR

# 全量测试（含 tests/ 集成测试）：done 门的行为等价证据是 verifier 在迁移项目
# tests/ 下生成的行为等价/golden 差异 harness（集成测试，文件名不固定）；`--lib`
# 会整体漏跑这些集成测试，导致模块可在等价从未实跑时被签批 done（M3-VAL-03 实测
# 教训）。故跑全量，不限 --lib。
cargo nextest run
# --all-targets 连测试码一并 lint：golden harness 等测试文件的 dead_code/告警
# 同样应纳入 -D warnings 门，避免仅 lib 干净而测试码潜伏问题。
cargo clippy --all-targets -- -D warnings
