# MDR-021: 取消 JSON Schema 校验——如实化 + 摘除未用依赖

- **状态**: 已落地（2026-08-03）
- **日期**: 2026-08-03
- **范围**: `cli/Cargo.toml` + `cli/crates/core/Cargo.toml`（摘依赖）+ `cli/crates/core/src/error.rs`（`ParseFailed` 建议文案 + `EnumIter`）+ 设计文档 `03`/`04`/`05`/`06`/`09`/`README`（如实化）+ 新增守卫 `cli/crates/cli/tests/design_error_codes.rs`。收口 STATUS 记账 TODO ③。

## 背景

`jsonschema = "0.18"` 在 `cli/Cargo.toml:36` + `cli/crates/core/Cargo.toml:21` **两处在册，源码零引用**（`rg jsonschema --type rust cli/` 零命中）。设计文档多处以「JSON Schema 校验」描述 `validate state` / `validate config` 的能力，而 `docs/design/schemas/` 下只有 4 个 `*.example.json`（填了值的数据样例，非 schema 定义），`validate_state` 实为手写判据。

STATUS 把修法记为二选一、**需先定方向**：⒜ 如实化 + 摘依赖；⒝ 补真实现（落地 schema 文件 + 校验器）。

## 决策

**采 ⒜ 如实化 + 摘除依赖。** 方向由实证定，不靠推断——关键问题是「JSON Schema 声称补的『字段类型层』，serde 是否已覆盖」。实测 6 类 state 文件损坏：

| 损坏注入 | CLI 反应 | JSON Schema 能否更好 |
|---|---|---|
| `source_loc` 数字 → 字符串 | `E010` + `invalid type: string "x", expected u64 at line 1 column 219` | 否——已精确到行列号 |
| 删 required 字段 `project.name` | `E010` + `missing field \`name\`` | 否 |
| `source_loc = -999`（负行数） | `E010` + `invalid value: integer \`-999\`, expected u64` | 否——`u64` 已表达非负 |
| `state = "martian"`（非法枚举） | `E010` + `unknown variant \`martian\`, expected one of \`init\`, \`profile\`, …` | 否——已枚举合法值 |
| 多余未知字段 `project.bogus_field` | 静默接受（未启用 `deny_unknown_fields`） | **是**，但这是 serde 配置选项，无需 schema |
| `name = ""`（空串） | 静默接受 | **是**（`minLength`），见下方「未采纳的收窄」 |

**结论**：serde 的类型化反序列化在**加载期**就覆盖了 schema 的字段类型 / required / 值域 / 枚举四项，且诊断质量不低于 schema 校验器（带行列号）。schema 只在「未知字段」「字符串 minLength」两处有增量，而这两处分别有更轻的解法（`deny_unknown_fields` / 手写判据）。补实现是净负收益：

- **成本侧**：该依赖默认启用远程 `$ref` 解析，摘除时连带移出 **80 个传递 crate**，含 `tokio` / `hyper` / `reqwest` / `url` 整套异步 HTTP 栈（`Cargo.lock` −855 行）。一个纯离线 CLI 挂着 HTTP 客户端栈，既是构建时间也是供应链攻击面。
- **收益侧**：schema 无法表达本项目真正在意的约束——`state_history` 链完整性（首条须 `init`、末条与当前状态一致、`exited_at` 时间链）、相邻状态转换合法性、各状态前置条件。这些是**状态机语义**，只能手写，而 `validate_state` 已实现。

## 实施

