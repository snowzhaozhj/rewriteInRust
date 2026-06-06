# 审查轮 1 报告

本轮共 50 条 finding：confirmed 39 / adjusted 9 / rejected 2。按维度分组，每条列 ID / 严重度 / 位置 / 状态 / 问题 / 优化方案；rejected 附 verifier 驳回理由。末尾为本轮修复文件清单。

---

## D1 迁移质量与翻译方法论

- **R1-D1-01** | high | 01-positioning-and-methodology.md | confirmed
  问题：方法论要求「意图摘要人类确认后才翻译」，但 09 可执行流程只做文件存在性检查，Step1→Step2 自动衔接，缺人类确认门禁。
  方案：09 骨架加 Step 1.5 意图确认门禁（展示摘要+暂停），03 加对应 Step 2.5，沿用既有人类决策点模式，配置键 auto_confirm_intent 写入 06 §11.1。

- **R1-D1-02** | high | 03-execution-model.md | confirmed
  问题：verifier 双职责（对抗审查在 Phase B 前、测试生成在 Phase B 后），测试基于意图摘要生成但需通过 Phase B 重写后的代码，存在时序歧义。
  方案：09:299 测试输入改为「基于 Phase B 最终代码」；Phase B 涉错误/返回值/并发可见性改写时先校验与意图摘要 parity，不符则先更新 {module}-intent.md；verifier 系统提示加「测试目标 Phase B」。

- **R1-D1-03** | medium | 03-execution-model.md | adjusted
  问题：Phase B 允许并发/内存重写（MDR 在 Step7 记录，晚于 Step6 测试），测试可能与实际实现不匹配。
  方案（调整为文档澄清而非新增验证步）：明确意图摘要依据源语义非 Phase A 代码、Phase B 重写边界（不改签名/错误语义/副作用）、MDR 涉协议改变时触发语义影响复核。
  驳回/调整理由：核心关切真实，但误读了现有防线——意图摘要是源语义抽象非 Phase A 直译，对抗审查已验证语义一致，MDR 限制变更范围，F2 反馈会捕获行为变更；建议的 Post-Phase-B 验证与对抗审查重合，会致流程膨胀。真正缺口是文档澄清不足。

- **R1-D1-04** | high→修复 | 01-positioning-and-methodology.md | confirmed
  问题：Phase A 承诺 1:1 对应便于 diff 审查，但无机制保证（无结构约束、可静默优化删死代码/合并辅助函数），diff 失去意义。
  方案：09 Step2/03 Step3 明确 Phase A 不优化+非平凡函数加 PORT NOTE 源码行号锚点；新增结构校验门禁（09 Step3.5/03 Step4.5）作为对抗审查后 Phase B 前的 gate，阈值对齐 §7.5 记分卡。

- **R1-D1-05** | low | 02-architecture.md | adjusted
  问题：100K token 预算须容纳意图生成+Phase A+对抗审查，大模块易耗尽，超预算拆分标准未定义。
  方案：§3.5.1 新增预算公式+拆分决策树+超预算降级；意图摘要按行数动态控制而非固定 500 token 硬约束；Preview-before-spend 强化为 >95K 提示确认。
  驳回/调整理由：问题部分成立但严重度从 medium 高估——预算约束与拆分原则已存在，已有摘要压缩/接口只注签名/Preview 预估等防护；实质是「拆分标准文档不完善」而非设计缺陷，无实证表明预算频繁突破，降为 low。

- **R1-D1-06** | high | 03-execution-model.md | confirmed
  问题：L0-L7 测试分层有 Tier 分配，但缺「不同模块类型必做哪些层」的矩阵，verifier 测试深度自由裁量致质量不一致。
  方案：§7.1.1 新增「模块类型×测试层」矩阵（纯函数/有状态/异步并发/FFI/性能敏感），M1 最低=全模块 L0+L1+L3简化、纯函数加 L2、并发加 L7，层选择由检测类型决定。

---

## D2 验证体系可靠性

- **R1-D2-01** | high | 03-execution-model.md | confirmed
  问题：§7.6「库经 FFI 比对」表述过简，未界定纯函数 vs 有状态边界，未估 FFI 性能成本（256 用例可能耗时翻倍）。
  方案：§7.6.1 FFI 可行性清单（4 条纯函数识别标准、有状态三方案权衡、>T ms 改抽样、单函数 <30s），proptest 行加「仅纯函数」标注，纳入 verifier 系统提示。

- **R1-D2-02** | high | 03-execution-model.md | confirmed
  问题：M1 的 L3 简化（shell 脚本对比）隐含单模块假设，未处理「A 输出是 B 输入」依赖链路的一致性。
  方案：§7.2 新增 Step 2.1 模块依赖记录（图查询→module-deps.json→链式录制）；§7.1 L3 脚注加依赖链路限制（仅叶子模块，否则整体迁移/降级FFI/延后M2）。

- **R1-D2-03** | medium | 04-toolchain.md | confirmed
  问题：cargo-nextest 并发执行时共享临时文件/SQLite 可能竞态致假阴，未规范隔离级别。
  方案：§5.2 加测试隔离段，[testing] nextest_threads 控制（auto|1|N），跨模块/FFI 测试串行，verify.sh 以 --test-threads 传递；字段同步 06 §11.1。

