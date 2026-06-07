# 项目状态快照

> 每次会话结束前更新。新会话读此文件 → 找到 PLAN.md 对应任务 → 继续执行。

## 当前位置

- **Milestone**: M0 假设验证 → **已完成**
- **Phase**: Sprint 0 ✅
- **下一步**: Phase 0（冻结合约，M1-TYPES）

## 进行中的任务

_无_

## 下一步

1. 执行 **Phase 0（M1-TYPES）**：冻结合约
   - 定义 types/common.rs, graph.rs, state.rs, config.rs
   - 定义 error.rs, response.rs
   - 创建 schema.sql
   - 参照 `docs/design/09-appendix-schemas.md` 和 `docs/design/04-toolchain.md`

2. Phase 0 完成后进入 **Phase 1（四路并行实现）**

## 阻塞项

- Plugin Live 验证（skill/agent/hook 实际触发）需在交互式会话中补全
  - 影响范围：仅 Phase 3（Plugin 实现），不阻塞 Phase 0-2

## Handoff Note

**本次完成**：M0 Sprint 0 两个 Spike

### Spike S0: Plugin 加载 + crate 编译 ✅

- plugin.json / skills / agents / hooks 结构验证通过
- Release 二进制 743K（LTO thin + strip），远低于 50MB
- 决策文档: `docs/decisions/001-plugin-viability.md`
- **待补全**: Live 验证（需交互式 Claude Code 会话测试 skill 触发、agent 调用、hook fire）

### Spike S3: tree-sitter TS 精度 ✅

- 20 个 TS 代码片段精度基准测试
- **Exports F1 = 1.000** (27/27)
- **Imports F1 = 1.000** (23/23)
- **Calls F1 = 1.000** (24/24)
- 覆盖 7 种 export 模式、7 种 import 模式（含 dynamic、type-only）、4 种 call 模式
- 已有 fixture 文件（14 个 TS 文件）同样正确提取
- 决策文档: `docs/decisions/002-parser-choice.md`
- 提取器代码: `cli/crates/core/src/ts_extract.rs`（M1 重构到 graph/build.rs）

### 产出物清单

| 新增文件 | 说明 |
|---------|------|
| `cli/crates/core/src/ts_extract.rs` | tree-sitter TS 提取模块 |
| `cli/crates/core/tests/tree_sitter_precision.rs` | 精度基准测试（F1 ≥ 0.90 断言） |
| `fixtures/ts-precision-bench/snippets/*.ts` | 20 个精度基准 TS 代码片段 |
| `docs/decisions/001-plugin-viability.md` | Plugin 可行性决策记录 |
| `docs/decisions/002-parser-choice.md` | 解析器选型决策记录 |

### Sprint 0 GATE 状态

- [x] Spike S0 有结论（Pass: 结构验证 + 二进制 743K）
- [x] Spike S3 有结论（Pass: F1 = 1.0 >> 0.90）
- [x] 决策文档写入 `docs/decisions/`
- [x] M1 执行路径已确定：tree-sitter + Plugin 架构

## 最近完成

| 时间 | 任务 | commit |
|------|------|--------|
| 2026-06-07 | 项目脚手架初始化 | 559da00 |
| 2026-06-07 | PLAN.md v1 + CLAUDE.md + STATUS.md | fd18544 |
| 2026-06-07 | 实施蓝图重构（PLAN.md v2 + fixtures + tooling） | c3acc34 |
| 2026-06-07 | M0 Sprint 0 Spike S0+S3 完成 | (本次) |