1. **摘依赖**：两处 `Cargo.toml` 删条目；`cli/Cargo.toml` 原「# Schema 校验」注释块改为「# 配置解析」并留下摘除理由指针。`cargo check` 通过、`cargo deny check` 四项全 ok。
2. **如实化设计文档**（10 处）：`06` 命令表行 118 / 目录树 166 / 依赖表 201 / 检查点模式 304 / L2 分级 378 / 文件通信 563 / L2 结构校验 571 / 产出物有效性 575；`04` 工具表 598；`05` 适配器贡献本地检查 192；`09` 附录 A/D 的「完整 JSON Schema 在 M1 阶段补充」（M1 早已过、从未补）与附录 E 的「工具化校验 JSON Schema」；`03` 附录 E 引用 234；`README` 把 `schemas/` 说成「JSON Schema 示例文件」订正为数据示例。
   - **保留不改**的两类：`06:889/896` 是 MDR-009 的历史沿革叙述（正确）；`08:123` 是未落地路线图的工作量估算（非现状声称）。
   - `09` 附录 E 的 9 属性契约**保留 JSON Schema 语法书写**（求精确），但明确标注「无程序化校验器消费」——实证确认 CLI 侧不解析 `{module}-intent.md`，检查由 verifier（LLM）按维度 9 逐字段核对。

## 连带修正（排查中撞出，比原记账严重）

原记账只说「06 表行声称 JSON Schema」。核实时发现同节还有三类失实，**都是编排器会照抄的具体值**——与 #86 修的 `--status` 值域漂移属同一失败模式。

### ① 三个 `VALIDATION_*` error_code 从不存在，而文档要求按它们分流（最严重）

`06 § 10.7` 的注要求：「SKILL.md 检查点须按 CLI 返回的 `error_code` 区分：若为 `VALIDATION_TIMEOUT` / `VALIDATION_OOM` / `VALIDATION_SCHEMA_CORRUPTED` 三者之一 → 工具故障，不进重试循环」。

实测三码在 CLI 源码**零命中**：`ValidationConfig` 是空结构（`[validation].timeout_secs` 配置项未落地）、无 L2 校验超时机制、无 schema 文件可损坏。**编排器照做则判据恒为假**，一切工具故障都被误判成产出物失效而进入无意义重试。

**判据重写为两级，均不写死码名**（故不随 M2 错误码细分失效）：

1. 能否解析出合法 error JSON。不能 → 工具故障（进程被信号杀死、超时无输出、stdout 非 JSON）。**实证**：`kill -9` 时 CLI 无任何输出，故「解析失败」是这类故障唯一可观测信号。
2. 能解析 → 读 `data.retryable`（CLI 已提供的权威字段）。`false` → 产出物真失效（进重试）；`true` → 环境侧瞬态故障（可重试但计入 `max_retries_per_step`）。

**初版判据被自查推翻**：我起初自造了一套「按具体码号归类」的分类（`E008`/`E010`/`E011` 归产出物失效、`E014` 归工具故障），实测发现 CLI 已有权威的 `retryable` 字段（`Timeout`/`IoError`/`DatabaseError` 为 true），且我把 `E014` 归错了方向（它 `retryable: true`，瞬态 IO 重试有意义）。改为引用现成字段——更简单，也不会随码域变化漂移。

### ② 整张错误码表 11 个码只有 3 个真实存在，且形态各不相同

表头声明这些是「CLI 失败时输出的 `error_code`」。实测：

- CLI 实际码是 `E001`–`E015` **十五个数字码**，且位于 `data.error_code`，非 § 10.7 示例所示的 **JSON 顶层**；示例里的 `error_context.{module, attempt_num, compiler_errors, suggested_fix}` 字段**整体不存在**（源码零命中），实际是 `data.{kind, error_code, message, retryable, suggestion}`。
- `ADAPTER_TOOL_MISSING` / `RUST_TOOL_MISSING` 确实出现，但形态是 **`warnings[]` 文本前缀**、`status` 降级 `warning` 而非 `error`；`install_hint` 的真实路径是 `data.tool_checks[].install_hint`（非 `error_context.install_hint`）——实测构造一个不存在的工具确认。
- `SCHEMA_VERSION_UNSUPPORTED` 仅存于源码 `TODO(M2-ERR-01)` 注释，该场景**实测返 `E008`**。
- 其余 6 个码名在 CLI 源码零命中（属 Plugin 提示词层语义标签或未落地设计）。

处置：新增「**CLI 实际错误码全表**」（15 行，含 `retryable` 列）作为编排器分流的权威依据；原表保留为「设计意图语义码表」并加「当前实际返回」列逐条如实标注。

