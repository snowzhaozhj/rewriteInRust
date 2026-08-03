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

实测：`blocked_by` 引用不存在的模块名时，`validate state --check-blocked` 返 `status:warning` / `valid:true`，该模块只落入 `still_blocked` 且**永久无法解除**（无任何告警指出引用非法）。这正是该码本该防的场景。已在 `06` 表行与 `09` 附录 B 检查点表如实标注「当前须人工核对」，并记为待办（见文末）。

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

## 未采纳

- **补真 schema 实现（方向 ⒝）**：见「决策」段实证——serde 已覆盖 schema 能管的层，增量仅「未知字段」「minLength」两处，而代价是 80 个传递 crate。
- **启用 `deny_unknown_fields`**：能堵住「多余未知字段静默接受」，但会**破坏向前兼容**——旧版 CLI 读新版写的 state 文件会硬失败，与 `schema_version` 主版本兼容策略（跨主版本才拒绝）冲突。当前静默忽略未知字段是有意的宽松。
- **`name` 空串等 minLength 类收窄**：实测空串 `project.name` 被静默接受、`validate state` 返 `valid:true`。属独立的值域校验诉求（可在 `validate_state` 手写，同 `--status` 值域扫描先例），与本 MDR 的「摘依赖 + 如实化」正交，记为待办而非借机夹带。

## 待办（本次核实出、判超范围）

1. **`blocked_by` 幽灵引用无检出**（连带修正 ③）——真实功能缺口：模块永久 `still_blocked` 而 `valid:true`。修法方向：`validate_state` 的 blocked 检查段增加「`blocked_by` 各项须存在于 `modules` 键中」扫描，不存在则告警（沿用 MDR-019「防御性可观测」惯例，不硬判损坏——旧文件须可读）。
2. **`E002` 定义了但从不出现在输出里**（连带修正 ② 末）——`graph topo-sort` 走 `ErrorData::new`。要么改用 `with_error_code(CyclicDependency, …)` 使码域自洽，要么把 `CyclicDependency` 从枚举摘掉；两者都会动 CLI 输出契约（前者新增字段、后者改码域），需独立 PR。
3. **`project.name` 等字符串字段空值无校验**（见「未采纳」）。
4. **`ValidationConfig` 空结构 vs `[validation]` 配置段**——06 § 11.1 的 `timeout_secs = 30` 已注释并标注未落地。若 M2 真要做校验超时，须同时落配置字段与超时机制，否则应把该段从配置样例中彻底删去。