- **R1-D2-04** | high | 06-plugin-structure.md | confirmed
  问题：M0 Spike1「成功」定义粗糙（仅 7 步完成+文件存在），未含语义有效性；3-4 次成功的临界区间无决策规则。
  方案：§10.5 产出物有效性分级（L1 执行/L2 结构/L3 语义），Spike1 仅校验 L1/L2；07 §12.2 补失败判定（>20% 触发 Plan B、20%-50% 临界追加样本）。

- **R1-D2-05** | medium | 03-execution-model.md | confirmed
  问题：覆盖率作等价代理的边界未明（覆盖率高≠行为等价、源测试不足时同样不足、L3 通过但覆盖率低如何处理）。
  方案：§7.5 加脚注（必要不充分、低于源需 L2/L3 补充、≥源但 L2/L3 失败则阻塞）+ verifier 三条判别规则关联状态转移；门槛配置指向 06 §11.1。

- **R1-D2-06** | medium | 04-toolchain.md | confirmed
  问题：Tier1 可关闭与「验证可靠性」张力未调和——关 proptest 则纯函数等价无法证明，外部审计无法判断降级。
  方案：§5.3 加 Tier1 启用规则（有纯函数强制 proptest、cargo-llvm-cov 不可关），禁用记 tier1_exceptions(reason)，/migrate review 输出验证画像；字段同步 06 §11.1。

---

## D3 工具架构与工程质量

- **R1-D3-01** | high | 06-plugin-structure.md | confirmed
  问题：file-guard.sh 仅查路径不防 source-graph.db 并发写；即便 SubAgent 串行，Skill 主上下文仍可并行调 CLI 写入。
  方案：file-guard.sh 加 flock 排他锁+SubAgent 锁协议；§10.5 加全局并发隔离段，引用 04 §5.7.3 WAL checkpoint。

- **R1-D3-02** | medium | 09-appendix-schemas.md | adjusted
  问题：blocked 状态可达但未定义检测/恢复时机、多依赖恢复顺序，模块可能永久卡死。
  方案：09 加 blocked 检测/恢复责任边界表（MVP 手动/M2 自动）、blocked_by 数组说明；/migrate run 骨架加 Step 0.5（自动解除）+ Step 0.6（依赖就绪门禁）。
  驳回/调整理由：内核成立但严重度从 high 降 medium——MVP 已明确排除自动状态机（02 §3.4），blocked 概念混入 MVP 枚举但程序化检测推迟 M2 造成混淆；快速修复（Skill 加前置检查）M1 可做，非阻塞。

- **R1-D3-03** | medium | 06-plugin-structure.md | confirmed
  问题：§10.5 编排检查点伪码过简，无超时时限、重试退避、产出物校验深度定义。
  方案：§10.5 加超时重试策略（subagent_timeout_secs=600/max_retries=2/backoff=[5,15]）+检查点失败处理段，migration-state.json 加 subagent_calls 数组；同步 §11.1 [orchestration]。

- **R1-D3-04** | low | 06-plugin-structure.md | adjusted
  问题：fmt.sh 文件过滤对多工作区不可靠、stdin JSON 格式未定义、多文件并发写互相干扰。
  方案（缩小）：仅补 Hook stdin JSON 格式定义（tool_name/tool_input/cwd），标注需 M0 Spike0 验证；删除越范围的 flock/CARGO_MANIFEST_DIR 建议。
  驳回/调整理由：混合一真缺口+多误读，严重度从 medium 降 low——stdin 格式确未定义（真，仅影响 Spike0）；多工作区/多文件并发是对 MVP 串行架构（M2 才并行）的误读。

- **R1-D3-05** | medium | 06-plugin-structure.md | adjusted
  问题：CLI 与 analyzer 都读写 source-graph.db，未定义打开模式/锁策略/事务包装/合并原子性。
  方案：§10.5 M2 并发协议表定义 BEGIN IMMEDIATE+analyzer query_only 只读+等待 graph_build_completed；§11.1 加可选 [storage] 段。
  驳回/调整理由：缺口真实但 high 高估——MVP 强制串行消除并发写风险（10.5），属「为 M2 并行预留+防御指令跟随失败」而非 MVP 阻塞，应框定为 M1→M2 就绪而非「修复阻塞漏洞」，维持 medium。

- **R1-D3-06** | medium | 06-plugin-structure.md | adjusted
  问题：SKILL.md 25K token/500 行预算可能不足以容纳完整编排约束（超时/恢复/blocked/锁协议）。
  方案：§10.1 加行数预算与外部引用模式（500 行软约束、抽 checkpoint-patterns.md、800 行触发 Plan B3）；MVP=L1/M2+=L2/L3 检查点升级。
  驳回/调整理由：真问题是「规格完整性」非「行数不足」——500 行限制本身存疑（01 有 45KB UA 先例），错误处理细化后 ~200-260 行足够；MVP 已刻意最小化检查点，膨胀压力小于声称。

---

## D4 技术选型审查

