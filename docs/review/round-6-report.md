# 本轮审查报告

**轮次**: Round 1 (本轮)
**Finding 总数**: 20 条 | confirmed: 14 | adjusted: 3 | rejected: 3

---

## D1 迁移质量与翻译方法论

| ID | 严重度 | 位置 | 状态 | 问题 | 优化方案 |
|----|--------|------|------|------|----------|
| D1-01 | high | 03-execution-model.md §4.3 Step 2 | confirmed | Step 2 指令 translator 生成「6 个核心字段」(缺 observable_side_effects)，但 Step 4/6、§7.5/7.7、09 附录 E 均要求 7 字段，L2 校验必然失败。 | Step 2 line 87「6」改「7」，列表追加「⑦ 可观测副作用」；§4.3.1 line 200 同步。 |
| D1-02 | high | 03-execution-model.md / 09 / 06 | confirmed | 03 §4.3.1 三模式表规定 auto_confirm_intent 全部默认 false；但 09 line 425 和 06 line 713 写「/goal 与 Workflow 默认跳过」，安全门禁被架空。 | 09 line 425、06 line 713 改为「各模式默认值均为 false，详见 03 §4.3.1」。 |
| D1-03 | medium | 03-execution-model.md line 738 / 09 line 462 | confirmed | Phase B 允许重写范围权威定义为三类（并发/取消安全/局部性能优化），但 line 738 和 09 line 462 摘要为「仅限并发/内存管理」，遗漏第三类。 | 两处摘要补全为「并发模式/取消安全/局部性能优化」。 |

## D2 验证体系可靠性

| ID | 严重度 | 位置 | 状态 | 问题 | 优化方案 |
|----|--------|------|------|------|----------|
| D2-01 | high | 06-plugin-structure.md §10.0.1 | confirmed | `rustmigrate stats compare` 列为 M2 命令，但 03 Step 4.5 和 09 Step 3.5 将其作为 MVP 确定性门禁调用，命令不存在则门禁不可执行。 | 将 stats compare 从 M2 提升至 MVP 表（13 命令），删除推迟理由，同步更新 08 计数。 |
| D2-02 | medium | 06-plugin-structure.md | rejected | state transition --to done 未程序化强制验证通过证据，最关键转移仅靠指令跟随。 | **Verifier 理由**: R5 已裁定「证据验证属 Skill 级，不在 CLI 层」；06 line 355 明确两级架构设计；09 line 404 标注此为 MVP 已知约束。属已裁定架构决策的重复提报。 |

## D3 工具架构与工程质量

| ID | 严重度 | 位置 | 状态 | 问题 | 优化方案 |
|----|--------|------|------|------|----------|
| D3-01 | medium | 02-architecture.md §3.4 / 09 Step 4 | confirmed | compile_fixing 状态在枚举和路由中存在，但 02 表述「cargo check 通过后才转移」使其不可达，且 SKILL.md 从未写入该状态。 | 02 改为分支语义（通过→testing，失败→compile_fixing）；09 Step 4 添加 transition 指令；checkpoint 表复位到改为 compile_fixing。 |
| D3-02 | medium | 06-plugin-structure.md §10.2 | confirmed | 接口表将意图摘要与代码翻译合并为一行，与调度序列（两次独立调用+人类确认门禁）和检查点表矛盾，隐藏关键中断点。 | 拆为两行：translator(意图摘要) 前置条件 pending/L2 + translator(Phase A/B) 前置条件 translating+已确认/L1。 |
| D3-03 | medium | 09-appendix-schemas.md line 183 / Step 0.3 | confirmed | substatus 声明「无枚举约束，不参与流转判断」但 Step 0.3 路由依赖子串匹配，LLM 写入变体可能误路由。 | line 183 改为说明 translating 下 3 个保留值参与路由（精确全等匹配）；Step 0.3「含」改为 == 全等匹配。 |

## D4 技术选型审查

| ID | 严重度 | 位置 | 状态 | 问题 | 优化方案 |
|----|--------|------|------|------|----------|
| D4-01 | medium | 04-toolchain.md §5.7.4 | confirmed | graph build 管线边类型产出归属三处矛盾：表写 tree-sitter 产出 calls，正文写基础图无 calls，06 写 ast-grep-core 贡献 calls。 | 管线表拆分各工具边类型贡献；修正正文使其与表一致；补充 ast-grep-core provenance 标注和冲突规则。 |
| D4-02 | low | 04-toolchain.md §5.7.1/§5.7.4 | confirmed | 社区检测算法 Leiden(§5.7.1/08) 与 Louvain(§5.7.4 三处) 交替使用，选型未统一。 | 三处 Louvain 统一为 Leiden + Traag et al. (2019) 引用；UA 的 Louvain 描述保留。 |

