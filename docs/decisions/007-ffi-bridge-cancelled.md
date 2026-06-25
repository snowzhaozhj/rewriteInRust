# MDR-007: FFI 桥接取消，degrade_skip 为唯一降级路径

- **状态**: 已决策
- **日期**: 2026-06-25
- **范围**: M3-FFI-CLOSE

## 问题

M1/M2 设计中 `degrade_ffi` 作为降级路径之一，通过 FFI binding（napi-rs）让 Rust 端调用未翻译的 TS 模块。但 napi-rs 方向是 Node.js→Rust（Node addon），与降级需求（Rust→TS）方向不匹配。M2 Sprint F 实测中 FFI 降级路径无法跑通。

## 决策

**取消 FFI 桥接**。`degrade_skip` 为唯一降级路径。

翻不了的模块：
1. 推荐 Rust 生态替代 crate（降级报告输出推荐）
2. 标记 out-of-scope，上游模块注入 `blocked_by_skip` context

## 理由

- 模块级跨运行时桥接造成状态不同步
- 调试复杂（两个运行时 + FFI 边界）
- 部署复杂（需要 Node.js 运行时 + native addon 编译链）
- M2 实测中 `degrade_skip` 已覆盖所有降级场景

## 影响

- `scaffold/ffi.rs` 中 `generate_ffi_binding` 标记 `#[deprecated]`
- `select_cycle_break_point` / `count_exports` 保留（环断点选择仍有价值）
- 设计文档 08-roadmap M3 段删除 PyO3 binding 验收指标
- PLAN.md §11 删除 PyO3 相关内容
