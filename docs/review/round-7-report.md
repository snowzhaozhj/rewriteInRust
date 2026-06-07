# 本轮审查报告

Round 1 | 25 findings | 4 high | 11 medium | 10 low

---

## D1 迁移质量与翻译方法论

| ID | 严重度 | 位置 | 状态 |
|----|--------|------|------|
| D1-01 | low | 03-execution-model.md | adjusted |

**问题**: Phase B「消除翻译腔」活动无边界正面定义、无 MDR 要求、无 verifier 审计专属判据，translator 无法区分该类变更与需记 MDR 的三类语义重写。
**方案**: Step 5 第四项后追加边界定义：不改变 Step 6 四项审查维度的表达方式变更；改变任一项即归入三类重写并须记 MDR。

---

| ID | 严重度 | 位置 | 状态 |
|----|--------|------|------|
| D1-02 | medium | 03-execution-model.md | rejected |

**问题**: Phase A 翻译重试耗尽后终端动作未定义，03 与 09 形成「定义真空」。
**Rejected 理由**: 06 section 10.2.2 已定义所有 SubAgent 调用重试耗尽的通用终端动作（三选项：重试/部分跳过/完整回滚）。09 表设计为与 06 通用规则配合阅读（表头明确引用 06 section 10.2.2）。Phase A 与 Phase B 是架构上不同的失败模式（one-shot SubAgent call vs iterative compilation loop），无定义真空。

---

| ID | 严重度 | 位置 | 状态 |
|----|--------|------|------|
| D1-03 | medium | 07-pitfalls-and-risks.md | rejected |

**问题**: section 9.2 语义陷阱清单缺少 OOP 继承/多态 -> trait 映射的 TS->Rust 专项条目。
**Rejected 理由**: 因果链不成立。section 9.2 并非 verifier 系统提示的直接输入（verifier 用的是 section 7.7 维度表和 Phase B 审查清单），而是规则覆盖评估指标和规则 frontmatter 元数据来源。RULE-26 已显式覆盖「多态/动态分发映射」领域，verifier 在审查时应用该规则。

---

| ID | 严重度 | 位置 | 状态 |
|----|--------|------|------|
| D1-04 | low | 03-execution-model.md | adjusted |

**问题**: section 7.7 proptest 生成指令的执行时机在 Step 4 与 Step 6 间存在歧义，双阶段执行模型未在 section 7.7 处显式说明。
**方案**: section 7.7 行动指南处插入括号注释：「执行时序：维度选定在 Step 4，proptest 生成在 Step 6 执行，时序详见 section 7.1」。

---

## D2 验证体系可靠性

| ID | 严重度 | 位置 | 状态 |
|----|--------|------|------|
| D2-01 | medium | 06-plugin-structure.md | adjusted |

**问题**: `state transition --to done` 缺乏验证产出物前置检查，README 承诺（AI 无法跳过门禁）与实际防御深度（prompt 级约束）存在错位。
**方案**: 修正 README 措辞区分两层保证：verify.sh 内部逻辑确定性不可修改；MVP 阶段调用由 SKILL.md 编排保证，M2 升级为程序化强制。不改 CLI（尊重 CLI-vs-Plugin 边界）。

---

| ID | 严重度 | 位置 | 状态 |
|----|--------|------|------|
| D2-02 | medium | 03-execution-model.md | confirmed |

**问题**: L7(loom/shuttle) section 7.1.1 要求模块级必须 vs section 4.5 仅 F3 级别 vs verify.sh 不执行 --cfg loom，并发模块可不经 loom 验证达到 done。
**方案**: section 4.5 F2 列改为「部分（loom 基础验证）」；06 verify.sh 追加 async/concurrent 模块的 loom 执行子句。

---

## D3 工具架构与工程质量

| ID | 严重度 | 位置 | 状态 |
|----|--------|------|------|
| D3-01 | **high** | 09-appendix-schemas.md | confirmed |

