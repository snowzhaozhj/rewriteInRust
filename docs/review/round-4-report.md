# 本轮审查报告

**轮次**: Round 1
**发现总数**: 22 条（confirmed 18 / adjusted 3 / rejected 1）

---

## 维度 1: 迁移质量与翻译方法论

### D1-01 | medium | 03-execution-model.md | confirmed
**问题**: `min_applicable_dimensions_per_function` 阈值方向反转——简单函数（1-2 参数）要求 >= 5 维高于复杂函数（3+ 参数）>= 3 维，逻辑与测试工程原则相悖，会对简单函数产生系统性虚假 incomplete 标记。
**方案**: 将默认方向调整为「1-2 参数 >= 3 维，3+ 参数 >= 5 维」，同步修改 06 § 11.1 配置注释。

### D1-02 | medium | 03-execution-model.md | confirmed
**问题**: Phase B 3 轮编译重试耗尽后的升级路径（暂停 + 降级分析报告）仅存在于 09 附录 B，权威文档 03 § 4.3 Step 5 缺少这一关键终态定义。
**方案**: 在 03 § 4.3 Step 5「最多 3 轮」后补一句引用：「3 轮后仍失败 -> 暂停，触发降级流程（详见 09 附录 B Step 4）」。

### D1-03 | low | 06-plugin-structure.md | confirmed
**问题**: 06 § 10.2 translator 将「多候选生成」列为当前能力（无里程碑限定），但 01/08 明确标注 M2+，跨文件范围不一致。
**方案**: 06 § 10.2 translator 行改为「多候选生成 [M2+]」，与 01/08 对齐。

---

## 维度 2: 验证体系可靠性

### D2-01 | low | 06-plugin-structure.md | adjusted
**问题**: § 10.3 原则「agent 无法说服自己跳过」与 F2 verify.sh 实际由 SKILL.md 指令调用（非 Hook 触发）存在表述张力。
**方案**: 重写原则措辞区分两级保障：「Hook 级自动触发不可跳过；Skill 级调用依赖指令跟随，脚本内部逻辑确定性执行」。
**调整理由**: 文档已在 § 10.5 和 line 432 做了澄清，残留问题仅为 line 350 措辞过宽，严重度从 medium 降为 low。

### D2-02 | medium | 03-execution-model.md | confirmed
**问题**: § 7.1.1 矩阵标注异步模块「L7 loom 必须」，但 04 § 5.3 Tier 1 自动提升规则未包含 loom，配置默认 loom=false 且无提升条件——verifier 面临不可解冲突。
**方案**: 在 04 § 5.3 追加「有异步/并发模式 -> loom 提升为条件强制 Tier 1」规则，同步更新 06 § 11.1 loom 配置注释。

### D2-03 | low | 03-execution-model.md | adjusted
**问题**: insta 快照（L1）缺少与 proptest 对等的基线更新权限策略，AI 可通过 `cargo insta accept` 自行接受变更快照。
**方案**: 在 03 proptest 策略末尾追加一句：「同等原则适用于 insta 快照：verifier/translator 不得执行 cargo insta accept」。
**调整理由**: 风险较低——verifier 指令集中无 `cargo insta accept` 命令，且有人类审查 git diff 兜底，严重度从 medium 降为 low。

---

## 维度 3: 工具架构与工程质量

### D3-01 | high | 02-architecture.md | confirmed
**问题**: 02 § 3.2.4 声称的 `phase_a_version` / `phase_a_audit_passed` 字段在 09 附录 A schema 中完全不存在，Phase A/B 失败归因机制无法实现。
**方案**: 在 09 附录 A 模块级字段补充两字段定义（各一行），附录 B Step 3.5 补写入语义（一句）。

### D3-02 | medium | 09-appendix-schemas.md | confirmed
**问题**: reviewing 状态在映射表/枚举/实际执行序列间自相矛盾——对抗审查实际发生在 translating 阶段，testing->reviewing 转换的触发条件和实际工作未定义。
**方案**: 将 reviewing 重定义为 post-test sign-off（TODO(port) 清零 + 最终签批），三处各改一句话消除语义冲突。