**顺带实测出一处实现例外**：`graph topo-sort` 的循环依赖走 `ErrorData::new`（不带码），输出 `kind:"cyclic_dependency"` + `data.cycle_path` 但**无 `error_code` 字段**，退出码为 **2**（非 1）——即 `E002` 定义了却从不出现在输出里。已在表中如实标注，编排器判环须看 `data.cycle_path` 或 `kind`。

### ③ `BLOCKED_BY_VALIDATION_FAILED` 不只未落地，该校验本身缺失（真实功能缺口）

实测：`blocked_by` 引用不存在的模块名时，`validate state --check-blocked` 返 `status:warning` / `valid:true`，该模块只落入 `still_blocked` 且**永久无法解除**（无任何告警指出引用非法）。这正是该码本该防的场景。当时在 `06` 表行与 `09` 附录 B 检查点表如实标注「当前须人工核对」，并记为待办。

> **已收口（后续 PR）**：待办 1 已落地为告警式检出——`validate state` 检出幽灵引用即降级 warning 并点名「引用方 → 被引 key」，`--check-blocked` 另在 `data.ghost_refs` 给逐模块明细；`06`/`09` 两处「当前须人工核对」标注同步收回。详见下方「待办」段第 1 条。

### ④ `E010` 对 state 文件损坏给出误导性建议（代码改动）

`ParseFailed` 的两类真实来源是 `MigrateError::Parse`（源码语法）与 `Json`（state 文件），而 `suggestion()` 只拿到 `self`、区分不了来源。原文案「源码解析失败，请检查文件语法」会把 state 文件损坏的用户**指去查源码语法**。实测复现：改坏 `migration-state.json` 即得此建议。

改为同时覆盖两类来源，并排除两处会把用户引向死路的表述：

- **不提 `.rustmigrate.toml`**（初版文案提了，自查实测推翻）：`From<&MigrateError>` 虽把 `Toml`/`TomlSer` 也映射到 `ParseFailed`，但那两个变体除 `#[from]` 外**零构造点**，且全部三处 `toml::from_str`（`cli/src/lib.rs:1934/3513/3535`）都显式包成 `MigrateError::Config`——**配置损坏实测返 `E012`**，永不到 `E010`。已补 e2e `e2e_broken_config_returns_config_error_not_parse_failed` 钉住：若将来改成经 `?` 上抛，测试会红并提示同步文案。
- **不提「从备份恢复」**：实测澄清了完整行为——主文件 JSON 损坏且 `.migration-state.json.backup` 可用时，`load` **自动回退**并降级 warning（`已从 .backup 恢复`）；能走到 `E010` 说明备份不存在或同样不可用，建议用户去恢复是把他引向死路。

## 守卫

新增 `cli/crates/cli/tests/design_error_codes.rs`（**5 测试**），把「文档声明的错误码必须真实存在」变成 CI 硬门。真值域取自 `ErrorCode::iter()`（为此给枚举加 `strum::EnumIter`），不写死清单：

1. `design_06_declared_error_codes_all_exist_in_cli`——表里 `` `E0NN` `` 形态的码必须在真值域内（防笔误幽灵码）。
2. `all_cli_error_codes_are_documented_in_design_06`——每个真实码必须在 § 10.7 登记（防新增码不登记）。**该断言落地即抓到真实缺口**：`E002`/`E003`/`E004`/`E005`/`E006`/`E007`/`E009`/`E012`/`E013` **9 个真实码在 § 10.7 完全没登记**，编排器拿到时无从查证——这是补「实际码全表」的直接动因。
3. `retired_validation_codes_are_not_reintroduced_as_cli_returns`——三个已证实不存在的码不得再以「CLI 会返回」的表行形态出现（判据按**条目形态**而非全文出现，沿用 #86 做法，使订正说明本身可与守卫共存）。
4. `retryable_codes_match_design_06_table`——表里的 `retryable` 列与 `is_retryable()` 逐码一致（该字段是新判据的分流依据，漂移会让编排器反复重试不可重试的错误）。
5. `error_code_domain_has_expected_size`——冻结码数 15，防「加了码但断言 2 恰因散文里出现过该数字而通过」。

