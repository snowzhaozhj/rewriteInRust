# 实施路线图与关键数据参考

> [返回主索引](./README.md)

---

## 十三、实施路线图

### M0: 假设验证周（2-3 周）

**目标**：验证 6 个关键技术假设，产出假设验证报告，而非项目骨架

**6 个 Spike（每个 1-2 天，Spike 0 最先执行，Spike 3/5 可并行执行以缩短总时长）**：
- [ ] **Spike 0: Plugin API 骨架验证** — 用最小 Plugin（1 个 Skill + 1 个 SubAgent + 1 个 Hook）验证 Claude Code Plugin 的完整加载路径。确认 `plugin.json`、`agents/` 自动发现、`hooks/hooks.json` 格式、SubAgent 通过 Agent tool 调用等核心机制的实际行为与设计假设一致。这是所有后续 Spike 的前提。（Plan B：回退到纯 CLAUDE.md + slash commands 的非 Plugin 方案）
- [ ] **Spike 1: SubAgent 编排可靠性** — 验证 Claude 能否可靠执行 `/migrate analyze` 的 7 步序列（含 3 次 SubAgent 串行调用）。验收标准：5 次独立测试中成功率 ≥ 80%（即 ≥4 次完成全部步骤且产出物有效）。低于阈值触发 Plan B（微 Skill 链 / 外部脚本编排）
- [ ] **Spike 2: rust-analyzer LSP 验证** — 验证 rust-analyzer LSP Plugin 在写入 .rs 文件后的诊断反馈延迟和可靠性（Plan B：回退到 PostToolUse Hook + cargo check）
- [ ] **Spike 3: tree-sitter 精度** — 验证 tree-sitter 对 TS 项目的 AST 解析精度是否满足模块拆分需求（Plan B：TS Compiler API / LLM 直接读源码）
- [ ] **Spike 4: SKILL.md 跟随边界** — 验证 SKILL.md 长指令（>2000 字）的指令跟随率和遗漏率（Plan B：拆分为多个短 Skill）
- [ ] **Spike 5: Beads/AgentMemory 集成评估** — 评估 Beads（任务状态持久化）和 AgentMemory（知识记忆）的集成可行性

**交付物**：
- [ ] `DESIGN_ASSUMPTIONS.md` — 假设验证报告（每个 Spike 的结论 + Plan B 是否触发）
- [ ] `migration-state.json` schema 定义（沿用）
- [ ] `.rustmigrate.toml` 配置 schema（沿用）

**验收指标**：6 个 Spike 全部完成，每个假设有明确的"验证通过"或"触发 Plan B"结论。Spike 0 必须最先完成且验证通过，否则后续 Spike 无法执行。

#### M0 → M1 决策检查点

