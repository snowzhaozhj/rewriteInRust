# 项目状态快照

> 每次会话结束前更新。新会话读此文件 → 找到 PLAN.md 对应任务 → 继续执行。

## 当前位置

- **Milestone**: M1 MVP ✅ → **M2 质量提升**
- **Phase**: M1 全部任务完成（PR #3/#5/#7/#8/#9/#10 均已合并 master）
- **下一步**（**新会话从这里开始**）: PLAN §10 M2，优先级 **B 复杂度自适应循环（M2-TIER-01）> A 并行 sprint（M2-SCALE）> Sprint 5.5 类型重构**

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

### 优先级排序（PLAN §10）

1. **M2-TIER-01 复杂度自适应循环**（工作量 M）— trivial/standard/full 分档，维度 9 永不跳过
2. **M2-SCALE 并行 sprint**（工作量 L）— parallel_groups 已实现未消费 + worktree 隔离
3. **Sprint 5.5 类型安全重构**（M2-REFAC-01..12）— M1 审查遗留
4. **Sprint 5 验证管线增强**（M2-VER-01..04）
5. **Sprint 6 高级功能** + Sprint 7 并行与规模 + Sprint 8 M2 验收

### 未决设计决策（需团队定夺）

- `done + --force` 重做：设计行 379（暗示可重做）vs 行 209 矩阵（done 硬终态）矛盾
- blocked 进入：设计行 206「可从任何状态进入」vs 矩阵（仅 blockable 活跃态可进）

### M1 历史归档

M1 各 Phase 的详细审查修复记录、提交历史、Live 验证产物见 [STATUS-M1-archive.md](STATUS-M1-archive.md)。

### M1 deferred TODO（M2 接线时处理）

1. profile 自动定位 analysis-tools.json（需 `CLAUDE_PLUGIN_ROOT` env 约定）
2. 完整子进程超时（当前仅 stdin(null)）
3. ToolStatus 枚举化 / LocReport 派生 totals
