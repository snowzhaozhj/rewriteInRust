# 本轮审查报告

> Round 1 — 共 38 条 finding（confirmed 22 / adjusted 15 / rejected 1）。
> 每条格式：ID / 严重度 / 位置 / 状态 / 问题 / 优化方案。rejected 项附 verifier 不成立理由。

## D1 迁移质量与翻译方法论

- **D1-01** / medium / `01-positioning-and-methodology.md` §2.3 / **confirmed**
  问题：§2.3「原生重塑」单步原则与 v0.9 落地的 Phase A（忠实翻译 1:1）→ Phase B（惯用化）两阶段设计存在张力，新用户首入口处认知偏差。
  方案：§2.3 末补约 80 字澄清，显式映射三步原则到 Phase A（结构保留+语义验证）+ Phase B（惯用重构）协同实现。

- **D1-02** / medium / `03-execution-model.md` §4.3 Step 2 / **confirmed**
  问题：意图摘要规范中「spec」未定义指规范还是实现，源码有 bug 时摘要应记录 bug 还是正确语义不清，维度 9 审查会误判复现 bug 为「正确」。
  方案：明确摘要捕获源码实际行为；识别出 bug 须在 MDR 标 `bug_replica:true` 列位置/内容供人工确认。

- **D1-03** / medium / `03-execution-model.md` §4.3 Step 5/6 / **adjusted**
  问题：Phase B 三类重写边界（并发原语/取消安全/性能）依赖「可观测副作用顺序」精确定义，但该词在并发场景含糊，Step 6 频繁更新意图摘要近似使其失效。
  方案：Step 5 补可观测副作用顺序定义（happens-before 等价）+ 禁止案例；Step 6 改为标 super-boundary、记 MDR 映射、同模块更新>1 次则 `requires_manual_review`。

## D2 验证体系可靠性

- **D2-01** / high / `03-execution-model.md` §7.6.2 / **confirmed**
  问题：链式依赖 A→B 中「A 已迁 Rust 但 status≠done」的中间态未覆盖，verifier 无法判定 L3 是否可执行（MVP Sprint 进行中常态）。
  方案：补可对比条件 (c)（marshaling 适配器 M2 / 降级 `type_conversion_pending`+`L3_blocked: pending_upstream`）；verifier 返建议三选一不自动决策；延后须在 toml 记录目标 sprint。

- **D2-02** / high / `03-execution-model.md` §7.6.1 / **confirmed**
  问题：模块多数导出函数 `purity_confidence` 为 low 时如何处理 L3 未定义，评分卡缺纯函数检测失败率补偿，PARITY 无 L3 confidence 列（外部审阅不可见）。
  方案：补模块级 L3 FFI 置信度决策规则，与 §5.3 tier1 confidence 对齐，low<high 占比>30% 进降级路径，PARITY 加 L3 confidence 列，纳入 Step 6 verifier 提示。

- **D2-03** / high / `03-execution-model.md` §7.1 / **confirmed**
  问题：proptest 回归集何时初始化/更新、Phase B 失败时谁有权更新、与源项目测试关系均未定义，质量评分 proptest 信号虚化。
  方案：补「回归集管理策略」：Phase A 完成初始化基准、Phase B 对标记失败原因、更新须经人审否则 `requires_manual_review`、基线传递为上一版本支撑趋势检测。

- **D2-04** / low / `03-execution-model.md` §7.5 / **adjusted**
  问题：「覆盖率≥源」即达标，忽视覆盖率提升来源成分（错误处理强化 vs 死代码），AI 膨胀翻译可能虚假达标。严重度因多层防御（结构门禁/L2-L3/行数比）下调。
  方案：达标级 case study 须确认新增行为有意错误处理；行数比>1.5x 且覆盖率≥源时交叉核对 Phase B MDR，缺依据则 `requires_manual_review`。不采纳逐行覆盖率分解。

- **D2-05** / medium / `02-architecture.md` §3.2.4 / **confirmed**
  问题：F2 verify.sh 失败后谁有权改代码、能否改 test fixture vs 必须回 Phase B、与 Phase B 3 轮上限的关系不清。
  方案：补失败分诊——脚本本身不可改/跳过；处置权归 verifier，可改 Phase B 代码（走 3 轮）或自身 test fixture（记 MDR、不计翻译轮数、单模块上限 2 次）；环境失败记 KNOWN_DIFFERENCES.md。