**问题**: SKILL.md 骨架从未调用 `--to testing` 或 `--to reviewing`，断点续传路由表依赖这两个状态但永远无法触发；且 `--to done` 在单跳校验下会被 CLI 拒绝。
**方案**: Step 5 开头添加 `--to testing`，Step 5 检查点通过后添加 `--to reviewing`。净增 2 行。

---

| ID | 严重度 | 位置 | 状态 |
|----|--------|------|------|
| D3-02 | medium | 09-appendix-schemas.md | confirmed |

**问题**: 意图摘要文档反复称「7 字段」但 JSON Schema required 含 9 个属性（module 元数据 + pre/postconditions 拆分）。
**方案**: 将「7 字段」统一改为「7 个内容维度（对应 9 个 required 属性）」并注明映射关系，同步更新 06 中 3 处引用。

---

| ID | 严重度 | 位置 | 状态 |
|----|--------|------|------|
| D3-03 | low | 06-plugin-structure.md | confirmed |

**问题**: M2 CLI 命令小节标题「6 个命令」与表格实际 5 行不一致。
**方案**: 标题改为「M2 扩展 -- 5 个命令」。

---

## D4 技术选型审查

| ID | 严重度 | 位置 | 状态 |
|----|--------|------|------|
| D4-01 | medium | 04-toolchain.md | confirmed |

**问题**: section 5.7.4 分工表排除 tree-sitter 产出 calls 边，但合并策略包含「tree-sitter 和 ast-grep-core 均产出 calls 边时保留 TreeSitter」的死逻辑。
**方案**: 删除该死逻辑子句。

---

| ID | 严重度 | 位置 | 状态 |
|----|--------|------|------|
| D4-02 | medium | 04-toolchain.md | confirmed |

**问题**: clippy.toml 在 .rust-migration/ 而 verify.sh 在 rust_root 执行 cargo clippy，默认向上查找无法命中；06 对 clippy 配置路径传递无规范。
**方案**: 04 section 5.2 将「或经 CLIPPY_CONF_DIR」改为「须经」；06 section 10.3 追加 CLIPPY_CONF_DIR 设置约束。

---

## D5 编排可靠性与确定性

| ID | 严重度 | 位置 | 状态 |
|----|--------|------|------|
| D5-01 | **high** | 09-appendix-schemas.md | confirmed |

**问题**: flock 全局锁在 Claude Code 短命 Bash 进程模型下逻辑失效——进程退出即释放 advisory lock，对并发会话提供零保护。stale 检测路径为不可达死代码。
**方案**: 改为基于 link() 原子创建的内容锁 + $PPID 会话进程存活检测；lock_timeout_secs 降为 300；M0 Spike 0 验证 $PPID 可靠性。

---

| ID | 严重度 | 位置 | 状态 |
|----|--------|------|------|
| D5-02 | medium | 09-appendix-schemas.md | confirmed |

**问题**: Step 0.3 路由遗漏 phase_b_optimization_in_progress substatus，路由结果未定义（非 null 无法匹配兜底）。
**方案**: 增加路由行 `phase_b_optimization_in_progress -> Step 4`；修正匹配说明为前缀匹配。

---

| ID | 严重度 | 位置 | 状态 |
|----|--------|------|------|
| D5-03 | low | 06-plugin-structure.md | rejected |

**问题**: subagent_timeout_secs 在 SKILL.md 模式下仅为 post-hoc 分类阈值，无法主动终止 SubAgent。
**Rejected 理由**: 文档从未声称主动终止能力。参数一贯在 recording/classification 语境使用；默认值 600s 已与 LLM API 超时对齐；line 525-526 已承认 MVP 无确定性程序控制。无虚假承诺，无实际用户困惑。

---

## D6 规模化与性能

| ID | 严重度 | 位置 | 状态 |
|----|--------|------|------|
| D6-01 | medium | 04-toolchain.md | confirmed |