- **R1-D4-01** | high | 04-toolchain.md | confirmed
  问题：petgraph bus factor=1、279 open issues，是图核心依赖，但 fallback「自建 adjacency list」无成本估算、无切换触发标准、未列入 M0 Spike。
  方案：§5.7.2 加风险+回退段（Vec<Vec<NodeIndex>>+HashMap 覆盖 ~95%、3-5 人天、petgraph_fallback_threshold 阈值、M0 后写 DESIGN_ASSUMPTIONS.md）；阈值同步 06 §11.1。

- **R1-D4-02** | low | 04-toolchain.md | adjusted
  问题：SQLite+FTS5 选型仅凭「CodeGraph 验证」，无 MVP 规模对标，<100 节点项目 FTS5 过重。
  方案：§5.7.3 加 MVP/M2 存储策略分层注（MVP 20-100 节点、运行时内存 petgraph、FTS5/社区检测延迟 M2、列升级触发条件）；不引入 source-graph.json 主存储。
  驳回/调整理由：问题真实但严重度从 medium 降 low——SQLite 可工作不影响 MVP 可用性，rusqlite 已纳入估算，有替代方案但非必需，不影响迁移质量核心路径。

- **R1-D4-03** | medium | 04-toolchain.md | adjusted
  问题：tree-sitter vs OXC 仅凭「0.x 不稳定」排除 OXC，无精度对比，「100% 确定性」未量化，无 fallback。
  方案：§5.7.4 加注（确定性≠零解析错误），Spike3 用 50-100 文件语料量化 TS 兼容率（>1% 核查 GH issues），加 [parser] ast_engine 回退选项；OXC 对比非 MVP 必需。
  驳回/调整理由：缺口真实但被夸大——设计已提 Spike3 与 Plan B fallback；实质是 Spike3 规格不含 OXC 对比+「100% 确定性」未验证+无错误率阈值，比声称的窄。

- **R1-D4-04** | high | 04-toolchain.md | confirmed
  问题：clippy.toml 作迁移规则执行器表达力有限（仅 disallowed_methods/types/macros），无法编码语义规则；规则>20 不可维护无 fallback；与目标 Cargo.toml 耦合。
  方案：§5.2 加表达力边界与演化路径（≤10 留 clippy.toml / >15 或 >30% 语义→自定义 lint crate）、澄清「AI 无法绕过」真实含义、解耦目标 Cargo.toml；决策树同步 05 §6.6。

- **R1-D4-05** | low | 04-toolchain.md | confirmed
  问题：FTS5 全文搜索表已建（nodes_fts+BM25），但 MVP 11 命令无全文搜索命令，graph deps 是精确 BFS，FTS5 无 MVP 价值。
  方案：§5.7.3 将 nodes_fts 注释为 M2 延迟创建，注明 MVP deps 已 O(V+E)、FTS5 无 MVP 性能收益，量化节省 ~50 行同步+5-10% 存储。

- **R1-D4-06** | medium | 02-architecture.md | adjusted
  问题：M2 多 agent 并行+共享单 SQLite，写并发受限（单 writer），未定义并发更新/冲突解决/WAL 是否足够。
  方案（按发现指引写入 08 M2 段）：图并发写策略段，限定 analyzer+scaffolder 并行、WAL 写锁实测决策点（≤20ms 足够/>50ms 走只读+批量或分片），明确不进 M0 Spike。
  驳回/调整理由：缺口真实但 high 高估——MVP 串行不适用，M2 仅限 analyzer+scaffolder 并行且已有 WAL+worktree 隔离；属 M2 设计细节非 MVP 阻塞，降 medium 并框定为 M2 专属。

---

## D5 编排可靠性与确定性

- **R1-D5-01** | medium | 06-plugin-structure.md | adjusted
  问题：MVP 编排全依赖 SKILL.md 的 LLM 遵守度，Spike1 验证标准模糊（无成功/失败阈值、重试策略）。
  方案：07 §12.2 Spike1 行补验收标准（5 次成功率≥80%）+失败分级处理；08 补资源估算（1-2 人天、$20-50）。
  驳回/调整理由：问题部分成立但严重度有误——08 已含量化标准「5 次成功率≥80%」，故「验证条件模糊」不准确，实为「信息分散在两文件」；已有阈值与 Plan B，不构成 blocker，维持 medium。

- **R1-D5-02** | medium | 09-appendix-schemas.md | adjusted
  问题：blocked 恢复缺确定性保障（谁检测解除、pre_blocked_status 是否仍有效、无超时、blocked_by 单值还数组）存永久阻塞风险。
  方案：06 §10.5 /migrate run 前加 blocked 检查点；09 /migrate run 骨架加 Step 0.5（自动解除可解除 blocked）含伪码。
  驳回/调整理由：问题真实但 high 高估——文档已体现 blocked_by 为数组；M1 编排本依赖指令跟随，永久阻塞更多源于指令不清且降级需人类确认；定性为「MVP 指令清晰度缺口」非「死锁缺陷」，降 medium。

- **R1-D5-03** | high | 06-plugin-structure.md | adjusted
  问题：10.5 声称检查点确定性（脚本判定），但 09 骨架仅文件存在检查，未调用 jsonschema 校验，Markdown 产出物（PARITY/AGENTS）无 Schema，且 10.5 内部矛盾（253 行说靠 AI 输出文本判断 vs 408 行说不靠 AI）。
  方案：解决 10.5 矛盾为两级校验（L1 文件存在+L2 CLI Schema 校验，不解析 AI 输出文本）；骨架检查点调用 rustmigrate validate state；补 PARITY/AGENTS Markdown 结构检查与失败恢复策略表。
  驳回/调整理由：框定需纠正——Schema 已定义（09 §A），问题是 SKILL.md 未调用它；PARITY/AGENTS 是 Markdown 设计应做结构校验非 JSON Schema；核心矛盾（253 vs 408 行）真实未解。