- **D2-06** / low / `03-execution-model.md` §7.6.1 / **confirmed**
  问题：M0 Spike 1 纯函数检测验收「≥3 项目 10+ 函数一致率≥80%」采样过小，「一致」定义模糊（结果相同 vs 结论相同）。
  方案：改 ≥2 项目×≥5 函数(20+)；定义「一致」为 `purity_confidence` 等级二值匹配；按类别统计假阳/假阴，任一类<75% 收紧该类规则；基线写 DESIGN_ASSUMPTIONS.md 不作 M1 阻塞。

## D3 工具架构与工程质量

- **D3-01** / medium / `06-plugin-structure.md` §10.5 / **adjusted**
  问题：COMMIT 与 `graph_build_completed` 写入间的崩溃窗口未形式化关闭；严重度下调因 MVP 串行下 Step 2.5 报错重跑是正确行为，真实缺口是窗口/幂等性文档缺失。
  方案：注明崩溃窗口（窄、COMMIT 后元数据前）、标志用 §10.8 原子写入、graph build 重跑幂等性须 M0 Spike 0 验证记入 DESIGN_ASSUMPTIONS.md。

- **D3-02** / medium / `06-plugin-structure.md` §10.3 / **confirmed**
  问题：file-guard.sh 依赖 Hook 提供 `tool_input.file_path`，若 Spike 0 证伪则 fallback 只在嵌套注脚提及，未预承诺，MVP 设计中途需重设计。
  方案：升格为 DESIGN_ASSUMPTIONS.md 预承诺决策点 F1（SUCCESS/FAILURE 两分支预写定），失败时 file-guard 退化 no-op、SKILL.md 强制 Bash 白名单。

- **D3-03** / medium / `06-plugin-structure.md` §10.2 / **adjusted**
  问题：SubAgent 写出 L2 通过但语义损坏（如 `blocked_by` 引用不存在模块）时重试逻辑捕获不到，状态静默损坏。严重度下调因 Step 0.5 已做确定性门禁，真实缺口是可调试性。
  方案：明确引用一致性检查延后到 Step 0.5（非完成后立即），新增错误码 `BLOCKED_BY_VALIDATION_FAILED` 及诊断指引，删除对不存在「L2 速查表」的虚假引用。

- **D3-04** / medium / `06-plugin-structure.md` §10.5 / **confirmed**
  问题：§10.5 引用 09 附录 B 的「Step 0 锁协议骨架与故障恢复」，但附录 B 无该内容（循环自引用），陈旧锁恢复逻辑缺失，存在并发写竞态。
  方案：09 附录 B 新增「Step 0 全局锁获取与陈旧锁恢复」骨架（锁文件 JSON {pid,started_at,hostname}、ps -p PID 检查、平台差异、手动逃生口）；06 定义 `lock_timeout_secs` 语义。

- **D3-05** / medium / `06-plugin-structure.md` §10.0.2 / **confirmed**
  问题：兼容性策略覆盖 schema/命令签名，但未涵盖 SKILL.md 程序行为（步骤序列/检查点）变更的版本规则，进行中 Sprint 升级 Plugin 后无适配指引。
  方案：兼容性表新增「SKILL.md 程序步骤」行（重排/删除/语义改变升 major，新增可选升 minor，CHANGELOG 列破坏性变更）；升级清单补 Breaking Changes 审查要求。

- **D3-06** / medium / `08-roadmap-and-reference.md` Spike 0 / **confirmed**
  问题：crate 集成回退缺确切阈值（二进制体积/编译时间/冷启动）、未指明 fallback crate 候选、未约定何时记录 Spike 0 结果。
  方案：新增三行回退表（指标|测量方法|触发条件|逐级裁剪 rusqlite→ast-grep-core→tokei|返工估算），先记环境基线再据基线定阈值，结果写 DESIGN_ASSUMPTIONS.md。

## D5 编排可靠性与确定性

- **D5-01** / high / `06-plugin-structure.md` §10.5 / **rejected**
  问题：声称 `/migrate analyze` 缺意图确认门禁、编排确定性/非确定性分工含糊。
  方案（建议无需修复）：可在 §10.5 补一句区分 analyze（无意图确认）与 run（含 Step 1.5）的流程差异。
  **verifier 不成立理由**：基于误读。`/migrate analyze` 是全项目初始化阶段，不生成单模块意图摘要，那是 `/migrate run` Step 1.5 的职责，文档已正确设计；编排分工虽有塑性但属已知风险（06§10.5 已说明、07§12.2 列 Plan B），是 Spike 1 验证对象而非设计缺陷。建议方案（加 Step 0.5）会造成 analyze/run 流程重复混淆。

