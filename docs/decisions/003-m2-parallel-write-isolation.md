# MDR-003: M2 跨模块并行翻译的写隔离方案

> 状态：已定稿（2026-06-15，用户拍板）。详细执行见 `docs/PLAN-M2.md` §2 D3 / §7 SCALE-02。

## 背景

M2 核心价值是跨模块**并行翻译**（max_concurrent_agents=3）。多个 LLY SubAgent 并发把 TS 模块翻成 Rust，需要一套「写隔离 + 合并」机制：既要各 agent 互不踩踏、又要最终合成一个可编译的 Rust crate。设计文档 06 §10.5 原预设 git worktree，但 M1 计划标「两方案待定」，复审草稿一度改成自造的「隔离完整 crate 副本 + 自检」方案。

## 决策

**git worktree + 约束包**。恢复 06 §10.5 原始设计：每 agent 一个 worktree（含全部 done 代码），在内跑完整 M1 per-module 编译循环并自检；编排器结构化合并后整体 `cargo check` 为唯一 done 真门。

## 关键论据（为何 worktree）

1. **M1 翻译架构本质是 per-module 编译反馈循环**：Phase A L1 要求「编译通过」、Phase B `max_retry_rounds=3` + `compile_fixing` 状态 + 3 轮不过转 degrade。worktree 是干净并行化这个循环的方式（每模块在自己 worktree 内 translate→check→fix，done 依赖在场=真实完整 crate 检查）。
2. **Rust 编译单元是 crate 不是文件**：单模块「隔离 crate 副本」要么编译不过（引用兄弟模块符号），要么退化为复制整个 rust_root = 低配 worktree（无 git 去重/合并语义）。
3. **某些冲突只有整 crate 编译能发现**：orphan rule / coherence(E0119)、Cargo feature 合并冲突、proc-macro/derive 展开——任何「隔离后合并」方案都有此盲区，故 **merge 后整体 check 不可省、且必须是唯一 done 真门**。

## 约束包（实现必须落实）

| # | 约束 | 目的 |
|---|------|------|
| 1 | worktree 内做**完整 crate 真自检**（非模块副本假信心） | 保留 M1 per-module 编译反馈环 |
| 2 | **禁止 SubAgent 改共享文件**（lib.rs mod/Cargo.toml/公共 error/trait）；需共享 API 变更则回传请求由编排器**串行决策** | 防并发改共享文件冲突；配合叶子优先(sprint=1)先冻结基础 |
| 3 | **dependency-mapping 前置强约束生态**（anyhow xor thiserror、异步运行时唯一…） | 防并发模块各引不同 crate 产 Frankenstein Cargo.toml |
| 4 | 装配=**结构化合并**：新 `.rs` 复制 + Cargo.toml(deps+feature 集)/lib.rs(mod) 程序化合并 + 意外共享冲突 git/碰撞检测 | 非纯 copy-out（copy-out 处理不了共享文件合并） |
| 5 | **merge 后整体 `cargo check`/`cargo test` = 唯一 done 真门**（worktree 自检过 ≠ done） | 兜住跨并发兄弟模块冲突 |
| 6 | 整体 check 暴露「同层本不该有的跨模块引用」→判**图缺陷**，相关模块回退串行+记录修图 | 兜 REFAC-10 档1 图不完整致同层假独立 |
| 7 | target dir：先各 worktree 独立 `CARGO_TARGET_DIR` 保正确，Sprint F 实测后用 sccache/共享 target 优化 | 避免并发 cargo 锁争用 |

## 被否决的方案

| 方案 | 否决理由 |
|------|---------|
| **隔离完整 crate 副本 + 自检**（复审草稿） | 违背 crate 编译边界；per-module 自检是假信心（对接口 stub 单编过、merge 后真实类型不一致照崩）；=低配 worktree |
| **轻量 staging（agent 不自检 + 编排器整体 check + 串行修复）** | = 取消 M1 per-module 编译反馈环，非等价并行化；首轮编译错误全堆 merge 后修、API/feature/宏纠缠。**保留为逃生口**（见下） |
| **无隔离纯共享工作树（P2）** | 单 crate 下兄弟文件半写时无法自检；共享文件编辑 silent 覆盖 |
| **多 crate workspace 作并行单元（P11）** | **范畴错误**：并行机制与输出 crate 结构正交；多 crate 只搬移共享瓶颈、受 crate 间禁环硬约束、让构建期便利污染输出架构。属输出质量议题（M3 视目标架构定），非并行债 |

## 诚实标注的不确定性与逃生口

worktree 优于轻量 staging 的核心论据「M1 首轮普遍带编译错误」是**合理推断而非实证**——M1 Live 的 migration-state.json/attempts 未入库，真实首轮通过率无留痕（codex 第三轮指出）。

故 **Sprint F 必测**：① worktree 内自检前的首轮编译通过率 ② worktree target 冷编/锁开销。
**若实测首轮通过率高 且 target 成本高 → 「降级为轻量 staging」是已论证的简化路径。** 即「先上 worktree 保 M1 反馈环，用数据决定是否可简化」，而非反向赌首轮干净。

## 审查过程

codex 三轮对抗审查 + 用户两次质疑收敛：草稿隔离副本 →（用户质疑+codex 二轮推翻）→ 一度倾向 staging →（用户追问「共享改动少是否不必 worktree」深挖）→ worktree（自检价值=M1 反馈环）→（codex 三轮终审）定 worktree + 结构化合并，否决 staging、否决纯 copy-out。