- **R1-D5-04** | high | 02-architecture.md | adjusted
  问题：MVP 文本指令驱动 vs M2 程序化状态机，M1 积累的 migration-state.json 若 M2 改格式则向后不兼容，无迁移策略。
  方案：02 §3.4.1 新增 MVP→M2 演进与向后兼容（version 必填+语义化、加字段安全/重命名 breaking+迁移脚本）；08 M2 加两交付物（状态机二进制+兼容框架，+3-5 人天）；07 §12.1 加风险行。
  驳回/调整理由：核心成立，且严重度从 medium 上调 high——M1 用户会积累状态，M2 升级可能破坏现有项目，是前向兼容债务且无任何缓解策略（不同于 MVP 编码可靠性已有 Plan B）。

- **R1-D5-05** | medium | 02-architecture.md | confirmed
  问题：100K 预算多处提及但无运行时检查（token counter、自动拆分算法、溢出恢复、interface_only 规则、拆分后 topo-sort 重算）。
  方案：§3.5.1 覆盖运行时预算公式+interface_only 签名 200-500 token/接口估算+拆分后重算 topo-sort+超预算降级；06 §11.1 [context] 扩展 budget_check_mode/enable_auto_split。

- **R1-D5-06** | low | 06-plugin-structure.md | adjusted
  问题：SubAgent 文件通信未定义共享文件锁/并发写保护（M2 并行时 porting 共享文件/migration-state.json/source-graph.db 冲突）。
  方案（M2 预留）：06 §10.5 后追加 M2 并发文件通信协议章节（文件夹权属分割、乐观锁 version 字段、SQLite 单 writer、worktree 隔离）；09 预留 M2 字段注释；08 M2 加并发安全设计（2-3 人天）。
  驳回/调整理由：缺口真实但严重度从 high 降 low——MVP 串行无即时风险，M2 并行是计划内演进（06:418 已明确），属打磨项非「成熟项目必须解决」，应 M2 阶段补而非现在阻塞。

---

## D6 规模化与性能

- **R1-D6-01** | high | 04-toolchain.md | confirmed
  问题：M2 多 agent 共享单 SQLite，仅提 WAL/页缓存/mmap，未定义事务隔离、busy_timeout 错误恢复、state.json 与 db 分布式一致性、增量更新外键约束原子化。
  方案：§5.7.3 加并发写入策略段（busy_timeout=5000+指数退避、synchronous=NORMAL、单 writer 多 reader、删+插单事务、先 commit+WAL checkpoint 再 atomic rename 保 all-or-nothing）。

- **R1-D6-02** | high | 02-architecture.md | confirmed
  问题：100K 预算+interface_only 在深依赖链（5-10 层）下可行性无定量分析（深链未定义、压缩率未估、超预算无拆分算法）。
  方案：§3.5.1 覆盖 transitive_dependency_depth≥5 为深链（拓扑最长路径）、签名约为实现 1/3 压缩说明、深链降级策略，标注 M2+ 需真实中型项目验证压缩率。

- **R1-D6-03** | medium | 04-toolchain.md | confirmed
  问题：图构建无性能基准（Louvain 复杂度、tree-sitter 各语言性能、35 文件/批依据、退化方案性能后果均缺）。
  方案：§5.7.4.1 新增性能基准与扩展性（TS 100/300/500 基准表标 Spike3 TBD、Louvain O(m log n) 引 Blondel 2008、35 文件依据=token 预算约束、目录递归分组退化、graph build --profile JSON 插桩）。

- **R1-D6-04** | medium | 03-execution-model.md | confirmed
  问题：M1 串行吞吐无估算、M2 max_concurrent_agents=3 无依据、M1→M2 升级门槛不清。
  方案：§4.10 新增性能与并行转换指南（M1 串行吞吐预期表标理论值、M2 并发度公式 min(API限/IO/worktree)+激进5/平衡3/保守1、瓶颈分层表、可观测升级指标由 /migrate review 展示）。

- **R1-D6-05** | medium | 04-toolchain.md | confirmed
  问题：传递性更新（反向 BFS 深度 3）无依据、循环导入无环检测、反向 BFS 可能 O(n) 全图遍历未分析、退化分组时行为未定义。
  方案：§5.7.5 扩展边界条件（深度≤3 依据、visited 环检测、>50 文件熔断为仅直接导入者、O(V+E) 复杂度、与社区检测退化交互），附 transitive_update 伪码。

- **R1-D6-06** | medium | 08-roadmap-and-reference.md | adjusted
  问题：M1/M2 验收指标缺性能与可扩展性量化门禁（图构建耗时、上下文利用率、吞吐），评估「足够成熟」无客观依据。
  方案：M1 加性能门禁（图构建<10s/100文件、单agent流程30-40分钟、利用率<90%）；M2 加（多agent吞吐≥1.5模块/h@3agent、并发写冲突<10%、±10% 无退化），各附测试方法。
  驳回/调整理由：缺口真实但原方案严重度与指标需调整——话题有讨论但未转化为里程碑验收标准；分 M1 单agent 与 M2 多agent 两组指标，避免 MVP 阶段要求未规划的 4-agent 并行。

