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
    cd cli && cargo nextest run --workspace

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
