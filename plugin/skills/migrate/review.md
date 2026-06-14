# /migrate review — 验证管线 + 进度仪表板

跑完整验证管线（Sprint 级集成验证）+ 展示迁移进度仪表板。共享约定见 [SKILL.md](./SKILL.md)。数据源是 `migration-state.json`（确定性事实）+ verifier 的验证结果。

## 前置条件
迁移已开始（至少一个模块离开 `pending`）。若无模块进展，提示用户先 `/migrate analyze` 再迁移，本命令暂无可展示数据。

## 流程

### 1. 全量验证（verifier）
调 `rust-migrate:verifier` 做全量验证 + pattern 新鲜度扫描（把过期 pattern 标 `needs-review`）。执行 `hooks/scripts/full-verify.sh`（`cargo deny check` + `cargo audit` 等 Sprint 级检查）。

### 2. 生成报告
产出 `sprint-N-report.json`（质量评分数据）；更新 `PARITY.md`（源↔Rust 模块对应 + 完成度）。

### 3. 终端仪表板
从 `migration-state.json` 聚合输出（**只读状态字段，不臆造数字**）：

| 区块 | 数据来源 |
|------|---------|
| **Sprint 进度** | `sprint.current`（目标模块数 vs 已完成）、`history[].completed_modules` |
| **模块状态分布** | 各模块 `status`：done / testing / compile_fixing / paused / degrade_* / blocked 计数 |
| **质量指标** | 各模块 `test_pass_rate`、`coverage`、clippy 警告数 |
| **止损指标** | DEGRADE 模块比例、LLM 成本、Sprint 停滞周期、质量评分回归（对照 sprint-N-report.json） |
| **验证画像** | 实际生效的 Tier 1 工具、被禁用工具及理由（`tier1_exceptions`，含 confidence）、FFI 模块覆盖率采样范围 |

对 `tier1_exceptions` 中 confidence 为 low/medium 的条目，由 verifier 在本命令中重新核查（属性测试关闭可能让纯函数等价性验证失效）。

### 4. 阻塞 / 异常提示
有 `paused` + `requires_manual_review` 或 `blocked` 模块时，明确列出并给下一步建议（如 `/migrate run <m> --retry`、解除 blocked 依赖、`--degrade=ffi` 确认降级）。

## 失败处理
verifier 调用失败按 SKILL.md「失败恢复」三步处理；验证工具（cargo-deny / audit）不可用时降级并 warning，不静默跳过。
