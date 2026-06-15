# M2 计划复审总结（2026-06-15）

> 对 `docs/m2-plan-review` 分支「M2 计划复审草稿」(06cb435) 的全面复审 + codex 对抗审查产物。
> 结论：草稿方向基本正确，但含 3 处自相矛盾 / 1 处悬空引用 / 1 处过度设计 + 6 项未落地缺口。已全部修正并写入 `docs/PLAN-M2.md`。本文件为审查台账，新会话据此可直接开干。

## 一、复审方法

1. 全仓代码取证（grep 验证草稿每条事实性断言）
2. 权威设计文档核对（08-roadmap / 04-toolchain / 09-appendix / 02-architecture / 03-execution-model）
3. **codex 对抗审查**（逐条证伪 8 条结论，额外发现 2 个矛盾）
4. 用户决策 3 个改动验收门禁/设计优先级的战略点

## 二、草稿断言核验结论（codex 复核）

| # | 草稿断言 | 核验 | 处理 |
|---|---------|------|------|
| 1 | D3 写隔离方案 | ❌ 「隔离 crate 修正版」被推翻（详见下「D3 三轮演进」） | **终定 git worktree + 约束包**（用户拍板） |
| 2 | `risk` 是零读取死字段 | ✅ `rg '\.risk\b'` 无判断/分支命中 | 删除，但**非「直接删」**：需同步 CLI 构造/测试/插件文档/schema，并入 TIER-01a |
| 3 | `tier_signals` 自造、零先例、疑似过度设计 | ✅ 全仓零命中 | **撤销**：不进 schema，分档信号改写 run 日志/AttemptRecord |
| 4 | 方案B 下翻译期 graph 无并发写 | ✅ 唯一写入口 graph build；run 走 state transition 单写 | 据此正式化 D5 |
| 5 | 删 SQLite WAL 测试与设计文档 + PLAN 自身矛盾 | ✅ 08/04 以并发写为默认架构；§1.2/§1.3/F5/Level5 自相矛盾 | **新增 D5**：降级为 WAL 配置回归（用户批准） |
| 6 | auto-unblock CLI 任务缺失 | ✅ 09:228 承诺 M2 交付，被打包进推迟的状态机程序化 | **新增 M2-CLI-06**（用户批准抽出） |
| 7 | TIER-01 对 REFAC-10 是伪依赖 | ✅ 分档为模块内 AST 特征，PLAN.md:548 亦称不依赖跨文件 calls | 解依赖：TIER-01a 仅依赖 REFAC-09，路径 A 缩短 3-4d |
| 8 | 「60min 含多模块」与吞吐目标矛盾 | ⚠️ 非硬数学矛盾，是 F9 口径未定义 | 重定义为「单模块 full 档<60min」（用户批准） |

**codex 额外发现（草稿主审漏掉）**：
- **D4 自相矛盾**：D4 决议删 risk，但 TIER-01a 任务仍写「risk 自动填充」→ 已改为「tier 自动填充」。
- **D3 vs §7 伪代码矛盾**：§7 行 322–353 还是旧方案B（「SubAgent 不写 Cargo.toml」），与 D3 修正版（SubAgent 在隔离 crate 自 check 必写自己的 Cargo.toml）冲突 → 已改写伪代码并加「主/隔离 Cargo.toml 区分」说明。

## 三、用户决策（3 项，均取推荐）

| 决策点 | 选择 | 影响 |
|--------|------|------|
| SQLite 并发门禁 | 降级为 WAL 配置回归 | M2→M3 判据(c) 改 N/A；§1.2/§1.3/F5/Level5/PETGRAPH-01 同步 |
| 全流程 60min 口径 | 单模块 full 档<60min | §1.2/F9 重定义；多模块归吞吐门禁 |
| 状态机程序化 | 推迟二进制 + 抽 auto-unblock 到 M2 | 二进制留 M2.5；新增 M2-CLI-06 |

### D3 三轮演进（写隔离方案，本次复审最大争点）