### D3-03 | medium | 06-plugin-structure.md | confirmed
**问题**: § 11.5 [pipeline] 段与 § 11.1 [tools] 段对验证工具启用存在控制重叠、命名不一致（fuzz vs cargo_fuzz）且无优先级定义。
**方案**: 删除 § 11.5 [pipeline] TOML 块，逐工具控制统一收归 § 11.1 [tools] 段，净减约 18 行。

---

## 维度 4: 技术选型审查

### D4-01 | medium | 04-toolchain.md | confirmed
**问题**: `Provenance::AstGrep` 语义与实际适配器工具（dependency-cruiser）不匹配，且 graph build 是否嵌入 ast-grep-core 在三处文件互相矛盾。
**方案**: 将 AstGrep 重命名为 ToolAssisted（保留 deterministic vs non-deterministic 区分力），§ 5.7.4 表删除 ast-grep 联合表述。

### D4-02 | medium | 04-toolchain.md | confirmed
**问题**: syn+quote 作为嵌入 crate 的理由「翻译阶段核心依赖」与 CLI/Plugin 边界矛盾——translator SubAgent 是 LLM agent 无 Rust 运行时，scaffold 实际需 toml_edit 而非 syn。
**方案**: syn+quote 降级为 M2 条件嵌入；translator 工具列删除 syn+quote；scaffold 补注 toml_edit。

### D4-03 | low | 04-toolchain.md | confirmed
**问题**: § 5.3 Tier 1 表残留「tokei + scc」（scc 已在 v0.9.2 移除），§ 5.8 tokei 输出规格含不存在的「复杂度」（tokei 不计算复杂度）。
**方案**: § 5.3 删 scc 改「代码行数对比」；§ 5.8 输出列删「复杂度」改「空行/注释占比」。

---

## 维度 5: 编排可靠性与确定性

### D5-01 | medium | 06-plugin-structure.md | confirmed
**问题**: `subagent_calls` 字段在 06 § 10.5 有完整内联定义并声称「Schema 见 09」，但 09 附录 A 完全不存在该字段定义。
**方案**: 在 09 附录 A 补 subagent_calls 数组示例 + 语义描述；修正 06 交叉引用锚指向附录 A。

### D5-02 | medium | 06-plugin-structure.md | confirmed
**问题**: § 10.5 调度表将 /migrate run 描述为 4 次 SubAgent 调用，遗漏了 09 附录 B 定义的 Step 1 translator（意图摘要）+ 人类门禁。
**方案**: 调度表补入 Step 1 translator(语义解构/意图摘要) + 人类确认门禁，关键产出物增加 {module}-intent.md。

### D5-03 | high | 09-appendix-schemas.md | confirmed
**问题**: 失败恢复权威表缺少 /migrate run Steps 2/3/4（Phase A/对抗审查/Phase B）的回滚定义——核心翻译管线的保留/删除文件和复位状态完全未指定。
**方案**: 补 3 行表格定义各步骤的保留文件、删除文件、重试次数和复位 substatus。

### D5-04 | medium | 09-appendix-schemas.md | confirmed
**问题**: Step 0.5/Step 6 对 migration-state.json 的写入未走 CLI 原子写入路径，与 § 10.8 crash-safe 统一要求矛盾——Claude Code Write 工具无法保证 tmp-fsync-rename。
**方案**: 骨架中明确所有状态写入须通过 `rustmigrate state transition` CLI 命令执行。

---

## 维度 6: 规模化与性能

### D6-01 | medium | 04-toolchain.md | confirmed
**问题**: § 5.7.6 引导文字「M1 实现前 4 项」与同表 MVP 列标注矛盾（rdeps/cycles=M2，stats=MVP 却不在前 4），06/08 确认 MVP 仅 3 项。
**方案**: 引导文字改为「M1 实现标注'是'的 3 项查询」，表格重排使 MVP 查询连续排列。