## D5 编排可靠性与确定性

| ID | 严重度 | 位置 | 状态 | 问题 | 优化方案 |
|----|--------|------|------|------|----------|
| D5-01 | high | 06-plugin-structure.md / 03 / 09 | confirmed | 与 D2-01 同根：stats compare 列为 M2 但 MVP SKILL.md 作为确定性门禁调用，且 Checkpoint 失败表缺 Step 3.5。 | 由 D2-01 提升统一解决；Checkpoint 表补 Step 3.5 行。 |

## D6 规模化与性能

| ID | 严重度 | 位置 | 状态 | 问题 | 优化方案 |
|----|--------|------|------|------|----------|
| D6-01 | high | 06-plugin-structure.md | confirmed | 与 D2-01/D5-01 同根：stats compare M2 vs MVP 矛盾。 | 由 D2-01 提升统一解决，同步更新 08 计数和估算。 |
| D6-02 | low | 06-plugin-structure.md line 92 | adjusted | graph interfaces --deps-of 未定义 traversal scope（direct vs transitive），与 02 预算公式（仅计 imports 边数）和 graph deps(BFS) 语义有歧义。 | 改为「直接依赖模块（imports 边 1-hop 邻居）的导出接口签名」，加括号注明区别于 graph deps BFS。**调整理由**: 影响降级为规格澄清，因 02 line 241 runtime budget check 可兜底。 |
| D6-03 | medium | 09-appendix-schemas.md | rejected | Translator context 仅接收源语言签名而非已翻译依赖的 Rust API，浪费 topo-sort 优势。 | **Verifier 理由**: F1 反馈机制（rust-analyzer 秒级类型检查）已覆盖类型不匹配；topo-sort 保证依赖已编译故编译器即时验证 B 对 A 的 Rust 类型使用；源语言签名提供语义理解，编译器提供类型正确性，属有意分离。 |

## D7 可维护性/可扩展性/社区贡献

| ID | 严重度 | 位置 | 状态 | 问题 | 优化方案 |
|----|--------|------|------|------|----------|
| D7-01 | low | 06-plugin-structure.md §11.2.1 | adjusted | analysis-tools.json 无内部 schema 定义，社区贡献者无从知晓格式。 | adapter.json schema line 850 描述扩展为含格式定义：JSON array {tool_id, display_name, min_version, install_hint, required}。**调整理由**: 该字段实为 optional（非 required），且 §11.2.1 范围为脚本 I/O 非静态配置，降为 low。 |
| D7-02 | low | 05-documentation-system.md §6.12 | confirmed | Pattern 新鲜度周期声称可配置但 .rustmigrate.toml 无对应字段。 | 删除括号内「（或按 Sprint 周期配置）」，消除与 Schema 的不一致；净删文本。 |

## D8 范围控制/过度设计/路线图

| ID | 严重度 | 位置 | 状态 | 问题 | 优化方案 |
|----|--------|------|------|------|----------|
| D8-01 | medium | 01-positioning-and-methodology.md line 161 | confirmed | Bun 迁移规模 01 写「100 万行」，07/08 均写「75 万行」，跨文件事实不一致。 | 01 line 161 改为「75 万行」与权威来源 08 对齐。 |
| D8-02 | medium | 08-roadmap-and-reference.md line 120 | confirmed | 工作量表标题写 11 个 MVP 子命令（漏 graph interfaces），同小节 line 130 又写 12 个，自相矛盾。 | 标题改 12，命令列表补入 interfaces，人天区间调整为 11-15。 |
| D8-03 | low | 08-roadmap-and-reference.md §13.1.1(b) | confirmed | 规则元数据 YAML frontmatter schema 权威定义错放在路线图文件，05 反向引用它。 | schema 代码块移至 05 规则贡献节作为权威；08 改为一句话引用 05。 |

## BS1 实操盲点