---

## D7 可维护性/可扩展性/社区贡献

- **R1-D7-01** | high | 05-documentation-system.md | confirmed
  问题：26 类规则体系完整，但无社区贡献工作流、规则变更评审标准、版本化/向后兼容机制、冲突仲裁。
  方案：§6.2 新增规则版本管理子节（## RULE-NN (v<N>)、major.minor.patch、breaking 随主版本+记 KNOWN_DIFFERENCES）、冲突仲裁「项目专有优先」、规则维护责任表。

- **R1-D7-02** | high | 06-plugin-structure.md | confirmed
  问题：适配器扩展工程成本缺失（无工作量拆解、新语言验收标准、代码复用率、贡献教程），社区无法评估投入产出。
  方案：§11.2 加工作量拆解表（detect/extract-types/extract-deps/template/ffi/测试+Python +0.5 人天系数）、验收标准表（type_precision 0.95/dep_coverage 0.90）；总工时引用 08；§11.1 加 [adapter_validation] 段。

- **R1-D7-03** | high | 06-plugin-structure.md | confirmed
  问题：Plugin API 变更向后兼容策略缺失，07 识别为高风险但缓解仅「薄适配层」，无 deprecation policy/版本约束/通知机制。
  方案：§10.0.2 新增版本控制与向后兼容策略（plugin.json SemVer、deprecation 期、schema_version+迁移工具、兼容窗口=当前+前2 major、M0 Spike0 自动化兼容测试）。

- **R1-D7-04** | medium | 05-documentation-system.md | confirmed
  问题：知识沉淀 L0-L3 仅说写入时机，缺更新触发、关联索引、新鲜度管理、验收标准，有「死文档」风险。
  方案：§6.12 新增知识生命周期与维护政策（Frontmatter 加 last_verified/status/related_rules、每 6 月标 needs-review、context/index.json 仅注入 active pattern、规则变更触发标记、confidence:low 不自动注入、deprecated 归档）。

- **R1-D7-05** | medium | 06-plugin-structure.md | adjusted
  问题：跨文档一致性无自动验证，CLAUDE.md 文件权威来源表不完整（缺 26 类规则等），易漂移。
  方案：CLAUDE.md 权威表加 3 条（26 类规则→05 §6.2、规则分层→06 §10.1.1、Plugin 目录→06 §10.0）；06 §10.1.1 回链 05 §6.2。grep 自动化可选。
  驳回/调整理由：缺口真实但严重度从 medium 降 low——现有手动审查清单维持一致性、无实际矛盾、跨引用已间接存在；属未来维护预防措施非当前缺陷。

- **R1-D7-06** | medium | 05-documentation-system.md | confirmed
  问题：PARITY.md/KNOWN_DIFFERENCES.md 是质量承诺但无社区可见性/异议机制/审批流程/版本绑定。
  方案：§6.4.1 新增社区透明度与异议协议（随 GitHub Release 发布快照绑定 commit hash、Issue 引用 KD 编号异议、冲突以更窄描述为准、审批引用 file-guard.sh 制度化既有 @-mention+日期）。

---

## D8 范围控制/过度设计/路线图

- **R1-D8-01** | medium | 04-toolchain.md | adjusted
  问题：图存储 13 节点+12 边+SQLite+FTS5 超 <5K 行 MVP 实际需求，文档未量化典型图规模。
  方案（采纳方案2）：§5.7.1 末加注列 MVP 实际触发的 9 类节点+8 类边，明确 Community/TypeAlias 等及 member_of/depends_on 等为 M2 预留、<5K 不产生，指向 08 §M1 对照。
  驳回/调整理由：缺口真实但「过度设计」定性偏重——设计完整性是「一 schema 支两阶段」的架构价值，实为文档清晰度问题（缺典型规模量化），降为 medium，改进是文档标注（低风险高价值）。

- **R1-D8-02** | high | 06-plugin-structure.md | confirmed
  问题：MVP 编排（analyze 7 步含 3 次 SubAgent）全依赖指令跟随，文档承认是高风险但 README 未表述，用户旅程展平为无条件线性流程。
  方案：README 加 MVP 可靠性限制段；§10.5 编排流程图加每 SubAgent 后验证节点+失败分支；骨架预留降级路径注；M0 验收明确 Spike1<80% 强制 Plan B3 不可选。

- **R1-D8-03** | medium | 06-plugin-structure.md | **rejected**
  问题（不成立）：声称 CLI 表混合列举 11 命令，其中 5 个实为 M2，实际 MVP 只需 6-7 个，致工作量估算混乱。
  方案（不适用）：原建议拆两表分离 MVP/M2、工作量降 5-10%。
  **verifier 驳回理由**：核心主张事实错误。06 §10.0.1 已是两张独立带标题的表——「MVP（M1）— 11 个命令」(83-97 行) 与「M2 扩展 — 5 个命令」(99-107 行)，发现声称「混入」的 5 个命令（graph rdeps/cycles/export、stats compare、validate config）仅出现在 M2 表中。08:70「CLI 核心（11 个 MVP 子命令）10-14 人天」准确对应 MVP 表的 11 命令（init/profile/graph 4 子/validate-state/state 2 子/stats-loc/scaffold）。发现混淆了「设计列为 MVP 的命令」与「发现认为 MVP 应含的命令」，文档结构清晰无歧义，未提供 11 命令范围不当的证据。

