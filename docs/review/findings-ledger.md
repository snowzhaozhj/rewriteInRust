# Findings Ledger

> 审查循环跨轮状态台账。格式见 [REVIEW_LOOP.md §6](./REVIEW_LOOP.md#6-findings-ledgermd-格式)。
> **完成轮数 = 7（R1+R2+R3+R4+R5+R6+R7 已修，待独立复验）**。第 1 轮由用户执行 `/goal` 启动并验证；兜底 routine 在完成轮数=0 时不会自动开跑（见 REVIEW_LOOP.md §9）。

## 轮次汇总

| Round | new_high+ | open_blocker | open_high | open_medium | open_low | converged |
|-------|-----------|--------------|-----------|-------------|----------|-----------|
| 0 (init) | — | — | — | — | — | no |
| 1 | 24 | 0 | 0(已修待复验) | 31(已修待复验) | 7(已修待复验) | no |
| 2 | 6 | 0 | 20 | 37 | 2 | no |
| 3 | 1 | 0 | 11 | 29 | 11 | no |
| 4 | 4 | 0 | 4 | 19 | 7 | no |
| 5 | 2 | 0 | 5 | 14 | 6 | no |
| 7 | 3 | 0 | 3 | 12 | 8 | no |

## Findings

| ID | 维度 | 严重度 | 文件:章节 | 标题 | 状态 | 首现轮 | 末更新轮 |
|----|------|--------|-----------|------|------|--------|----------|
| R1-D1-01 | D1 | high | 01-positioning-and-methodology.md | 意图确认门禁缺失，Step1→Step2 自动衔接无人类确认 | reopened | 1 | 2 |
| R1-D1-02 | D1 | high | 03-execution-model.md | verifier 双职责致测试与 Phase B 代码时序歧义 | fixed | 1 | 1 |
| R1-D1-03 | D1 | medium | 03-execution-model.md | Phase B 并发/内存重写晚于测试，测试可能不匹配 | reopened | 1 | 3 |
| R1-D1-04 | D1 | high | 01-positioning-and-methodology.md | Phase A 1:1 对应无机制保证，diff 审查失效 | fixed | 1 | 1 |
| R1-D1-05 | D1 | low | 02-architecture.md | 100K 预算超预算拆分标准未定义 | reopened | 1 | 5 |
| R1-D1-06 | D1 | high | 03-execution-model.md | 缺模块类型×测试层矩阵，测试深度自由裁量 | fixed | 1 | 1 |
| R1-D2-01 | D2 | high | 03-execution-model.md | FFI 比对纯函数/有状态边界与性能成本未界定 | reopened | 1 | 3 |
| R1-D2-02 | D2 | high | 03-execution-model.md | L3 简化隐含单模块假设，未处理依赖链路一致性 | reopened | 1 | 5 |
| R1-D2-03 | D2 | medium | 04-toolchain.md | nextest 并发共享临时文件/SQLite 竞态致假阴 | fixed | 1 | 1 |
| R1-D2-04 | D2 | high | 06-plugin-structure.md | Spike1「成功」定义粗糙，缺语义有效性与临界规则 | reopened | 1 | 3 |
| R1-D2-05 | D2 | medium | 03-execution-model.md | 覆盖率作等价代理的边界未明 | reopened | 1 | 3 |
| R1-D2-06 | D2 | medium | 04-toolchain.md | Tier1 可关闭与验证可靠性张力未调和 | reopened | 1 | 7 |
| R1-D3-01 | D3 | high | 06-plugin-structure.md | file-guard.sh 不防 source-graph.db 并发写 | reopened | 1 | 3 |
| R1-D3-02 | D3 | medium | 09-appendix-schemas.md | blocked 状态检测/恢复时机与依赖顺序未定义 | fixed | 1 | 1 |
| R1-D3-03 | D3 | medium | 06-plugin-structure.md | 编排检查点伪码过简，无超时/重试/校验深度 | reopened | 1 | 3 |
| R1-D3-04 | D3 | low | 06-plugin-structure.md | fmt.sh Hook stdin JSON 格式未定义 | reopened | 1 | 3 |
| R1-D3-05 | D3 | medium | 06-plugin-structure.md | CLI/analyzer 共享 db 锁策略/事务/原子性未定义 | reopened | 1 | 2 |
| R1-D3-06 | D3 | medium | 06-plugin-structure.md | SKILL.md 行数预算可能不足以容纳编排约束 | fixed | 1 | 1 |
| R1-D4-01 | D4 | high | 04-toolchain.md | petgraph bus factor=1 风险，fallback 无成本/触发标准 | reopened | 1 | 3 |
| R1-D4-02 | D4 | low | 04-toolchain.md | SQLite+FTS5 选型无 MVP 规模对标 | fixed | 1 | 1 |
| R1-D4-03 | D4 | medium | 04-toolchain.md | tree-sitter vs OXC 无精度对比，确定性未量化 | reopened | 1 | 5 |
| R1-D4-04 | D4 | high | 04-toolchain.md | clippy.toml 作规则执行器表达力有限无 fallback | reopened | 1 | 7 |
| R1-D4-05 | D4 | low | 04-toolchain.md | FTS5 已建但 MVP 无全文搜索命令，无 MVP 价值 | fixed | 1 | 1 |
| R1-D4-06 | D4 | medium | 02-architecture.md | M2 多 agent 共享单 SQLite 写并发未定义 | fixed | 1 | 1 |
| R1-D5-01 | D5 | medium | 06-plugin-structure.md | MVP 编排全依赖 SKILL.md，Spike1 标准模糊 | fixed | 1 | 1 |
| R1-D5-02 | D5 | medium | 09-appendix-schemas.md | blocked 恢复缺确定性保障，存永久阻塞风险 | reopened | 1 | 3 |
| R1-D5-03 | D5 | high | 06-plugin-structure.md | 检查点未调 Schema 校验，10.5 内部矛盾(253 vs 408) | reopened | 1 | 3 |
| R1-D5-04 | D5 | high | 02-architecture.md | M1→M2 状态格式向后不兼容，无迁移策略 | fixed | 1 | 1 |
| R1-D5-05 | D5 | medium | 02-architecture.md | 100K 预算无运行时检查/自动拆分/溢出恢复 | reopened | 1 | 3 |
| R1-D5-06 | D5 | low | 06-plugin-structure.md | SubAgent 文件通信未定义共享文件锁 | reopened | 1 | 3 |
| R1-D6-01 | D6 | high | 04-toolchain.md | 多 agent 共享 SQLite 事务隔离/一致性/原子化缺失 | reopened | 1 | 7 |
| R1-D6-02 | D6 | high | 02-architecture.md | 100K+interface_only 在深依赖链下可行性无定量分析 | reopened | 1 | 2 |
| R1-D6-03 | D6 | medium | 04-toolchain.md | 图构建无性能基准（Louvain/tree-sitter/35 文件批） | reopened | 1 | 3 |
| R1-D6-04 | D6 | medium | 03-execution-model.md | M1 吞吐无估算，M2 并发度无依据，升级门槛不清 | reopened | 1 | 3 |
| R1-D6-05 | D6 | medium | 04-toolchain.md | 传递性更新深度/环检测/复杂度/退化行为缺失 | reopened | 1 | 5 |
| R1-D6-06 | D6 | medium | 08-roadmap-and-reference.md | M1/M2 验收缺性能与可扩展性量化门禁 | reopened | 1 | 3 |
| R1-D7-01 | D7 | high | 05-documentation-system.md | 规则体系无社区贡献/评审/版本化/冲突仲裁 | reopened | 1 | 3 |
| R1-D7-02 | D7 | high | 06-plugin-structure.md | 适配器扩展工程成本/验收标准/复用率缺失 | reopened | 1 | 5 |
| R1-D7-03 | D7 | high | 06-plugin-structure.md | Plugin API 变更向后兼容策略缺失 | reopened | 1 | 3 |
| R1-D7-04 | D7 | medium | 05-documentation-system.md | 知识沉淀缺更新触发/索引/新鲜度，有死文档风险 | reopened | 1 | 5 |
| R1-D7-05 | D7 | medium | 06-plugin-structure.md | 跨文档一致性无自动验证，权威来源表不完整 | reopened | 1 | 2 |
| R1-D7-06 | D7 | medium | 05-documentation-system.md | PARITY/KNOWN_DIFFERENCES 无社区可见性/异议机制 | fixed | 1 | 1 |
| R1-D8-01 | D8 | medium | 04-toolchain.md | 图存储超 <5K MVP 需求，未量化典型图规模 | reopened | 1 | 5 |
| R1-D8-02 | D8 | high | 06-plugin-structure.md | MVP 编排依赖指令跟随，README 未表述风险 | fixed | 1 | 1 |
| R1-D8-03 | D8 | medium | 06-plugin-structure.md | （驳回）CLI 表混淆 11 命令含 5 个 M2 | reopened | 1 | 7 |
| R1-D8-04 | D8 | medium | 08-roadmap-and-reference.md | M1 估算 Plan B 缓冲不足以覆盖多 Spike 同时失败 | reopened | 1 | 7 |
| R1-D8-05 | D8 | medium | README.md | 「3 命令 MVP」表述与内部多步复杂度鸿沟 | reopened | 1 | 4 |
| R1-D8-06 | D8 | low | 08-roadmap-and-reference.md | M1-M4 目标与工作量无 tie-out（累积/复用） | reopened | 1 | 5 |
| R1-BS1-01 | BS1 | high | 03-execution-model.md | 缺源文件变更检测/同步，迁移期源演化无法感知 | reopened | 1 | 7 |
| R1-BS1-02 | BS1 | high | 06-plugin-structure.md | SubAgent 通信缺超时/产出物校验/失败恢复 | reopened | 1 | 5 |
| R1-BS1-03 | BS1 | high | 03-execution-model.md | L3 经 FFI 调旧实现，依赖未迁移时管线断掉 | reopened | 1 | 3 |
| R1-BS1-04 | BS1 | high | 02-architecture.md | 100K 预算深依赖(20+)超预算无应对策略 | fixed | 1 | 1 |
| R1-BS1-05 | BS1 | medium | 02-architecture.md | PAUSE 降级用户无法判断选哪种，报告内容未定义 | reopened | 1 | 2 |
| R1-BS1-06 | BS1 | medium | 05-documentation-system.md | 规则版本化无示例格式，无回溯流程与版本追踪 | reopened | 1 | 3 |
| R1-BS2-01 | BS2 | high | 08-roadmap-and-reference.md | M2 CI/CD 集成完全缺实现细节 | fixed | 1 | 1 |
| R1-BS2-02 | BS2 | high | 03-execution-model.md | 记分卡缺公式/权重/AI 检查表/持久化/止损 | reopened | 1 | 2 |
| R1-BS2-03 | BS2 | medium | 03-execution-model.md | 性能基准缺基线时机/tolerance 来源/可复现 | reopened | 1 | 3 |
| R1-BS2-04 | BS2 | medium | 06-plugin-structure.md | AI 错误/验证失败信息质量无设计 | reopened | 1 | 3 |
| R1-BS2-05 | BS2 | high | 03-execution-model.md | 可复现性不足（无 seed/版本锁/schema 版本化） | reopened | 1 | 3 |
| R1-BS3-01 | BS3 | medium | 02-architecture.md | 状态机粒度三文件不一致，Phase A/B 映射未明 | reopened | 1 | 3 |
| R1-BS3-02 | BS3 | medium | 06-plugin-structure.md | verifier 双角色边界模糊，Phase A 产出物访问未文档化 | reopened | 1 | 3 |
| R1-BS3-03 | BS3 | low | 04-toolchain.md | Tier0 与 rust-analyzer LSP 覆盖描述三文件不同 | fixed | 1 | 1 |
| R1-BS3-04 | BS3 | medium | 06-plugin-structure.md | §4.7 异步策略与 PROFILE 纯自动采集定义矛盾 | fixed | 1 | 1 |
| R1-BS3-05 | BS3 | low | 06-plugin-structure.md | （驳回）类型映射 MVP vs M2 矛盾 | rejected | 1 | 1 |
| R2-D1-01 | D1 | high | 03-execution-model.md | 意图摘要内容规范缺失，verifier 缺意图一致性维度 | reopened | 2 | 5 |
| R2-D1-02 | D1 | high | 03-execution-model.md | Phase A 结构校验阈值与 §7.5 不一致且无数据支撑 | fixed | 2 | 2 |
| R2-D1-03 | D1 | medium | 03-execution-model.md | TODO(port) 清理缺自动化执行机制与门禁 | fixed | 2 | 2 |
| R2-D2-01 | D2 | high | 03-execution-model.md | 8 探测维度未与 proptest Strategy 关联，无完整性保证 | reopened | 2 | 3 |
| R2-D2-02 | D2 | high | README.md | 独立脚本门禁仅 F2，F1/F3 非独立且 F2 完整性无规范 | reopened | 2 | 5 |
| R2-D3-01 | D3 | medium | 06-plugin-structure.md | F1 rust-analyzer LSP 诊断假设无量化验收标准 | fixed | 2 | 2 |
| R2-D3-02 | D3 | medium | 09-appendix-schemas.md | 「等待 CLI 完全释放」语义不清，graph_build_completed 不在 Schema | reopened | 2 | 3 |
| R2-D4-01 | D4 | medium | 03-execution-model.md | （驳回）FFI 降级快照过期机制——误引文档位置/方法 | rejected | 2 | 2 |
| R2-D4-02 | D4 | medium | 04-toolchain.md | cargo-llvm-cov 不计 FFI 源实现，覆盖率虚高致误判 | fixed | 2 | 2 |
| R2-D5-01 | D5 | high | 06-plugin-structure.md | migration-state.json 原子写缺 crash-safe 机制 | fixed | 2 | 2 |
| R2-D5-02 | D5 | medium | 06-plugin-structure.md | L2 校验层自身超时/OOM/损坏无错误码，盲目重试 | fixed | 2 | 2 |
| R2-D7-01 | D7 | high | 05-documentation-system.md | 26 类规则维护成本/升级判据矩阵/deprecation 不明 | reopened | 2 | 3 |
| R2-D8-01 | D8 | medium | 01-positioning-and-methodology.md | （驳回）26 规则+SubAgent 分发对 MVP 过细——混淆分布与混乱 | rejected | 2 | 2 |
| R2-D8-02 | D8 | medium | 03-execution-model.md | 行为录制框架（Tier1 默认）与 <5K 纯函数模块不匹配 | fixed | 2 | 2 |
| R2-BS1-01 | BS1 | medium | 02-architecture.md | 类型复杂度评估缺定量标准，非 PROFILE 前置预判 | fixed | 2 | 2 |
| R2-BS2-01 | BS2 | medium | 03-execution-model.md | Dogfooding 为纯概念无交付物/CI 集成/验证标准 | reopened | 2 | 4 |
| R2-BS2-02 | BS2 | medium | 05-documentation-system.md | Release 流程与产出物版本化不完整（版本同步/CHANGELOG/artifact） | fixed | 2 | 2 |
| R2-BS3-01 | BS3 | medium | 04-toolchain.md | MVP 图规模节点数三处不一致（13/10/9），违反单一权威 | fixed | 2 | 2 |
| R3-D1-01 | D1 | medium | 01-positioning-and-methodology.md | §2.3 原生重塑单步原则与 Phase A/B 两阶段张力，新用户认知偏差 | fixed | 3 | 3 |
| R3-D2-01 | D2 | medium | 02-architecture.md | verify.sh(F2) 失败后改代码/test fixture vs 回 Phase B 分诊不清 | fixed | 3 | 3 |
| R3-D2-02 | D2 | low | 03-execution-model.md | 纯函数检测验收采样过小、「一致」定义模糊 | fixed | 3 | 3 |
| R3-D3-01 | D3 | medium | 08-roadmap-and-reference.md | Spike 0 crate 集成回退缺阈值/候选/记录时机 | fixed | 3 | 3 |
| R3-D6-01 | D6 | medium | 08-roadmap-and-reference.md | M1 验收限<5K 与 Spike 3a 中型项目语义不一致，图构建超时风险 | reopened | 3 | 4 |
| R3-BS2-01 | BS2 | medium | 06-plugin-structure.md | 预编译二进制分发缺平台矩阵/更新时机/版本一致性检查 | reopened | 3 | 5 |
| R3-BS3-01 | BS3 | high | 01-positioning-and-methodology.md | Spike 1 <80% 降级终局性跨文件不一致（08/07/06/01） | fixed | 3 | 3 |
| R3-BS3-02 | BS3 | low | 02-architecture.md | PROFILE/PLAN 边界与 Sprint Planning 选择/重算歧义，README 未映射阶段↔命令 | fixed | 3 | 3 |
| R3-D5-01 | D5 | high | 06-plugin-structure.md | （驳回）/migrate analyze 缺意图确认门禁——误读 analyze/run 职责 | rejected | 3 | 3 |
| R3-BS1-01 | BS1 | high | 02-architecture.md | （驳回）预算粗估精度/interface_only 压缩率/M2 超时——属 Spike 实证非设计缺陷 | rejected | 3 | 3 |
| R4-D1-01 | D1 | medium | 03-execution-model.md | min_applicable_dimensions 阈值方向反转（简单函数高于复杂函数） | fixed | 4 | 4 |
| R4-D1-02 | D1 | medium | 03-execution-model.md | Phase B 3 轮重试耗尽后升级路径 03 §4.3 缺终态定义 | fixed | 4 | 4 |
| R4-D1-03 | D1 | low | 06-plugin-structure.md | 多候选生成缺里程碑限定，与 01/08 M2+ 标注不一致 | fixed | 4 | 4 |
| R4-D2-01 | D2 | low | 06-plugin-structure.md | §10.3「不可跳过」原则与 Skill 级调用机制表述张力 | fixed | 4 | 4 |
| R4-D2-02 | D2 | medium | 03-execution-model.md | 异步模块 loom 提升规则 04 §5.3 缺失，verifier 面临不可解冲突 | reopened | 4 | 7 |
| R4-D2-03 | D2 | low | 03-execution-model.md | insta 快照缺与 proptest 对等的基线更新权限策略 | fixed | 4 | 4 |
| R4-D3-01 | D3 | high | 02-architecture.md | phase_a_version/phase_a_audit_passed 字段 09 附录 A 缺失 | fixed | 4 | 4 |
| R4-D3-02 | D3 | medium | 09-appendix-schemas.md | reviewing 状态映射表/枚举/执行序列三方自相矛盾 | fixed | 4 | 4 |
| R4-D3-03 | D3 | medium | 06-plugin-structure.md | §11.5 [pipeline] 与 §11.1 [tools] 控制重叠/命名不一致（含 D7-01） | fixed | 4 | 4 |
| R4-D4-01 | D4 | medium | 04-toolchain.md | Provenance::AstGrep 语义与实际适配器工具不匹配 | fixed | 4 | 4 |
| R4-D4-02 | D4 | medium | 04-toolchain.md | syn+quote 嵌入理由与 translator/scaffold 实际需求矛盾 | fixed | 4 | 4 |
| R4-D4-03 | D4 | low | 04-toolchain.md | Tier1 表残留 scc、tokei 输出含不存在的「复杂度」列 | fixed | 4 | 4 |
| R4-D5-01 | D5 | medium | 06-plugin-structure.md | subagent_calls 字段 06 声称见 09 但 09 附录 A 无定义 | fixed | 4 | 4 |
| R4-D5-02 | D5 | medium | 06-plugin-structure.md | /migrate run 调度表遗漏 Step 1 translator + 人类门禁 | fixed | 4 | 4 |
| R4-D5-03 | D5 | high | 09-appendix-schemas.md | 失败恢复表缺 /migrate run Steps 2/3/4 回滚定义 | reopened | 4 | 5 |
| R4-D5-04 | D5 | medium | 09-appendix-schemas.md | Step 0.5/6 状态写入未走 CLI 原子写入路径与 §10.8 矛盾 | fixed | 4 | 4 |
| R4-D6-01 | D6 | medium | 04-toolchain.md | §5.7.6「M1 前 4 项」引导文字与 MVP 列标注矛盾 | fixed | 4 | 4 |
| R4-D8-01 | D8 | medium | README.md | TL;DR「30 秒理解」实际 ~2300 字符需 3-5 分钟 | reopened | 4 | 5 |
| R4-D8-02 | D8 | low | 08-roadmap-and-reference.md | （驳回）§13.1.1 路线图预写实现细节——05 已指定 08 为权威来源 | rejected | 4 | 4 |
| R4-BS1-01 | BS1 | high | 03-execution-model.md | proptest FFI 方向限制：napi-rs 仅 Node→Rust，L2 等价不可实现 | fixed | 4 | 4 |
| R4-BS1-02 | BS1 | medium | 03-execution-model.md | loom cfg-gated 原语导入无 SubAgent 负责插入 | fixed | 4 | 4 |
| R4-BS1-03 | BS1 | low | 03-execution-model.md | 翻译前未预检外部 npm 依赖 Rust 映射可用性 | fixed | 4 | 4 |
| R4-BS2-01 | BS2 | medium | 06-plugin-structure.md | MVP CLI 缺外部工具安装状态检测，失败掩盖真因 | reopened | 4 | 7 |
| R4-BS3-01 | BS3 | high | 08-roadmap-and-reference.md | 08 M1「不含属性测试」与 03/04 proptest 强制要求矛盾 | fixed | 4 | 4 |
| R4-BS3-02 | BS3 | medium | 03-execution-model.md | /migrate run 步骤编号 03 与 09 偏移 1 位，跨文件追溯歧义 | fixed | 4 | 4 |
| R5-D1-01 | D1 | medium | 03-execution-model.md | auto_confirm_intent 默认值 Step 2.5 与 §4.3.1 矛盾 | fixed | 5 | 5 |
| R5-D1-02 | D1 | medium | 03-execution-model.md | MDR 模板缺 bug_replica 字段，03/05 不一致 | fixed | 5 | 5 |
| R5-D1-03 | D1 | medium | 03-execution-model.md | Phase A 结构校验 redo 缺重试上限和失败降级路径 | fixed | 5 | 5 |
| R5-D2-01 | D2 | medium | 06-plugin-structure.md | state transition 前置条件检查内容未定义（adjusted） | reopened | 5 | 7 |
| R5-D3-01 | D3 | high | 09-appendix-schemas.md | /migrate analyze SKILL.md 骨架缺 init 调用，依赖目录不存在 | fixed | 5 | 5 |
| R5-D4-01 | D4 | high | 04-toolchain.md | loom 对 async/tokio 不兼容，选型错误需拆分规则 | fixed | 5 | 5 |
| R5-D5-01 | D5 | medium | 06-plugin-structure.md | （驳回）state transition 合法转换拒绝行为未规约 | rejected | 5 | 5 |
| R5-D5-02 | D5 | medium | 06-plugin-structure.md | （驳回）Spike 1 未覆盖 /migrate run 4-SubAgent 序列 | rejected | 5 | 5 |
| R5-D6-01 | D6 | medium | 09-appendix-schemas.md | interface_only 接口提取缺 CLI 命令，违反确定性计算原则 | fixed | 5 | 5 |
| R5-D7-01 | D7 | low | 06-plugin-structure.md | Release checklist 缺 pattern freshness 检查项 | fixed | 5 | 5 |
| R5-BS2-01 | BS2 | medium | 06-plugin-structure.md | MSRV 策略未定义，缺 rust-version/CI 矩阵/版本指引 | fixed | 5 | 5 |
| R5-BS3-01 | BS3 | medium | 03-execution-model.md | Step 4.5 标记门禁却由 AI 执行，违背独立脚本原则 | fixed | 5 | 5 |
| R7-D1-01 | D1 | low | 03-execution-model.md | Phase B 消除翻译腔边界定义缺失 | fixed | 7 | 7 |
| R7-D1-02 | D1 | medium | 03-execution-model.md | Phase A 翻译重试耗尽后终端动作未定义 | rejected | 7 | 7 |
| R7-D1-03 | D1 | medium | 07-pitfalls-and-risks.md | §9.2 缺 OOP 继承/多态→trait 映射专项条目 | rejected | 7 | 7 |
| R7-D1-04 | D1 | low | 03-execution-model.md | proptest 生成指令 Step 4/6 执行时序歧义 | fixed | 7 | 7 |
| R7-D3-01 | D3 | high | 09-appendix-schemas.md | SKILL.md 骨架缺 --to testing/reviewing 断点续传不可触发 [R7修] | fixed | 7 | 7 |
| R7-D3-02 | D3 | medium | 09-appendix-schemas.md | 意图摘要「7 字段」与 Schema 9 required 属性不一致 | fixed | 7 | 7 |
| R7-D4-01 | D4 | medium | 04-toolchain.md | 分工表排除 tree-sitter calls 边但合并策略含死逻辑 | fixed | 7 | 7 |
| R7-D5-01 | D5 | high | 09-appendix-schemas.md | flock 全局锁在短命 Bash 进程下逻辑失效 [R7修] | fixed | 7 | 7 |
| R7-D5-02 | D5 | medium | 09-appendix-schemas.md | Step 0.3 路由遗漏 phase_b_optimization_in_progress | fixed | 7 | 7 |
| R7-D5-03 | D5 | low | 06-plugin-structure.md | subagent_timeout_secs 仅 post-hoc 分类无主动终止 | rejected | 7 | 7 |
| R7-D7-01 | D7 | low | 05-documentation-system.md | 6 个治理角色术语未定义相互关系 | fixed | 7 | 7 |
| R7-D8-01 | D8 | low | 08-roadmap-and-reference.md | M2→M3 升级判据锁定未经验证理论值 | rejected | 7 | 7 |
| R7-D8-02 | D8 | low | 08-roadmap-and-reference.md | §13.1.2 虚引 09 附录 F performance_metrics | fixed | 7 | 7 |
| R7-BS1-02 | BS1 | low | 03-execution-model.md | subprocess bridge 类型序列化边界未定义 | fixed | 7 | 7 |
| R7-BS2-02 | BS2 | medium | 08-roadmap-and-reference.md | rustmigrate 自身开发 CI pipeline 未定义 | fixed | 7 | 7 |
| R7-BS2-03 | BS2 | medium | 06-plugin-structure.md | rustmigrate 自身无依赖维护策略 | fixed | 7 | 7 |
| R7-BS3-03 | BS3 | high | 06-plugin-structure.md | M2 并行声明与 scaffolder 前置条件矛盾 [R7修] | fixed | 7 | 7 |
