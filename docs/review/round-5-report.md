# 本轮审查报告

**轮次**: Round 1（8 维度并行审查）  
**日期**: 2026-06-07  
**Findings 总数**: 22 条（confirmed 17 / adjusted 3 / rejected 2）

---

## 维度 1: 迁移质量与翻译方法论

| ID | 严重度 | 位置 | 状态 | 问题 | 优化方案 |
|---|---|---|---|---|---|
| D1-01 | medium | 03-execution-model.md | confirmed | Step 2.5 括号注释称 /goal 默认跳过确认（auto_confirm_intent=true），但同文件 §4.3.1 表格明确标注默认 false，两处对同一配置默认值完全相反。 | 删除 Step 2.5 矛盾注释，改为指向 §4.3.1 表格的交叉引用，以表格为唯一权威。 |
| D1-02 | medium | 03-execution-model.md | confirmed | 03 §4.3 Step 2 要求 MDR 标记 bug_replica: true，但 05 §6.5 MDR 权威定义的模板/必填字段表均无此字段。 | 05 行动指南触发列表追加「源码 bug 复刻决策」+ 必填字段表追加一行 bug_replica 字段。 |
| D1-03 | medium | 03-execution-model.md | confirmed | Phase A 结构校验门禁 redo 缺少重试上限和失败降级路径，可能造成 Step 2-3-3.5 无限循环。 | Step 4.5 末尾追加「重做后仍越界则 paused + requires_manual_review」终态。 |
| D1-04 | medium | 09-appendix-schemas.md | confirmed | 意图摘要 6 字段缺少「副作用」专项，但验证链以此为锚检查副作用保持（Step 6 审查清单第 4 维度）。 | 附录 E 新增第 7 字段 observable_side_effects（array of string），03 Step 6 直接引用。 |

## 维度 2: 验证体系可靠性

| ID | 严重度 | 位置 | 状态 | 问题 | 优化方案 |
|---|---|---|---|---|---|
| D2-01 | medium | 06-plugin-structure.md | adjusted | `state transition --to done` 的「带前置条件检查」未定义具体校验内容，与 §10.3 独立脚本门禁核心卖点形成绕过路径疑虑。 | 明确括号说明为「校验当前→目标为合法转换路径」（状态机合法性），证据验证属 Skill 级，不在 CLI 层。 |
| D2-02 | medium | 03-execution-model.md | confirmed | L3-简化对「有状态对象/服务」模块的具体执行机制未定义，仅覆盖 CLI 工具和纯函数库场景。 | 追加有状态场景定义：从操作序列录制出发逐操作 diff 可观测响应，内部状态由 L1 insta 快照承担。 |

## 维度 3: 工具架构与工程质量

| ID | 严重度 | 位置 | 状态 | 问题 | 优化方案 |
|---|---|---|---|---|---|
| D3-01 | high | 09-appendix-schemas.md | confirmed | /migrate analyze SKILL.md 骨架从未调用 rustmigrate init，Step 0 flock 和 Step 2 graph build 均依赖不存在的 .rust-migration/ 目录。 | 插入 Step -1 Bootstrap（幂等调用 rustmigrate init）；Step 3 改为「更新」语义避免覆写 metadata。 |
| D3-02 | medium | 06-plugin-structure.md | confirmed | M2 CLI `state update --cas-version`（§10.5 并发协议）未出现在 M2 命令表中，命令表声称恰好 5 个命令。 | M2 命令表增加 state update 行（CAS 语义 + 推迟理由），计数更新为 6。 |

## 维度 4: 技术选型审查

| ID | 严重度 | 位置 | 状态 | 问题 | 优化方案 |
|---|---|---|---|---|---|
| D4-01 | high | 04-toolchain.md | confirmed | loom 对 async/tokio 代码条件强制提升为选型错误：loom::sync::Mutex::lock() 是同步方法，与 tokio::sync::Mutex 的 async API 不兼容，cfg-gate 无法编译。 | 拆分规则：std::sync -> loom（cfg-gate）；tokio::sync/async -> shuttle（executor-level）。同步修正 03/06 相关示例。 |
| D4-02 | low | 04-toolchain.md | adjusted | ast-grep-core（0.x）承担 graph build 关键角色但未入 §5.5 TRIAL 列表，OXC（0.x 可选备选）却已标 TRIAL，标准不对称。 | §5.5 TRIAL 追加 ast-grep-core 行 + §5.8 Category A 回链注释。已有 Spike 0 回退路径故严重度降为 low。 |

