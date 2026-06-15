# 项目状态快照

> 每次会话结束前更新。新会话读此文件 → 找到 PLAN.md 对应任务 → 继续执行。

## 当前位置

- **Milestone**: M1 MVP ✅ → **M2 质量提升**
- **Phase**: M1 全部任务完成（PR #3/#5/#7/#8/#9/#10 均已合并 master）
- **下一步**（**新会话从这里开始**）: **PR-1 = M2-TIER-00**（TIER-01 硬前置）。范围：① 00a `ModuleState.semantic_signals` schema（7 维 `SignalState=Absent|Present|Unknown` + 可选 evidence；**删旧 `risk` 字段**；tier/risk 纯函数派生不落库；**不引入 confidence**）② 00b CLI `state annotate-module-signals` 校验落库 ③ 00c analyze.md 接线产 per-module signals。**不做分档执行逻辑**（那是 TIER-01）。详见 PLAN §10.0.1.4。
- **修订后依赖顺序**（Codex 审计，§10.0.1.9）: B 线 TIER-00→TIER-01；A 线 REFAC-06→SCALE-00→SCALE-02a→SCALE-01/02，其中 REFAC-08b(blocked_by)+ADV-07 是 SCALE-01 硬前置。REFAC-08a(reopen)/08b 作可用性前置从 Sprint 5.5 提前。

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

0. **M2-TIER-00**（TIER-01 硬前置）— analyzer 语义信号 + ModuleState 落库字段 + CLI 落库命令（§10.0）
1. **M2-TIER-01 复杂度自适应循环**（工作量 M，依赖 TIER-00）— trivial/standard/full 分档，维度 9 永不跳过
2. **M2-SCALE 并行 sprint**（工作量 L）— 前置 M2-SCALE-00（populate 消费 parallel_groups）+ ADV-07（headless TODO 决策）；写隔离 = worktree + toml 合并
3. **Sprint 5.5 类型安全重构**（M2-REFAC-01..15 + 08a）— M1 审查遗留 + done/blocked 决策落地
4. **Sprint 5 验证管线增强**（M2-VER-01..05）
5. **Sprint 6 高级功能** + Sprint 7 并行与规模 + Sprint 8 M2 验收

### 已决设计决策（2026-06-15 定夺）

- **done 终态**：硬终态。返工走**独立 `state reopen` 命令**（非扩转换矩阵，须保持 `Done => false`），记 history + audit attempt，`--force` 不再隐式重做。→ 废弃设计行 379「done+--force 重做」。**Codex 核实：`can_transition_to`（state.rs:98-124）已实现 done 硬终态，REFAC-08 主体已完成、降为 S（仅补测试）；reopen 是新命令 REFAC-08a。**
- **blocked 进入**：仅 blockable 活跃态可进（按行 209 矩阵），done/失败终态不可进。→ 废弃设计行 206。**已在 `can_transition_to` 实现**；另缺 `--blocked-by` CLI 写入口 → 新任务 REFAC-08b（SCALE 前置）。
- **per-module 难度信号来源**：analyzer 增产语义信号 → CLI 落库为 `ModuleState.semantic_signals`（7 维 `SignalState=Absent|Present|Unknown`+evidence）；**删旧 `risk` 字段**，tier/risk 纯函数派生不落库，不引入 confidence（见 PLAN §10.0.1.4）。→ 否决 populate 纯 LOC 自动评估（detect.rs 反面教材）。
- **M2-SCALE 共享写隔离**：worktree 隔离每模块 + 组末 merge。`Cargo.toml` 程序化合并取并集（SemVer 取高/冲突报错）；**`migration-state.json` 用 state delta 主 worktree 单写合并**（Codex：atomic rename 只防半截不防并发 lost update）。每 SubAgent 在自己 worktree 内独立编译+测试自验证。

### M1 历史归档

M1 各 Phase 的详细审查修复记录、提交历史、Live 验证产物见 [STATUS-M1-archive.md](STATUS-M1-archive.md)。

### M1 deferred TODO（M2 接线时处理）

1. profile 自动定位 analysis-tools.json（需 `CLAUDE_PLUGIN_ROOT` env 约定）
2. 完整子进程超时（当前仅 stdin(null)）
3. ToolStatus 枚举化 / LocReport 派生 totals
