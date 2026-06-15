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
- ✅ **M2-VER-05 + REFAC-05 + REFAC-07**（commit `64b8b67`，A 档「加载健壮性」批）——REFAC-05: parse_node_extra 暴露 serde 错误细节到 warning；VER-05: timestamp 格式校验**下沉到 `Timestamp` 自定义 `Deserialize`**（反序列化即校验，类型层保证全字段覆盖、不可能漏）——经审查后重构，删除了手写 `validate_loaded`(~90 行) + `InvalidTimestamp` 变体 + kind 映射（REFAC-07 后置校验钩子随之移除）；非法 timestamp 现归 Json 错误（kind=json）并触发 backup 回退（视为文件损坏，语义更一致）。净减 ~75 行。205 测试全过、clippy 零
- ✅ **M2-PERF-BASE**（commit `a7851d1` + v2 增强）——M1 性能基线快照落盘。`tools/perf-baseline/`：`measure.py`(python3 无依赖, schema v2) + `baseline.json` + README。**双维度**：① **真实项目（F6 硬门）**——复用 graph-validation/repos.txt 钉死仓库，fp-ts ~314ms(2365N/5828E)、rxjs ~99ms(807N/2168E)、zod ~99ms(538N/1207E)，±10%=±31ms@fp-ts 区分度好；② **fixture（仅冒烟参考，不判）**——~22ms 由进程启动主导、区分度低。`just perf-baseline`(刷新) / `just perf-baseline-check`(F6 回归门，仅真实项目超容差 FAIL)。仓库 clone 到共享 `.work/`(gitignore)，measure 仅本地 checkout 不联网。单模块翻译时长 LLM 驱动不可脚本化、M1 无记录 → `not_measured` + Sprint F 人工协议
- **M2 B 档「封装类」批**（分支 `feat/m2-refac-06`）——经封装价值复审，仅保留有不变量依据的部分：
  - ✅ **REFAC-14**(`d09109b`): ErrorData 加 `details: Option<Value>` + `#[serde(flatten)]`，`data.cycle_path` 保持顶层（对齐 09 § Step 2.8 + analyze.md），cmd_graph_topo 环分支走统一 ErrorData
  - ✅ **REFAC-13 仅 ToolStatus 保留**(`501b446`): 三态收敛为私有枚举 `ToolProbe{Missing|ProbeFailed|Available}`——**真价值=让非法字段组合不可表示**；自定义 Serialize 保 tool_checks 扁平契约。PLAN 标 types/state.rs 系笔误，实际 profile/tools.rs
  - ⏪ **REFAC-06 已回退**: MigrationSequence 私有化+getter 是**过度封装**——Rust 纯数据 struct 惯例即 pub 字段（无字段间不变量、不跨 crate、外部仅 `&` 只读，私有化零收益、纯增代码）。回退为 pub 字段删 getter，SCALE-P 直接读 `sequence.parallel_groups`
  - ⏪ **REFAC-13 的 LocReport totals 私有化已回退**: 同 REFAC-06 病因，字段改回 pub、删 4 个 getter；**保留 `from_languages`**（把累加收成单一入口，有整洁价值）
  - **教训**：Rust 不套 Java「全字段私有+getter」；私有化仅当 ①字段间有不变量 或 ②跨 crate API 稳定性。M1 review 按惯例提的封装债须按此甄别。211 测试全过、clippy 零
- **执行模式（2026-06-15 定，详见 PLAN-M2 §14「并行执行模式」）**：sprint 内独立任务用 **worktree 隔离 subAgent 每波 3-4 路并行**，主会话当瘦编排器（派发→收摘要→review→集中 merge 解冲突→整体 `just ci`→**同 sprint 合批 PR**）。碰同一热点文件（lib.rs/graph.rs/build.rs/machine.rs）的任务合成一个工作单元串行做
- ✅ **Sprint A C 档收尾（波次1，2 路 worktree 并行，分支 `feat/m2-sprint-a-finish`）**——首次走「并行执行模式」：
  - ✅ **VER-04**(`70d3a07`): populate 孤儿 pending 清理——新增 `MigrationStateMachine::retain_modules(live_keys)`，重填前剔除源码图已无对应节点的 pending 模块（key 用 `id.to_string()` vs `as_str()` 已核一致），被清理 key 经 warning 降级告知；docstring 补孤儿清理流程；record-subagent-call 无 init 返回 `FileNotFound`（非 panic）e2e
  - ✅ **COMPAT-01**(`9c7e62e`): version 写入已就位（`STATE_SCHEMA_VERSION="1.0.0"` 改 pub）；validate 新增 `check_version_compat`——语义化**主版本号**判兼容（空/格式非法/跨主版本 → SchemaValidation 并提示当前支持版本，同主版本放行）
  - ✅ **ADV-06**(`85b6ed4`): stats compare 非占位——`stats/compare.rs` 三维度（LOC/函数数/控制流嵌套）比值 `rust/source`，分母 0→`ratio:null`。源码侧复用 tree-sitter（`build_graph_ts` 同口径），**Rust 侧词法扫描**（无 tree-sitter-rust 依赖，`method` 字段标注手段；偏差经 review 认可——设计评分卡 Rust 侧本标注 tokei/scc）；目录缺→warning 跳过不报错
  - **质量门 + 审查闭环**：PR [#17](https://github.com/snowzhaozhj/rewriteInRust/pull/17)。design-checker（零必修 MISMATCH）+ code-reviewer（1 important + 2 nit）已闭环修复：char 字面量误吞后续代码（important，含引号 `'"'`）、method→`CountMethod` enum、空源码图跳过孤儿清理（防整表清空）、`version` JSON 键加 serde rename 对齐设计 `schema_version`（06 §10.0.2/§10.7）、错误码 TODO 指向 ERR-01、09 示例同步。整体 `just ci` 全过（clippy/deny/fmt/shellcheck；新增 char/转义/生命周期 3 测试）。strsim/toml duplicate 系 tokei 既有传递依赖非本次引入
  - **用户复审追加（设计层，commit `3160a55`）**：用户质疑 stats compare ① **强绑 TS**（measure_source 写死 + 非 TS 源静默收集 0 文件给半残比值）② **两侧控制流/函数计数口径不一致**（源 AST 精确 vs Rust 词法启发式，关键字集/算法均不同）。处理——问题1：`cmd_stats_compare` 加 `source_language` gate，非 TS 显式报错 + e2e；问题2 按「文档化 + nesting 降为参考」（**不改算法**，接受为粗粒度信号）——决策写入**设计文档 03 §4.3 Step 4.5**（函数/嵌套比仅作粗粒度告警、行数比为门禁主依据），compare.rs 注释精简指向 03、不二次文档化。**教训**：自动审查链（实现 agent + design-checker + code-reviewer + 主会话）全部只查「局部正确性」，漏了「度量学有效性/功能目的」层——design-checker 还把控制流维度标 MATCH。审查需上升到功能目的层。**待用户审阅合并**
  - CTX-01 需真实项目实测 → 推迟 Sprint F
- **下一步**（**新会话从这里开始**）: PR 审查闭环后转 **Sprint B（4 路并行红利最大区）**：7 个独立 REFAC（01/02/03/04/11/12/15）多挤 graph.rs/build.rs，merge 时集中解冲突；REFAC-09→10 串行另起
  - 注：M2-TIER-01a 删 risk 时需同步 plugin 提示词 analyze.md:37 的 `risk:low` 表述。**PR 粒度已放宽**（CLAUDE.md 改）：同 Sprint 紧密相关小任务可合批
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