- **R1-D8-04** | medium | 08-roadmap-and-reference.md | confirmed
  问题：M1 估算 50-70 人天含 Plan B 缓冲仅 2-5 人天，无法覆盖多个高概率 Spike（1/2/4）同时失败的场景。
  方案：M1 工作量表后加场景 A/B 对比（基线 50-65 vs 失败缓冲 60-75，加粗标注取决于 M0 验收）；M0/M1 间插决策检查点（Spike1<80% 自动 Plan B3 +5-10 人天）；01 §2.1 末加关键风险揭示段。

- **R1-D8-05** | medium | README.md | confirmed
  问题：README 宣称「3 命令 MVP」对标 OpenSpec，但 run 内部 4 步串行+analyze 7 步，骨架未展示如何编码成指令（45KB 长指令还是拆 7 子 Skill），存在表述与复杂度鸿沟。
  方案：README TL;DR 框定「3 命令」是入口数非「只跑 3 步」并列内部步骤；06 §10.5 Plan B 加触发粒度 note（命令级 vs 步骤级）；09 骨架加 token 成本预估 note+实例编码交叉引用。

- **R1-D8-06** | low | 08-roadmap-and-reference.md | adjusted
  问题：M1-M4 目标与工作量无 tie-out（M1 规则库累积对 M2/M3 的加速效应、Python 适配器复用率、规则成熟度演化路径未体现）。
  方案：08 §13.1.1 新增 M1→M2→M3 规则库累积效应分析（规则累积+预期 15-25 条新通用规则、M1 后成熟度评估检查点、M3 Python 复用 TS type-extraction 框架）；01 §1.3 加规则成熟度说明；README TL;DR 不动（已准确）。
  驳回/调整理由：缺口真实但一处不实——README 并未说「开箱即用的工作流」，而是「验证管线开箱即用」且明确「项目专有规则本地生成」；实际缺口更窄（依赖链与复用文档化），按更精确范围修复。

---

## BS1 实操盲点（真实迁移开源项目会在哪崩）

- **R1-BS1-01** | high | 03-execution-model.md（任务文写作 04-execution-model.md，实为 03） | adjusted
  问题：提三种并行策略但缺源文件变更检测/同步机制，迁移期源码演化无法感知，无法区分「代码错误」vs「源演化」。
  方案：§4.6.1 复用既有结构指纹体系（04 §5.7.5 三级变更检测+file_fingerprints）不新建 source-tracking.json；§4.8 加变更监控基线；§4.2 Sprint Planning 加脚本化变更检测；source check-changes 列为 M2 候选不破 11+5 清单。
  驳回/调整理由：问题部分成立但需调整理解——文档已有 source_commit 锁定+source-ref/ 副本，缺口是「无自动检测机制+无变更影响路由+可复用指纹资产被浪费」；维持 high 但理由改为「补自动化集成+流程编排」非全新设计。

- **R1-BS1-02** | high | 06-plugin-structure.md | confirmed
  问题：SubAgent 文件通信缺超时管理、产出物有效性校验、失败诊断与恢复，analyzer 超时生成损坏 db 后 translator 才发现浪费 token。
  方案：§10.2 接口表加第 6 列 Schema 校验规则（analyzer 节点≥5/边≥5）；§10.2.2 新增失败恢复机制三步（记录调用→诊断重试→降级/回滚）；超时配置 §11.1 [orchestration]；migration-state.json 加 agent_attempts。

- **R1-BS1-03** | high | 03-execution-model.md（任务文写作 04，实为 03） | confirmed
  问题：L3 差异测试经 FFI 调旧实现，但 A 依赖未迁移 B 时 source-ref 找不到 B 完整实现，验证管线断掉。
  方案：§7.6.2 新增 L3 依赖状态前置检查（verifier 验依赖能否提供稳定旧实现侧，不满足则整体迁移/降级/延后 M2）；PARITY.md 加依赖图状态列；ffi_ready_dependencies 作 M2 自动化建议。

- **R1-BS1-04** | high | 02-architecture.md | confirmed
  问题：100K 预算在深依赖（20+ 依赖）下，依赖 10K+源码 40K+规则 20K=70K，仅剩 30K，超预算无应对策略。
  方案：§3.5.1 拆分决策树覆盖（≤1K 直接/1-5K 全量/>5K 或依赖≥20 拆分/循环或 >100K 降级），禁止硬填满预算、超预算输出 JSON 诊断报告并暂停，token 估算公式写入。

- **R1-BS1-05** | medium | 02-architecture.md | confirmed
  问题：3 轮失败入 PAUSE 等 --degrade=ffi|manual|skip，但用户无法判断选哪种，降级分析报告内容未定义。
  方案：§3.4 在「生成降级分析报告」基础上定义内容（失败分类+代码片段+已试策略+三种降级代价）；paused 渲染对比表；骨架代码自动生成（scaffolder FFI 桩/translator TODO/skip 更新 PARITY），无需人工手写。