- **D5-02** / high / `09-appendix-schemas.md` Step 0.5 / **confirmed**
  问题：blocked 恢复的 DFS 环检测仅在 SKILL.md 伪码定义，无对应确定性 CLI 命令规范，违反「确定性检查不依赖 LLM 指令跟随」原则，存在永久阻塞风险。
  方案：Step 0.5 补「MVP 实现归属与确定性边界」（MVP 由 SKILL.md 执行属已知约束，M2 抽取为 `validate state --check-blocked --auto-unblock`），Verifier 须实证环检测；08 M2 补任务+JSON schema {unblocked,still_blocked,cycle_detected}。

- **D5-03** / low / `06-plugin-structure.md` §10.5 / **adjusted**
  问题：L1/L2 通过但 SubAgent 语义不符时编排检查点发现不了；`graph_build_completed` 字段缺失时 Step 2.5 报错 vs 重试不明。严重度下调因仅需一句澄清，无逻辑漏洞。
  方案（并入 D3-01）：明确 Step 2.5 属前置条件验证、不走 `max_retries_per_step`，报错停止并提示重跑 `graph build`。

- **D5-04** / low / `06-plugin-structure.md` §10.5 / **adjusted**
  问题：Spike 1 在 80%-95% 临界区间、超时 1-2 次时如何决策（Plan B1 局部拆 vs B3 整体升级）的决策树缺失。严重度下调因核心阈值已明（<80%→B3），设计有意简化避免矩阵冲突。
  方案：触发粒度注补一句临界区间裁量（依单步重试次数+总耗时人工判定 B1 vs B3，决策记 DESIGN_ASSUMPTIONS.md），不新增矩阵。

- **D5-05** / medium / `06-plugin-structure.md` §10.5 / **confirmed**
  问题：全局命令锁缺陈旧锁自动清理策略、用户恢复指引（如何确认无进行中任务）、异常终止时锁释放语义（macOS/Linux 差异）。
  方案（并入 D3-04）：09 附录 B Step 0 含陈旧锁三态检测（进程已死→删/活→报错/超时兜底）+ 平台差异说明；06 §10.5 错误信息告知可手动删 `.migration-lock`。

## D6 规模化与性能

- **D6-01** / high / `04-toolchain.md` §5.7.3 / **confirmed**
  问题：MVP 声明「单 writer 串行」但增量更新（§4.6.1）可触发 `graph build` 与 SubAgent 读竞争，STRUCTURAL 变更删+插需原子完成，MVP 无并发写策略；WAL+busy_timeout 是否适用 MVP 表述含混。
  方案：交叉引用既有两层锁（Step 0 全局命令锁 + file-guard flock 保护 source-graph.db）保障 MVP 串行化，不依赖 WAL+busy_timeout；标注 STRUCTURAL 单事务原子性为通用。不新增第三把锁。

- **D6-02** / high / `02-architecture.md` §3.5.1 / **confirmed**
  问题：超预算「降级路径」未定义自动化执行流程；§3.4「降级分析报告」是编译失败事后诊断，与 §3.5.1 事前预算检查时序相反、职责模糊；预算检查由谁负责（Skill/translator/编排器）不明。
  方案：「降级分析报告」改名「编译失败诊断报告」（事后）与 §3.5.1「预估超预算检查」（事前）分离；明确 token 预估职责归 Skill 主上下文（调 translator 前），给出 80K/95K/100K 三档分支。（收回方案原拟的 `rustmigrate tokenize` 子命令与 Step 0.7，避免破坏 11 命令边界。）

- **D6-03** / medium / `08-roadmap-and-reference.md` Spike 3a / **adjusted**
  问题：batching（35 文件/批）三指标无通过阈值，M0 结束无法客观判定批大小是否足够，风险延伸 M1。方案需调整因不应在数据到达前预设阈值。
  方案：新增「M0→M1 决策树」（仿 tree-sitter 分档），测三指标→对标 §5.7.4.1 实践值→达中位数则 batch_size=35 进 M1，低于下四分位则调 20/50 重测或混合策略，只定流程不预填阈值。

- **D6-04** / medium / `08-roadmap-and-reference.md` M1/M2 门禁 / **confirmed**
  问题：M1 门禁「30-40 分钟」是含 AI 生成的总墙钟时间，无法拆出编排层开销；M2 仅约束多 agent 吞吐，未约束单 agent 串行边际成本。
  方案：Spike 1 增「编排性能基线」（并行于可靠性验收、零额外测试），记录各步骤耗时，阈值单步<2s、全编排<5 分钟（占 12-17%），M2 门禁复用。

