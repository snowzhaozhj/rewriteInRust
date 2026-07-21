# 实施路线图与关键数据参考

> [返回主索引](./README.md)

---

## 十三、实施路线图

### M0: 假设验证周（2-3 周）

**目标**：验证 6 个关键技术假设，产出假设验证报告，而非项目骨架

**6 个 Spike（每个 0.5-2 天，均值约 1 天；Spike 0 最先执行，Spike 3/5 可并行执行以缩短总时长）**：
- [ ] **Spike 0: Plugin API 骨架验证** — 用最小 Plugin（1 个 Skill + 1 个 SubAgent + 1 个 Hook）验证 Claude Code Plugin 的完整加载路径。确认 `plugin.json`、`agents/` 自动发现、`hooks/hooks.json` 格式、SubAgent 通过 Agent tool 调用等核心机制的实际行为与设计假设一致。这是所有后续 Spike 的前提。**附加：crate 集成风险评估** — 编译含 tree-sitter + ast-grep-core + tokei + petgraph + rusqlite 的最小 CLI，**先记录环境基线**（OS / CPU / Rust 版本）再实测编译时间、二进制体积、冷启动感知时间三项，写入 `DESIGN_ASSUMPTIONS.md` 的 **Spike 0 crate 集成**节（测量**后**回填，不在测量前预填阈值；此处「Spike 0 节」是假设报告的小节，与 Tier 0 反馈循环 F1/F2/F3 无关）。回退按下表分级触发、**逐级裁剪 crate**（Plan B：回退到纯 CLAUDE.md + slash commands 的非 Plugin 方案）：

  | 指标 | 测量方法 | 回退触发条件 | 回退动作（按序裁剪） | 返工估算 |
  |------|---------|-------------|-------------------|---------|
  | 冷启动感知时间 | 100 文件项目首次 `graph build` 计时 | > 10s（设计目标） | ① 去 rusqlite → JSON 持久化、SQLite 推迟 M2 | -2 人天 CLI、SQLite 推迟 |
  | 二进制体积 | 发布构建产物大小 | > 基线 2x 或分发不可接受 | ② 去 ast-grep-core → tree-sitter 直接查询 | +1-2 人天改查询层 |
  | 编译时间（冷） | `cargo build --release` 计时 | > 基线 2x 或 CI 超时 | ③ 去 tokei → 自实现 LOC 统计 | +0.5 人天 |

  > 表中阈值（如「基线 2x」）为**回退判定规则**而非预设绝对数；实测基线与最终阈值统一记入上述「Spike 0 crate 集成」节。决策依据见 [04 § 5.7.3](./04-toolchain.md#573-持久化存储)前向兼容权衡。
- [ ] **Spike 1: SubAgent 编排可靠性** — 验证 Claude 能否可靠执行 `/migrate analyze` 的 7 步序列（含 3 次 SubAgent 串行调用）。验收标准：5 次独立测试中成功率 ≥ 80%（即 ≥4 次完成全部步骤且产出物有效）。低于阈值触发 Plan B（微 Skill 链 / 外部脚本编排）。**附加：编排性能基线**（并行于可靠性验收，零额外测试）— 同 5 次测试记录各步骤耗时，算单步均值与占比，写入 `DESIGN_ASSUMPTIONS.md`，明确编排层（SKILL.md 步骤调度 + SubAgent 间通信）开销阈值：单步 < 2s、全编排 < 5 分钟（占单模块 30-40 分钟流程的 12-17%），作为「增加 SubAgent/SKILL.md 步骤时性能是否退化」的边际成本基线（M2 门禁复用，见下方 M2 性能门禁）。
- [ ] **Spike 2: rust-analyzer LSP 验证** — 验证 rust-analyzer LSP Plugin 在写入 .rs 文件后的诊断反馈延迟和可靠性（Plan B：回退到 PostToolUse Hook + cargo check）
- [ ] **Spike 3: tree-sitter 精度** — 验证 tree-sitter 对 TS 项目的 AST 解析精度是否满足模块拆分需求。须用**现代 TS 语法语料**（装饰器、泛型约束、const 断言）实测**按语法类别分桶**的解析错误率并在 `DESIGN_ASSUMPTIONS.md` 记录，声明通过阈值（≤ 1% 误差为通过，否则触发 Plan B；分档降级流程见 [04 § 5.7.4](./04-toolchain.md#574-图构建管线)）（Plan B：TS Compiler API / LLM 直接读源码）
  - [ ] **Spike 3 补充 a** — 移至 M1 首 2 周独立任务（见 M1 工作量表「批大小优化验收」行）；M0 Spike 3 仅验证 tree-sitter 解析精度本身。
  - [ ] **Spike 3 补充 b：增量更新常数验证** — 实测「import 链深度 > 3 文件占比 / 反向 BFS 耗时 @100/300/500 文件 / > 50 直接导入者占比」三项，回填 [04 § 5.7.5 实测校准表](./04-toolchain.md#575-增量更新策略)，将「深度 ≤ 3 / 熔断 = 50」的最终值或调整规则写入 `DESIGN_ASSUMPTIONS.md`。
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
| Spike 0 crate 集成超限（编译时间/二进制体积/冷启动三项任一超 Spike 0 基线） | 纯 JSON 持久化（去 rusqlite，SQLite 延后 M2；断点续传用 atomic rename） | CLI 约 -2 人天（去 rusqlite），SQLite 推迟 M2；同上方 Spike 0 回退表 ① 行与 [04 § 5.7.3](./04-toolchain.md#573-持久化存储) |
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
- [ ] proptest 基础集成（纯函数 L2 等价测试 + seed 管理，不含完整 FFI 等价管线与行为录制）

**MVP 不包含（后续迭代）**：
- 多候选生成
- 模糊测试（cargo-fuzz）/ 变异测试（cargo-mutants）
- 多 agent 并行
- 行为录制框架
- `/migrate graduate`（含 unsafe 审计）

**验收指标**：
- 在 3 个真实 TS 小项目（<5K 行）中，每个项目至少完成 1 个纯函数模块的迁移（合计 ≥3 个模块）
- **项目选取规则**：3 个验收项目须**至少包含 1 个依赖数 15-25 的模块**（覆盖中段上下文预算场景；> 25 依赖触发降级路径，见 [02-architecture.md § 3.5.1](./02-architecture.md#351-上下文预算运行时检查与拆分策略)）。性能门禁与基准项目（~5K 行、约 150 行/文件）属"平均"分布，边界情况（深嵌套、高依赖）的全面覆盖后移 M2，本阶段仅验证降级机制可触发（见下方边界降级检查项）
- 迁移后代码通过 Tier 0 门禁
- insta 快照测试（即黄金文件测试，L1）100% 通过
- **质量评分通过线**（§ 7.5 `final_score`，分层）：`done` 状态模块 `final_score ≥ 80`，`degrade_ffi` 状态模块 `≥ 60`（不只靠 Tier 0 通过判定迁移质量）。M1 阶段评分为 **per-module 级别**（确定性指标自动化 + verifier AI 指标），sprint 聚合与跨 Sprint 趋势检测推至 M2（依赖多 Sprint 数据积累）
- 用户从 `/migrate analyze` 到 `/migrate review` 的完整流程可在 30-40 分钟内走通（单模块）
- 断点续传验证：中断会话后恢复，不丢失 migration-state.json 状态
- **边界降级确认**：至少 1 个**故意超出上下文预算**的模块被验证可触发拆分或降级路径（非静默失败）——本项验证 fallback 机制本身能工作，不要求边界场景全部通过翻译
- **可扩展性初步检验**（不阻断验收，仅采基线避免「M0 有 Spike 但 M1 验收不反映」的信息丢失）：在 M1 批大小优化验收使用的 3 个中型项目上复用其优化参数，额外记录三项指标——图构建耗时 @200+ 行/文件、单模块上下文峰值分布、编排稳定性样本——写入 `DESIGN_ASSUMPTIONS.md` 的「M1 可扩展性基线」节，作为 M2 中型项目验收的对比起点（而非依赖隐含的 Spike 结论）

**性能门禁（量化，作为"是否可升级 M2"的客观依据）**：

| 指标 | 阈值 | 测试方法 |
|------|------|---------|
| 图构建耗时 | `rustmigrate graph build` < 10s（100 文件项目） | 测试环境：M1/M2 Mac 或 4 核 CI runner；测试项目：100 个 `.ts` 文件、约 5K 行；冷启动（无 SQLite 缓存）计时 |
| 单 agent 完整流程用时 | 单模块 analyze→run→review 在 30-40 分钟内 | 在 3 个验收项目上各跑 1 个纯函数模块，取耗时区间；不含人工审阅等待 |
| 上下文预算利用率 | < 90%（5K 行以下模块的单次 Work Unit） | 记录 Work Unit 峰值上下文 token / 预算上限（≤100K，见 02-architecture.md）；interface_only 加载策略下统计 |

> 性能门禁仅约束 MVP 范围内的串行单 agent 路径。多 agent 并行吞吐属 M2 指标（见下），M1 不要求。
>
> 基准项目（~5K 行、约 150 行/文件）属"平均"分布；中型项目（5-20K 行、200-300 行/文件）的图构建/上下文性能校准见 M1 批大小优化验收结论，在 M1 结束前回填上表（即上方「可扩展性初步检验」采集的基线）。M2 验收需复测同类项目确认无退化。

**关键路径与依赖排序（避免误读为可全并行）**：

- **M0 内**：Spike 0（Plugin 骨架）是前置，必须最先通过；Spike 1/2/3/4/5 在 Spike 0 通过后可并行。
- **M1 内串行链**：`analyzer SubAgent` → `translator 生成 porting 规则`（**按项目即时生成，非预训练**，规则随 3 个真实项目迁移逐步产出）→ `TS 适配器 detect/extract` 脚本对接。三者非循环——适配器脚本提供输入，analyzer 消费图，translator 据图与陷阱清单生成规则。
- 可并行项：Hook 脚本、`.rustmigrate.toml`、Plugin 打包结构与 CLI 核心命令开发互不阻塞，可与上述链并行推进。
- **运行时串行边界（显式锁定）**：MVP 运行时 analyzer/scaffolder **严格串行执行**（`/migrate analyze` 的 3 次 SubAgent 调用顺序见 [06 § 10.5](./06-plugin-structure.md#105-编排调度路径)），Spike 1 验证的正是这条串行路径的可靠性，**不预期 MVP 内并行**；跨模块并行属 M2 范围（图写架构见下方 M2 段落）。上述「可并行项」指开发期任务可并行，与运行时 SubAgent 串行无关。

**M1 工作量分解（粗估）**：

| 交付物 | 预估人天 | 说明 |
|--------|---------|------|
| 3 个 SKILL.md（analyze/run/review） | 4-6 | 每个 Skill 约 1.5-2 天（含调试）；analyze 最复杂 |
| 4 个 SubAgent agent.md | 3-4 | 系统提示编写 + 职责边界定义 |
| 2 个 Hook 脚本（fmt.sh + verify.sh） | 1 | check.sh 被 rust-analyzer 替代，减少 1 个 |
| 文件保护 Hook（file-guard.sh） | 0.5 | PreToolUse 文件保护 |
| SubAgent 核心规则 + 参考指南 + 社区贡献模板 | 2-3 | agents/*.md 内嵌核心规则 + skills/migrate/references/ 参考指南编写 + 社区贡献模板。覆盖规则创建与基于模板的本地检查（见 [05 § 6.2.1](./05-documentation-system.md#621-26-类规则的演化路径)）；自动化版本追踪 / index.json 推迟 M2（见 [05 § 6.11.1](./05-documentation-system.md#6111-四层知识的-mvp-vs-m2-分阶段实施)），M1 不新增规则治理范围 |
| TS 语言适配器 | 3-5 | detect.sh + extract-types.sh + extract-deps.sh + porting-template.md |
| Plugin 打包结构 | 1-2 | plugin.json + 目录组织 |
| migration-state.json 管理 | 2-3 | Schema 定义 + 状态流转逻辑 + 断点续传 |
| .rustmigrate.toml | 1 | Schema 定义 + 默认值生成 |
| 单模块集成测试 | 2 | 单模块路径打通 + 调试 |
| 三项目端到端验证 + 规则缺陷回修 | 5-6 | 含规则缺陷发现→回修 agents/translator.md→重测的 1-2 轮迭代；含性能基准数据采集与环境标准化 0.5-1 天（环境元信息记录 + 写入 sprint-N-report.json，见下方 § 13.1.2） |
| 批大小优化验收（原 Spike 3 补充 a，非阻断） | 2-3 | M1 前 2 周执行，可与 TS 适配器并行；使用 3 个公开中型 TS 项目（列入 `.rustmigrate.toml` fixture）；在批大小 20/35/50 上测量批内符号引用覆盖度/跨批重复/依赖链准确度，结论写入 `DESIGN_ASSUMPTIONS.md`。**非 M1 阻断**：MVP <5K 行不触发批处理（>200 文件才激活），时间压力下可推迟到 M2 前 2 周合并执行 |
| proptest 集成与 seed 管理 | 1-2 | 纯函数 L2 等价测试基础设施（strategy 模板 + regression 文件管理），不含 FFI 等价管线 |
| CLI 核心（13 个 MVP 子命令） | 12-16 | init/profile/graph(build+topo-sort+deps+interfaces+stats)/validate-state/state(get+transition)/stats-loc/stats-compare/scaffold |
| CLI 嵌入 crate 集成 | 5-7 | tree-sitter 绑定(~2d) + ast-grep-core(~1.5d) + tokei(~1d) + petgraph(~1d) + rusqlite(~1.5d) + 跨平台编译/调试。**风险**：rusqlite 嵌入抬高编译时间与二进制体积，若实测触上限（见 M0 Spike 0 crate 集成风险评估）则 M1 切 JSON 持久化回退、SQLite 推迟到 M2（决策见 [04 § 5.7.3](./04-toolchain.md#573-持久化存储)前向兼容权衡） |
| CLI 测试 | 3-4 | 集成测试 + fixtures + stdout JSON 格式快照测试 + 1 个自建微型项目（dogfooding fixture：50-100 行 TS 输入 + 手写期望 Rust 输出，0.5 天；`dogfooding.yml` workflow，1 天，仅验证到 Tier 0，见 [03 § 4.11.4](./03-execution-model.md#4114-项目自验证dogfooding-m2-概念设计)）；含 `ci.yml` PR workflow：`cargo fmt --check` + `cargo clippy -- -D warnings` + `cargo nextest run` + `cargo audit`（advisory-only）；矩阵 = `{MSRV, stable}` × `{ubuntu-latest, macos-latest}`；覆盖率 informational M1、≥70% target M2 |
| 可复现性脚本与 CI 集成 | 1.5-2 | 规范补充（排除规则 / 环境快照 JSON Schema / 哈希工具标准化）0.5 天 + 脚本实现 1-1.5 天：`verify-reproducibility.sh`（环境快照 + 过滤后 `sha256sum -b` 比对）+ GitHub Actions 集成（见 [03 § 4.11.3](./03-execution-model.md#4113-可复现性保证)） |
| Plan B 缓冲（单 Spike） | 2-5 | **仅**单个 Spike 触发时的额外开发量；多 Spike 失败（尤其 Spike 1+4 同时失败需重设计编排层）见上方决策检查点行「+5-10 人天」与下方场景 B，**不在本行覆盖** |
| **合计** | **50-70** | 1 人约 12-18 周，2 人约 6-9 周 |

> **优先级削减顺序**（工期超预算时）：显式标记「非阻断」的项（批大小优化、可扩展性检验）最先推迟；其次为 proptest 集成、Plan B 缓冲（未触发时归零）；核心串行链（SubAgent agent.md → TS 适配器 → 端到端验证）不可削减。

> **M0 假设验证周**（5-10 人天）不在上述 M1 估算中，应单独核算。
> **与 v0.9.2 估算的差异**：集成测试（+4 天）、CLI graph build（+3 天）、crate 集成（+2 天）、Plan B 缓冲（+3 天）。详见可行性审查报告。
>
> **估算透明度说明**：CLI 13 个命令（12-16 天）按每命令 ≈0.9-1.3 天（纯命令逻辑）+ 1-2 天 graph 子命令共享开销（petgraph 初始化）折算，crate 集成风险（见上表「CLI 嵌入 crate 集成」行）与集成测试 fixture（见「CLI 测试」行）已**单列不重复计入**本行；表内人天含同行评审 + 1 轮集成测试 fixture（dogfooding），纯开发时间约为估算的 60%。M0 Spike 0 的 crate 风险评估直接喂给 M1 决策——若 rusqlite + tree-sitter 触编译/体积上限，M1 执行 JSON 回退（见 Spike 0 回退表），CLI 开销约 -2 天但图持久化推迟 M2。

> **场景 A / B 对比**（**实际范围取决于 M0 验收结果**）：
> - **基线 M1**（所有 Spike 验证通过，无 Plan B）：**50-65 人天**。
> - **失败缓冲 M1**（Spike 1 编排 / Spike 4 SKILL.md 跟随触发 Plan B3 混合方案）：**60-75 人天**。上表「Plan B 缓冲 2-5 人天」仅覆盖单个 Spike 回退；若 Spike 1+4 同时失败，需额外 +5-10 人天实施 Plan B3（编排层改用微 Skill 链 / 外部脚本编排），故上限抬高到 75 人天。
> 决策规则见下方「M0→M1 决策检查点」。

### M2: 质量提升（8-12 周）

**目标**：验证管线完整，翻译质量可靠

**交付物**（内部排序：状态机程序化 → 多 agent worktree → CLI 扩展；前 2 周见首项标注；详细分期于 M1 验收后产出）：
- [ ] **上下文预算实证校验（M2 前 2 周，优先）** — 用中等复杂度真实项目实测 [02 § 3.5.1](./02-architecture.md#351-上下文预算运行时检查与拆分策略) 预算表在深嵌套/高依赖模块上的准确度，结果反馈规则库改进（承接 M1 后移的边界场景全面覆盖）
- [ ] 多候选生成 + 最优选择
- [ ] 属性测试完整管线（proptest FFI 等价性验证 + 跨模块回归集联动，承接 M1 基础集成）
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
- [ ] petgraph 副本策略验证 + WAL **配置**回归测试（**D5 定稿：翻译期图只读、编排器集中 writer**，各 agent 从 DB 加载独立只读内存副本；断言 `PRAGMA journal_mode=WAL` + `busy_timeout` 配置正确，**非并发写压测**，见 [04 § 5.7.3](./04-toolchain.md#573-持久化存储)，1-2 人天）
- [ ] 自定义 lint crate 架构（如 M1 确认需语义规则占比 > 30% 或规则 > 15 条，见 [04 § 5.2](./04-toolchain.md#52-tier-0硬性门禁)，预估 3-5 人天）
- [ ] 规则治理工具化（含规则元数据 schema、deprecation 工具、社区评审检查清单自动化，承接 M1 模板驱动方案；MVP=模板驱动、M2=工具驱动，见 [05 § 6.2.1](./05-documentation-system.md#621-26-类规则的演化路径)，预估 2-3 人天）
- [ ] 状态机程序化实现（独立 Rust orchestrator 二进制，确定性编排，替代 MVP 的 SKILL.md 指令驱动；见 [02 § 3.4.1](./02-architecture.md#341-mvp--m2-演进与向后兼容)）
  - [ ] 将 blocked 状态恢复逻辑从 [SKILL.md Step 0.5](./09-appendix-schemas.md#step-05-自动解除可解除的-blocked-模块) 抽取为确定性 CLI 命令 `rustmigrate validate state --check-blocked --auto-unblock`：以 DFS 环检测 + 拓扑排序实现自动逐层解除，返回 JSON `{ "unblocked": [<module>...], "still_blocked": [<module>...], "cycle_detected": <bool>, "cycle_path": [<module>...] | null }`。替代 MVP 期由 SKILL.md 指令跟随驱动的非确定性恢复（1-2 人天，并入上方状态机程序化人天）
- [ ] migration-state.json 向后兼容框架（`version` 字段 + 自动迁移脚本，集成进 M2 CLI 的 `init`/`validate state` 版本检测与升级）
- [ ] /goal 自主迁移循环支持
- [ ] CLI 扩展（M2 已定 5 命令：rdeps/cycles/export/validate-config/state-update，权威清单见 [06 § 10.0.1](./06-plugin-structure.md#1001-cli-命令清单)；M2 规划期另有候选命令待评估）
- [ ] CI/CD 集成（`rustmigrate` 在 GitHub Actions 中使用；落地设计见 [03-execution-model.md § 4.11](./03-execution-model.md#411-cicd-集成m2-范围)）
- [ ] Dogfooding fixture 编写与 CI workflow 完成（承接 M1 备料的 fixture，落地 `dogfooding.yml` 并按验收标准升级为 required，0.5-1 天，见 [03 § 4.11.4](./03-execution-model.md#4114-项目自验证dogfooding-m2-概念设计)）

> **图写架构（跨模块并行阶段，D3/D5 定稿）**：M2 跨模块并行采用 git worktree 方案，**翻译期 SubAgent 在各自 worktree 内只读加载图、不直写 db**；`source-graph.db` 的写者**只有编排器（集中 writer）**——`graph build → save_to_db` 写图结构 + 模块终态把 `migration_status` 回写图节点（与 `migration-state.json` 同序，见 [06 § 10.5](./06-plugin-structure.md#105-编排调度路径) 与 [04 § 5.7.3](./04-toolchain.md#573-持久化存储)）。**因翻译期无多 writer 并发，原「共享 DB 并发写 + WAL 串行化」架构被集中 writer 取代**；原 WAL 并发写策略（busy_timeout/退避重试/写锁等待门禁）**保留为未来「SubAgent 直写 db」模式的可选回退**，当前仅保留 WAL **配置**回归（断言连接 PRAGMA 正确，非并发压测），「冲突率/锁等待」量化门禁经 **D5 降级为 N/A**。多 agent 的真实冲突面转移到 Rust 代码装配（共享 `.rs`/Cargo.toml/lib.rs），由 D3 约束包治理（见 [06 § 10.5](./06-plugin-structure.md#105-编排调度路径)）。状态机程序化 + schema 向后兼容框架合计 +3–5 人天。

**验收指标**：
- 在 3 个真实 TS 中型项目（5K-20K 行）中完成多模块迁移
- 属性测试覆盖核心函数
- 翻译膨胀率 < 3.0x
- 降级路径（FFI 桥接）在至少 1 个复杂模块上成功

**性能门禁（量化，评估规模化就绪）**：

| 指标 | 阈值 | 测试方法 |
|------|------|---------|
| 多 agent 并行吞吐 | ≥ 1.5 模块/小时（`max_concurrent_agents=3`，假设条件见 [03 § 4.10(2)](./03-execution-model.md#410-性能与并行转换指南)） | 在中型验收项目上跑 Workflow 批量模式，统计同批内 3 agent 并行完成模块数 / 墙钟小时；**记录同批模块耗时分布（P50/P95/P99）**，确认 1.5 baseline 基于 P50/平均值；若 P99 超预期（> 50% 模块），重新评估目标可达成性或调整 `max_concurrent_agents`；不含人工审阅 |
| WAL 配置回归（D5：取代「并发写冲突率」门禁） | 连接配置正确 | 断言 `PRAGMA journal_mode=WAL` + `busy_timeout` 配置正确（防御未来回退并发写模式时配置丢失）。**D5 定稿：翻译期图只读、编排器集中 writer，原「3 agent 并发写冲突率 < 10% / 锁等待 ≤ 20ms」量化门禁 N/A**；该门禁仅在未来「SubAgent 直写 db」回退模式重新激活 |
| 性能基准无退化 | 相对 M1 基线波动 ≤ ±10% | 复用 M1 性能门禁三项（图构建、单 agent 流程、上下文利用率），在相同测试项目上回归对比 |

> M2 并行吞吐指标上限按 `max_concurrent_agents=3` 设定，与 MVP 范围一致；不引入未规划的 4+ agent 并行（属 M4 优化项）。

**M2 → M3 升级判据**：M2 验收通过（即可进入 M3 多语言支持）的充要条件为以下全部满足——(a) P50 ≥ 1.5 模块/小时；(b) P99 ≥ 0.8 模块/小时（允许部分低效批次但不破此线）；(c) **（D5 定稿）集中 writer 模型，SQLite 并发写门禁 N/A；仅保留 WAL 配置回归**（断言连接 PRAGMA 正确）；(d) 相对 M1 基线性能波动 ≤ ±10%（上表已定）。理论上 3 agent 理想并行可达 ~5 模块/小时（35 分钟/模块 ÷ 3），1.5 为含 worktree 启动开销后的**及格线**而非天花板。若 (a)-(d) 全过则验收通过；任一不满足则 M2 增 1-2 周优化（SQLite 分片 / worktree 缓存等），或降 `max_concurrent_agents` 至 2 后重新验收。实测 P99 分布须与 [03 § 4.10(2)](./03-execution-model.md#410-性能与并行转换指南) 假设条件对标，超预期须在 MDR 记录偏差根因。

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

M4 重定位为**双主线**（详见 [PLAN-M4.md](../PLAN-M4.md) D-M4-06）：巩固线（迁移质量度量框架 + Community 结构诊断 + 循环健壮性）+ Go 扩语言线。交付物及状态：

- ✅ **Go LanguageAdapter**（Go 线旗舰，Sprint C/D/E）：tree-sitter-go 0.21 解析 + 包系统映射（扩 trait 暴露目录列举，D-M4-01）+ interface 隐式实现不强连（D-M4-02）+ 并发/cgo/unsafe 降级边界（D-M4-05）。**Sprint E 端到端验收达标**：2 个真实 Go 项目（semver 单包 / go-humanize 多模块）各完成多模块迁移、≥1 模块 done，差异测试逐条等价（semver 276/276、humanize 87/87），质量度量 final_score=100 与既有语言基线同档。
- ✅ **迁移质量度量框架**（巩固线，Sprint B QUAL-01）：源行为覆盖率 / degrade 率 / 人工修订率 / `final_score`（§7.5 公式），`rustmigrate stats quality` 输出。
- ✅ **Community 结构偏离度诊断**（巩固线，Sprint B QUAL-04，Tier 1）：自实现 Louvain 社区检测 vs 目录分区 NMI/ARI，`rustmigrate stats community` 输出。
- ✅ **循环健壮性**（巩固线，Sprint F ROB-01a/b/c）：checkpoint 硬化 + watchdog stall 恢复 + 额度耗尽优雅暂停/续跑。
- ✅ **并行编排收口**（巩固线，Sprint F ORCH-01）：保留并行 + worktree 写隔离（[MDR-018](../decisions/018-keep-parallel-migration.md)），`scc_groups` 拓扑分层调度。
- ⏸️ **C/C++ LanguageAdapter（bindgen + cbindgen）**：**推迟**（D-M4-03）——无类型 IR 下 C 语义翻译 + 等价审查难度高、ROI 低（非「宏一票否决」）。
- ⏸️ **Kani 集成（关键路径形式化验证）**：**推迟**（D-M4-04，非砍）——Kani 验证正确性（无 panic/溢出/越界）、proptest 验证等价性，互补不替代；当前 ROI 不足，推迟到 C 路线或更大项目验证阶段。
- **社区反馈驱动的规则库积累**：部分——Sprint F GOV-01 版本检测 + Sprint B QUAL-04 结构诊断落地；社区运营非代码范围。
- ⏸️ **Strangler Fig 模式工具支持**：降为文档——离线场景下共存需求不强。

### 13.1.1 M1→M2→M3 规则库累积效应分析

路线图各阶段并非孤立——M1 产出的规则库是 M2/M3 的输入，这条隐含的演化路径在此显式化，避免把 MVP 误读为"零规则即开箱即用"。

- **(a) M1 → M2/M3 规则累积**：M1 在 3 个 <5K 项目迁移中发现的项目规则（写入 `.rust-migration/porting/`），作为 M2「多候选排序」的打分依据与 M3 Python 适配器的初始模板来源。预期 M1 发现 **15-25 条新通用规则**（覆盖 TS→Rust 常见陷阱，对应 [07-pitfalls-and-risks.md § 9.2 跨语言语义陷阱](./07-pitfalls-and-risks.md#92-跨语言语义陷阱补充) 与 [03-execution-model.md § 7.7 不等价探测维度](./03-execution-model.md#77-不等价证据探测维度清单)）。
- **(b) M1 规则库分类与复用评估**：消除「通用陷阱规则 vs 项目专有规则」混淆——每条规则带分类元数据（frontmatter schema 权威定义见 [05 § 6.2 社区贡献快速参考](./05-documentation-system.md#社区贡献快速参考)），使 M2/M3 复用率可评估而非拍脑袋。`TypeMapping`/`LanguageSemantics` 多为跨语言通用陷阱（如 Promise eager、闭包引用捕获）；`ProjectPolicy`/`NamingConvention` 多为项目专有（如禁用 deprecated `foo()`）。分类即决定 M3 可复用性。
- **(c) M1 验收检查表（规则库维度，不阻断验收）**：
  1. **分类完成度**：全部规则已标注 `category` + `target_languages` + `ts_only`；
  2. **覆盖率报告**：对照 § 9.2 的 8 类陷阱与 § 7.7 的 8 维度，计算有规则覆盖的比例，**目标 ≥ 60%**；缺失维度列入 M2 补充计划并记录理由；
  3. **M3 可复用性预估**：统计 `target_languages` 含 `py` 的规则占比，**预期 ≥ 40%**。

  本表评估不阻断 M1 验收（与原「成熟度评估检查点」一致），仅作为 M2/M3 范围输入。
- **(d) M3 Python 适配器复用**：M3 复用的是**统一契约**而非脚本代码——依赖提取（`extract-deps.sh`）输出统一 JSON、类型提取（`extract-types.sh`）输出统一 type-map（`{source_type, source_language, rust_type, notes, rule_ref}`，见 [schemas/type-map.example.json](./schemas/type-map.example.json)）；Mypy 集成、ts-compiler-api 调用与 PyO3 桥接为各语言专有实现，M3 对接此 schema 而非试图共享脚本（类型提取按设计为语言专用、不走统一 IR，见 [06-plugin-structure.md § 11.2](./06-plugin-structure.md#112-语言扩展架构)）。差异测试基础设施与社区检测诊断（`stats community` 用 Louvain；批次划分已由 [MDR-011](../decisions/011-coupling-agglomerative-decomposition.md) 目录优先凝聚取代）与语言无关，M3 多语言场景直接复用。

> **规则成熟度演化**：MVP 的验证管线开箱即用，但**项目专有规则首轮迁移需 1-2 个 Sprint 积累**（见 [01-positioning-and-methodology.md § 1.3](./01-positioning-and-methodology.md#13-我们做什么)）。通用规则随版本沉淀，项目规则随首轮迁移生成。

### 13.1.2 M1 性能基准建立与回归检测流程

衔接 M1/M2 性能门禁，明确「基准如何建立、如何持久化、如何检测回归、与质量评分如何协调」，避免门禁数字悬空：

1. **测试环境标准化**：性能数据采集时须记录环境元信息（CI runner 型号 / Rust 版本 / tree-sitter 版本），以便跨 Sprint 对比时对齐环境变量；M1 基准即作为 M2「相对 M1 基线波动 ≤ ±10%」回归对比的起点（同环境复测，环境不一致时须重采基准）。
2. **性能数据持久化**：复用 `sprint-N-report.json` 的 `quality_scores`，新增**可选**字段 `performance_metrics: {env_info, build_time, module_throughput, context_utilization}`（M1 实现时追加到 [09 附录 F](./09-appendix-schemas.md#附录-f评分报告-sprint-n-reportjson-schema) schema，当前为计划字段），不引入额外文件格式。
3. **回归检测框架**（非独立 CLI 子命令）：对比相邻 Sprint 的 `performance_metrics`，超阈值时采 criterion 的 `relative_difference` + P99 延迟双指标定位，结果由 verifier 在 `/migrate review` 报告中呈现（M2 范围，跨 Sprint 趋势检测口径见 [03 § 7.5 M1/M2 时序划分](./03-execution-model.md#75-质量评估分层评分卡) 与 § 4.9 回归阈值）。
4. **与质量评分协调**：当 `migration_motives` 含 `performance` 时，[03 § 7.5](./03-execution-model.md#75-质量评估分层评分卡) 评分卡的 L6 性能回归（criterion）结果作为 `final_score` 的**可选加权项**纳入（非性能动机时不计权），与既有「性能动机时 L6 必须」一致。
5. **工作量**：M1 工作量表「三项目端到端验证」行已含「性能基准数据采集与环境标准化 0.5-1 天」。

> 关键流程（环境记录、对比逻辑、诊断手段）在设计阶段明确，具体实现推迟到 M2；M1 仅按上述结构采集 per-module 性能基线。

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
