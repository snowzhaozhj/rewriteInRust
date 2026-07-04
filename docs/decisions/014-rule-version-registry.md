# MDR-014: 规则版本权威清单 + `validate rules` 命令（M4-GOV-01）

- **状态**: 已决策
- **日期**: 2026-07-05
- **范围**: M4 Sprint F GOV-01 —— 新增权威规则版本清单 `rule-registry.json` + CLI `validate rules` 子命令，落地设计已留的 `[rules].enforce_rule_version_consistency` 开关。改 `cli/`（core 新模块 + CLI 命令）、`plugin/`（清单文件）、`docs/`。

## 背景

各语言适配器 `porting-template.md` 的 frontmatter 已有 `rule_version` 字段（`06 § 11.2.1` / R3-D7-03），记录该模板依据的核心规则版本（如 `RULE-3:v2.0.0`）。核心规则破坏性升级后模板漏同步 bump 即产生「陈旧」漂移——新旧规则版本混用会打破 `05 § 6.2`「项目专有规则优先」约束。此前**无程序化检测**：三个模板（go/python/typescript）的 `rule_version` 是否与当前核心规则一致，全靠人工。

PLAN-M4 GOV-01 要求：CLI 校验各 `porting-template.md` frontmatter `rule_version` vs **权威清单**一致性；落地 `[rules].enforce_rule_version_consistency` 开关；不一致返回明确错误（复用 `profile/tools.rs` JSON 框架）；**砍 index.json 自动生成**（YAGNI）。

## 决策

### 决策 1：权威清单 = 手工维护的小型 `rule-registry.json`，非自动聚合

新增 `plugin/skills/migrate/references/rule-registry.json`（`{schema_version, rules: {RULE-N: 版本}}`），作为核心规则当前版本的**单一真相源**。

- **为何需要一个真相源**：`rule_version` 此前只在三个模板各自声明（当前恰好一致）。仅做「跨模板互相一致」无法发现「三模板一起漂移偏离核心规则」。设计（`06 § 890`）明确要求比对**当前核心规则版本**，故需一个独立的权威版本表。
- **为何不违反「砍 index.json」**：被砍的 `index.json` 是**自动生成的模块数据聚合**（投机数据模型、3 门语言规模未到回本点）。本清单是**手工维护的治理清单**（当前 9 条核心规则），直接支撑 GOV-01 一致性校验，非投机。规则作者变更某规则版本时同步 bump 清单——这正是治理动作本身。
- **放 `references/`**：与已有 `patterns/`/`anti-patterns/` 同级，属跨适配器的核心规则资产（非某语言 adapter 私有），不放进 `adapters/<lang>/`（那是 MDR-009 两文件契约，只含 `analysis-tools.json` + `porting-template.md`）。

### 决策 2：新增 CLI 子命令 `validate rules`，不复用 `validate/rules.rs`

命令挂在 `validate` 族下：`validate rules --registry <json> --adapters-dir <dir>`。核心逻辑落**新模块** `core/src/rule_version.rs`（`load_rule_registry` / `parse_template_rule_version` / `check_template_consistency` / `check_adapters_dir`）。

- **为何不放 `validate/rules.rs`**：该文件已存在，语义是 **CI 验证管线 tier**（cargo check/clippy/nextest 分层），与「porting 规则版本治理」是不同域，同名易混。用独立顶层模块 `rule_version` 避免概念污染。
- **路径显式传参（不内建定位）**：`--registry` / `--adapters-dir` 由调用方（plugin SKILL/hooks）传插件相对路径，与既有 `profile --adapter-tools <json>` 显式传适配器资产路径的模式一致。CLI 运行在**目标迁移项目**目录，插件资产不在目标项目内，故必须外部传入。
- **命令清单归属**：设计 `05 § 6.2` 曾把确定性版本校验列为 M2「备选」命令 `validate rules --check-module-versions`（未纳入 M2 5 命令）。GOV-01 由 PLAN-M4 显式授权正式落地为 `validate rules`（scope 收窄为**适配器模板级**校验，非模块级），已同步 `06 § 10.0.1` 命令清单。

### 决策 3：一致性 = 严格双向（缺失 / 版本不符 / 未知规则）

`check_template_consistency` 对每个模板产出三类 issue：
- `missing_in_template`：清单有该规则，模板未声明（模板覆盖缺失）；
- `version_mismatch`：模板声明版本 ≠ 清单版本（陈旧或超前）；
- `unknown_rule`：模板声明了清单外的规则（typo 或清单遗漏）。

要求模板**全覆盖**清单规则（当前三模板均声明全部 9 条）。若未来某语言合理地不覆盖某规则，届时调整（放宽为「声明的须一致」）。遍历按 `BTreeMap` 字典序 → 输出确定性。

### 决策 4：严重度由 `[rules].enforce_rule_version_consistency` 控制（默认 true）

`RulesConfig`（原空结构体）新增 `enforce_rule_version_consistency: bool`，自定义 `Default` 为 `true`（对齐 `06 § 11.1` 缺省）。

- `true`（默认）：任一模板不一致 → `MigrateError::SchemaValidation` → `status=error`、退出码 1（**非静默**，满足 GOV-01 验收）；
- `false`：降级为 warning（`status=warning`、退出码 0），逐条 issue 仍落 `data.checks[].issues` 供人读。

开关从**目标项目根** `.rustmigrate.toml` 读取（`load_config_or_default`）；无配置 → 默认 true。`version_tracking` / `auto_regenerate_on_rule_upgrade`（设计 `[rules]` 示例中另两字段）**本 PR 未实现**——`RulesConfig` 未 `deny_unknown_fields`，用户配置含它们时被静默忽略（前向兼容），留待后续任务（verifier 侧运行时陈旧提示）。

## 影响

- **新增**：`plugin/skills/migrate/references/rule-registry.json`；`core/src/rule_version.rs`（9 单测）；`validate rules` CLI 命令 + `cmd_validate_rules`（4 cli_e2e：真实模板一致回归守卫 / enforce=true 报错 / enforce=false 降级 / 清单缺失报错）。
- **改**：`RulesConfig` 加字段（默认 true）；`06 § 10.0.1` 命令清单 + `§ 11.1` `[rules]` 注释同步。
- **回归守卫**：`e2e_validate_rules_shipped_templates_consistent` 断言随发布的三模板与清单一致——任一处 bump 漏同步会红。

## 后续 TODO（记账，非阻塞）

1. **verifier 侧运行时陈旧提示**（`05 § 6.2` 第 4 项）：`/migrate analyze`/`run` 时比对项目模块的 Porting Version 与当前规则，标 `rule_stale` substatus——本 PR 只做**适配器模板级**静态校验，模块级运行时校验留后续。
2. **`version_tracking` / `auto_regenerate_on_rule_upgrade` 字段落地**：当前仅 `enforce_rule_version_consistency` 实现，另两字段设计已定义但未接线。
3. **规则版本变更 changelog**：清单 bump 时同步 `porting/changelog.md`（`05 § 6.2`）——当前清单是 v1.0.0 全量基线，尚无版本变更历史。