**Markdown 渲染语义**沿用 `design_command_table.rs` 的教训（#86 异构交叉实证的假绿路径）：代码块与 HTML 注释内的内容不算「读者看到的声明」。

**两处判据经自查变异收紧**（初版有假绿/假红，均已实证）：

- **断言 2 从「全文 contains」改为「须有表行」**：实测 § 10.7 节内 `E008` 出现 **4 次**（表行 1 次 + 散文 3 次），`E010`/`E011`/`E012`/`E014`/`E002` 亦各有散文提及。按全文匹配时删掉某码的表行仍会通过——**变异实证**：删 `E008` 表行后旧写法假绿、新写法报红并列出 `["E008"]`。
- **`extract_e_codes` 改为逐行配对**：原对整节做一次 `split('`')`，节内任一行出现**奇数个反引号**（中文引号误用、未闭合行内代码）就让此后配对全部错位。**变异实证**：插入一行含单个反引号 → 断言 1 假红（会把正常改文档的人拦在门外）；改逐行配对后同一变异 5 测试全过，而幽灵码变异（`E008`→`E080`）仍正确报红。

**节定位失效不静默**：`visible_section_10_7()` 对空正文硬断言。变异实证把 `## 10.7` 降级为 `### 10.7` → 报「未能在 06 中定位 § 10.7 节可见正文——标题格式可能已变，守卫失去作用」，而非静默放行。

## 可达性核查（主审视角发现，本 MDR 最重要的一轮修正）

**初版的「CLI 实际错误码全表」自身重演了它要消灭的失实模式。** 表头写着「`data.error_code` 的完整值域，编排器分流以此为准」，却把 3 个**当前不可能出现在任何输出里**的码列成普通可分流值——按它们写分支与按已废弃的 `VALIDATION_*` 写分支同样恒不命中，只是藏在一张「据实纠错」的新表内部。逐条实证：

| 码 | 不可达成因 | 实际替代 |
|---|---|---|
| `E002` `CyclicDependency` | 唯一构造点在 `graph/topo.rs` 的 `topological_sort`；其唯一非测试调用点（`graph topo-sort`）就地 match 消费该错误、改用 `ErrorData::new` 重构造 | 输出 `kind:"cyclic_dependency"` + `data.cycle_path`，**无 `error_code`**、退出码 **2** |
| `E003` `ModuleNotFound` | **`From<&MigrateError>` 中无分支映射到它**（无对应 `MigrateError` 变体） | 「模块不存在」实测返 **`E012`** |
| `E006` `ModuleBlocked` | 源变体 `MigrateError::Blocked` **零构造点**（只有 match arm） | 实测返 **`E012`** |

初版把 `E002` 写成「例外：`graph topo-sort` 不走本码」——这个框架本身误导，它暗示存在非例外路径，实际那是唯一路径。已改为「本码当前不可达」。

**`E010` 的来源清单初版一半失实、一半漏项**（同一视角发现）：

- **失实**：写了「源码语法错误」，而 `MigrateError::Parse` 的全部三个调用点（`graph/build.rs` 的 `analyze_file` 处）都把它**降级成 `warnings` 并跳过该文件**，绝不上抛——源码语法错误实测不产出本码。这句是用户拿到的**第一条建议**，恒指错方向。
- **漏项**：`validate rules --registry` 指向的 `rule-registry.json` 损坏同样经 `?` → `#[from] serde_json::Error` → E010（实证：`{"error_code":"E010","kind":"json"}`）。而初版建议的「无法修复则重新执行 `init`」对这条路径是**无效动作**——`init` 不生成该文件（它在 `plugin/skills/migrate/references/` 下）。已把 `init` 兜底收进 state 分句内部，并加断言钉住两句不得混。

**守卫扩到 7 条**，新增两条把可达性纳入 CI：