- **D6-05** / medium / `08-roadmap-and-reference.md` M1 验收 / **adjusted**
  问题：M1 验收限 <5K 小项目，与 Spike 3a 要求的中型项目（5K-20K）测参数语义不一致；图构建<10s@100 文件基于 150 行/文件假设，200-300 行/文件可能超时。
  方案：M1 增「可扩展性初步检验」子项（复用 Spike 3a 中型项目采图构建@200+行、上下文峰值、编排稳定性三项写 DESIGN_ASSUMPTIONS.md）；性能门禁注脚增中型项目校准回填说明。

- **D6-06** / medium / `04-toolchain.md` §5.7.2 / **confirmed**
  问题：petgraph 回退方案有阈值但无自动化监控（谁在监控 issue 数）、触发后是否纳入 M1 紧急计划不明、无渐进式中间状态。
  方案：阈值由隐含「持续监控」改为「不做持续监控→M0 Spike 3 API 验收兜底 MVP→M2 启动一次性评估」决策链，补 DESIGN_ASSUMPTIONS.md 记录格式与项目级回退逃生口。

## D7 可维护性/可扩展性/社区贡献

- **D7-01** / low / `05-documentation-system.md` §6.2 / **adjusted**
  问题：26 类规则三层维护职责无量化成本，社区贡献评审 SLA 仅写「CONTRIBUTING.md 有详述」无具体流程。严重度下调因成本基线可在 M1 实践建立。
  方案：§6.2.1 补「规则变更通知机制」（CHANGELOG 列 affected_modules）；快速参考补「PR 评审承诺 14 日内初审、超期 escalate」。删除原方案臆想的 `deprecation_approval_days` 字段建议。

- **D7-02** / medium / `05-documentation-system.md` §6.11.1 / **adjusted**
  问题：L1 知识沉淀称「可选」但无「何时有价值」判断标准，SKILL.md pattern 注入时机/信号未定义，无防 L1 写成事件日志的格式约束。方案需避免与 M2 index.json 策略冲突。
  方案：补「L1 启用判据与格式」紧凑块（触发条件 模块>50 行 AND 异步/FFI/生命周期 + 4 字段强制模板 + SKILL.md Step 1.5 注入指令），保留手工 Read 不自动生成索引。

- **D7-03** / medium / `06-plugin-structure.md` §11.2 / **adjusted**
  问题：adapter.json 无 version 字段，核心规则 breaking change 时已生成的 porting-template 与新规则版本混用矛盾，M2 多语言碎片化。指控「缺 version」过绝对（template 已有 rule_version）。
  方案：adapter.json 加可选 `adapter_last_updated`（供 M2 检测过期）；澄清 porting-template 已有 `rule_version` 语义；陈旧检测程序化推 M2 复用既有 `enforce_rule_version_consistency`，不新增配置。

- **D7-04** / medium / `06-plugin-structure.md` §10.0.2 / **adjusted**
  问题：「当前+前 2 major」支持窗口无 EOL 截止日期；升级失败无 fallback 恢复脚本；「CI 加版本一致性检查」仅口号无实现。方案需避免整段新增（守净删除纪律）。
  方案：兼容性窗口补「N-2 外 major 发布后 180 天 deprecation、365 天下线」；升级清单加「失败诊断检查点」三项+错误码（PLUGIN_VERSION_MISMATCH/SCHEMA_VERSION_UNSUPPORTED）；CI 检查并入既有 Release 流程。

- **D7-05** / low / `05-documentation-system.md` §6.11.1 / **confirmed**
  问题：index.json 为 M2 才出，MVP「按规则类别手工 Read」无具体指导——translator 何时触发 pattern 注入、规则类别映射表均缺，pattern 库 50+ 后不可扩展。
  方案：§6.11.1 补「MVP 手工 Read 触发口径」：translator 在 §4.3 Step 2 基于 NodeData 输出 3-5 个 pattern 文件名，编排器预扫描（≤3 文件且<20K token 自动 Read，否则提示对应 RULE）。

- **D7-06** / medium / `05-documentation-system.md` §6.2 / **confirmed**
  问题：「项目专有规则优先」原则在核心规则 breaking change 时，冻结的项目专有规则变体可能与新核心规则矛盾，无检测/升级时机机制。
  方案：§6.2 新增「跨版本冻结规则的升级检测」子节，用 `.rule-freeze-metadata.json` 记 `core_rules_snapshot`，`/migrate analyze` 后续运行比对当前版本，major 升级则告警提示复审。不引入 toml 新配置。