**问题**: M2 source-graph.db 并发访问模型跨文件矛盾（04 允许 agent 并发写 vs 06 要求 agent 只读+集中写），且 worktree 物理隔离使相对路径无法指向同一 DB。
**方案**: 04 增加降级条件（>50ms 切只读模型）并交叉引用；06 标注为备选策略；04 追加 M2 worktree 通过 shared_db_path 绝对路径引用主 DB。

---

## D7 可维护性/可扩展性/社区贡献

| ID | 严重度 | 位置 | 状态 |
|----|--------|------|------|
| D7-01 | low | 05-documentation-system.md | confirmed |

**问题**: 6 个治理角色术语在流程关键上下文中使用但未定义相互关系（Plugin 维护者/Plugin Lead/技术委员会/社区维护委员会等）。
**方案**: section 6.2 段首插入一句消歧说明：三个维护者称谓为同一角色池、两个委员会为同一机构，指向 GOVERNANCE.md。

---

## D8 范围控制/过度设计/路线图

| ID | 严重度 | 位置 | 状态 |
|----|--------|------|------|
| D8-01 | low | 08-roadmap-and-reference.md | rejected |

**问题**: M2->M3 升级判据以未经验证的理论值锁定「充要条件」，属前向承诺过度。
**Rejected 理由**: 属于「仅要求实现阶段实测数据」类诉求（REVIEW_LOOP section 5 明确排除）。文档已含修订机制：escape clause（不满足则延期/降级）+ MDR 记录偏差根因 + 03 section 4.10(2) 明确声明实测超出时须重新校准。具体初始数值+校准机制是合理设计方法，非缺陷。

---

| ID | 严重度 | 位置 | 状态 |
|----|--------|------|------|
| D8-02 | low | 08-roadmap-and-reference.md | confirmed |

**问题**: section 13.1.2 引用 09 附录 F 为 performance_metrics schema 权威，但该字段不存在于 09 附录 F 中，属虚引。
**方案**: 括号注释改为「M1 实现时追加到 09 附录 F schema，当前为计划字段」。

---

| ID | 严重度 | 位置 | 状态 |
|----|--------|------|------|
| D8-03 | low | 08-roadmap-and-reference.md | adjusted |

**问题**: M1 工作量表 18 项交付物仅 2 项有显式可推迟标记，缺少工期超预算时的优先级削减顺序。
**方案**: M1 工作量表后新增一行 blockquote 脚注：非阻断项最先推迟 -> proptest/Plan B 次之 -> 核心串行链不可削减。

---

## BS1 实操盲点

| ID | 严重度 | 位置 | 状态 |
|----|--------|------|------|
| BS1-01 | medium | 03-execution-model.md | confirmed |

**问题**: dual-track 模式下 STRUCTURAL 变更触发重新翻译时，test-fixtures（golden/recordings/proptest-regressions）仍锚定旧源行为，导致 verifier 误报。
**方案**: section 4.6.1 第 4 点追加：重新翻译时须同步更新 source-ref/ 并重新录制 test-fixtures，过期判定复用 MDR translated_from_source_commit 字段。

---

| ID | 严重度 | 位置 | 状态 |
|----|--------|------|------|
| BS1-02 | low | 03-execution-model.md | adjusted |

**问题**: subprocess bridge 未定义类型序列化边界——纯但参数含 Map/Set/RegExp/Buffer 等非 JSON-safe 类型的函数通过 purity gate 后会在 bridge 层运行时失败。
**方案**: scaffolder 条件步骤追加 JSON-safe 类型限定条件，不满足时归入 section 7.6.1「部分纯化」行。

---

## BS2 OSS 工程基线

| ID | 严重度 | 位置 | 状态 |
|----|--------|------|------|
| BS2-01 | low | 06-plugin-structure.md | adjusted |

**问题**: 对 Rust 生态工具依赖（cargo-nextest）无 pre-flight 检查，用户可能在 verify.sh 时遇到 command not found。
**方案**: 扩展 rustmigrate profile 描述包含 Tier 0 外部二进制检查；error table 增加 RUST_TOOL_MISSING 错误码。

