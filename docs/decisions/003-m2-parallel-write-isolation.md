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

> 共享编辑策略经 codex 第四轮验证排序 **D > A > B > C**：**D（porting 规则最小化共享写面）+ A（worktree 自由改 + 轻协议）**；**否决** B（声明式 schema，追不上 Rust 共享语义）与 C（interface-first 串行冻结，和 Phase A 忠实翻译冲突）。

| # | 约束 | 目的 |
|---|------|------|
| 1 | worktree 内做**完整 crate 真自检**（非模块副本假信心） | 保留 M1 per-module 编译反馈环 |
| 2 | **D：porting 规则最小化共享写面**——并行模块优先用既有共享 API + `Error::Other`/`anyhow` 逃生口，复杂共享扩展留**串行 cleanup** | 减少共享写面比事后智能合并可靠（编排器非确定性） |
| 3 | **A：worktree 自由改任意文件（自检必需）+ 回传 `{own_files, shared_touched}`**（代码留盘）；**禁删除/改签名既有共享 API，新增只 append** | 不破坏自检、无同步 round-trip；只挡危险共享改动 |
| 4 | **dependency-mapping 前置强约束生态**；Cargo.toml deps+feature 集结构化 union + default-feature 校验；Cargo.lock 合并后重解析 | 防 Frankenstein Cargo.toml / feature 语义冲突 |
| 5 | 装配=**结构化合并为主**：新 `.rs` 复制（同名/布局碰撞当冲突）+ Cargo.toml/lib.rs(mod) 程序化 union + 其余共享 `.rs` git merge | 非纯 copy-out |
| 6 | **两层 done**：worktree 自检过 = `agent_done`（substatus，非终态）；**merge 后整组 `cargo check`/`test` 过才升最终 `done`** | 兜 orphan/coherence(E0119)/feature/宏/命名空间 等只能整组编译暴露的冲突 |
| 7 | **reconcile 机制 + 轮次上限**：共享 `.rs` git 冲突 → 冲突模块串行 rebase 重译（**非 LLM 手解冲突块**）；轮次上限（默认 3）超限→该 sprint 降级串行/人工 | 防 A合并→B重译又改共享→C重译 的**活锁** |
| 8 | 整体 check 暴露「同层本不该有的跨模块引用」→判**图缺陷**，相关模块回退串行+记录修图 | 兜 REFAC-10 档1 图不完整致同层假独立 |
| 9 | target dir：先各 worktree 独立 `CARGO_TARGET_DIR` 保正确，Sprint F 实测后用 sccache/共享 target 优化 | 避免并发 cargo 锁争用 |

## 进度保证 + 死锁语义（codex 第四轮确认，需补轮次上限才成立）

- **结构上无死锁**：① 翻译期 agent 各在独立 worktree、不持共享锁、互不等待 → 无循环等待 ② reconcile 串行全序 + **轮次上限** → 无活锁 ③ 仅编排器写共享状态 → 无写竞争。
- **卡住的模块不冻全 sprint**：某模块 3 轮不过 → pause→degrade（headless 由 **M2-ADV-07 自动 degrade** 达终态）；同 sprint 其他模块照常完成；下游由 **M2-CLI-06 auto-unblock** 解阻塞。
- **最坏情况 = 全 sprint 退化串行 = M1 速度**，慢但不卡死。worktree 并行是加速器，失败时优雅退化到已验证串行路径。
- **依赖项（否则保证缺文档依据）**：① M2-ADV-07 须真正把 headless `paused→自动 degrade` 写进状态机（02 §3.4 + 09 附录 A）② sprint barrier 精确定义：终态=`done`/`degrade_*`，`agent_done`/merge-pending/reconcile-pending 均非终态。

## codex 第四轮纠正的低估项

- 「共享编辑基本不存在」**过度修正**——文本冲突可控，**语义冲突真实存在**，尤其**孤儿规则逼 `From/Display/Serialize` impl 落进共享类型文件**（单 crate + 共享 Error 下不罕见）。
- Cargo.toml **feature 集**非「必成 union」（default-features/optional/target-specific 有语义冲突）；Cargo.lock 需合并后重解析；`pub use`/re-export 影响公开 API 命名；单 crate 命名空间撞名（同名 helper/trait）、`foo.rs` vs `foo/mod.rs` 布局——均当冲突处理。

## 被否决的方案

| 方案 | 否决理由 |
|------|---------|
| **隔离完整 crate 副本 + 自检**（复审草稿） | 违背 crate 编译边界；per-module 自检是假信心（对接口 stub 单编过、merge 后真实类型不一致照崩）；=低配 worktree |
| **轻量 staging（agent 不自检 + 编排器整体 check + 串行修复）** | = 取消 M1 per-module 编译反馈环，非等价并行化；首轮编译错误全堆 merge 后修、API/feature/宏纠缠。**保留为逃生口**（见下） |
| **无隔离纯共享工作树（P2）** | 单 crate 下兄弟文件半写时无法自检；共享文件编辑 silent 覆盖 |
| **多 crate workspace 作并行单元（P11）** | **范畴错误**：并行机制与输出 crate 结构正交；多 crate 只搬移共享瓶颈、受 crate 间禁环硬约束、让构建期便利污染输出架构。属输出质量议题（M3 视目标架构定），非并行债 |
| **共享编辑「禁止+回传编排器批准」** | 破坏 worktree 自检（需共享改动的模块编译不过）+ 制造同步 round-trip + worktree+merge 本就解决 silent 覆盖。用户质疑触发否决 |
| **共享编辑声明式 `shared_edits` schema（B）** | schema 追不上 Rust 共享语义（impl 放置/feature/pub use/coherence），膨胀且收益低；touched-list 已够 |
| **interface-first 串行冻结本 sprint 共享 API（C）** | 把每 sprint 变串行 API 设计会，与 Phase A「翻译中发现接口」现实冲突 |

## 诚实标注的不确定性与逃生口

worktree 优于轻量 staging 的核心论据「M1 首轮普遍带编译错误」是**合理推断而非实证**——M1 Live 的 migration-state.json/attempts 未入库，真实首轮通过率无留痕（codex 第三轮指出）。

故 **Sprint F 必测**：① worktree 内自检前的首轮编译通过率 ② worktree target 冷编/锁开销。
**若实测首轮通过率高 且 target 成本高 → 「降级为轻量 staging」是已论证的简化路径。** 即「先上 worktree 保 M1 反馈环，用数据决定是否可简化」，而非反向赌首轮干净。

## 审查过程

codex 四轮对抗审查 + 用户多次质疑收敛：
1. 草稿隔离副本 →（用户三点质疑 + codex 二轮推翻）→ 一度倾向 staging
2. （用户追问「共享改动少是否不必 worktree」深挖）→ worktree（自检价值=M1 反馈环）
3. （codex 三轮终审）定 worktree + 结构化合并，否决 staging、否决纯 copy-out
4. （用户质疑「禁止改共享文件+回传批准」是否好实践 + 死锁担忧 → codex 四轮探索）→ 改共享编辑策略为 **D+A**、补两层 done / reconcile 轮次上限 / sprint barrier 定义 / 进度保证，纠正「共享编辑基本不存在」的过度修正