### D6-02 | low | 02-architecture.md | confirmed
**问题**: § 3.5.1 拆分阈值使用硬编码 80K/95K，与可配置参数 max_tokens_per_translation 之间缺少显式比例绑定——用户修改预算后行为未定义。
**方案**: 补一句参数化声明（阈值 = 0.80B / 0.95B），同步将 06 § 11.1 注释改为比例表达。

---

## 维度 7: 可维护性/可扩展性/社区贡献

### D7-01 | medium | 06-plugin-structure.md | confirmed
**问题**: § 11.5 [pipeline] 与 § 11.1 [tools] 双重控制、命名不一致、无 schema 定义——与 D3-03 为同一问题。
**方案**: 同 D3-03，删除 § 11.5 [pipeline] 段统一收归 [tools]。

### D7-02 | low | 05-documentation-system.md | confirmed
**问题**: § 6.12 pattern 新鲜度「自动标记过期」声明无实际执行者——无 CLI/Hook/Skill 步骤负责运行时间比对。
**方案**: 将「自动」改为「verifier 在 /migrate review 时标记」；06 § 10.5 review 序列追加 pattern freshness scan。

---

## 维度 8: 范围控制/过度设计/路线图

### D8-01 | medium | 08-roadmap-and-reference.md | confirmed
**问题**: Spike 3 补充 a 嵌套于 M0（1-2 天/Spike）但同时声称 M1 阻塞性交付物，所需 3-5 个中型项目在 M0/M1 均无来源——归属、资产、时间框架三重矛盾。
**方案**: M0 缩减为前向引用；M1 工作量表新增「批大小优化验收」行（2-3 人天）；引用改指 M1。

### D8-02 | medium | README.md | confirmed
**问题**: TL;DR 标题「30 秒理解」但实际 ~2300 字符含 6+ 链接和防御性注解，需 3-5 分钟消化，违背自身承诺且信息与下文/权威来源大量重复。
**方案**: TL;DR 压缩为 3 短段 + 最小概念集，冗余信息直接删除不迁移（已存在于权威位置），净删约 12 行。

### D8-03 | low | 08-roadmap-and-reference.md | rejected
**问题**: （原主张）§ 13.1.1 在路线图中预写 M1 实现细节（YAML schema），违反「权威来源唯一」原则。
**Verifier 驳回理由**: 05-documentation-system.md line 177 显式将 08 § 13.1.1(b) 指定为规则元数据分类标准的唯一权威来源（「分类标准与字段含义见 [08 § 13.1.1(b)]」）。08 IS 被设计为该信息的权威所在地，05 主动引用而非重复。数值目标标注「不阻断 M1 验收」属路线图合法规划职能。执行提案反而会打破 05 的现有交叉引用。

---

## 实操盲点

### BS1-01 | high | 03-execution-model.md | confirmed
**问题**: L2 proptest 等价验证假设 napi-rs 支持 Rust->TS 调用（从 proptest 中调原 TS 实现），但 napi-rs 仅支持 Node->Rust 方向——核心 L2 等价承诺对 MVP 目标语言 TS 不可实现。
**方案**: 补 FFI 方向不对称说明（需反转运行器或子进程桥接）；M0 Spike 追加端到端可行性验证。

### BS1-02 | medium | 03-execution-model.md | confirmed
**问题**: L7 loom/shuttle 要求代码使用 cfg-gated 替换原语，但无 SubAgent 负责在翻译后代码中插入 `#[cfg(loom)] use loom::sync::Mutex` 等条件导入——测试退化为普通顺序执行。
**方案**: translator Phase B 完成时为并发模块添加 cfg-gated 原语导入；verifier L7 测试添加 `#[cfg(loom)]` 注解。

