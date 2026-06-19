# rustmigrate 开发任务
set dotenv-load

# 默认任务
default:
    @just --list

# === CLI ===

# 编译检查
check:
    cd cli && cargo check --workspace

# 完整构建
build:
    cd cli && cargo build --workspace

# 运行测试
test:
    cd cli && cargo nextest run --workspace --no-tests=pass

# Lint
lint:
    cd cli && cargo clippy --workspace -- -D warnings

# 格式化
fmt:
    cd cli && cargo fmt --all

# 格式检查
fmt-check:
    cd cli && cargo fmt --all -- --check

# 覆盖率（nextest 驱动，输出终端 summary + lcov 文件）
cov:
    cd cli && cargo llvm-cov nextest --workspace --lcov --output-path lcov.info

# 覆盖率 summary（仅终端输出，CI 门禁用）
cov-summary:
    cd cli && cargo llvm-cov nextest --workspace

# 依赖审计
deny:
    cd cli && cargo deny check

# === Plugin ===

# Shell 脚本 lint
shellcheck:
    shellcheck plugin/hooks/scripts/*.sh

# === 全量 CI 本地模拟 ===

ci: fmt-check lint test deny shellcheck
    @echo "✓ All CI checks passed"

# === 图差分校验（M1 验收门）===

# 源码图差分校验：自研文件级 import 图 vs dependency-cruiser ∩ dpdm 双 oracle 交集。
# 硬门：边召回 ≥0.98 且环集合一致。非 ci 项（需联网拉真实仓库），独立运行。
# 用法：just validate-graph        跑全部仓库
#       just validate-graph rxjs   只跑指定仓库
validate-graph *repo:
    tools/graph-validation/run.sh {{repo}}

# 符号级精度对比（自研 tree-sitter 启发式 vs ts-morph 类型检查器，软门不阻断）。
# 需 npm install ts-morph（钉于 oracle/package.symbol.json），首次自动安装。
validate-graph-symbol *repo:
    tools/graph-validation/run-symbol.sh {{repo}}

# === 性能基线（M2-PERF-BASE / Sprint F6 回归门）===

# release 构建后测量各 fixture graph build 时长并刷新 baseline.json（刷新须走 PR）。
perf-baseline:
    cd cli && cargo build --release --workspace
    python3 tools/perf-baseline/measure.py snapshot

# 测量当前时长并对比基线，median 超 ±10% 退出非零（F6 用，同机跑）。
perf-baseline-check:
    cd cli && cargo build --release --workspace
    python3 tools/perf-baseline/measure.py check