1. **草稿「隔离完整 crate 副本 + 自检」** → 用户三点质疑（=手搓 worktree / 改主干共享 crate / 模块间依赖）+ codex 第二轮对抗审查推翻：Rust 编译单元是 crate 不是文件，隔离单模块副本要么编译不过要么=低配 worktree；且 orphan/coherence/feature/宏 冲突只有整 crate 编译能发现（任何隔离方案的盲区）。
2. **一度倾向「轻量 staging（不自检）」** → 用户追问「共享改动少是否就不必 worktree」促深挖：发现 worktree 真正价值是 **per-agent 自检**（非冲突检测），而 M1 翻译架构本质是 per-module 编译反馈循环（Phase A 要编译通过 + Phase B 3 轮修复 + compile_fixing 状态）。
3. **codex 第三轮终审 → 定 git worktree + 约束包**：staging = 取消 M1 反馈环、非等价并行化；worktree target 成本真实但不足以推翻自检收益。纠正：装配需**结构化合并**（Cargo.toml/lib.rs/mod）非纯 copy-out。**诚实标注**：worktree 优于 staging 的核心论据（M1 首轮普遍带编译错误）M1 未留痕、属推断 → Sprint F 实测首轮通过率 + target 成本，数据favor staging 则记录为已论证简化路径。

**约束包**：① worktree 内完整 crate 真自检 ② 禁改共享文件→共享 API 变更回传编排器串行决策（配合叶子优先）③ dependency-mapping 前置强约束生态（防 Frankenstein Cargo.toml）④ merge 后整体 check 为唯一 done 真门 ⑤ 图缺陷（REFAC-10 档1 漏边致假独立）→回退串行+修图。

**关键认知**：多 crate workspace **不是**长期最优的并行单元（用户质疑促修正）——它只是搬移共享瓶颈、受 crate 间禁环硬约束、且让构建期并行便利污染输出架构（范畴错误）。**并行机制（worktree）与输出 crate 结构正交**。

**orchestrator 二进制 vs CLI 边界（澄清，未改决策）**：CLI=无状态工具箱（做一件事吐 JSON，不决定下一步）；orchestrator=有状态决策循环（决定模块顺序/sprint推进/重试/并发调度）。MVP/M1 该决策角色由 SKILL.md(LLM) 担任；二进制方案把它搬到确定性 Rust 代码。M2 维持 SKILL.md 编排，仅把确定性子项（auto-unblock）下沉为普通 CLI 子命令（进已有 `rustmigrate`，非新二进制）。**残留风险**：并行编排（3 agent+merge）仍非确定性，靠 D3 方案B 的「单写+兜底check+逐个回滚」控制（§13 R2）。

## 四、PLAN-M2.md 改动清单（共 ~18 处）

- §1.2 验收表：SQLite 行降级；性能无退化口径明确（per-module+graph build，非吞吐）；60min 重定义
- §1.3：判据(c) → 集中 writer N/A
- §2 D4：撤 tier_signals，risk 删除并入 TIER-01a，补同步面
- §2 **D5 新增**：SQLite 并发模型与验收口径
- §3 路径 A：解依赖后缩短为 ~14-18d
- §6 TIER-01a：risk→tier + 仅依赖 REFAC-09
- §6 新增「trivial 档维度 9 退化形式」（导出符号/类型签名一致性）
- §6 完成标志：graduate 措辞对齐 ADV-03（不新增 graduated 状态）
- §7 伪代码 + compile_fixing 子流程：改写为修正版方案B + 主/隔离 Cargo.toml 区分
- §7 PETGRAPH-01：删测试→WAL 配置回归
- §7 完成标志：WAL 合规→配置回归
- §9 F5/F9：同步降级 + 重定义
- §10 计数：52→55；索引 + 依赖图同步
- §11 状态机程序化行：记录用户批准 + auto-unblock 抽出 + 残留风险
- §12 Level5：WAL 配置回归
- 新增任务：M2-DESIGN-03（设计文档架构同步）、M2-PERF-BASE（M1 性能基线）、M2-CLI-06（auto-unblock）

## 五、新会话开干起点

1. 读 CLAUDE.md → STATUS.md → 本文件 → `docs/PLAN-M2.md`
2. 从 **Sprint A** 开始；Sprint A 新增 3 项中，**M2-DESIGN-03** 落实 D3/D4/D5 对设计文档的同步（04 §5.7.3 / 08 §M2 / 06 §10.5），应优先做以免实现与设计文档漂移
3. 每阶段独立 PR，走 §14 执行协议

## 六、仍需注意（非阻塞）

- D5 对设计文档 04/08 的「并发写架构→集中 writer」改写由 M2-DESIGN-03 执行；改前再跑一次 CLAUDE.md「设计文档一致性检查」grep
- M2-SCALE-02 实现时须在 translator.md 落实「写隔离 crate 的 Cargo.toml、回传 new_deps、不写主 Cargo.toml」，并把 run.md 旧「直接写 rust_root」语义改为「写隔离目录 + 编排器 merge」