## D8 范围控制/过度设计/路线图

- **D8-01** / medium / `04-toolchain.md` §5.7.3 / **adjusted**
  问题：petgraph+rusqlite 超 MVP 最小需求为 M2 预留，但风险门控缺可执行定量门禁（无编译时间/体积阈值），易致 M0 末期来不及切 Plan B。严重度由 high 下调因决策机制已存在、缺的是数值。
  方案：门控决策规则结构化（编译/体积/冷启动三项任一超 Spike 0 基线即触发 JSON 回退）；08 决策表新增「Spike 0 crate 集成超限」行。不硬编码绝对值（避免臆造实证数据）。

- **D8-02** / medium / `README.md` TL;DR / **adjusted**
  问题：「3 命令 MVP」与「50-70 人天」并置易致 OSS 新用户误判可快速上手，实际含 11 CLI 子命令+4 SubAgent。属表述清晰度而非阻塞缺陷。
  方案：将唯一真实缺口（内部入口数口径+<2K/5K+ 适用范围）折叠进 TL;DR 既有「规模预期」句并交叉引用 01 §1.3，三文件口径对齐，净增量近 0。

- **D8-03** / medium / `04-toolchain.md` §5.7.4.1 / **adjusted**
  问题：性能基准表 TS 行全标 `[M0 Spike 3 TBD]`，35 文件/批来自逆向工程未实证，跨批重复率超预期时无 Plan B。实际缺口是「假设失败的决策自动化」而非「验收标准模糊」。
  方案：06 §11.1 [analysis] 加 `batch_reuse_rate_threshold=0.30`（schema 权威）；§5.7.4.1 引用该 key 取代硬编码并指向 Spike 3a 决策树；07 §12.2 补 Plan B 行使风险防护对称。

- **D8-04** / medium / `08-roadmap-and-reference.md` M1 工作量表 / **adjusted**
  问题：14 项交付物共 50-70 人天但缺单元单价与行业基线对标，CLI 11 命令 10-14 天偏乐观，无削减项对比。「估算膨胀」未经 M0 数据不可作设计 finding 报。
  方案：M1 表后增「估算透明度说明」（每命令≈0.9-1.3 天+graph 子命令共享开销，crate 集成已单列，人天约 60% 纯开发，Spike 0 触上限则 JSON 回退减~2 天）。聚焦透明度不做投机 benchmark。

- **D8-05** / medium / `05-documentation-system.md` / `08` / **adjusted**
  问题：26 类规则+4 SubAgent 的维护成本未折算进 M1，「核心规则+参考指南 2-3 人天」低估，无规则淘汰/合并机制。严重度由 medium 下调因设计已大致完整、缺的是预算记账明确性。
  方案：08（工作量权威）M1 行改「SubAgent 核心规则+参考指南+社区贡献模板 (2-3 人天)」加边界脚注（自动追踪/index.json 推迟 M2）；M2 新增「规则治理工具化 (2-3 人天)」行项。

- **D8-06** / medium / `08-roadmap-and-reference.md` M2 门禁 / **adjusted**
  问题：M1 串行（30-40 分钟/模块）与 M2「≥1.5 模块/小时」无因果推导，M2 验收「通过」客观判据不明（P99=1.2 是否接受未定）。属定义缺陷非实证。
  方案：M2 门禁后增「M2→M3 升级判据」（P50≥1.5/P99≥0.8/冲突率<10% 且锁等待≤20ms/波动≤±10% 四项充要，1.5 为及格线非天花板）；03 §4.10 加注区分启动条件 vs 充要判据。

## BS1 实操盲点（真实迁移开源项目会在哪崩）

- **BS1-01** / medium / `03-execution-model.md` §7.6 / **adjusted**
  问题：FFI 降级 A 模块的 C 接口差异如何验证、wrapper 类型映射是否验证、深链全降级时无止损量化界限。核心是文档清晰度而非流程缺陷，严重度下调。
  方案：§7.6 开头补「FFI 降级模块验证策略」三层（验证范围仅 Rust 侧+wrapper 纳手工审/L3_blocked 标记/止损联动）；§4.9 止损表新增「FFI 降级占比」行（单链路>30%/多链路>50% 触发链路评审）。