6. `unreachable_codes_are_marked_as_such_in_design_06`——表的「可达性」列须与 `UNREACHABLE_CODES` 常量一致（双向：标了但实际可达、可达但标了不可达，都报红）。
7. `codes_without_error_mapping_are_all_marked_unreachable`——**不依赖人工清单**的类型级补强：解析 `error.rs` 的 `impl From<&MigrateError> for ErrorCode` 块，任何未被映射的码必然不可达、必须在常量里。这条能抓「新增 ErrorCode 变体却忘接线 `From`」——该码会成死码，若同时被表登记为可达即是新幽灵码。

**负向实证 3 轮**：① 把 `E003` 标注改「可达」→ 断言 6 红；② 从常量摘掉 `E002` 但表仍标不可达 → 断言 6 红；③ **把表与常量同时改成自洽的错误状态**（`E003` 两处都改成可达）→ 前 6 条全绿、**仅断言 7 报红**，证明类型级判据提供了独立于人工维护的防线。

## 验证

**负向实证 5 轮**（逐条确认断言有区分力，非推断）：

| 变异 | 结果 |
|---|---|
| 表里 `E011` 的 retryable 改 `false` | 断言 4 红，归因「实现 = true，06 表行 = false」 |
| 码号写错 `E008` → `E080` | 断言 1 红，列出幽灵码 `["E080"]` |
| `VALIDATION_TIMEOUT` 恢复成正常表行 | 断言 3 红，「以未标注废弃的表行形态出现」 |
| 新增 `ErrorCode` 变体不登记文档 | 断言 2/4/5 三条同时红 |
| 实际码全表整体包进 `<!-- -->` | 断言 2/4 红（渲染语义处理生效，未被影子表骗过） |

`E010` 建议文案同样做了负向实证：还原旧文案后 `e2e_parse_failed_suggestion_covers_state_file_not_only_source` 立即红，报错信息复现原始症状（`建议须点明 state 文件这一路…: 源码解析失败，请检查文件语法`）。

**新增 e2e 测试 3 个**（守卫 5 个另计，共 +8）：文案守卫 + 两条对照面——`e2e_corrupt_state_with_backup_recovers_instead_of_parse_error`（实证「无备份才到 E010」这一前提）与 `e2e_broken_config_returns_config_error_not_parse_failed`（实证「配置损坏走 E012 不走 E010」，即文案不提 `.rustmigrate.toml` 的依据）。前两者带**前置假设断言**（先证 `.backup` 存在/不存在符合前提，机制变化时会红而非让断言静默失去意义）。

## 异构交叉审查（codex）推翻的两项——判据与守卫的能力边界

### ⓐ `data.retryable` 承担不了「错误归属」判定（双向反例，均实证）

上文「判据订正」的初版把第 2 级简化成读 `data.retryable`：`false` → 产出物失效、`true` → 环境瞬态。异构交叉审查双向推翻：

- **反例 A（`retryable:true` 但重试永无意义）**：`source-graph.db` 写入非 SQLite 内容后 `graph stats` **两次**均返 `E011` + `retryable:true`。这是产出物**持久损坏**，不重建数据库则重试次数用尽也不会成功，而判据会把它当环境瞬态故障反复重试。
- **反例 B（`retryable:false` 但产出物完好）**：`state update --cas-version 99` 返 `E007` + `retryable:false`，成因只是**调用方用了陈旧版本号**——判据却会判它「产出物真失效」并走 `state reset` 重做该步，那是对完好产出物的破坏性处置。

根因：`retryable` 表达的是「是否建议原样重试」这一**处置建议**，与「错误归属」是两个正交维度，一个 bool 承担不了。**已改为按 `error_code` 归入四类**（产出物失效 / 环境瞬态 / **调用方错误** / 工具能力缺失）——「调用方错误」这一类是初版二分完全缺失的，而它恰是编排器最常遇到的（CAS 陈旧、前置未满足、模块不存在）。

### ⓑ 守卫的可达性承诺过强（假绿实证）

