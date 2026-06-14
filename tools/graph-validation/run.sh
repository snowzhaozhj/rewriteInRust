#!/usr/bin/env bash
# 源码图差分校验主驱动（详见 README.md）。
#
# 对 repos.txt 中每个钉版本仓库，差分对比「自研文件级 import 图」与
# 「dependency-cruiser ∩ dpdm」双 oracle 交集，输出 reports/<name>.md。
# 硬门：对双 oracle 交集的边，自研图召回率 ≥ 0.98 且环集合一致（见 compare.js）。
#
# 用法：
#   ./run.sh            跑 repos.txt 全部仓库
#   ./run.sh <name>     只跑指定仓库（调试用）
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
ORACLE_BIN="$SCRIPT_DIR/oracle/node_modules/.bin"
WORK_DIR="$SCRIPT_DIR/.work"
REPORT_DIR="$SCRIPT_DIR/reports"
ONLY="${1:-}"

mkdir -p "$WORK_DIR" "$REPORT_DIR"

# --- setup：oracle 工具 + 自研 example 二进制 ---
if [[ ! -x "$ORACLE_BIN/depcruise" ]]; then
  echo "[setup] 安装 oracle 工具（dependency-cruiser / dpdm）..." >&2
  (cd "$SCRIPT_DIR/oracle" && npm install)
fi
echo "[setup] 编译 dump_import_graph example..." >&2
(cd "$REPO_ROOT/cli" && cargo build -p rustmigrate-core --example dump_import_graph) >&2
DUMP_BIN="$REPO_ROOT/cli/target/debug/examples/dump_import_graph"

declare -a SUMMARIES=()

# repos.txt 格式：<name> <git_url> <commit_sha> <src_root>，# 开头为注释。
while read -r name url sha src_root || [[ -n "${name:-}" ]]; do
  [[ -z "${name:-}" || "$name" == \#* ]] && continue
  [[ -n "$ONLY" && "$name" != "$ONLY" ]] && continue

  echo "==== [$name] $url @ ${sha:0:12} (src=$src_root) ====" >&2
  repo="$WORK_DIR/$name"
  work="$WORK_DIR/$name.out"
  mkdir -p "$work"

  # 1. clone + checkout 钉死 sha（幂等）
  if [[ ! -d "$repo/.git" ]]; then
    echo "[$name] clone..." >&2
    git clone --quiet "$url" "$repo"
  fi
  if ! git -C "$repo" cat-file -e "${sha}^{commit}" 2>/dev/null; then
    git -C "$repo" fetch --quiet --depth 1 origin "$sha" || git -C "$repo" fetch --quiet origin
  fi
  git -C "$repo" checkout --quiet "$sha"

  abs_src="$repo/$src_root"

  # 2. 自研图 dump → 归一化
  echo "[$name] 自研图 dump..." >&2
  "$DUMP_BIN" "$abs_src" > "$work/self-dump.json"
  node "$SCRIPT_DIR/compare/normalize-self.js" < "$work/self-dump.json" > "$work/self-norm.json"

  # 3. dependency-cruiser（主 oracle）→ 解析归一化
  echo "[$name] dependency-cruiser..." >&2
  tsconfig_arg=()
  [[ -f "$repo/tsconfig.json" ]] && tsconfig_arg=(--ts-config "$repo/tsconfig.json")
  # --ts-pre-compilation-deps：跟踪「编译后消失」的 type-only import，与 dpdm/自研口径一致
  # （否则 dc 默认丢弃纯类型依赖，边数严重偏少且算不出 type-only 参与的环）。
  ( cd "$repo" && "$ORACLE_BIN/depcruise" --no-config --ts-pre-compilation-deps --output-type json \
      "${tsconfig_arg[@]}" "$src_root" ) > "$work/dc-raw.json"
  node "$SCRIPT_DIR/oracle/parse-depcruise.js" "$repo" "$src_root" \
      < "$work/dc-raw.json" > "$work/dc-norm.json"

  # 4. dpdm（交叉验证 oracle）→ 解析归一化
  echo "[$name] dpdm..." >&2
  ( cd "$repo" && "$ORACLE_BIN/dpdm" --no-warning --no-progress \
      --output "$work/dpdm-raw.json" "$src_root/**/*.ts" >/dev/null ) || true
  node "$SCRIPT_DIR/oracle/parse-dpdm.js" "$repo" "$src_root" \
      < "$work/dpdm-raw.json" > "$work/dpdm-norm.json"

  # 5. 差分对比 → 报告 + stdout summary（JSON 一行）
  echo "[$name] compare..." >&2
  summary="$( node "$SCRIPT_DIR/compare/compare.js" \
      --self "$work/self-norm.json" \
      --depcruise "$work/dc-norm.json" \
      --dpdm "$work/dpdm-norm.json" \
      --name "$name" --sha "$sha" --src "$src_root" \
      --out "$REPORT_DIR/$name.md" )"
  echo "$summary"
  SUMMARIES+=("$summary")
done < "$SCRIPT_DIR/repos.txt"

# --- 汇总 ---
echo "" >&2
echo "==== 汇总（详见 reports/）====" >&2
printf '%s\n' "${SUMMARIES[@]}"
