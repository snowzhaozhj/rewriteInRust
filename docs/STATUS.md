# 项目状态快照

> 每次会话结束前更新。新会话读此文件 → 找到 PLAN.md 对应任务 → 继续执行。

## 当前位置

- **Milestone**: M0 假设验证
- **Phase**: Sprint 0（即将开始）
- **基线 commit**: 待本次提交后填入

## 进行中的任务

_无_

## 下一步

1. 执行 **M0-S0**（Plugin 加载验证）
   - 将 `plugin/` 安装到 Claude Code，验证 skill/agent/hook 三件套
2. 执行 **M0-S3**（tree-sitter 精度验证）
   - 用 fixtures/ 中的 TS 文件测试 tree-sitter-typescript 提取精度

两个 Spike 可并行。

## 阻塞项

_无_

## Handoff Note

**本次完成**：实施蓝图重构
- PLAN.md 全量重写（15 节，662 行，覆盖 M0-M4）
- 4 个 TS fixture 创建（含 ground-truth.json 偏序约束）
- CLI crate 加 [lib] target（run_with_args 泛型 Write，支持 in-process 测试）
- deny.toml 创建
- CLAUDE.md 补充（fixture 说明 + 质量门 + 续接参考）

**非显而易见发现**：
- tree-sitter 0.22 + tree-sitter-typescript 0.21 当前能编译通过（Workflow 声称的版本不兼容是幻觉）
- shellcheck 未安装，CI 中 `just shellcheck` 会失败（可选依赖）
- cargo-deny 未安装，`just deny` 需要先 `cargo install cargo-deny`

**对下游影响**：
- Sprint 0 Spike S3 现在有 fixture 可用，直接用 `fixtures/linear-deps/src/` 测试
- Phase 0 冻结合约时，types/ 目录结构已在 PLAN.md §1 确定
- Phase 1 并行时每个 Worker 的文件所有权已明确

## 最近完成

| 时间 | 任务 | commit |
|------|------|--------|
| 2026-06-07 | 项目脚手架初始化 | 559da00 |
| 2026-06-07 | PLAN.md v1 + CLAUDE.md + STATUS.md | fd18544 |
| 2026-06-07 | 实施蓝图重构（PLAN.md v2 + fixtures + tooling） | (本次) |