06 表头初版写「可达性标注与实现不符**均**报红」。异构交叉实证该承诺不成立：断言 7 只解析 `From<&MigrateError>` 的映射文本，**证明不了源错误变体有可上抛的构造点**——「有映射但源变体零构造点」型死码（`E006` 正是此类：`MigrateError::Blocked` 有映射却零构造点）能让 7 条断言全绿。编排器复现确认：把 `E006` 从 `UNREACHABLE_CODES` 摘掉 + 表标「可达」后 **7 测试全过**。

（codex 另推断「注释里保留 `Self::XxxError` 字符串可绕过解析」——编排器实测该变异**被编译器穷尽性检查拦下**，`non-exhaustive patterns` 直接编译失败，故此路不通。但这不影响上面的核心论点。）

两项处置：

1. **收回过强承诺**：06 表头改为如实描述能力边界——「可达」侧由逐码命令级产出测试证明，「不可达」侧靠人工常量 + 一条**无法发现零构造点型死码**的类型级补强。
2. **「可达」侧改为真正的运行时实证**：新增 `e2e_codes_marked_reachable_actually_appear_in_output`，用真实命令产出逐个证明 **10 个**标「可达」的码确实出现在 `data.error_code` 里（`E013` 需睡满 60s 超时、`E015` 需配 C 语言路径，两者 codex 已实证但不适合进 CI，测试注释如实记录未覆盖及原因，不假装全覆盖）。

**codex 的两个 nit 亦成立、已修**（均核实构造点）：`E001` 覆盖图**操作**错误（`graph/persist.rs` 有构造点，实测 `graph deps <不存在节点>` 返本码），码名与原建议「重试 graph build」在查询期指错方向；`E007` 的「另有迁移进程运行」是过度归因——唯一构造点是 CAS 版本检查，正确处置是重读版本号而非等锁。两处 `suggestion()` 与表行同步订正。

**codex 实证确认无问题的项**：11 个标「可达」的码全部可由现有二进制产出（逐一给出复现场景）；`jsonschema` 摘除无隐式使用遗漏（`cargo metadata` 无 feature 引用、无 `build.rs`、无 cfg/doctest 条件引用）。

## 变更性质登记

按 MDR-019/020 先例逐项判定。**MDR 主体（摘依赖 + 如实化）无破坏性变更**；后续收口待办 1 时引入**一项源码破坏性变更**（见本段末）：

- **摘除 `jsonschema` 依赖**：该 crate 源码零引用，无 `pub` API 依赖它、无类型出现在任何签名里，故对下游 Rust 调用者与 CLI 的 JSON 契约均零影响。`Cargo.lock` 变动不构成 API 变更（0.x 阶段 + 无 `cargo publish` 流程）。
- **`ParseFailed` 的 `suggestion` 文案变化**：`suggestion` 是**用户可见文本，非机读契约**——`plugin/` 下对该键零命中（无提示词或脚本按其内容分支），CLI 侧亦无消费方。文案纠错不改结构、不改字段名、不改 `error_code` 值域。故不按 `--status` 值域那类（#86，值被 clap 解析期强校验、编排器照抄）登记为破坏性变更。
- **`ErrorCode` 加 `strum::EnumIter`**：纯派生宏新增，不改 serde 表示（`rename_all = "snake_case"` 未动）、不改变体集合与码号，对外部零影响。
- **设计文档措辞如实化**：不改任何契约值，只让描述与实现一致。表格新增「可达性」列属信息补充，未删任何既有行。
- **收口待办 1 的新增项均为纯新增，无破坏性变更**：`GhostReference` 类型、`scan_ghost_references` 函数、CLI 输出的 `data.ghost_refs` 键都是新增，不改既有签名与 JSON 键。
  - 过程记录：中途曾给 `pub struct BlockedCheckResult` 加过 `missing` 字段（无 `#[non_exhaustive]`，对外部 struct-literal 构造是源码破坏性变更，一度按 MDR-020 先例登记于此）。后经专项审查指出该字段在 CLI 改用 `scan_ghost_references` 后**已无生产消费方**，遂**移除**——它与 `GhostReference` 是同一概念的两份表示，正是「`ghost_refs` 两处各算一遍导致口径不一致」那个 important 的同类风险来源。移除后破坏性变更归零，概念只剩一处表示。