## 维度 5: 编排可靠性与确定性

| ID | 严重度 | 位置 | 状态 | 问题 | 优化方案 |
|---|---|---|---|---|---|
| D5-01 | high | 09-appendix-schemas.md | confirmed | /migrate run 骨架纯线性 happy-path，无基于 substatus 的断点续传路由逻辑，崩溃恢复、done/paused 守卫均缺失。 | Step 0.5 前插入 Step 0.3 状态路由段（10 行伪码），按 status/substatus 跳转对应入口。 |
| D5-02 | medium | 06-plugin-structure.md | rejected | 声称 state transition CLI 的合法转换强制行为未规约（拒绝非法请求的 error_code、--force 层级等）。 | **Verifier 理由**: 「带前置条件检查」+ 完整合法转换表 + state.rs 模块定位已隐含拒绝语义；error_code 完整枚举已明确推迟到 M2（R1-BS2-01）；--force 属 Skill 层参数非 CLI 层参数，为有意分层设计。R3 净删除目标下不宜为已隐含语义增加冗余文字。 |
| D5-03 | medium | 06-plugin-structure.md | rejected | Spike 1 仅验证 3-SubAgent 序列（/migrate analyze），未覆盖 /migrate run 的 4-SubAgent 序列，Plan B 终局性决策基础不足。 | **Verifier 理由**: 终局性仅适用于 Plan B3 整项目决策；Plan B1 局部拆分（line 560）可随时针对单步应用；人类确认门禁（Step 1.5）提供上下文重置使 /migrate run 不等同于纯 4 连续调用；per-step 重试+检查点提供额外鲁棒性。finding 混淆了「项目级终局性」与「无缓解路径」。 |

## 维度 6: 规模化与性能

| ID | 严重度 | 位置 | 状态 | 问题 | 优化方案 |
|---|---|---|---|---|---|
| D6-01 | medium | 09-appendix-schemas.md | confirmed | interface_only 接口提取无 CLI 命令支撑，违反「确定性计算由 CLI 承担」原则，Skill 被迫 ad-hoc 提取。 | MVP CLI 新增 `rustmigrate graph interfaces <module>`（查询 is_exported 签名 + token 估算），09 Step 4 引用。 |
| D6-02 | medium | 04-toolchain.md | confirmed | 增量更新熔断后图一致性无程序化标记，下游 topo-sort 可在截断图上静默运行产出不完整排序。 | source-graph.db metadata 增加 graph_integrity 字段；topo-sort 执行前读取该字段，truncated 时阻塞并输出诊断。 |

## 维度 7: 可维护性/可扩展性/社区贡献

| ID | 严重度 | 位置 | 状态 | 问题 | 优化方案 |
|---|---|---|---|---|---|
| D7-01 | medium | 06-plugin-structure.md | confirmed | Adapter 目录规范缺 tests/ 结构，验证标准（precision>=0.95/coverage>=0.90）无法自动化执行，社区贡献者无自验路径。 | 目录规范增加 tests/fixtures/ + tests/expected/，验证标准表引用 tests/expected/ 为比对源。 |
| D7-02 | low | 05-documentation-system.md | confirmed | §6.12 scope 声称覆盖 SPRINT_LEARNINGS.md 的新鲜度机制，但后续全部机制仅针对 patterns/anti-patterns 定义。 | 收窄 scope 声明仅保留 patterns/anti-patterns；补一句说明 SPRINT_LEARNINGS.md 按 Sprint 编号天然时间戳。 |
| D7-03 | low | 06-plugin-structure.md | confirmed | Release checklist 缺 pattern freshness 检查项，needs-review 状态的 pattern 可随 release 发布。 | Release checklist 追加一项：grep 检查 references/ 下无 status needs-review 的 pattern。 |

## 维度 8: 范围控制/过度设计/路线图

