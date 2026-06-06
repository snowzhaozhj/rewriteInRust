# 审查轮 1 报告

本轮共 36 条 finding（confirmed / adjusted / rejected 状态逐条标注）。按维度分组，每条列 ID / 严重度 / 位置 / 状态 / 问题 / 优化方案；rejected 附 verifier 驳回理由。末尾为本轮修复文件清单。

---

## 维度 D1：迁移质量与翻译方法论

### R1-D1-01 · high · 03-execution-model.md · confirmed
- 问题：意图摘要内容规范缺失，必须项无清单，verifier 对抗审查仅比对「源码 vs Phase A」、不含「意图摘要 vs Phase A」契约一致性。
- 方案：定义 6 字段意图摘要模板，verifier §7.7 新增第 9 维「意图一致性」，Phase B 按维度 9 校验，09 加模板+Schema。

### R1-D1-02 · high · 03-execution-model.md · confirmed
- 问题：Phase A 结构校验阈值（0.9–2.0x 函数、1.2–3.0x 行数）与 §7.5 不一致，且无实测/论文数据支撑。
- 方案：门禁阈值显式定为 §7.5 告警上界，标注为初始估计，M1 须 ≥3 真实项目实测、偏差 >20% 调整。

### R1-D1-03 · medium · 03-execution-model.md · confirmed
- 问题：TODO(port) 清理缺自动化执行机制——Phase B 可否新增、"趋势下降"量化、verifier 是否据此拒 done、状态转换前置均未定义。
- 方案：Phase B 禁新增，done 须计数=0（verifier Step 6 扫描），净增 ≤0/消除率 ≥10%，按模块分解报告。

### R1-D1-04 · high · 03-execution-model.md · confirmed
- 问题：质量评分卡 AI 指标（30% 权重）无客观规则，裸 unwrap 例外/语义一致定义/可维护性量化/JSON Schema/归一化均缺失。
- 方案：三指标改客观锚点（clippy JSON/裸 unwrap 计数+例外/intent 6 字段/proptest 回归/cargo doc），ai_avg 改 min/median/max 加权，M1 ICC ≥0.75 校准。

### R1-D1-05 · medium · 03-execution-model.md · confirmed
- 问题：Phase B「合规重写」边界模糊（async 改签名是否超界、取消安全是否夹带业务逻辑、性能优化范围），verifier 判"一致"无自动化检查。
- 方案：三类改动给精确边界（async 改签名即超界），Step 6 加审查清单+针对性测试（loom/criterion），否决"事前 proptest 重放"循环；MDR 加必填字段模板。

### R1-D1-06 · high（medium→high）· 03-execution-model.md · adjusted
- 问题：意图确认门禁交互机制不足，且存在矛盾——Step 2.5 说 MVP 默认开启，但 /goal 与 Workflow 模式"默认跳过"，自动化场景完全无人工审视。
- 方案：新增 §4.3.1 交互规范（长度指南/拒绝 5 特征/修订 ≤2 轮升级/三模式默认值表），两个可靠性假设写 DESIGN_ASSUMPTIONS.md。严重度上调因直接影响 MVP 可靠性。

---

## 维度 D2：验证体系可靠性

### R1-D2-01 · medium · 03-execution-model.md · adjusted
- 问题：L3 差异测试模块间依赖可行性断言不足——依赖链前置检查仅"人工核查"，无时机/决策归属/PARITY.md 列定义。
- 方案：§7.6.2 新增项 3（Step 6 前完成、verifier 只标记不改状态、KNOWN_DIFFERENCES 标 L3_blocked、决策交回 translator/用户），PARITY.md 加"依赖图状态"列。设计已述但实现细节不全，故 adjusted。

### R1-D2-02 · high · 03-execution-model.md · confirmed
- 问题：覆盖率 < 源码时重试策略缺失——标记权属/标记后阻塞与否/Sprint 积压管理均未定义，verifier 自评与"不自评"原则冲突。
- 方案：硬化三级级联（源-10% 区间不可跳 L2、覆盖<源-10% 强制 manual），verifier 标记+人类确认 done，Retrospective 加积压 >20% 观测，PARITY 加 Manual Review 列。