- **BS1-02** / high / `04-toolchain.md` §5.7.6 / **confirmed**
  问题：topo-sort 算法细节未定义，源码本身循环依赖（JS re-export 间接循环）时返回部分排序/抛异常/不可用排序不明；Step 0.5 环检测仅管理层阻塞，与源码循环是两个问题域。
  方案：§5.7.6 改 Kahn+无环说明补降级注脚；09 新增 Step 2.8 拓扑排序探测门禁（检测到环则暂停+列环路径）；06 命令描述补环处理；03 §4.2 新增「循环依赖处理」小节（源码改动/`requires_manual_review` 跳过）。

- **BS1-03** / medium / `03-execution-model.md` §4.3.1 / **adjusted**
  问题：意图摘要生成可靠性≥90% 仅为假设，M0 实测<85% 时是否改流程/放弃中间步骤不明，「意图正确但表述不清」灰色情况判定权不清。严重度由 high 下调因基础框架已存在、可经 manual_review 规避。
  方案：补「降级路径与灰色情况处理」（<85% 时三模式默认值分级收紧保留摘要；Phase B 发现不符视为意图漂移、更新 intent.md 记 MDR、不计违反 Step 2.5；验收以 5 特征↔6 字段↔Schema 映射判定，取消 `intent_summary_min_quality` 配置）。

- **BS1-04** / medium / `02-architecture.md` §3.5.1 / **rejected**
  问题：预算粗估（字节/4）对类型丰富 TS 误差大；深链 interface_only 压缩率定性估计未覆盖高依赖（>100 接口）；M2 并行模式 agent 超预算卡住时其他 agent 处理未说明。
  方案：补「运行时预算管理」（Spike 3 增高依赖接口压缩测试、Step 1 显式预算检查、M2 超时自动 `timeout_degrade`）。
  **verifier 不成立理由**：混合三类均不符标准。(1) 预算粗估与 interface_only 压缩率明确是 M0 Spike 3 假设验证清单（文档已注「粗估；编排器调用 tokenizer 校准」「待 Spike 3 实测校准」），属 §5 排除的实现期实证；(2)「>100 接口」无文档依据，文档阈值是 ≥20 依赖/depth≥5，发现凭空引入「≥15 接口」测试目标；(3) M2 并行超时降级超出 MVP(M1) 范围且属实现期工程决策；(4) 发现声称修复 R1-BS1-04 但修复内容与所指不符（实为补拆分决策树/降级报告，非预算精度/高依赖压缩/M2 超时）。误读既有 spike 计划为设计缺陷。

- **BS1-05** / low / `03-execution-model.md` §4.6.1 / **adjusted**
  问题：变更检测成本未单独量化；反向 BFS 遇全项目影响（改顶层 index.js）如何处理未说；STRUCTURAL 告警后已译 Rust 侧与变化的源依赖一致性谁负责无策略。变更检测性能属已知 spike，应剔除。
  方案：§4.6.1 补全局影响处理（反向 BFS>30% 模块自动建议 full re-graph+提示重评双轨成本）；05 §6.5 MDR 模板新增 `translated_from_source_commit` 字段追溯一致性。不要求变更检测性能定量化（Spike 3 已承诺）。

## BS2 OSS 工程基线（CI/release/dogfooding/eval/性能基准/错误信息/可复现）

- **BS2-01** / high / `06-plugin-structure.md` §10.7 / **confirmed**
  问题：line 630 声称完整错误码表（30-40 条）以「09 §A.5」为唯一权威，但 09 无 A.5 节（锚点指向 A 节 migration-state schema），形成虚假权威+断链，CI/诊断无法工业化。
  方案：删除对不存在 §A.5 的虚假权威引用，改为明确下表+VALIDATION_* 三码即 MVP(M1) 错误码表，完整体系 M2 补充并回链。修复文档完整性违规，MVP 不阻塞于完整枚举。

- **BS2-02** / high / `03-execution-model.md` §4.11.3 / **confirmed**
  问题：可复现性验收「哈希一致」缺规范——哈希范围（是否排除注释/时间戳）、5 字节差异定义、清理策略、环境快照格式、SHA256 工具参数均未明，M1 验收主观、难自动化。
  方案：§4.11.3 就地补 4 项（排除规则列表/环境快照 JSON Schema 7 字段+清缓存/`sha256sum -b` 并定义 5 字节为元数据偏差或改相对容限<0.1%/工具标准化）；08 工作量改规范补充 0.5 天+脚本 1-1.5 天。

