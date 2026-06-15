# 项目状态快照

> 每次会话结束前更新。新会话读此文件 → 找到 PLAN.md 对应任务 → 继续执行。

## 当前位置

- **Milestone**: M1 MVP ✅ → **M2 质量提升**
- **Phase**: M2 计划**已复审定稿**（`docs/PLAN-M2.md`，复审台账 `docs/review/M2-plan-review-2026-06-15.md`），待执行
- **下一步**（**新会话从这里开始**）: 读 `docs/PLAN-M2.md` + 复审台账，从 **Sprint A 基础加固** 开始；Sprint A 先做 **M2-DESIGN-03**（落实 D3/D4/D5 对设计文档的同步，避免实现漂移）
- **复审结论**：草稿方向正确，已修正 3 处自相矛盾 + 1 处悬空引用 + 撤销 tier_signals 过度设计 + 补 6 项缺口；新增 D5（SQLite 集中 writer）+ 3 任务（DESIGN-03/PERF-BASE/CLI-06 auto-unblock）；任务总数 52→55。3 个战略决策经用户批准（SQLite 门禁降级 / 60min 单模块 / 状态机程序化推迟+抽 auto-unblock）

## M1 完成总结

| Phase | 内容 | PR | 测试 |
|-------|------|-----|------|
| M0 Sprint 0 | Spike S0/S3 假设验证 | — | — |
| Phase 0 | 冻结合约（types/error/response/schema） | — | cargo check |
| Phase 1 | 四路并行实现（graph/state/profile/hooks） | PR #5 | 121→202 |
| Phase 2 | 集成验证（14 命令路由 + E2E） | PR #3 | +25 e2e |
| Phase 3 | Plugin 实现（4 agent + SKILL + hooks） | PR #8/#9 | Live 验证 |
| Phase 4 | 翻译循环 + MVP 验收 | PR #9 | 4 fixture Live |
| §9.5 | analyze→run 衔接 + 审查加固 | PR #10 | +3 e2e, 202 总 |

**M1 验收（§9 + §9.5）**：
- linear(3 模块) + diamond(5 模块) 完整迁移到 done，nextest 33/33 + 12/12、clippy 零
- circular 环暂停正确；edge 含 M2 特性不 done（验证鲁棒性）
- review 仪表板、断点续传均验证通过
- 质量门：202 测试 | clippy -D warnings 零 | fmt | shellcheck | design-checker 零 MISMATCH

**M1 已知限制（沉淀到 M2）**：
- diamond 靠决策注入跑通，headless 无人值守撞 TODO(port) 必卡 → M2「默认 TODO 决策策略」
- 单文件 module + 完整 11 步循环 + 串行对真实项目不实用 → M2-TIER-01 + M2-SCALE
- populate 孤儿清理 + 契约加固 → M2-VER-04

## M2 起点

### M2 计划概览（详见 `docs/PLAN-M2.md`）

```
Sprint A (基础加固)  → Sprint B (类型+图精度) → Sprint C (核心功能双线)
  → Sprint D (并行+高级) ‖ Sprint E (验证+CLI) → Sprint F (验收)
```

- **52 项任务 + 5 项验收活动**，预计 25-33 天纯开发（日历 5-7 周）
- 4 个设计决策已在计划中推荐（done 终态/blocked 规则/写隔离方案/分档策略）
- M1 deferred TODO 已分配到对应 M2 任务（ADV-08/09, REFAC-13）
- 部分设计文档 M2 交付物推迟到 M2.5/M3（状态机程序化、行为录制框架等）

### M1 历史归档

M1 各 Phase 的详细审查修复记录、提交历史、Live 验证产物见 [STATUS-M1-archive.md](STATUS-M1-archive.md)。