### R1-D2-03 · medium（低端）· 03-execution-model.md · adjusted
- 问题：proptest FFI 回调性能阈值（T≈10ms）为经验值无实测，抽样降覆盖率、无工具支持、MVP 预算未对标。
- 方案：FFI 延迟 p50/p95/p99 融入现有 M0 Spike（不新增 Spike），自动下调 proptest_cases（256→64→32）替代三值策略，加 ffi_sampling_threshold_ms。降为 medium 低端因范围限库类迁移且有 manual_review 兜底。

### R1-D2-04 · high · 03-execution-model.md · confirmed
- 问题：8 个探测维度未与 proptest case generation 关联——无维度→Strategy 映射、无完整性保证机制、维度选择全凭 LLM。
- 方案：新增确定性"维度→proptest Strategy 映射表"，每适用维度 ≥1 case 并记录 dimension-coverage.yaml，verifier 自校验阻塞 done，加 min_applicable_dimensions 配置。

### R1-D2-05 · medium · 04-toolchain.md · confirmed
- 问题：Tier 1 强制工具的"无纯函数"判定权属 analyzer，判定失误无追溯/复核机制——reason 字段无枚举、无验证者、无升级路径。
- 方案：保守偏置原则 + confidence 枚举（low/medium/high），verifier 自动复核（low/medium 报告 ⚠️ 标注、有权推翻并触发 proptest 补测），schema 加 confidence 字段。

### R1-D2-06 · high · README.md · confirmed
- 问题：「独立脚本门禁让 AI 无法跳过」承诺在 MVP 未充分实现——F1/F3 非独立脚本，仅 F2(verify.sh) 是，但 F2 完整性检查（通过率阈值/L2 proptest/是否阻止 Phase B）无规范。
- 方案：README 纠正措辞（F2 才是不可跳过门禁、不过不进 Phase B），verify.sh Schema 与 Step 5 门禁化交 06/09 权威文件落地。

---

## 维度 D3：工具架构与工程质量

### R2-D3-01 · medium（high→medium）· 06-plugin-structure.md · adjusted
- 问题：F1 rust-analyzer LSP 诊断假设未验证——大项目索引时效、增量诊断延迟、Plugin Context 隔离冲突无验收标准。
- 方案：DESIGN_ASSUMPTIONS.md 补 F1 验收标准（5K/15K/100K LOC 量化指标）、隔离要求、失败回退为 PostToolUse Hook。降级因 F2 兜底+Spike 2 已规划，真实问题是验收标准定义不清。

### R2-D3-02 · medium（high→medium）· 06-plugin-structure.md · adjusted
- 问题：CLI 与 analyzer 的 source-graph.db 事务隔离缺失——"完成"语义未定、flock 不约束 CLI 调用、WAL COMMIT≠fsync。
- 方案：复用既有 graph_build_completed 字段（非新建 graph_persisted）明确"COMMIT 后同步写=完成"，检查点判定责任归 Skill。降级因 MVP flock+串行已保证，缺口主要影响容错与 M2。

### R2-D3-03 · high · 06-plugin-structure.md · confirmed
- 问题：L2 Schema 校验粗糙——Markdown 产出物无 Schema、JSON 仅校验格式不校验语义（status=done 但 test_pass_rate=null、blocked_by 引用不存在模块）、L1/L2 分界混写。
- 方案：接口表区分 Markdown=L1 存在性、JSON=L2 结构校验（含 blocked_by∈modules 引用一致性），L3 现阶段人工 sampling。

### R2-D3-04 · medium · 06-plugin-structure.md · adjusted
- 问题：file-guard.sh 不防 Bash 命令注入对 source-graph.db——无法从命令字符串提取路径、flock 不防内容修改、stdin JSON 字段未验证。
- 方案：补威胁模型澄清（flock 是进程级互斥非内容锁），Bash 工具防护依赖 M0 Spike 0 验证 Hook stdin，否则回退 Skill 层白名单。问题是 Hook API 契约未验证而非文档缺陷。