- **BS2-03** / medium / `06-plugin-structure.md` §10.0.1 / **confirmed**
  问题：预编译二进制分发缺平台矩阵、更新时机、跨平台构建流程、版本一致性检查实现，导致非主流平台无法自动获取、版本漂移风险、Release artifact 管理不清。
  方案：分发方式后增「跨平台分发与版本管理」小节（首发矩阵 darwin/linux × x86_64/arm64+Windows best-effort、命名约定、更新规则、GitHub Actions 矩阵+release.yml 版本一致性 fail gate、ensure-cli.sh 降级 cargo install）；工作量 2d 并入 08。

- **BS2-04** / low / `03-execution-model.md` §4.11.4 / **adjusted**
  问题：dogfooding「信息性状态检查」成功/失败判定、fixture 语言特性范围、M1.5 升级 required 的「无失败」量化、故障隔离均未定义。严重度下调因 dogfooding 标注 M2 概念、不阻塞 M1。
  方案：补「Dogfooding 验收标准」（fixture 涵盖纯函数/有状态/错误处理三类、`all([check,clippy,test])==passed` 单失即败、M1.5 连续 3 周期无失败或首失根因已修、故障隔离分离到不同 artifact）；08 M2 补落地行 0.5-1 天。

- **BS2-05** / medium / `08-roadmap-and-reference.md` 性能门禁 / **confirmed**
  问题：性能门禁缺基准建立方法（环境控制）、回归检测自动化（数据持久化/CI 门禁）、回归诊断工具、与质量评分卡的关系，性能承诺无法兑现。
  方案：新增 §13.1.2「M1 性能基准建立与回归检测流程」五点（环境标准化、复用 sprint-N-report.json 加 `performance_metrics` 字段、criterion relative_difference+P99 由 verifier 在 /migrate review 呈现、与 §7.5 L6 加权协调、工作量 0.5-1 天）。不新增 CLI 命令。

## BS3 内部矛盾/跨文件一致性/逻辑漏洞

- **BS3-01** / low / `02-architecture.md` §3.4 / **adjusted**
  问题：Phase A/B 在状态机中位置混淆——02 说「内部子步骤不占节点」，09 映射表只有单一 translating 态，03 §4.3 列出完整内循环步骤，substatus 字段与「不占节点」原则似冲突，断点续传重入点不清。实为文档组织（规格分散）而非逻辑矛盾。
  方案：02 §3.4 末注就地标明「PORT-REVIEW 接受」理由（02 是状态机定义权威，09 substatus 约定已自洽，checkpoint 入口注属 03 范畴），不重复 03 执行细节。

- **BS3-02** / high / `01-positioning-and-methodology.md` §2.1 / **confirmed**
  问题：Spike 1 <80% 降级规则跨文件不一致——08 明确「一次性判定、不许重测、自动锁 Plan B3」，但 07 §12.2 Plan B 表只给触发条件未述终局性，06 §10.5 循环引用未解决，读者不知是否允许重测。
  方案：07 §12.2 新增「触发判定终局性」脚注（M0 验收会一次性判定、不许重测/重设计、M1 锁 Plan B3、工作量按 08、与 08 互为权威）；06 §10.5 改为「07+08 协同约定」；01 §2.1 追加一句。

- **BS3-03** / high / `06-plugin-structure.md` §10.2 / **confirmed**
  问题：L1/L2/L3 定义分散——§10.2 接口表用「L1/L2」标注但无本地定义（定义在远处 §10.5）；§10.1 说 MVP=L1 但 §10.2 标 verifier 输出为 L2（同表矛盾）；§10.2.2/§10.5 引用的「失败恢复规则表」在 09 不存在（断链）；L1/L2 差异化失败行为未文档化。
  方案：§10.2.1 改为权威分级定义并明确 MVP 边界（migration-state/测试 JSON/source-graph.db/intent.md 为 L2，其余 Markdown 停 L1）；09 附录 B 新增「检查点失败恢复规则」表（7 检查点×校验级/保留/删除/重试/复位）；§10.5 反向链接回 §10.2。

- **BS3-04** / low / `06-plugin-structure.md` §10.0.1 / **adjusted**
  问题：CLI 命令计数口径歧义——`validate rules` 候选是否占用 M2 那 5 个名额还是另加不明。M2 列表实为封闭的 5 命令，歧义仅修辞性非结构性，严重度由 medium 下调。
  方案：按 verifier 建议在引用处（05 §6.2 line 144）补一句而非被引用处冗余——明确 `validate rules` 为 M2「备选」、不在 06 当前已定 5 个之内、是否纳入由 M2 规划确定，命令权威仍以 06 为准。