M0 收尾时依据 `DESIGN_ASSUMPTIONS.md` 的 Spike 结论，按下表确定进入 M1 的方案与工作量档位（Plan B 体系与触发规则见 [07-pitfalls-and-risks.md § 12.2](./07-pitfalls-and-risks.md#122-plan-b-体系)，强制触发判定见 [06-plugin-structure.md § 10.5](./06-plugin-structure.md#105-编排调度路径)）：

| M0 结果 | M1 方案 | M1 工作量档位 |
|---------|---------|--------------|
| Spike 1 成功率 ≥ 80% 且 Spike 4 通过 | 基线：SKILL.md 指令编排 | 50-65 人天 |
| Spike 1 成功率 < 80%（或 Spike 4 失败） | **强制** Plan B3 混合编排（不可选） | 60-75 人天（+5-10 人天） |
| Spike 0 失败 | 回退非 Plugin 方案（CLAUDE.md + slash commands），M1 重新评估 | 单独核算 |

> 规则：**只要 Spike 1 < 80% 通过率，M1 自动采用 Plan B3 混合方案，工作量 +5-10 人天**，不进入基线档位。该判定在 M0 验收会上一次性做出，避免 M1 中途返工。

### M1: MVP（6-8 周）

**目标**：跑通 TypeScript → Rust 的**单模块纯函数/CLI 子模块**迁移

**范围限定（MVP 必须）**：
- [ ] `/migrate analyze` 完整版（合并原 init + plan + test：TS 项目画像 + 依赖图 + 迁移规则生成（porting/ 目录）+ 黄金文件测试搭建）
- [ ] `/migrate run` — 单模块迁移内循环（含 Phase A/B 双阶段翻译）
- [ ] `/migrate review` — 验证管线 + 进度仪表板（合并原 verify + status）
- [ ] Tier 0 门禁集成（cargo check + clippy + cargo test）
- [ ] 编译器反馈迭代修复（最多 3 轮）
- [ ] `migration-state.json` 状态管理 + 断点续传
- [ ] PARITY.md 自动更新
- [ ] 基础 MDR 模板

**MVP 不包含（后续迭代）**：
- 多候选生成
- 属性测试 / 模糊测试
- 多 agent 并行
- 行为录制框架
- `/migrate graduate`（含 unsafe 审计）

**验收指标**：
- 在 3 个真实 TS 小项目（<5K 行）中，每个项目至少完成 1 个纯函数模块的迁移（合计 ≥3 个模块）
- 迁移后代码通过 Tier 0 门禁
- 黄金文件测试 100% 通过
- 用户从 `/migrate analyze` 到 `/migrate review` 的完整流程可在 30 分钟内走通（单模块）
- 断点续传验证：中断会话后恢复，不丢失 migration-state.json 状态

**性能门禁（量化，作为"是否可升级 M2"的客观依据）**：

| 指标 | 阈值 | 测试方法 |
|------|------|---------|
| 图构建耗时 | `rustmigrate graph build` < 10s（100 文件项目） | 测试环境：M1/M2 Mac 或 4 核 CI runner；测试项目：100 个 `.ts` 文件、约 5K 行；冷启动（无 SQLite 缓存）计时 |
| 单 agent 完整流程用时 | 单模块 analyze→run→review 在 30-40 分钟内（与上方定性指标一致） | 在 3 个验收项目上各跑 1 个纯函数模块，取耗时区间；不含人工审阅等待 |
| 上下文预算利用率 | < 90%（5K 行以下模块的单次 Work Unit） | 记录 Work Unit 峰值上下文 token / 预算上限（≤100K，见 02-architecture.md）；interface_only 加载策略下统计 |

> 性能门禁仅约束 MVP 范围内的串行单 agent 路径。多 agent 并行吞吐属 M2 指标（见下），M1 不要求。

**M1 工作量分解（粗估）**：

| 交付物 | 预估人天 | 说明 |
|--------|---------|------|
| 3 个 SKILL.md（analyze/run/review） | 4-6 | 每个 Skill 约 1.5-2 天（含调试）；analyze 最复杂 |
| 4 个 SubAgent agent.md | 3-4 | 系统提示编写 + 职责边界定义 |
| 2 个 Hook 脚本（fmt.sh + verify.sh） | 1 | check.sh 被 rust-analyzer 替代，减少 1 个 |
| 文件保护 Hook（file-guard.sh） | 0.5 | PreToolUse 文件保护 |
| SubAgent 核心规则 + 参考指南 | 2-3 | agents/*.md 内嵌核心规则 + skills/migrate/references/ 参考指南编写 |
| TS 语言适配器 | 3-5 | detect.sh + extract-types.sh + extract-deps.sh + porting-template.md |
| Plugin 打包结构 | 1-2 | plugin.json + 目录组织 |
| migration-state.json 管理 | 2-3 | Schema 定义 + 状态流转逻辑 + 断点续传 |
| .rustmigrate.toml | 1 | Schema 定义 + 默认值生成 |
| 集成测试 + 调试 | 5-8 | 3 个真实项目上的端到端验证 |
| CLI 核心（11 个 MVP 子命令） | 10-14 | init/profile/graph(build+topo-sort+deps+stats)/validate-state/state(get+transition)/stats-loc/scaffold |
| CLI 嵌入 crate 集成 | 5-7 | tree-sitter + ast-grep-core + tokei + petgraph + rusqlite 绑定 + 跨平台编译 |
| CLI 测试 | 3-4 | 集成测试 + fixtures + 1 个自建微型项目 |
| Plan B 缓冲 | 2-5 | M0 Spike 触发 Plan B 时的额外开发量 |
| **合计** | **50-70** | 1 人约 12-18 周，2 人约 6-9 周 |

> **M0 假设验证周**（5-10 人天）不在上述 M1 估算中，应单独核算。
> **与 v0.9.2 估算的差异**：集成测试（+4 天）、CLI graph build（+3 天）、crate 集成（+2 天）、Plan B 缓冲（+3 天）。详见可行性审查报告。

> **场景 A / B 对比**（**实际范围取决于 M0 验收结果**）：
> - **基线 M1**（所有 Spike 验证通过，无 Plan B）：**50-65 人天**。
> - **失败缓冲 M1**（Spike 1 编排 / Spike 4 SKILL.md 跟随触发 Plan B3 混合方案）：**60-75 人天**。上表「Plan B 缓冲 2-5 人天」仅覆盖单个 Spike 回退；若 Spike 1+4 同时失败，需额外 +5-10 人天实施 Plan B3（编排层改用微 Skill 链 / 外部脚本编排），故上限抬高到 75 人天。
> 决策规则见下方「M0→M1 决策检查点」。

### M2: 质量提升（8-12 周）

**目标**：验证管线完整，翻译质量可靠

**交付物**：
- [ ] 多候选生成 + 最优选择
- [ ] 属性测试（proptest 等价性验证）
- [ ] 模糊测试（cargo-fuzz 差异对比）
- [ ] 变异测试（cargo-mutants 测试质量验证）
- [ ] 覆盖率门禁（cargo-llvm-cov）
- [ ] 行为录制框架（CLI + 库接口）
- [ ] KNOWN_DIFFERENCES.md 自动生成
- [ ] 降级路径实现（FFI 桥接）
- [ ] `/migrate review` 完整验证管线
- [ ] `/migrate graduate` 基础版（含 unsafe 审计）
- [ ] Sprint 循环外循环支持
- [ ] Workflow 定义文件（ultracode 格式）
- [ ] 多 agent worktree 隔离机制
- [ ] 状态机程序化实现（独立 Rust orchestrator 二进制，确定性编排，替代 MVP 的 SKILL.md 指令驱动；见 [02 § 3.4.1](./02-architecture.md#341-mvp--m2-演进与向后兼容)）
- [ ] migration-state.json 向后兼容框架（`version` 字段 + 自动迁移脚本，集成进 M2 CLI 的 `init`/`validate state` 版本检测与升级）
- [ ] /goal 自主迁移循环支持
- [ ] CLI 扩展（search/analyze/report 等 16 个子命令）
- [ ] CI/CD 集成（`rustmigrate` 在 GitHub Actions 中使用；落地设计见 [03-execution-model.md § 4.11](./03-execution-model.md#411-cicd-集成m2-范围)）

> **图并发写策略（analyzer + scaffolder 并行阶段）**：M2 的有限并行仅限 analyzer + scaffolder 两个 SubAgent（见 [06 § 10.5](./06-plugin-structure.md#105-编排调度路径)），但两者最终都写入共享的 `source-graph.db`（单 SQLite，同一时刻仅一个写者）。除上方「SQLite 并发写冲突率」门禁外，M2 规划阶段还需实测这两个 agent 对 `nodes`/`edges` 表的并发写在 WAL 模式（[04 § 5.7.3](./04-toolchain.md#573-持久化存储)已选用）下的写锁等待：若单次 ≤ 20ms，记为「WAL 足够，记录锁行为」；若 > 50ms，采用「共享只读图 + 写批量化」或「每 agent 写分片后合并」并记录权衡。此为 M2 规划项，不进 M0 Spike（M0 Spike 仅验证 MVP 串行关键路径）。状态机程序化 + schema 向后兼容框架合计 +3–5 人天。

**验收指标**：
- 在 3 个真实 TS 中型项目（5K-20K 行）中完成多模块迁移
- 属性测试覆盖核心函数
- 翻译膨胀率 < 3.0x
- 降级路径（FFI 桥接）在至少 1 个复杂模块上成功

**性能门禁（量化，评估规模化就绪）**：

| 指标 | 阈值 | 测试方法 |
|------|------|---------|
| 多 agent 并行吞吐 | ≥ 1.5 模块/小时（`max_concurrent_agents=3`） | 在中型验收项目上跑 Workflow 批量模式，统计同批内 3 agent 并行完成模块数 / 墙钟小时；不含人工审阅 |
| SQLite 并发写冲突率 | < 10%（3 agent 负载下） | 3 agent 并发写 source-graph.db，统计 `SQLITE_BUSY`/重试次数 占总写操作比例（WAL 模式连接配置见 04-toolchain.md § 5.7） |
| 性能基准无退化 | 相对 M1 基线波动 ≤ ±10% | 复用 M1 性能门禁三项（图构建、单 agent 流程、上下文利用率），在相同测试项目上回归对比 |

> M2 并行吞吐指标上限按 `max_concurrent_agents=3` 设定，与 MVP 范围一致；不引入未规划的 4+ agent 并行（属 M4 优化项）。

### M3: 多语言支持（8-16 周）

**目标**：支持 Python → Rust

**交付物**：
- [ ] Python LanguageAdapter（Mypy 类型提取 + PyO3 桥接）
- [ ] Python 专用迁移规则模板（porting-template.md）
- [ ] 统一差异测试框架
- [ ] `/migrate graduate` 毕业评估
- [ ] 性能基准对比自动化（criterion 集成）
- [ ] 并发测试（loom/shuttle 集成）
- [ ] 依赖图可视化（Mermaid 自动生成）

**验收指标**：
- 在 2 个真实 Python 项目中完成至少 1 个模块迁移
- Python FFI 桥接（PyO3）在迁移项目中可用
- 毕业评估能正确识别"已完成"vs"未完成"状态

### M4: 完善（持续）

- C/C++ LanguageAdapter（bindgen + cbindgen）
- Go LanguageAdapter
- Kani 集成（关键路径形式化验证）
- 社区反馈驱动的规则库积累
- 多 agent 并行编排优化
- Strangler Fig 模式工具支持

### 13.1.1 M1→M2→M3 规则库累积效应分析

路线图各阶段并非孤立——M1 产出的规则库是 M2/M3 的输入，这条隐含的演化路径在此显式化，避免把 MVP 误读为"零规则即开箱即用"。

- **(a) M1 → M2/M3 规则累积**：M1 在 3 个 <5K 项目迁移中发现的项目规则（写入 `.rust-migration/porting/`），作为 M2「多候选排序」的打分依据与 M3 Python 适配器的初始模板来源。预期 M1 发现 **15-25 条新通用规则**（覆盖 TS→Rust 常见陷阱，对应 [07-pitfalls-and-risks.md § 9.2 跨语言语义陷阱](./07-pitfalls-and-risks.md#92-跨语言语义陷阱补充) 与 [03-execution-model.md § 7.7 不等价探测维度](./03-execution-model.md#77-不等价证据探测维度清单)）。
- **(b) M1 验收后「规则库成熟度评估检查点」**：M1 验收会上评估规则库对 TS→Rust 常见陷阱的覆盖率——以 § 9.2 跨语言语义陷阱、§ 7.7 探测维度清单为对照基线；覆盖率不足的陷阱类别列入 M2 规则补充计划。此评估不阻断 M1 验收，仅作为 M2 范围输入。
- **(c) M3 Python 适配器复用**：M3 应复用 TS 适配器的 type-extraction 脚本框架（`extract-types.sh` 的调用契约）与差异测试基础设施；Mypy 集成与 PyO3 桥接为 Python 专有部分，不影响通用框架（适配器契约见 [06-plugin-structure.md § 11.2](./06-plugin-structure.md#112-语言扩展架构)）。
- **(d) 社区检测的多语言价值**：M2 的 Leiden 社区检测按依赖耦合分批，与语言无关，M3 多语言场景直接复用。

> **规则成熟度演化**：MVP 的验证管线开箱即用，但**项目专有规则首轮迁移需 1-2 个 Sprint 积累**（见 [01-positioning-and-methodology.md § 1.3](./01-positioning-and-methodology.md#13-我们做什么)）。通用规则随版本沉淀，项目规则随首轮迁移生成。

---

## 十四、关键数据参考

### 成本估算

> **注意**：以下"预估成本"栏**仅指 LLM API 调用成本**，不含人力、基础设施、CI/CD 等其他费用。

| 规模 | 预估时间 | 预估成本（仅 LLM API） | 备注 |
|------|---------|----------------------|------|
| 1K 行 | 1-3 天 | $3-$30 | 纯函数模块 |
| 10K 行 | 2-4 周（1-2 人） | $30-$300 | 含测试搭建 |
| 50K 行 | 2-4 个月（2-4 人） | $150-$1500 | 含 FFI 桥接 |
| 100K+ 行 | 6-12 个月（团队） | 视项目而定 | 必须用 Strangler Fig |

### 行业参考案例

> **注意**：以下案例仅作为行业参考，不作为时间或质量基准。不同项目的复杂度、团队经验、工具成熟度差异巨大，直接对标可能产生误导。

| 项目 | 规模 | 耗时 | 结果 | 证据等级 | 可参考维度 |
|------|------|------|------|---------|-----------|
| Bun (Zig→Rust) | 75 万行 | 11 天 | 99.8% 测试通过 | **商业案例**（[Anthropic 博客](https://claude.com/blog/a-harness-for-every-task-dynamic-workflows-in-claude-code)） | Dynamic Workflows 并行子代理编排、测试驱动验证 |
| Claw-Code (TS→Rust) | 48.6K 行 | 4 天 | 功能完整 | **开源项目**（[GitHub](https://github.com/ultraworkers/claw-code)） | Mock Parity Harness 行为验证、PARITY.md 进度跟踪、9-lane 并行推进 |
| Cloudflare Pingora (C→Rust) | 86K 行 | N/A | CPU-70%, 内存-67% | **商业案例**（[GitHub](https://github.com/cloudflare/pingora)） | 完全重写（非渐进式 FFI）、语义移植、Trait 替代回调函数、分层 crate 架构 |
| Discord (Go→Rust) | 单服务 | N/A | 消除 GC 延迟尖刺 | **商业案例**（[Discord 博客](https://discord.com/blog/why-discord-is-switching-from-go-to-rust)） | GC→所有权模型迁移动机、**非 AI 辅助**（2020 年手动重写） |

> **注意**：Bun 和 Claw-Code 的极端速度可能包含未公开的前期准备工作（Bun 由 Anthropic 收购后作为 Dynamic Workflows 标杆案例），不应作为时间估算基准。Bun 迁移产生了 10,000+ 个 unsafe 块，社区对此争议很大。

> ~~Pokemon Showdown (JS→Rust)~~ — v0.9.3 验证确认**此案例为 LLM 幻觉**，GitHub 和全网搜索均无任何相关仓库、博客或报道。已从参考列表中移除。

**Bun PORTING.md 验证结论**：v0.9.3 通过直接访问 Bun 仓库验证，**确认 Bun 仓库中不存在独立的 PORTING.md 文件**。迁移规则融入了 CLAUDE.md（Bun 的 CLAUDE.md 描述项目为 "written primarily in Rust"）。PR 分支名 `claude/phase-a-port` 暗示使用了类似 Phase A 的翻译阶段概念。本项目设计中对 "Bun 576 行 PORTING.md" 的引用已确认为不准确信息，后续文档中不再引用具体行数。

### 关键论文

| 论文 | 会议 | 核心贡献 | 与本项目关系 | 验证状态 |
|------|------|---------|-------------|---------|
| SafeTrans | ACM CCS 2025 | 迭代修复 54%→80% | 反馈循环设计基础 | 已验证 |
| DepTrans | ACM FSE 2026 | 7B 模型超 32B，依赖图引导 | 拓扑排序翻译策略 | 待验证 |
| Environment-in-the-Loop | ACM ReCode 2026 | 编译环境作为反馈参与者 | F1 反馈循环理论依据 | 待验证 |
| MatchFixAgent | ICML 2026 | 99.2% 等价性判定 | 验证层方法参考 | 待验证 |
| Hayroll | PLDI 2026 | C 宏翻译 | C→Rust 适配器参考 | 待验证 |
| LLMigrate | arXiv 2025 | 调用图引导，<15% 修改 | 依赖图分析策略 | 已验证 |
