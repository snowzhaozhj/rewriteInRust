# 项目状态快照

> 每次会话结束前更新。新会话读此文件 → 找到 PLAN.md 对应任务 → 继续执行。

## 当前位置

- **Milestone**: M1 MVP ✅ → **M2 质量提升**
- **Phase**: M2 质量提升 **Sprint A 进行中**（计划见 `docs/PLAN-M2.md`，复审台账 `docs/review/M2-plan-review-2026-06-15.md`）
- **已完成（文档系列，分支 `docs/m2-design-03`，整体一个 PR）**:
  - ✅ **M2-DESIGN-03**（commit `eb8aeb0`）——D3/D4/D5 同步：04 §5.7.3 + 08 §M2 + 06 §10.5 集中 writer(D5)；06 §10.5 新增 D3 约束包小节；03 新增 §4.3.2 TIER-01 分档(D4)。design-checker A-E 通过、2 处连带 MISMATCH 已修
  - ✅ **M2-DESIGN-01/02**（commit `13ca06a`）——done 硬终态(D1) + blocked 仅活跃状态进入(D2)，02 §3.4 对齐 09 附录 A（09 已正确，仅 02 需改）
- **文档 PR**: [#14](https://github.com/snowzhaozhj/rewriteInRust/pull/14)（DESIGN-01/02/03）审查已闭环——design-checker 4 轮 + codex 对抗审查 1 轮，查出并修复 6 处 D5/D4 同步遗漏 + 1 处 09 risk 标注缺口，最终零 MISMATCH。pr-review-toolkit/code-review skill 因纯文档 PR（代码导向）跳过
  - **追加修正（用户追问触发）**：① D5 写模型精确化——原「翻译期 db 只读 / 唯一写入口 graph build」遗漏 run 期编排器对 `migration_status` 的回写（与 PLAN-M2 D3 line 129/381 矛盾），统一为「SubAgent 只读 + 编排器集中 writer：图结构 + 终态 migration_status 回写」（04/06/08 + PLAN-M2 §2 D5）；② 04 §5.7.1 新增「迁移映射机制接线状态」blockquote——`maps_to`/`RustTarget` 为前瞻预留（当前 build.rs 不产生）、依赖链翻译不依赖此机制（靠源码依赖边+state.json+直读 rust_root/.rs+cargo check），`migration_status` M2 D3 计划回写。design-checker 复检零 MISMATCH + 代码事实佐证
  - **待用户审阅合并**
- ✅ **M2-PERF-BASE**（commit 待提）——M1 性能基线快照落盘。`tools/perf-baseline/`：`measure.py`(python3 无依赖) + `baseline.json`(schema v1) + README。release 下各 fixture `graph build` median ~22ms（50 次采样，commit dd2d0a3，Darwin arm64）；`just perf-baseline`(刷新) / `just perf-baseline-check`(F6 回归门 ±10%)。**诚实披露**：fixture 仅数十行，~22ms 由进程启动主导、对解析细粒度退化区分度低（Sprint F 真实项目后刷新增区分度）；单模块翻译时长 LLM 驱动不可脚本化、M1 无历史记录 → 记 `not_measured` + Sprint F 人工记录协议（F6 该项用趋势判据非严格 ±10%）
- **下一步**（**新会话从这里开始**）: Sprint A 剩余代码类，无依赖可并行：VER-04/05、REFAC-05/06/07/13/14、COMPAT-01、ADV-06；CTX-01 需真实项目实测。REFAC-06 在关键路径（Sprint C SCALE-P 依赖）建议优先。注：M2-TIER-01a 删 risk 时需同步 plugin 提示词 analyze.md:37 的 `risk:low` 表述（本文档 PR 未含）。**PR 粒度已放宽**（CLAUDE.md 改）：同 Sprint 紧密相关小任务可合批
- **复审结论**：草稿方向正确，已修正 3 处自相矛盾 + 1 处悬空引用 + 撤销 tier_signals 过度设计 + 补 6 项缺口；新增 D5（SQLite 集中 writer）+ 3 任务（DESIGN-03/PERF-BASE/CLI-06 auto-unblock）；任务总数 52→55。3 个战略决策经用户批准（SQLite 门禁降级 / 60min 单模块 / 状态机程序化推迟+抽 auto-unblock）
- **D3 写隔离方案已定稿（重点，见 [MDR-003](decisions/003-m2-parallel-write-isolation.md)）**：经 codex 四轮对抗审查 + 用户多次质疑收敛为 **git worktree + 约束包**（否决「隔离 crate 副本/轻量 staging/多 crate workspace 作并行单元」）。核心：
  - worktree 内完整 crate 真自检（保留 M1 per-module 编译反馈环）；**两层 done**：`agent_done`(自检) vs `done`(整组 check)
  - 共享编辑策略 **D+A**：porting 规则最小化共享写面（用既有 API/`Error::Other`/`anyhow` 逃生口）+ worktree 自由改+回传 touched-list + 禁删/改签名既有共享 API；**不用声明式 schema**
  - 共享 .rs 冲突 → 串行 rebase 重译（**非 LLM 手解**）+ **reconcile 轮次上限防活锁**；整组 check 为唯一 done 真门
  - **进度保证**：结构无死锁，最坏退化全串行=M1 速度、不卡死；headless 靠 ADV-07 自动 degrade（**须改状态机**）+ auto-unblock 推进
  - **Sprint F 必实测**（判断有不确定性，已留逃生口）：首轮编译通过率 / worktree target 成本 / reconcile 频率；数据 favor 则降级轻量 staging

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

- **55 项任务 + 5 项验收活动**，预计 25-33 天纯开发（日历 5-7 周）
- 5 个设计决策已定稿（D1 done 终态 / D2 blocked 规则 / **D3 写隔离=worktree+约束包** / D4 tier 分档 / D5 SQLite 集中 writer）
- M1 deferred TODO 已分配到对应 M2 任务（ADV-08/09, REFAC-13）
- 部分设计文档 M2 交付物推迟到 M2.5/M3（状态机程序化、行为录制框架等）

### M1 历史归档

M1 各 Phase 的详细审查修复记录、提交历史、Live 验证产物见 [STATUS-M1-archive.md](STATUS-M1-archive.md)。