### R2-D3-05 · medium · 09-appendix-schemas.md · confirmed
- 问题：「等待 CLI 完全释放」语义不清——graph_build_completed 字段不在 Schema、含义含糊、检查点实现/超时处理未定义。
- 方案：Schema 加 metadata 块（含字段语义=事务 COMMIT+连接关闭），SKILL Step 2.5 单次 Bash+jq 判定（无 busy-loop/超时），M2 再升级为 validate state 子命令。

### R2-D3-06 · high（medium→high）· 02-architecture.md · adjusted
- 问题：verifier 双角色（Step 3 审 Phase A、Step 5 测 Phase B）交叉依赖，Phase A 审查失败重做后 Phase B 输入依据、测试失败根因归属均无状态追踪。
- 方案：09 加 phase_a_version + phase_a_audit_passed 两字段解耦，02 加双角色责任边界注。上调因影响工程可靠性与可维护性。

---

## 维度 D4：技术选型审查

### R1-D4-01 · medium · 04-toolchain.md · confirmed
- 问题：tree-sitter 兼容率验证缺失败降级清单——仅"> 1% 核查 GH issues"，无切换时机/成本/混合方案/默认配置。
- 方案：单阈值扩为三档流程（≤0.3% 继续 / 0.3–1% 继续+列失败类别 / >1% 产回退报告由 M0 门禁决定），加 ast_engine_fallback_threshold 配置，混合策略推 M2。

### R1-D4-02 · high · 03-execution-model.md · confirmed
- 问题：proptest + FFI 在 MVP 可行性未验证——TS"纯函数"定义模糊（Promise eager vs Future），analyzer 无自动检测纯函数算法。
- 方案：新增 TS 纯函数分层检测（tree-sitter 静态规则 → high；translator 标注+verifier sign-off → medium；保守 low），source-graph 加 purity_confidence 输出，M0 Spike 1 ≥80% 一致率验收。

### R1-D4-03 · medium · 03-execution-model.md · rejected
- 问题（声称）：FFI 降级模块用 insta 快照，源项目演进致快照过期，无更新/验证/多版本参考机制。
- 驳回理由（verifier）：误引文档位置——§7.5 是质量评分卡非快照方法论；insta 实际在 04 §5.3（有 CLI/API 输出时启用，非 FFI 专用）；FFI 降级用 proptest 非 insta。且源引用版本化已由 source_commit/file_fingerprints/STRUCTURAL 检测处理，问题基于误读。

### R1-D4-04 · medium · 04-toolchain.md · adjusted
- 问题：petgraph 并发写策略与 M2 规模假设不一致——内存结构无 WAL，多 agent 读写竞态，副本合并/冲突检测算法未给。
- 方案：明确各 agent 从 SQLite 加载独立 petgraph 副本（写先入 SQLite 经 WAL 串行化、不合并），M2 加 1-2 人天验证。降级因混淆"petgraph 并发写"与"SQLite 并发写"，实为澄清缺口非功能缺口。

### R1-D4-05 · medium · 04-toolchain.md · confirmed
- 问题：cargo-llvm-cov 与 FFI 测试兼容缺口——FFI 模块只统计 Rust 侧、不计被调源实现，覆盖率虚高，审查者误判等价。
- 方案：标注 coverage_rust_only vs coverage_full，PARITY.md 加列，不可仅凭覆盖率判 done（降 manual_review_only），加 ffi_coverage_mode 配置；修正方案误用的 L7（改为手工审阅）。

### R1-D4-06 · low（→very-low）· 04-toolchain.md · adjusted
- 问题：clippy.toml 升级判据缺量化——MVP 目标规则数、"> 30% 需语义判断"量化方法、M2 lint crate 复杂度上限不明。
- 方案：verifier 复核处补 translator 规则标注(Y/N)+统计需语义判断占比 >30% 触发 M2 升级，08 M2 加"自定义 lint crate（如需 3-5 人天）"。降级因阈值多已文档化，真实缺口仅 M2 成本估算缺失。

---

## 维度 D5：编排可靠性与确定性