| ID | 严重度 | 位置 | 状态 | 问题 | 优化方案 |
|----|--------|------|------|------|----------|
| BS1-01 | medium | 03-execution-model.md | rejected | Behavior recording 缺 runtime-dependency 可行性预筛，库模块无法隔离执行。 | **Verifier 理由**: scaffolder L1 后置条件在翻译前即时检查，§10.2.2 failure recovery 处理失败；purity_confidence 已分类 I/O 依赖；有状态库「三选一」降级路径已存在；提议新增子步骤违反 R3 净删除原则。 |
| BS1-02 | medium | 03-execution-model.md §4.6.1 | confirmed | Dual-track 变更检测仅覆盖文件仍存在的三种 level，源模块重命名/删除/拆分后 module_path 成为孤立键，无告警无恢复。 | §4.6.1 输出追加第四级 IDENTITY_LOST；Sprint Planning 检测到时暂停要求用户更新路径映射。 |
| BS1-03 | medium | 06-plugin-structure.md §11.1 | confirmed | exclude patterns 下非排除文件 import 排除路径时，FK 约束或 edge skip 行为未定义，graph 不完整无校验。 | §5.7.4 补一句：exclude 目标 edge skip + excluded_imports 计数警告；03 Step 1 扩展内部排除路径导入检查。 |

## BS2 OSS 工程基线

| ID | 严重度 | 位置 | 状态 | 问题 | 优化方案 |
|----|--------|------|------|------|----------|
| BS2-01 | low | 06-plugin-structure.md | adjusted | CLI 12 命令的 data payload schema 未定义，SKILL.md 集成依赖隐式契约。 | 06 CLI 表前加 blockquote 注明 data schema 随 M1 落地为 insta 快照测试；08 CLI 测试行追加 stdout JSON 格式快照。**调整理由**: SKILL.md 主要依赖 exit code 和文件存在性而非 JSON 字段解析，fragility 被高估。 |
| BS2-02 | low | 08-roadmap-and-reference.md | rejected | 基础 dev CI workflow (ci.yml) 未列为 M1 交付物。 | **Verifier 理由**: M1 已规划 dogfooding.yml、verify-reproducibility.sh+GHA、release.yml 三个 workflow，CI 基础设施自然包含 ci.yml；line 88 已引用「4 核 CI runner」；粗估表无需列 15 分钟 boilerplate。 |

## BS3 内部矛盾/跨文件一致性

| ID | 严重度 | 位置 | 状态 | 问题 | 优化方案 |
|----|--------|------|------|------|----------|
| BS3-01 | high | 06-plugin-structure.md | confirmed | 与 D2-01/D5-01/D6-01 同根：stats compare M2 分类 vs MVP 门禁使用的跨文件矛盾。 | 由 D2-01 提升统一解决。 |
| BS3-02 | medium | 03-execution-model.md | confirmed | 与 D1-01 同一问题：Step 2 说 6 字段但验证规格说 7 字段。 | 由 D1-01 统一修复。 |

---

## 本轮修复文件清单

| 文件 | 修复 Finding | 改动概述 |
|------|-------------|----------|
| 03-execution-model.md | D1-01, D1-02, D1-03, BS1-02, BS3-02 | 字段数 6->7 + 列表追加；Phase B 摘要补全；变更检测追加 IDENTITY_LOST |
| 06-plugin-structure.md | D2-01, D3-02, D5-01, D6-01, D6-02, D7-01, BS1-03, BS2-01, BS3-01 | stats compare 提升至 MVP(13 命令)；接口表拆行；graph interfaces 语义澄清；exclude 行为补充 |
| 02-architecture.md | D3-01 | compile_fixing 转换语义改为分支条件 |
| 09-appendix-schemas.md | D1-02, D1-03, D3-01, D3-03 | auto_confirm 默认值对齐；Phase B 范围补全；compile_fixing 指令+checkpoint；substatus 路由收紧 |
| 04-toolchain.md | D4-01, D4-02 | 管线边类型归属明确；Louvain 统一为 Leiden |
| 05-documentation-system.md | D7-02, D8-03(接收) | 删除可配置声称；接收规则 schema 权威定义 |
| 01-positioning-and-methodology.md | D8-01 | Bun 规模 100 万->75 万 |
| 08-roadmap-and-reference.md | D8-02, D8-03(移出), D2-01(计数同步) | CLI 命令数 11->12+补 interfaces；schema 移出至 05；stats compare 计数 12->13 |

## 转 M0 spike 清单

本轮无被排除的实证类诉求。所有 rejected findings 均为已裁定架构决策重复提报或影响被高估，不属于需要 spike 验证的范畴。