- **R1-BS1-06** | medium | 05-documentation-system.md | confirmed
  问题：26 类规则说「版本化演化」但无示例格式；新规则与已生成代码矛盾时无回溯流程；PARITY.md 无法追踪模块在哪个规则版本下翻译。
  方案：§6.2 子节定规则版本格式+changelog 字段；§6.3 PARITY.md 加 Porting Version 列；回溯由 /migrate review 比对版本输出重译列表、关键规则 breaking 时 migration-state.json 记 affected_modules；未铸 check-compliance 子命令（归 06 M2 候选）。

---

## BS2 OSS 工程基线（CI/release/dogfooding/eval/性能基准/错误信息/可复现）

- **R1-BS2-01** | high | 08-roadmap-and-reference.md | confirmed
  问题：M2 仅声称「CI/CD 集成」，完全缺实现细节（调用时机/Tier 分级/错误格式/可复现/dogfooding）。
  方案：03 §4.11 新增 CI/CD 集成（M2）：GitHub Actions 模板（PR/merge/tag 分级触发 Tier 0/1/2、db 缓存、产物上传、status!=ok 则 fail）、错误标准化、可复现性、dogfooding 定位为 M2 概念设计；08 M2 交付物加交叉引用。

- **R1-BS2-02** | high | 03-execution-model.md | confirmed
  问题：质量记分卡仅一张表，缺计算公式/权重分布、动机驱动阈值调整、AI 指标评分检查表、跨 Sprint 持久化对比、止损触发定义。
  方案：§7.5 AI 指标改带 verifier 检查表+评分公式（det×0.7+ai×0.3）+结构化 JSON+sprint-N-report.json 持久化；§4.9 止损表加质量评分回归行（连续 2 Sprint 低于前 3 均值 −10% 触发评审）；§8 加动机阈值调整。

- **R1-BS2-03** | medium | 03-execution-model.md | confirmed
  问题：性能基准缺细节（criterion 基线何时建、tolerance=0.10 来源未解释、可复现保证、可视化审计、回退执行机制）。
  方案：§7.1 criterion 块补基线时机（Phase A 源基线仅参考/Phase B Rust 基线为验收）、tolerance 语义（≤10% 相对偏差）、超容忍度须 MDR 经维护者批准；CI 执行/目录结构指向 04 §5.3 与 06 §10.6。

- **R1-BS2-04** | medium | 06-plugin-structure.md | confirmed
  问题：CLI 有 JSON 输出但 AI 错误/验证失败信息质量无设计（SubAgent 失败格式、结构化差异报告、诊断指南、Hook 输出一致性、修复建议）。
  方案：§10.7 新增错误信息与可调试性（标准 error JSON status/error_code/error_context/suggested_fix、常见迁移错误码表 LIFETIME_MISMATCH 等、verify.sh 失败输出兼容）；诊断指南库标注 M2+ 后置。

- **R1-BS2-05** | high | 03-execution-model.md | confirmed
  问题：可复现性不足（LLM 非确定性无 temperature/seed、tree-sitter 版本未锁、中间产物无 schema 版本化、checkpoint 无向后兼容、fixtures 无版本锁定）。
  方案：§4.11.3 扩充（tree-sitter 锁定 [reproducibility] 段、LLM temperature/top_p 固定+seed 可用性免责、中间产物 schema_version 必填+checkpoint 迁移、M1 可复现验收标准两用户同输入产出哈希一致）；配置字段定义指向 06/09。

---

## BS3 内部矛盾/跨文件一致性/逻辑漏洞

- **R1-BS3-01** | medium | 02-architecture.md | adjusted
  问题：状态机图用 TRANSLATE 整体名 vs 执行模型 Phase A/B+对抗审查内部结构，三文件不同粒度，未明 Phase A/B 如何映射状态转换点。
  方案：02 §3.4 状态机图后加脚注（Phase A/B 是 TRANSLATE 内部子步，状态转换仅在 Phase B 完成且 cargo check 通过后发生，对抗审查归 reviewing 子状态）；交叉链接 03 §4.3 与 09 映射表。
  驳回/调整理由：缺口真实但严重度被夸大——09 映射表已澄清 TRANSLATE→translating、VERIFY→testing+reviewing；不一致是局部（未显式说 Phase A/B 都留在 translating），顺序阅读者可从映射表推断，非阻塞，属文档清晰度。

- **R1-BS3-02** | medium | 06-plugin-structure.md | adjusted
  问题：verifier 双角色（对抗审查/测试验证）边界模糊；Phase A→B 在 TRANSLATE 内时 verifier 如何访问 Phase A 产出物（无文档化中间产物）。
  方案：§10.2 接口表 verifier 对抗审查输入路径显式改为 .rust-migration/intermediate/attempts/{module}-phase-a.rs，原始源码改 source-ref/，translator 输出改 rust_root/；引用 03 §4.3；Phase A 持久化 Step 3a 归 03。
  驳回/调整理由：问题真实但 high 调 medium——MVP 单会话串行可经对话上下文隐含解决，问题主要是文档完整性与 M2 扩展性，非即时阻塞。

