# Findings Ledger

> 审查循环跨轮状态台账。格式见 [REVIEW_LOOP.md §6](./REVIEW_LOOP.md#6-findings-ledgermd-格式)。
> **完成轮数 = 2（R1+R2 已修，待独立复验）**。第 1 轮由用户执行 `/goal` 启动并验证；兜底 routine 在完成轮数=0 时不会自动开跑（见 REVIEW_LOOP.md §9）。

## 轮次汇总

| Round | new_high+ | open_blocker | open_high | open_medium | open_low | converged |
|-------|-----------|--------------|-----------|-------------|----------|-----------|
| 0 (init) | — | — | — | — | — | no |
| 1 | 24 | 0 | 0(已修待复验) | 31(已修待复验) | 7(已修待复验) | no |
| 2 | 6 | 0 | 20 | 37 | 2 | no |

## Findings

| ID | 维度 | 严重度 | 文件:章节 | 标题 | 状态 | 首现轮 | 末更新轮 |
|----|------|--------|-----------|------|------|--------|----------|
| R1-D1-01 | D1 | high | 01-positioning-and-methodology.md | 意图确认门禁缺失，Step1→Step2 自动衔接无人类确认 | reopened | 1 | 2 |
| R1-D1-02 | D1 | high | 03-execution-model.md | verifier 双职责致测试与 Phase B 代码时序歧义 | fixed | 1 | 1 |
| R1-D1-03 | D1 | medium | 03-execution-model.md | Phase B 并发/内存重写晚于测试，测试可能不匹配 | reopened | 1 | 2 |
| R1-D1-04 | D1 | high | 01-positioning-and-methodology.md | Phase A 1:1 对应无机制保证，diff 审查失效 | fixed | 1 | 1 |
| R1-D1-05 | D1 | low | 02-architecture.md | 100K 预算超预算拆分标准未定义 | fixed | 1 | 1 |
| R1-D1-06 | D1 | high | 03-execution-model.md | 缺模块类型×测试层矩阵，测试深度自由裁量 | fixed | 1 | 1 |
| R1-D2-01 | D2 | high | 03-execution-model.md | FFI 比对纯函数/有状态边界与性能成本未界定 | reopened | 1 | 2 |
| R1-D2-02 | D2 | high | 03-execution-model.md | L3 简化隐含单模块假设，未处理依赖链路一致性 | reopened | 1 | 2 |
| R1-D2-03 | D2 | medium | 04-toolchain.md | nextest 并发共享临时文件/SQLite 竞态致假阴 | fixed | 1 | 1 |
| R1-D2-04 | D2 | high | 06-plugin-structure.md | Spike1「成功」定义粗糙，缺语义有效性与临界规则 | fixed | 1 | 1 |
| R1-D2-05 | D2 | medium | 03-execution-model.md | 覆盖率作等价代理的边界未明 | reopened | 1 | 2 |
| R1-D2-06 | D2 | medium | 04-toolchain.md | Tier1 可关闭与验证可靠性张力未调和 | reopened | 1 | 2 |
| R1-D3-01 | D3 | high | 06-plugin-structure.md | file-guard.sh 不防 source-graph.db 并发写 | reopened | 1 | 2 |
| R1-D3-02 | D3 | medium | 09-appendix-schemas.md | blocked 状态检测/恢复时机与依赖顺序未定义 | fixed | 1 | 1 |
| R1-D3-03 | D3 | medium | 06-plugin-structure.md | 编排检查点伪码过简，无超时/重试/校验深度 | reopened | 1 | 2 |
| R1-D3-04 | D3 | low | 06-plugin-structure.md | fmt.sh Hook stdin JSON 格式未定义 | fixed | 1 | 1 |
| R1-D3-05 | D3 | medium | 06-plugin-structure.md | CLI/analyzer 共享 db 锁策略/事务/原子性未定义 | reopened | 1 | 2 |
| R1-D3-06 | D3 | medium | 06-plugin-structure.md | SKILL.md 行数预算可能不足以容纳编排约束 | fixed | 1 | 1 |
| R1-D4-01 | D4 | high | 04-toolchain.md | petgraph bus factor=1 风险，fallback 无成本/触发标准 | reopened | 1 | 2 |
| R1-D4-02 | D4 | low | 04-toolchain.md | SQLite+FTS5 选型无 MVP 规模对标 | fixed | 1 | 1 |
| R1-D4-03 | D4 | medium | 04-toolchain.md | tree-sitter vs OXC 无精度对比，确定性未量化 | reopened | 1 | 2 |
| R1-D4-04 | D4 | high | 04-toolchain.md | clippy.toml 作规则执行器表达力有限无 fallback | reopened | 1 | 2 |
| R1-D4-05 | D4 | low | 04-toolchain.md | FTS5 已建但 MVP 无全文搜索命令，无 MVP 价值 | fixed | 1 | 1 |
| R1-D4-06 | D4 | medium | 02-architecture.md | M2 多 agent 共享单 SQLite 写并发未定义 | fixed | 1 | 1 |
| R1-D5-01 | D5 | medium | 06-plugin-structure.md | MVP 编排全依赖 SKILL.md，Spike1 标准模糊 | fixed | 1 | 1 |
| R1-D5-02 | D5 | medium | 09-appendix-schemas.md | blocked 恢复缺确定性保障，存永久阻塞风险 | reopened | 1 | 2 |
| R1-D5-03 | D5 | high | 06-plugin-structure.md | 检查点未调 Schema 校验，10.5 内部矛盾(253 vs 408) | reopened | 1 | 2 |
| R1-D5-04 | D5 | high | 02-architecture.md | M1→M2 状态格式向后不兼容，无迁移策略 | fixed | 1 | 1 |
| R1-D5-05 | D5 | medium | 02-architecture.md | 100K 预算无运行时检查/自动拆分/溢出恢复 | fixed | 1 | 1 |
| R1-D5-06 | D5 | low | 06-plugin-structure.md | SubAgent 文件通信未定义共享文件锁 | reopened | 1 | 2 |
| R1-D6-01 | D6 | high | 04-toolchain.md | 多 agent 共享 SQLite 事务隔离/一致性/原子化缺失 | reopened | 1 | 2 |
| R1-D6-02 | D6 | high | 02-architecture.md | 100K+interface_only 在深依赖链下可行性无定量分析 | reopened | 1 | 2 |
| R1-D6-03 | D6 | medium | 04-toolchain.md | 图构建无性能基准（Louvain/tree-sitter/35 文件批） | reopened | 1 | 2 |
| R1-D6-04 | D6 | medium | 03-execution-model.md | M1 吞吐无估算，M2 并发度无依据，升级门槛不清 | reopened | 1 | 2 |
| R1-D6-05 | D6 | medium | 04-toolchain.md | 传递性更新深度/环检测/复杂度/退化行为缺失 | reopened | 1 | 2 |
| R1-D6-06 | D6 | medium | 08-roadmap-and-reference.md | M1/M2 验收缺性能与可扩展性量化门禁 | reopened | 1 | 2 |
| R1-D7-01 | D7 | high | 05-documentation-system.md | 规则体系无社区贡献/评审/版本化/冲突仲裁 | reopened | 1 | 2 |
| R1-D7-02 | D7 | high | 06-plugin-structure.md | 适配器扩展工程成本/验收标准/复用率缺失 | reopened | 1 | 2 |
| R1-D7-03 | D7 | high | 06-plugin-structure.md | Plugin API 变更向后兼容策略缺失 | reopened | 1 | 2 |
| R1-D7-04 | D7 | medium | 05-documentation-system.md | 知识沉淀缺更新触发/索引/新鲜度，有死文档风险 | reopened | 1 | 2 |
| R1-D7-05 | D7 | medium | 06-plugin-structure.md | 跨文档一致性无自动验证，权威来源表不完整 | reopened | 1 | 2 |
| R1-D7-06 | D7 | medium | 05-documentation-system.md | PARITY/KNOWN_DIFFERENCES 无社区可见性/异议机制 | fixed | 1 | 1 |
| R1-D8-01 | D8 | medium | 04-toolchain.md | 图存储超 <5K MVP 需求，未量化典型图规模 | reopened | 1 | 2 |
| R1-D8-02 | D8 | high | 06-plugin-structure.md | MVP 编排依赖指令跟随，README 未表述风险 | fixed | 1 | 1 |
| R1-D8-03 | D8 | medium | 06-plugin-structure.md | （驳回）CLI 表混淆 11 命令含 5 个 M2 | rejected | 1 | 1 |
| R1-D8-04 | D8 | medium | 08-roadmap-and-reference.md | M1 估算 Plan B 缓冲不足以覆盖多 Spike 同时失败 | reopened | 1 | 2 |
| R1-D8-05 | D8 | medium | README.md | 「3 命令 MVP」表述与内部多步复杂度鸿沟 | reopened | 1 | 2 |
| R1-D8-06 | D8 | low | 08-roadmap-and-reference.md | M1-M4 目标与工作量无 tie-out（累积/复用） | reopened | 1 | 2 |
| R1-BS1-01 | BS1 | high | 03-execution-model.md | 缺源文件变更检测/同步，迁移期源演化无法感知 | fixed | 1 | 1 |
| R1-BS1-02 | BS1 | high | 06-plugin-structure.md | SubAgent 通信缺超时/产出物校验/失败恢复 | fixed | 1 | 1 |
| R1-BS1-03 | BS1 | high | 03-execution-model.md | L3 经 FFI 调旧实现，依赖未迁移时管线断掉 | fixed | 1 | 1 |
| R1-BS1-04 | BS1 | high | 02-architecture.md | 100K 预算深依赖(20+)超预算无应对策略 | fixed | 1 | 1 |
| R1-BS1-05 | BS1 | medium | 02-architecture.md | PAUSE 降级用户无法判断选哪种，报告内容未定义 | reopened | 1 | 2 |
| R1-BS1-06 | BS1 | medium | 05-documentation-system.md | 规则版本化无示例格式，无回溯流程与版本追踪 | reopened | 1 | 2 |
| R1-BS2-01 | BS2 | high | 08-roadmap-and-reference.md | M2 CI/CD 集成完全缺实现细节 | fixed | 1 | 1 |
| R1-BS2-02 | BS2 | high | 03-execution-model.md | 记分卡缺公式/权重/AI 检查表/持久化/止损 | reopened | 1 | 2 |
| R1-BS2-03 | BS2 | medium | 03-execution-model.md | 性能基准缺基线时机/tolerance 来源/可复现 | reopened | 1 | 2 |
| R1-BS2-04 | BS2 | medium | 06-plugin-structure.md | AI 错误/验证失败信息质量无设计 | reopened | 1 | 2 |
| R1-BS2-05 | BS2 | high | 03-execution-model.md | 可复现性不足（无 seed/版本锁/schema 版本化） | reopened | 1 | 2 |
| R1-BS3-01 | BS3 | medium | 02-architecture.md | 状态机粒度三文件不一致，Phase A/B 映射未明 | reopened | 1 | 2 |
| R1-BS3-02 | BS3 | medium | 06-plugin-structure.md | verifier 双角色边界模糊，Phase A 产出物访问未文档化 | reopened | 1 | 2 |
| R1-BS3-03 | BS3 | low | 04-toolchain.md | Tier0 与 rust-analyzer LSP 覆盖描述三文件不同 | fixed | 1 | 1 |
| R1-BS3-04 | BS3 | medium | 06-plugin-structure.md | §4.7 异步策略与 PROFILE 纯自动采集定义矛盾 | fixed | 1 | 1 |
| R1-BS3-05 | BS3 | low | 06-plugin-structure.md | （驳回）类型映射 MVP vs M2 矛盾 | rejected | 1 | 1 |
| R2-D1-01 | D1 | high | 03-execution-model.md | 意图摘要内容规范缺失，verifier 缺意图一致性维度 | fixed | 2 | 2 |
| R2-D1-02 | D1 | high | 03-execution-model.md | Phase A 结构校验阈值与 §7.5 不一致且无数据支撑 | fixed | 2 | 2 |
| R2-D1-03 | D1 | medium | 03-execution-model.md | TODO(port) 清理缺自动化执行机制与门禁 | fixed | 2 | 2 |
| R2-D2-01 | D2 | high | 03-execution-model.md | 8 探测维度未与 proptest Strategy 关联，无完整性保证 | fixed | 2 | 2 |
| R2-D2-02 | D2 | high | README.md | 独立脚本门禁仅 F2，F1/F3 非独立且 F2 完整性无规范 | fixed | 2 | 2 |
| R2-D3-01 | D3 | medium | 06-plugin-structure.md | F1 rust-analyzer LSP 诊断假设无量化验收标准 | fixed | 2 | 2 |
| R2-D3-02 | D3 | medium | 09-appendix-schemas.md | 「等待 CLI 完全释放」语义不清，graph_build_completed 不在 Schema | fixed | 2 | 2 |
| R2-D4-01 | D4 | medium | 03-execution-model.md | （驳回）FFI 降级快照过期机制——误引文档位置/方法 | rejected | 2 | 2 |
| R2-D4-02 | D4 | medium | 04-toolchain.md | cargo-llvm-cov 不计 FFI 源实现，覆盖率虚高致误判 | fixed | 2 | 2 |
| R2-D5-01 | D5 | high | 06-plugin-structure.md | migration-state.json 原子写缺 crash-safe 机制 | fixed | 2 | 2 |
| R2-D5-02 | D5 | medium | 06-plugin-structure.md | L2 校验层自身超时/OOM/损坏无错误码，盲目重试 | fixed | 2 | 2 |
| R2-D7-01 | D7 | high | 05-documentation-system.md | 26 类规则维护成本/升级判据矩阵/deprecation 不明 | fixed | 2 | 2 |
| R2-D8-01 | D8 | medium | 01-positioning-and-methodology.md | （驳回）26 规则+SubAgent 分发对 MVP 过细——混淆分布与混乱 | rejected | 2 | 2 |
| R2-D8-02 | D8 | medium | 03-execution-model.md | 行为录制框架（Tier1 默认）与 <5K 纯函数模块不匹配 | fixed | 2 | 2 |
| R2-BS1-01 | BS1 | medium | 02-architecture.md | 类型复杂度评估缺定量标准，非 PROFILE 前置预判 | fixed | 2 | 2 |
| R2-BS2-01 | BS2 | medium | 03-execution-model.md | Dogfooding 为纯概念无交付物/CI 集成/验证标准 | fixed | 2 | 2 |
| R2-BS2-02 | BS2 | medium | 05-documentation-system.md | Release 流程与产出物版本化不完整（版本同步/CHANGELOG/artifact） | fixed | 2 | 2 |
| R2-BS3-01 | BS3 | medium | 04-toolchain.md | MVP 图规模节点数三处不一致（13/10/9），违反单一权威 | fixed | 2 | 2 |