## 未采纳

- **补真 schema 实现（方向 ⒝）**：见「决策」段实证——serde 已覆盖 schema 能管的层，增量仅「未知字段」「minLength」两处，而代价是 80 个传递 crate。
- **启用 `deny_unknown_fields`**：能堵住「多余未知字段静默接受」，但会**破坏向前兼容**——旧版 CLI 读新版写的 state 文件会硬失败，与 `schema_version` 主版本兼容策略（跨主版本才拒绝）冲突。当前静默忽略未知字段是有意的宽松。
- **`name` 空串等 minLength 类收窄**：实测空串 `project.name` 被静默接受、`validate state` 返 `valid:true`。属独立的值域校验诉求（可在 `validate_state` 手写，同 `--status` 值域扫描先例），与本 MDR 的「摘依赖 + 如实化」正交，记为待办而非借机夹带。

## 待办（本次核实出、判超范围）

1. ~~**`blocked_by` 幽灵引用无检出**~~ **✅ 已收口**（后续 PR）——落地方式与原修法方向一致（`validate_state` 加扫描 + 告警，不硬判损坏）。三点落地细化，均非原记账所能预见：
   - **扫描覆盖全部模块而非仅 blocked**。正常路径下 `transition_module` 离开 blocked 会清空 `blocked_by`（`machine.rs:637`），但手工编辑或旧文件可能在非 blocked 模块上留残值，该模块再被标 blocked 时会立刻踩中同一个坑。`check_blocked_modules` 只看 blocked 模块，这条只能由 `validate_state` 兜住。
   - **幽灵引用由 `scan_ghost_references` 单独提供**（`BlockedCheckResult` 不再单列）。原实现用 `.unwrap_or(false)` 把「依赖不存在」与「依赖未终态」抹平进同一个 `unresolved`，而两者处置动作**相反**：一个要重新同步 state，一个只需等待。幽灵引用**仍计入** `unresolved` 是有意的——否则它会让模块判为「就绪可解除」，`--auto-unblock` 就会在损坏数据上真的改状态；已有专门测试钉死这条性质。
   - **命名冲突需留意**：`state deps` 的 `unresolved` 指的正是幽灵引用，而 `BlockedCheckResult.unresolved` 指「未进终态的依赖」（含幽灵）。同词异义，故 `--check-blocked` 侧的输出键取名 `ghost_refs` 而非复用 `unresolved`，避免两个命令的同名字段语义相反。
   - **负向实证八轮**（独立 worktree，非推断，全部报红且归因准确）：① `missing` 恒空（退回二分）→ 2 core + 1 e2e 报红；② 摘掉告警扫描 → 2 core 报红；③ `missing` 不计入 `unresolved`（幽灵变可就绪）→ 3 core 报红，归因精确到「带幽灵引用的模块不得判为就绪」；④ 判据反转（合法引用当幽灵）→ 3 core 报红，其中「反向不误报」由独立测试钉住；⑤ 摘掉 `member_files` 归一 → 2 个 composite 测试报红；⑥ 环检测退回不归一 → 互锁测试报红；⑦ 歧义退回静默择一（按首个宿主判定）→ 坏划分测试报红；⑧ 摘掉排序与去重 → 3 个测试报红。
   - **归一修复自身又引入两处问题，均由主审实证抓出后修**（记此以备后来者：给判据加归一时，**所有共用该判据的路径必须同步**，否则会制造比原问题更隐蔽的盲区）：
     - **环检测未跟随归一 → 经成员 key 表达的互锁完全静默**。`check_blocked_modules` 归一了、`detect_blocked_cycles` 仍按原始字符串建边，而成员 key 不在 `blocked_set` 里 → 边被丢弃。实测 `shared blocked_by handler`（emitter 组成员）+ `emitter blocked_by shared` 构成真实死锁，输出 `cycles:[]` + `ghost_refs:[]` + `status:ok` **零诊断**——归一前这至少还会报幽灵告警，是实打实的回归。
     - **坏划分让 `--auto-unblock` 真的改状态**。同一文件被多组列为成员时初版静默取字典序最小宿主；实测 X 同属 done 组与 translating 组 → 取到 done 那组 → 判 ready → 模块被解除并落盘（`blocked→pending`、`blocked_by` 清空），全程零告警。这与本次守卫声称的「不在损坏数据上改状态」直接相反，只是入口换成坏划分。`machine.rs` 的 `canonical_module_key` 对同一不变量是 **release 硬错**（MDR-015:55），此处策略相反。修法：归一改三态返回（`Resolved`/`Ambiguous`/`Missing`），歧义一律按非终态处理（不判 ready、也不当幽灵——处置动作是修组划分而非重新同步），`validate_state` 补跨组不变量告警（校验命令不能硬错但绝不能沉默）。
   - **告警建议曾把用户领进死路（主审 imp，已修）**：初版建议「重新 `graph build` + `state populate-modules`」，但 populate 对**非 pending** 模块一律拒绝重填（断点续传保护），而幽灵引用按定义就发生在 `blocked` 等非 pending 态 → 照做**确定性失败**；补的 `state reset` 前置同样无效（reset 置为 `translating`，仍非 pending）。这是 MDR-020 finding 5 修过的同一失败模式。真正可执行的是 `state transition --module <M> --to <pre_blocked_status>`——离开 blocked 即清空 `blocked_by`、不丢进度、一步到位（实测有效）。根因是**e2e 只验告警文本含哪些 key、没验建议照做能成功**，已补 `smoke_ghost_reference_advice_actually_works` 端到端钉住「照做后问题真的消失」，并在幽灵态下反证 populate 路径确实被拒。
   - **未采纳（主审 nit）**：`resolve_blocked_ref` 每条引用做全表扫描，建议改建反向索引。判为不改——codex 实测 10 万模块 1.11s、近线性（`contains_key` 是哈希查找，非 O(n²)），真实规模是百级；引入索引会多一份需与 `member_files` 保持同步的中间状态，收益不抵复杂度。若将来模块数量级变化再议。