- **R1-BS3-03** | low | 04-toolchain.md | adjusted
  问题（重新定性）：三文件对 Tier0 与 rust-analyzer LSP 覆盖描述不同（clippy 在 Tier0 还是 F2？rust-analyzer 覆盖 cargo check 为何还要 verify.sh？）。
  方案：§5.2 加「Tier0 执行机制：双层递送」（F1 自动 LSP+F2 verify.sh 含 cargo check 兜底，互补非互斥），统一映射表指向 03 §4.4，Hook 表指向 06 §10.3。
  驳回/调整理由：三文件实际不矛盾——06 §10.3:337 已明确说明双机制分工+LSP 失败 Plan B+verify.sh 含 cargo check 兜底的理由；发现误读了 116 行（F2 反馈内容非工具范围）；属文档重组澄清非设计缺陷，降 low。

- **R1-BS3-04** | medium | 06-plugin-structure.md | confirmed
  问题：§4.7 说异步策略「PROFILE 阶段评估并记 .rustmigrate.toml」，但 PROFILE 定义为纯自动采集（无异步），Sprint Planning 无确认步，toml 示例无 async_strategy 字段。
  方案：§4.7 重构对齐 PROFILE/PLAN 边界（PROFILE 由 analyzer 客观检测异步模式输出 async_pattern_summary stdout JSON，PLAN 人类确认 async_strategy 写 toml 作 translator 上下文）。

- **R1-BS3-05** | low | 06-plugin-structure.md | **rejected**
  问题（不成立）：声称类型映射存 MVP vs M2 矛盾——05 列「类型映射表」rule#2 MVP=是，但 09 说 type-map.json 是 M2 产出物，translator 规则生成是否有类型映射不明。
  方案（不适用）：原建议在 05 §6.2 rule#2 加说明区分规则 vs 文件格式。
  **verifier 驳回理由**：文档已明确区分两概念。类型映射规则（rule#2，MVP）存于 agents/translator.md（核心）+porting/ 目录 Markdown；结构化 type-map.json 是 M2 产出物，09:399/444 明确标注「【M2 参考，MVP 不使用】」「MVP 阶段类型映射信息在 porting/ 规则中」。translator §10.2 输入/输出表（225 行）规则生成输入=porting-template.md、输出=porting/，不涉 type-map.json。发现者混淆了「规则」与「结构化文件格式」，设计一致无障碍。（修复者另在 05 §6.2 rule#2 加预防性说明以防误读，未改设计本体。）

---

## 本轮修复文件清单

| 文件 | 涉及 finding |
| --- | --- |
| docs/design/01-positioning-and-methodology.md | R1-D1-01, R1-D1-04（锚点），及 R1-D8-04/R1-D8-06/R1-BS1-04 的 §2.1 风险揭示与 §1.3 规则成熟度补充 |
| docs/design/02-architecture.md | R1-D1-05, R1-D5-04, R1-D5-05, R1-D6-02, R1-BS1-04, R1-BS1-05, R1-BS3-01 |
| docs/design/03-execution-model.md | R1-D1-02, R1-D1-03, R1-D1-06, R1-D2-01, R1-D2-02, R1-D2-05, R1-D6-04, R1-BS2-02, R1-BS2-03, R1-BS2-05, R1-BS3-04；R1-BS1-01, R1-BS1-03（§4.6.1/§7.6.2，任务文误写为 04-execution-model.md）；R1-BS2-01（§4.11 CI/CD） |
| docs/design/04-toolchain.md | R1-D2-03, R1-D2-06, R1-D4-01, R1-D4-02, R1-D4-03, R1-D4-04, R1-D4-05, R1-D6-01, R1-D6-03, R1-D6-05, R1-D8-01, R1-BS3-03 |
| docs/design/05-documentation-system.md | R1-D7-01, R1-D7-04, R1-D7-06, R1-BS1-06 |
| docs/design/06-plugin-structure.md | R1-D2-04, R1-D3-01, R1-D3-03, R1-D3-04, R1-D3-05, R1-D3-06, R1-D5-01, R1-D5-03, R1-D5-06, R1-D7-02, R1-D7-03, R1-D7-05, R1-D8-02, R1-D8-05, R1-BS1-02, R1-BS2-04, R1-BS3-02；R1-BS3-05（预防性说明） |
| docs/design/08-roadmap-and-reference.md | R1-D4-06, R1-D6-06, R1-D8-04, R1-D8-06, R1-BS2-01（M2 交叉引用） |
| docs/design/09-appendix-schemas.md | R1-D1-01（Step 1.5/3.5）, R1-D1-04, R1-D3-02, R1-D5-02 |
| docs/design/README.md | R1-D8-05 |
| docs/design/07-pitfalls-and-risks.md | R1-D2-04, R1-D5-01, R1-D5-04（§12.1/§12.2） |
| CLAUDE.md（仓库根） | R1-D7-05（文件权威来源表 +3 条） |

说明：rejected 的 R1-D8-03、R1-BS3-05 不在修复清单（R1-BS3-05 仅加预防性说明，非设计变更）。任务数据中「04-execution-model.md」实指 03-execution-model.md（04 为 toolchain，执行模式在 03）。