| ID | 严重度 | 位置 | 状态 | 问题 | 优化方案 |
|---|---|---|---|---|---|
| D8-01 | low | 08-roadmap-and-reference.md | adjusted | M2 列 21 项交付物无内部依赖排序和工作量分解，8-12 周估算不可验证。 | 交付物行内嵌入关键路径排序括注（状态机 -> worktree -> CLI），详细分期标注 M1 验收后产出。严重度从 medium 降为 low。 |
| D8-02 | medium | 04-toolchain.md | confirmed | 批大小优化验收标为 M1-blocking，但 MVP <5K 行 (~33 文件) 远低于 200 文件的 batch 激活阈值，验证了 MVP 不会触发的功能。 | 从「M1 阻塞性」降级为「M1 非阻断采集」，结论作为 M2 输入；08 工作分解表同步更新。 |
| D8-03 | low | README.md | confirmed | TL;DR 称「8 层测试验证」，但 MVP 仅强制 3-5 层（L4/L5/L6 属 M2+），夸大 MVP 能力。 | 改为「分层测试验证（L0-L7）」，准确描述框架存在而不暗示全部强制。 |

## 实操盲点（BS 维度）

| ID | 严重度 | 位置 | 状态 | 问题 | 优化方案 |
|---|---|---|---|---|---|
| BS1-01 | low | 02-architecture.md | adjusted | 预算公式「依赖接口」计数单位未定义（模块数 vs 符号数），barrel 文件场景下可能失真。 | 决策树首行追加括注「（依赖模块数，即 imports 边数）」消除歧义；符号级过滤归入 Spike 3。严重度从 medium 降为 low。 |
| BS1-02 | high | 06-plugin-structure.md | confirmed | L2 proptest FFI 对比所需的子进程桥接基础设施未纳入任何 SubAgent 产出物契约，纯函数模块 L2 测试无法生成。 | scaffolder 输出契约追加 test-fixtures/ffi-bridge/；adapter call chain 追加条件 ffi-bridge.sh 步骤。 |
| BS2-01 | medium | 06-plugin-structure.md | confirmed | MSRV 策略未定义：无 rust-version 字段、无版本轴 CI 矩阵、ensure-cli.sh 无工具链版本指引。 | workspace Cargo.toml 标注 rust-version、release.yml 增加 MSRV build job、release checklist 增加校验项。 |
| BS2-02 | medium | 06-plugin-structure.md | confirmed | 预编译 release 二进制无完整性校验（无 SHA256/签名），ensure-cli.sh 无法区分篡改/损坏。 | Release artifact 表增加 SHA256SUMS.txt；ensure-cli.sh 下载后校验 SHA256，失败走 cargo install 回退。 |
| BS3-01 | high | README.md | confirmed | README 核心差异声明「门禁不过不进 Phase B」与实际 F2 触发时机矛盾：verify.sh 在 Phase B 之后运行，非 A→B 之间。 | 修正为「Phase B 完成后...门禁不过模块不进入 done」，对比表同步修正。 |
| BS3-02 | medium | 03-execution-model.md | confirmed | Step 4.5 标记为「门禁，非软提示」却由 verifier AI 执行，违背 01 §2.2 独立脚本原则。 | 改为 Skill 主上下文调用 `rustmigrate stats compare`（确定性 CLI 门禁），与 F2 verify.sh 同级。 |

---

## 本轮修复文件清单

| 文件 | 修复 Finding IDs |
|---|---|
| docs/design/03-execution-model.md | D1-01, D1-02, D1-03, D2-02, BS3-02 |
| docs/design/09-appendix-schemas.md | D1-04, D3-01, D5-01, D6-01 |
| docs/design/06-plugin-structure.md | D2-01, D3-02, D7-01, D7-03, BS1-02, BS2-01, BS2-02 |
| docs/design/04-toolchain.md | D4-01, D4-02, D6-02, D8-02 |
| docs/design/05-documentation-system.md | D7-02 |
| docs/design/08-roadmap-and-reference.md | D8-01 |
| docs/design/README.md | D8-03, BS3-01 |
| docs/design/02-architecture.md | BS1-01 |

## 转 M0 Spike 清单

无。本轮所有 finding 均为设计文档层面的定义缺失/矛盾/不一致，修复方案均为最小文本编辑，无需实证验证即可确认。符号级过滤优化（BS1-01 相关）已归入现有 M0 Spike 3 范畴，不额外新增 Spike。