2. **`E002` 定义了但从不出现在输出里**（连带修正 ② 末）——`graph topo-sort` 走 `ErrorData::new`。要么改用 `with_error_code(CyclicDependency, …)` 使码域自洽，要么把 `CyclicDependency` 从枚举摘掉；两者都会动 CLI 输出契约（前者新增字段、后者改码域），需独立 PR。
3. **`transition_inner` 不校验 `blocked_by` 是否终态**（收口待办 1 时由异构交叉审查实证，**pre-existing**）——`state transition --to <pre_blocked_status>` 与 `state update --cas-version` 共用该路径，离开 `blocked` 只校验 `target == pre_blocked_status`，**不带 `--force` 即可解除阻塞**并清空 `blocked_by`；`--auto-unblock` 侧的「依赖须全终态」不变量在这条路径上不成立。对幽灵引用而言，意味着「不在损坏数据上改状态」只由 `--auto-unblock` 保证，普通 transition 仍可绕过。
   **判为不在待办 1 范围内**：该行为对所有 blocked 模块一视同仁（非幽灵专有），本 PR 之前即如此；收窄它等于给 `blocked → pre_blocked_status` 这条边加新前置条件，会改变既有转换语义并影响 `run.md` 步骤 2 的自动解除流程，须独立评估。修法方向：把不变量下沉进 `transition_inner`，使 transition / CAS / auto-unblock 三条路径共享；人工逃生改走显式 `--force` 或 repair 语义。已在 `check_blocked_modules` 的相关测试注释中如实限定承诺范围，不为该路径背书。
4. **`project.name` 等字符串字段空值无校验**（见「未采纳」）。
5. **`ValidationConfig` 空结构 vs `[validation]` 配置段**——06 § 11.1 的 `timeout_secs = 30` 已注释并标注未落地。若 M2 真要做校验超时，须同时落配置字段与超时机制，否则应把该段从配置样例中彻底删去。
