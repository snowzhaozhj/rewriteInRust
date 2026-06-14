#!/usr/bin/env bash
# 符号级精度对比驱动：自研 tree-sitter 启发式 vs ts-morph 类型检查器（真值）。
#
# 对 repos.txt 中每个钉版本仓库，对比 Calls / Extends / Implements 三类符号级边，
# 文件级聚合（caller_file→callee_file），输出 reports/<name>-symbol.md。
# 软门：F1 < 0.7 标注「启发式效果偏低」，退出码恒 0（不阻断）——启发式精度必然
# 低于类型系统，硬门会恒红，故仅作观测。详见 SYMBOL-PRECISION.md。
#
# 用法：
#   ./run-symbol.sh            跑 repos.txt 全部仓库
#   ./run-symbol.sh rxjs       只跑指定仓库
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
WORK_DIR="$SCRIPT_DIR/.work"
REPORT_DIR="$SCRIPT_DIR/reports"
ONLY="${1:-}"
mkdir -p "$WORK_DIR" "$REPORT_DIR"

# --- setup：ts-morph oracle（钉于 package.symbol.json）+ 自研 example ---
if [[ ! -d "$SCRIPT_DIR/oracle/node_modules/ts-morph" ]]; then
  tsm_ver="$(node -e 'console.log(require("'"$SCRIPT_DIR"'/oracle/package.symbol.json").dependencies["ts-morph"])')"
  echo "[setup] 安装 ts-morph@$tsm_ver..." >&2
  ( cd "$SCRIPT_DIR/oracle" && npm install "ts-morph@$tsm_ver" )
fi
echo "[setup] 编译 dump_symbol_graph example..." >&2
( cd "$REPO_ROOT/cli" && cargo build -p rustmigrate-core --example dump_symbol_graph ) >&2
EX="$REPO_ROOT/cli/target/debug/examples/dump_symbol_graph"

while read -r name url sha src_root || [[ -n "${name:-}" ]]; do
  [[ -z "${name:-}" || "$name" == \#* ]] && continue
  [[ -n "$ONLY" && "$name" != "$ONLY" ]] && continue

  repo="$WORK_DIR/$name"
  work="$WORK_DIR/$name.sym"
  mkdir -p "$work"

  # clone + checkout 钉死 sha（幂等，复用文件级 run.sh 已克隆的 .work/<name>）
  if [[ ! -d "$repo/.git" ]]; then
    echo "[$name] clone..." >&2
    git clone --quiet "$url" "$repo"
  fi
  git -C "$repo" checkout --quiet "$sha" 2>/dev/null \
    || { git -C "$repo" fetch --quiet --depth 1 origin "$sha" && git -C "$repo" checkout --quiet "$sha"; }

  echo "==== [$name] 符号级对比 ====" >&2
  "$EX" "$repo/$src_root" > "$work/self-sym.json"
  if ! node "$SCRIPT_DIR/oracle/symbol-graph-tsmorph.js" "$repo" "$src_root" \
        > "$work/oracle-sym.json" 2>"$work/tsmorph-err.txt"; then
    echo "[$name] ✗ ts-morph 提取失败（跳过）:" >&2
    tail -5 "$work/tsmorph-err.txt" >&2
    continue
  fi
  node "$SCRIPT_DIR/compare/compare-symbol.js" \
    --self "$work/self-sym.json" --oracle "$work/oracle-sym.json" \
    --name "$name" --sha "$sha" --src "$src_root" \
    --out "$REPORT_DIR/$name-symbol.md"
done < "$SCRIPT_DIR/repos.txt"