### BS1-03 | low | 03-execution-model.md | adjusted
**问题**: 翻译前未预检外部 npm 依赖是否有 Rust 映射，不可映射模块会耗尽 3 轮重试后才 PAUSE。
**方案**: Step 0.7 预检外部 import 与 dependency-mapping.md 交叉核验，MVP 为信息性警告（非阻塞）。
**调整理由**: 既有 Step 2.5 人类确认门禁可拦截；translator 已接收 dependency-mapping.md；原文对重试次数理解有误（非 3 次完整翻译），严重度从 medium 降为 low。

---

## OSS 工程基线

### BS2-01 | medium | 06-plugin-structure.md | confirmed
**问题**: 11 个 MVP CLI 命令无一负责检测外部工具（dependency-cruiser/npx tsc/mypy）安装状态，工具缺失时走 SubAgent 通用失败恢复（重试/降级），掩盖真因。
**方案**: 扩展 `rustmigrate profile` 纳入外部工具可用性检测；§ 10.7 错误码表增加 ADAPTER_TOOL_MISSING；不新增 doctor 命令守住 11 命令边界。

### BS2-02 | medium | 03-execution-model.md | adjusted
**问题**: § 4.11.4 dogfooding fixture 含「手写期望 Rust 输出」但 CI 验证止步于 Tier 0 编译通过——期望输出从未被消费，SubAgent prompt/规则变更无翻译质量回归门禁。
**方案**: § 4.11.4 验证深度追加确定性指标子集回归检测（行数比/函数数比/clippy 警告数，退化 >15% 则 CI 告警）。
**调整理由**: § 4.11.4 明确为「概念设计」且标注范围限制，Tier 0 仍可捕获灾难性退化，严重度维持 medium 但修复方案从引入新 schema 简化为单句追加。

---

## 内部矛盾/跨文件一致性

### BS3-01 | high | 08-roadmap-and-reference.md | confirmed
**问题**: 08 将「属性测试」列入 M1「不包含」，但 03/04 强制纯函数模块必须通过 L2 proptest 才能 done，且 M1 验收要求迁移纯函数模块——三方不可同时成立。
**方案**: 08 M1「不包含」改为仅排除 cargo-fuzz/cargo-mutants；M1 范围新增 proptest 基础集成（1-2 人天）；M2 改为「完整 FFI 等价管线」。

### BS3-02 | medium | README.md | confirmed
**问题**: TL;DR 写 /migrate run「共 2 次 SubAgent 调用」，但同文件命令表和 06 § 10.5 均为 4 次——同段内 /migrate analyze 用「次」表示调用次数，run 用「次」表示种类数，口径不一致。
**方案**: 随 D8-02 TL;DR 压缩一并删除矛盾描述，不再内联声明调用次数。

### BS3-03 | medium | 03-execution-model.md | confirmed
**问题**: /migrate run 步骤编号在 03（Step 3=Phase A）与 09（Step 2=Phase A）之间偏移 1 位，02 引用 09 编号但概念指向 03，跨文件追溯易出错。
**方案**: 03 § 4.3 标题下追加编号映射说明（03 Step N = 09 Step N-1）；02 § 3.2.4 标注编号来源。

---

## 本轮修复文件清单

| 文件 | 修复条目 |
|------|----------|
| `03-execution-model.md` | D1-01, D1-02, D2-02, D2-03, BS1-01, BS1-02, BS1-03, BS2-02, BS3-03 |
| `06-plugin-structure.md` | D1-03, D2-01, D3-03, D5-01, D5-02, D7-01, BS2-01 |
| `02-architecture.md` | D3-01, D6-02, BS3-03 (联动) |
| `09-appendix-schemas.md` | D3-02, D5-03, D5-04 |
| `04-toolchain.md` | D4-01, D4-02, D4-03, D6-01 |
| `05-documentation-system.md` | D7-02 |
| `08-roadmap-and-reference.md` | D8-01, BS3-01 |
| `README.md` | D8-02, BS3-02 |

## 转 M0 Spike 清单

（无。本轮所有实证类诉求均已在修复方案中转化为现有 M0 Spike 验证点的追加——如 BS1-01 纳入 Spike FFI 验证、BS1-03 为信息性预检而非阻塞。无需新增独立 Spike。）