---

| ID | 严重度 | 位置 | 状态 |
|----|--------|------|------|
| BS2-02 | medium | 08-roadmap-and-reference.md | confirmed |

**问题**: rustmigrate 项目自身的开发 CI pipeline 未定义（PR 检查项、测试矩阵、覆盖率目标），贡献者无 CI 契约。
**方案**: M1 工作量表 CLI 测试行末尾追加 ci.yml 规约：fmt/clippy/nextest/audit，矩阵 MSRV+stable x ubuntu+macos，覆盖率 M1 informational / M2 >=70%。

---

| ID | 严重度 | 位置 | 状态 |
|----|--------|------|------|
| BS2-03 | medium | 06-plugin-structure.md | confirmed |

**问题**: rustmigrate 自身无依赖维护策略——对用户项目要求 cargo-audit 但不要求自身，release checklist 遗漏。
**方案**: release checklist 增加 cargo audit 检查项 + 一行依赖维护策略（Dependabot + 7 天 SLA）。

---

## BS3 内部矛盾/跨文件一致性

| ID | 严重度 | 位置 | 状态 |
|----|--------|------|------|
| BS3-01 | medium | 06-plugin-structure.md | confirmed |

**问题**: M2 CLI 标题说 6、表列 5；跨文件引用「12+6」但 MVP 实际 13 条。多处数字不一致。
**方案**: 06 标题改 5；03/05 跨文件引用统一为「13+5」。

---

| ID | 严重度 | 位置 | 状态 |
|----|--------|------|------|
| BS3-02 | medium | 04-toolchain.md | confirmed |

**问题**: Tier 1 失败影响在 04 section 5.1 声明「警告但不阻塞」，与 03 section 7.5（L2/L3 失败阻塞）和 03 section 7.1.1（proptest 强制）矛盾。
**方案**: 04 section 5.1 Tier 1 失败影响改为「默认警告；条件强制时阻塞」并交叉引用 section 5.3 / 03 section 7.1.1 / 03 section 7.5。

---

| ID | 严重度 | 位置 | 状态 |
|----|--------|------|------|
| BS3-03 | **high** | 06-plugin-structure.md | confirmed |

**问题**: M2 parallel claim「analyzer + scaffolder 可并行」与同文件 scaffolder 前置条件「analyzer 已完成」直接矛盾。跨模块并行的真实意图从未显式说明。
**方案**: 4 处替换为「跨模块有限并行」表述，明确同一模块内仍遵守前置条件链，并发上限由 max_concurrent_agents 控制。

---

## 本轮修复文件清单

| 文件 | 修复 Finding |
|------|-------------|
| docs/design/03-execution-model.md | D1-01, D1-04, D2-02, BS1-01, BS1-02, BS3-01(2 sites) |
| docs/design/06-plugin-structure.md | D2-01, D2-02(verify.sh), D3-02(3 refs), D3-03, BS2-01, BS2-03, BS3-01, BS3-03(2 sites), D4-02, D5-01(sync) |
| docs/design/09-appendix-schemas.md | D3-01, D3-02, D5-01, D5-02 |
| docs/design/04-toolchain.md | D4-01, D4-02, D6-01, BS3-02 |
| docs/design/05-documentation-system.md | D7-01, BS3-01(1 site) |
| docs/design/08-roadmap-and-reference.md | D8-02, D8-03, BS2-02, BS3-01, BS3-03 |
| docs/design/README.md | D2-01 |

## 转 M0 Spike 清单

| 来源 | 待验证项 |
|------|----------|
| D5-01 | M0 Spike 0 验证 Claude Code Bash 环境中 $PPID 是否可靠指向会话级长生命进程 |
| D8-01 (rejected) | M2->M3 升级判据数值待 M2 第 4 周据实测校准（已有 MDR 修订机制，不作为设计缺陷） |

---

**统计**: confirmed 16 | adjusted 5 | rejected 4 | high 4 (全部 confirmed) | 净增约 +20 行设计文本
