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

# 覆盖率
cov:
    cd cli && cargo llvm-cov --workspace --lcov --output-path lcov.info

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