### R2-D5-01 · high · 06-plugin-structure.md · confirmed
- 问题：SKILL.md 步骤级部分失败缺确定性回滚策略——"完整回滚"未定义删哪些文件、保留哪些诊断产物、何时清理。
- 方案：09 附录 B 加"关键检查点失败恢复规则"表（3 个 SubAgent 调用点），§10.2.2 完整回滚改为按表清理+复位 pre-run，intermediate/attempts/*.json 始终保留作诊断。

### R2-D5-02 · high · 06-plugin-structure.md · confirmed
- 问题：多终端并发 /migrate run 冲突检测与强制隔离缺失——隔离如何强制执行、flock -n 是否用、CAS 版本检查、flock 失败处理均未定义。
- 方案：SKILL.md Step 0 用 flock -n .migration-lock 非阻塞获取全局锁、失败即报错退出不退避，末步释放，陈旧锁由 lock_timeout_secs 控制。

### R2-D5-03 · high · 06-plugin-structure.md · confirmed
- 问题：migration-state.json 原子性写入缺 crash-safe 机制——多次写入（Step 1/2.5/3/7）任一崩溃致半完成状态，JSON 无法解析恢复。
- 方案：新增 §10.8 crash-safe 持久化（.tmp→fsync→原子 rename、写前 .backup、读取失败自动恢复+三路径手工指引），加 [persistence] 配置段。

### R2-D5-04 · medium · 06-plugin-structure.md · confirmed
- 问题：L2 校验层自身超时/异常处理缺失——盲目重试无法区分"产出物无效"vs"校验工具超时/OOM/损坏"。
- 方案：错误码表加 VALIDATION_TIMEOUT/OOM/SCHEMA_CORRUPTED，三者记 validation_tool_error_<type> 不进重试循环、提示修环境，加 timeout_secs=30 配置。

### R2-D5-05 · medium · 09-appendix-schemas.md · confirmed
- 问题：blocked 模块自动恢复缺依赖循环检测——A→B→C→A 或自依赖致所有模块永久 blocked，无提示。
- 方案：Step 0.5 伪码顶部加循环依赖检测（DFS 着色法检测环含自依赖），报错中止并输出环路径+提示 --degrade=skip 打破，原因写 metadata.last_error。

---

## 维度 D6：规模化与性能

### R1-D6-07 · medium（high→medium）· 02-architecture.md · adjusted
- 问题：上下文预算 100K 的 interface_only 压缩率无定量验证，深依赖链（≥5）可行性无文档根据，无专项验证计划。
- 方案：08 Spike 3 扩展（3-5 个 5K-20K 行项目统计深链占比/压缩率/超预算占比，压缩率 <25% 或超预算 >20% 记预案），02 §3.5.1 加脚注。降级因已有降级路径不会崩溃，但修复须提前到 M0 而非推 M2。

### R1-D6-08 · high · 04-toolchain.md · confirmed
- 问题：35 文件/批的社区检测批大小无实测基准——高内聚目标、LLM 分析质量与批大小关系、退化算法 10-20% 重复均无依据，未纳入 Spike 3 验收。
- 方案：Spike 3 加批大小验收（3-5 项目 × 20/35/50），改用批内符号引用覆盖度/跨批重复次数/依赖链准确度指标，batch_size 配置化+运行时重复率 >30% 告警，标 M1 阻塞性交付。

### R1-D6-09 · medium（high→medium）· 03-execution-model.md · adjusted
- 问题：M1 串行吞吐为理论值无实现计划，M2 并发 1.5 模块/小时缺依据——并行度达成率、延迟分位、模块大小范围未说明。
- 方案：§4.10 补 M2 吞吐假设条件（1-2K 行/P50≤5 分/写竞争 <5%），08 §M2 补 P50/P95/P99 记录口径+SQLite 冲突率与吞吐下降关联。降级因 M1"1 天"已标注 placeholder、M2 模块定义可推导，真实缺口仅度量方式+冲突量化。

### R1-D6-10 · medium · 04-toolchain.md · confirmed
- 问题：传递性更新深度=3、规模熔断=50 为经验值无实测——import 链实际深度分布、熔断丢失间接导入者影响面、大项目增量成本均无量化。
- 方案：§5.7.5 末加"实测校准"预留表格（深度>3 占比/反向 BFS 耗时@100-300-500/熔断命中率），Spike 3 回填校准表并写 DESIGN_ASSUMPTIONS.md。

### R1-D6-11 · medium（high→medium）· 08-roadmap-and-reference.md · adjusted
- 问题：MVP<5K 单模块性能门禁未考虑实际分布，边界情况（20 层嵌套循环、100+ 依赖接口）无覆盖，仅用平均场景验证。
- 方案：M1 项目选取须含 1 个依赖数 15-25 模块+边界降级确认检查项（故意超预算须触发拆分/降级非静默失败），Spike 3 记 TS 语法错误率 ≤1%，M2 前 2 周补预算实证。降级因文档已显式延迟边界测试到 M2（非隐藏缺口）。

---

## 维度 D7：可维护性/可扩展性/社区贡献

### R1-D7-01 · high · 05-documentation-system.md · confirmed
- 问题：社区贡献流程信息分散，全推延到未编写的 Plugin 仓库 CONTRIBUTING.md，设计文档无法独立指导贡献。
- 方案：§6.2 后增"社区贡献快速参考"（规则/适配器/patterns 三类本地自检要点），统一结尾指向 CONTRIBUTING.md。

### R1-D7-02 · high · 05-documentation-system.md · confirmed
- 问题：26 类规则维护成本与演化路径不明——9 条"否"规则升级判据、跨适配器复用、规则 deprecation（§6.12 仅对 patterns 不延伸到核心规则）均缺。
- 方案：新增 §6.2.1（9 条规则升级判据矩阵+RULE-3/8/22 跨语言特化示例+deprecation 审批链），与 06 §10.0.2 兼容窗口对齐。

### R1-D7-03 · high · 06-plugin-structure.md · confirmed
- 问题：适配器扩展真实成本未验证——3-5 天基于未实现的 TS 适配器，Python/C++ 复杂度变化未量化，超期无兜底预案。
- 方案：工作量表加"置信度""多语言风险因子"两列，footnote 加成本重校触发器（超估 >20% 紧急复审）+PR SLA，§13.3 标 M1 强制 gate。

### R1-D7-04 · medium · 05-documentation-system.md · confirmed
- 问题：L0-L3 四层知识存储可维护性未评估——L1/L2 关系模糊、index.json 手工 vs 自动不明、成本/收益无对标。
- 方案：新增 §6.11.1（L0/L2 MVP 强制、L1 可选、L3 不强制全覆盖），index.json 标 M2 自动候选，加 MVP vs M2+ 成本/ROI 对标表。

### R1-D7-05 · medium · 06-plugin-structure.md · confirmed
- 问题：Plugin 版本升级时进行中项目的应对机制不完整——minor/patch 升级对中途 Sprint 项目的兼容界定不清。
- 方案：新增 §10.0.3（major/minor/patch 升级决策矩阵、.rustmigrate.toml 向后兼容承诺、升级检查清单 5 步、失败降级），精简到表格+要点。

### R1-D7-06 · medium · 06-plugin-structure.md · confirmed
- 问题：适配器 Schema 契约与脚本接口一致性无机制——extract-deps.sh 等输出格式只在自然语言描述、非机读 Schema。
- 方案：新增 §11.2.1 适配器脚本输入/输出 Schema 表，复用 09 附录 D（type-map/source-graph 导出）与 05 §6.1 frontmatter，CI 校验作社区 PR 门禁。

---

## 维度 D8：范围控制/过度设计/路线图

### R2-D8-01 · medium · 04-toolchain.md · adjusted
- 问题：SQLite+FTS5+petgraph 三层存储对 <5K 行 MVP 过重——MVP 图仅 20-100 节点，未用 StableGraph 动态特性，crate 集成时间高估。
- 方案：明示三层为"前向兼容权衡非 MVP 必需"，引入 crate 集成风险门控（M0 触上限则切 JSON 回退），Spike 0 加风险评估。verifier 纠正：发现引用 CodeGraph 规模 ">100K 行"不实（文档为"未知"），FTS5 已延迟 M2、petgraph 回退已预案。

### R2-D8-02 · medium · 01-positioning-and-methodology.md · rejected
- 问题（声称）：26 类规则+4 SubAgent 分发体系对 MVP 过于细粒度，三层逻辑分散难维护，工作量估算偏差。
- 驳回理由（verifier）：混淆"多文件分布"与"维护混乱"——三层有明确设计意图，规则部分仅估 2-3 人天表明分散未致工作量偏差，"可复用性无法量化"被 §13.1.1 的 15-25 条规则指标反驳，MVP 范围限"标记是的规则"（约 15 类非全 26 类）。提案改单文件反削弱 SubAgent 系统提示完整性。

### R2-D8-03 · medium · 08-roadmap-and-reference.md · adjusted
- 问题：M1 工作量 50-70 人天对单人/小团队不现实——缺关键路径分析、集成测试缓冲过小、Plan B 缓冲 2-5 人天与脚注 +5-10 内部矛盾。
- 方案：加关键路径/依赖排序说明（规则即时生成非预训练、非循环），集成测试拆为单模块+三项目（含 1-2 轮迭代），Plan B 行改写消除矛盾，README 加 M0 前置提醒。verifier 纠正：循环依赖指控无效（规则按项目生成）、Spike 4 范围指控被拒（Plan B3 已估 2-4 人天）。

### R2-D8-04 · low（medium→low）· README.md · adjusted
- 问题：MVP 与 M1 概念混淆，"3 命令"实际是 7 子步骤+4 SubAgent+11 CLI+断点续传，规模接近中等项目迁移。
- 方案：TL;DR 补"MVP=M1=50-70 人天（2 人 6-9 周）非开箱即用"，最小概念集补"验证管线模板可即装 vs M1 完整工作流需实施"。降级因文档已显式处理（line 13 callout+terminology 表统一），实为可读性/findability 问题非设计缺口。

### R2-D8-05 · medium · 03-execution-model.md · adjusted
- 问题：行为录制框架（Tier 1 默认启用）与 <5K 纯函数模块不匹配——纯函数应用 proptest 不需录制，无"有有状态→强制录制"对称规则，"始终启用"与 MVP 不含录制内部矛盾。
- 方案：04 §5.3 加对称规则（有有状态+跨语言 FFI→M2 行为录制，MVP 跳过记 tier1_exceptions），"黄金文件测试"改"insta 快照测试"，PARITY 加验证工具组合列。verifier 纠正：发现误读——黄金文件已重定义为 insta L1 快照、非缺失的录制框架。

### R2-D8-06 · medium · 08-roadmap-and-reference.md · adjusted
- 问题：M1→M2→M3 演化缺通用性论证——15-25 条规则来源（通用陷阱 vs 项目专有）、extract-types 跨语言"通用框架"形式、覆盖率量化标准均不清。
- 方案：§13.1.1 重构（规则 frontmatter 元数据 category/target_languages/ts_only、M1 验收检查表覆盖率 ≥60%/复用率 ≥40%、M3 复用统一契约非脚本代码），05 §6.2 贡献检查项补强制字段。严重度被低估但保持 medium。

---

## 维度 BS1：实操盲点（真实迁移开源项目会在哪崩）

### R2-BS1-01 · medium（high→medium）· 03-execution-model.md · adjusted
- 问题：黄金文件验证无递归一致性保障——链式模块 A→B→C 微妙偏差（浮点精度/集合排序）被 B 消化但产出不等价，"三选一"回落跳过链式验证。
- 方案：Step 2.1 链式录制改为检测到链路依赖时强制（否则降 manual_review），录制三层中间产物逐层比对，加 chain_tolerance 配置。降级因设计已承认限制并路由（非完全缺失），实为实现欠规范。

### R2-BS1-02 · medium · 02-architecture.md · confirmed
- 问题：CLI 交互式降级决策缺自动化恢复路径——同类错误（如库 API 不兼容）每个模块都需人工重复确认，大项目进度停滞。
- 方案：§3.4 加"降级决策学习（M2+ nice-to-have）"注（migration-state.json 增 degrade_decision_history+基于失败分类自动推荐），标非关键路径。MVP 人工确认为有意保守设计。

### R2-BS1-03 · medium（high→medium）· 03-execution-model.md · adjusted
- 问题：性能动机迁移缺基线锚定与退化检测——基线采集时机（Phase A 编译失败如何处理）、容忍度语义（吞吐 vs 延迟/p99 vs 平均）、退化止损机制均不明。
- 方案：§7.1 补 Phase A 编译失败用 source-baseline-reference，容忍度明确为相对源吞吐均值 criterion relative_difference+报 p99，未登记退化自动 block，KD 加性能模板。降级因属质量问题可经 MDR 绕过非 blocker。

### R2-BS1-04 · medium · 05-documentation-system.md · confirmed
- 问题：实验性规则生成缺版本化回溯与冲突仲裁——v1→v2 升级后无法批量识别旧模块、通用 vs 项目专有规则冲突无代码层约束、核心规则与参考指南版本关系未定义。
- 方案：模块级 _porting_manifest.json（机读规则版本）+函数级 PORT NOTE [RULE-NN:vX.Y.Z]+verifier 冲突检测+rule_version_stale substatus+M2 候选 validate rules 子命令，06 加 [rules] 配置段。

### R2-BS1-05 · medium · 02-architecture.md · confirmed
- 问题：类型复杂度评估缺定量标准——type_complexity 仅作 3 轮失败后事后分类标签、非 PROFILE 阶段前置预判，用户先投入后才被告知应 FFI。
- 方案：§3.5.1 加"类型复杂度作为前置降级信号（M2+）"注（analyzer PROFILE 预判导出类型数/泛型约束/条件类型频率写 PROFILE.md），采用精简注解而非新建评分文档（守 YAGNI）。

---

## 维度 BS2：OSS 工程基线（CI/release/dogfooding/eval/性能基准/错误信息/可复现）

### R1-BS2-01 · medium（high→medium）· 06-plugin-structure.md · adjusted
- 问题：error_code 表仅 4 个码，远不足覆盖常见失败模式，09 附录无完整枚举，CI 集成无法可靠机器分类。
- 方案：08 §M2 加"错误码体系扩展"（30-40 条按编译/翻译/验证/状态机/编排五类），06 §10.7 补 MVP 范围说明（4 码仅覆盖编排层通用故障）。降级因诊断指南库已明确为 M2 后置项、MVP 范围限"错误可被解析"。

### R1-BS2-02 · high · 03-execution-model.md · confirmed
- 问题：评估与基准体系不完整——评估工具链质量、跨 Sprint 趋势检测、基准建立流程、M1 验收质量通过线均缺。
- 方案：08 §M1 加质量通过线（done≥80/degrade_ffi≥60，M1 per-module、趋势推 M2），09 加附录 F sprint-N-report.json schema，06 加 [quality] 配置。

### R1-BS2-03 · high · 03-execution-model.md · confirmed
- 问题：CI/CD 集成缺故障恢复——[reproducibility] 段被引用但 06 配置 schema 不存在（跨文件引用断裂），代码哈希一致性无测试方法，CI reproduce 失败无诊断。
- 方案：06 加 [reproducibility] 配置块补齐悬空引用，§4.11.3 改单机连续两次哈希一致+verify-reproducibility.sh（环境快照/SHA256/<5 字节容忍），08 M1 加 1-2 人天。

### R1-BS2-04 · medium · 03-execution-model.md · confirmed
- 问题：Dogfooding 为纯概念无交付物——微型 fixture 是什么、如何集成 CI、验证标准、预期发现什么均未定义，M1 工具"自己没试过自己"。
- 方案：§4.11.4 具体化（fixture=50-100 行 TS+手写期望 Rust、仅验证到 Tier 0、dogfooding.yml 初期信息性/M1.5 升 required），08 §M1 CLI 测试行 itemize。

### R1-BS2-05 · medium · 05-documentation-system.md · confirmed
- 问题：Release 流程与产出物版本化不完整——plugin.json 与 CLI 版本同步、CHANGELOG 格式、artifact 打包规范、跨版本兼容检查清单均缺。
- 方案：06 §10.0.2 加"Release 流程"子节（版本一致+CI 校验、Keep a Changelog、Release artifact 标准表、发版前检查清单置 .github/RELEASE.md），05 §6.4.1 加引用。

---

## 维度 BS3：内部矛盾/跨文件一致性/逻辑漏洞

### R1-BS3-01 · medium · 04-toolchain.md · confirmed
- 问题：MVP 图规模节点数三处不一致（13 总数 / MVP 10 / 实际 9），README 未标注总数 vs MVP，违反单一权威来源。
- 方案：§5.7.1 表头改"12 种：MVP 9+M2 3"加口径注，§5.7.2"13 节点"改"12 节点"，README 同步。核实表内实际为 MVP 9+M2 3=12，原 13 为误算。

### R1-BS3-02 · high · 03-execution-model.md · confirmed
- 问题：Phase A/B 双阶段与状态机状态名映射不完整——映射表仅 4 状态、缺"Phase A 完成待审查"/"Phase B 优化中"中间状态、Phase B 崩溃无断点续传。
- 方案：09 substatus 表加 Phase 级示例（phase_a_complete_awaiting_review/phase_b_failed_at_round_N），03 Step 5 加 Phase B 失败持久化+--retry 续传，06 §10.6 加检查点文件。

### R1-BS3-03 · medium · 04-toolchain.md · confirmed
- 问题：MVP vs M2 图并发写责任边界不清——§5.7.3 标题 M2+ 却含 MVP 说明，file-guard.sh flock 在 MVP 串行下职责模糊，M0 Spike 仅验证串行路径。
- 方案：§5.7.3 开头补"MVP 不需本节策略"，06 §10.3 注释补 flock 为防御性编程真实目的，08 M1 补"运行时串行边界（显式锁定）"。

### R1-BS3-04 · medium · README.md · adjusted
- 问题：「3 个核心命令」与实际 Skill 结构——P3 项目自适应无对应 Skill 映射，"只需 /migrate analyze 开始"与实际需手动 3 命令有出入。
- 方案：line 11 改为"手动依次调用 analyze→run→review，每命令内部自动子步骤"，P3 映射归 01/06 权威不在 README 重复。verifier 澄清：命令数与结构无矛盾（06 §10.1 一致），真实缺口是 P3 实现路径文档关联不足，保持 medium。

### R1-BS3-05 · medium · 02-architecture.md · confirmed
- 问题：降级人类确认与自动降级边界不清——02 说"生成降级分析报告"、06 说"提示三选项"措辞不同，谁生成选项/何时展示/如何确认无明确责任划分。
- 方案：§3.4 就地补强（诊断由 translator 在第 3 轮失败后输出→Skill 读 migration-state.json 渲染表格→用户经 --degrade 参数或交互三选一，参数优先），补 06 §10.2.2 链接。

---

## 本轮修复文件清单

| 文件 | 修复 finding | 数量 |
|---|---|---|
| docs/design/03-execution-model.md | R1-D1-01/02/03/04/05/06, R1-D2-01/02/03/04, R1-D4-02, R1-D6-09, R2-D8-05, R2-BS1-01/03, R1-BS2-02/03/04, R1-BS3-02 | 18 |
| docs/design/04-toolchain.md | R1-D2-05, R1-D4-01/04/05/06, R1-D6-08/10, R2-D8-01, R1-BS3-01/03 | 11 |
| docs/design/README.md | R1-D2-06, R1-BS3-04, R2-D8-04 | 3 |
| docs/design/06-plugin-structure.md | R2-D3-01/02/03/04, R2-D5-01/02/03/04, R1-D7-03/05/06, R1-BS2-01 | 12 |
| docs/design/09-appendix-schemas.md | R2-D3-05, R2-D5-05 | 2 |
| docs/design/02-architecture.md | R2-D3-06, R1-D6-07, R2-BS1-02/05, R1-BS3-05 | 5 |
| docs/design/08-roadmap-and-reference.md | R1-D6-11, R2-D8-03/06 | 3 |
| docs/design/05-documentation-system.md | R1-D7-01/02/04, R2-BS1-04, R1-BS2-05 | 5 |

被 rejected 的 2 条（R1-D4-03、R2-D8-02）未产生修复。多数 finding 跨文件协同修复（权威文件主改 + 引用文件同步），上表按各 fix-agent 实际落账文件归类，故计数有重叠。所有修复均为就地 Edit、未 commit（交编排器统一落账）；版本号保持 v0.9.4 不变。
