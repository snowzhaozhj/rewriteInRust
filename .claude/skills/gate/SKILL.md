---
name: gate
description: 4 层质量门一键检查（fmt + lint + test + 设计文档对照）
---

依次执行，任一层失败即报告并停止：

1. `cd cli && cargo fmt --check && cargo clippy -- -D warnings`
2. `cd cli && cargo test -p rustmigrate-core`
3. `cd cli && cargo test`（全 workspace）
4. 对本次变更文件按 `.claude/agents/design-checker.md` 映射表做快速一致性扫描

输出汇总：每层 ✅/❌ + 失败项列表。