- **BS3-05** / low / `02-architecture.md` §3.2.3 / **adjusted**
  问题：PROFILE/PLAN 边界——§4.2 Sprint Planning「按拓扑排序选模块」似与 PROFILE 产出重复；每 Sprint 变更检测属 PROFILE 还是 PLAN 不明；README 未映射 PROFILE/PLAN/SCAFFOLD 到命令。文档内部一致，属清晰度/UX 而非逻辑缺陷。
  方案：02 §3.2.3 表格后就地标明「PORT-REVIEW 接受」理由（§3.2.3 已清晰区分，Sprint Planning 选择 vs 重算歧义与 README 命令↔阶段映射分属 03 §4.2 与 README 范畴），不在 02 新增映射表。

---

## 本轮修复文件清单

| 文件 | 落实的 finding |
|------|---------------|
| `docs/design/01-positioning-and-methodology.md` | D1-01, BS3-02 |
| `docs/design/02-architecture.md` | D6-02, D2-05, BS3-01(接受), BS3-05(接受) |
| `docs/design/03-execution-model.md` | D1-02, D1-03, D2-01, D2-02, D2-03, D2-04, D2-06, BS1-01, BS1-03, BS1-05, BS2-02, BS2-04 |
| `docs/design/04-toolchain.md` | D6-01, D6-06, D8-01, D8-03, BS1-02 |
| `docs/design/05-documentation-system.md` | D7-01, D7-02, D7-05, D7-06, D8-05(部分), BS3-04(引用处) |
| `docs/design/06-plugin-structure.md` | D3-01, D3-02, D3-03, D3-04, D3-05, D5-03, D5-04, D5-05, D7-03, D7-04, BS2-01, BS2-03, BS3-03 |
| `docs/design/08-roadmap-and-reference.md` | D3-06, D6-03, D6-04, D6-05, D8-04, D8-05, D8-06, BS2-05 |
| `docs/design/09-appendix-schemas.md` | D5-02, D3-04(Step 0 骨架), BS3-03(失败恢复表) |
| `docs/design/README.md` | D8-02 |

说明：多条 finding 跨文件协同修复（如 D3-04/D5-05 合并落在 06+09；BS3-02 落在 01+06+07）。rejected 项（D5-01, BS1-04）无修复。

## 转 M0 spike 清单（被排除的实证类诉求）

以下诉求被 verifier 判定为实现期实证数据，按 REVIEW_LOOP.md §5 护栏不作设计缺陷修复，转为 M0/M1 spike 验证项并记入 DESIGN_ASSUMPTIONS.md：

- **[D2-06]** analyzer 纯函数检测一致率基线（≥2 项目×≥5 函数，按类别假阳/假阴）→ M0 Spike 1 验收点，不作 M1 阻塞门槛。
- **[D3-01 / D5-03]** graph build 提交后重跑幂等性、CLI 崩溃处理 → M0 Spike 0 验证，记 DESIGN_ASSUMPTIONS.md。
- **[D3-02]** Hook stdin JSON（Bash 的 `tool_input.file_path`）是否可靠 → M0 Spike 0 预承诺决策点 F1（SUCCESS/FAILURE 双分支）。
- **[D3-06 / D8-01]** crate 集成编译时间/二进制体积/冷启动实测基线 → M0 Spike 0，先测后据基线定阈值（不预设绝对值）。
- **[D6-03 / D8-03]** batching 三指标实测（批内符号覆盖度/跨批重复率/依赖链准确度）→ M0 Spike 3a，按决策树映射 M1 入口条件。
- **[D6-04]** 编排层步骤耗时占比基线 → M0 Spike 1（并行于可靠性验收，零额外测试）。
- **[D6-05]** 中型项目（5K-20K 行、200+ 行/文件）图构建/上下文峰值/编排稳定性 → Spike 3a 采集，回填 M1 性能门禁表与上下文预算表。
- **[D6-06]** petgraph StableGraph API 稳定性（20-100 节点、<100ms、无 panic）→ M0 Spike 3。
- **[BS1-03]** 意图摘要生成可靠性（≥90% 假设）采样实测 → M0 Spike 采样验证，<85% 触发模式默认值收紧。
- **[BS1-04 (rejected)]** 预算粗估精度、interface_only 压缩率 → 文档原已列为 M0 Spike 3 校准项，非新增。
- **[BS1-05]** 变更检测（反向 BFS+重分析）性能成本 → M0 Spike 3 已承诺，不另作设计修复。
- **[D8-04]** M1 CLI/工作量估算可信度（vs 行业基线 benchmark）→ 属实现验证，仅以 M0 数据回填，不作 design finding。
