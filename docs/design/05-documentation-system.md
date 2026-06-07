> [返回主索引](./README.md)

# 六、文档与知识沉淀体系

## 6.1 核心产出物总览

| 产出物 | 用途 | 生成方式 | 生命周期 |
|--------|------|---------|---------|
| PORTING 规则体系 | 迁移规则宪法（通用 + 项目专有） | AI 初版 + 人工审查 + Sprint 迭代 | 长期保留 |
| PARITY.md | 迁移进度跟踪 | 自动更新 | 迁移完成后归档 |
| KNOWN_DIFFERENCES.md | 已知行为差异登记 | verifier 即时写入 + 人工确认 | 长期保留 |
| AGENTS.md | AI 行为约束（含反合理化表） | 模板 + 项目定制 | 迁移完成后可丢弃 |
| MDR（迁移决策记录） | 架构决策溯源 | 决策发生时立即记录 | 长期保留 |
| SPRINT_LEARNINGS.md | Sprint 级知识总结 | Sprint Review 时追加 | 长期保留 |
| DESIGN_ASSUMPTIONS.md | M0 假设验证报告 | M0 假设验证周产出 | 长期保留 |
| patterns/ | 翻译模式库（成功经验复用） | 模块完成时提取 | 长期保留 |
| anti-patterns/ | 失败经验库 | 模块失败时记录 | 长期保留 |
| migration-state.json | 状态机 + Sprint 元数据 | 自动管理 | 迁移完成后归档 |
| test-fixtures/ | 行为录制测试集 | 自动录制 | 长期保留（回归测试） |

### 格式选型原则

> 完整的格式约定表见 [README.md > 存储格式约定](./README.md#存储格式约定)。

知识/模式文件的 YAML frontmatter 格式示例：

```markdown
---
id: async-to-tokio
category: pattern
language: typescript
tags: [async, tokio, concurrency]
created: 2026-06-10
sprint: S2
confidence: high
---

# Async/Await 到 Tokio 的翻译模式

...正文...
```

---

## 6.2 迁移规则体系（通用 + 项目专有）

参考 Bun 和 Claw-Code 的迁移实践（Bun 将迁移规则融入 CLAUDE.md 而非独立 PORTING.md；Claw-Code 使用 PARITY.md + Mock Parity Harness 做行为验证），定义所有翻译规则。**渐进式生成**：PLAN 阶段生成必须的核心规则，后续 Sprint 按需追加。**确定性 vs AI 边界**：约 60% 的规则（类型映射、命名转换、标准库映射等）可通过确定性模板生成，仅语义复杂的规则（错误处理策略、并发模式选择等）交给 AI。

### 规则分层

拆分为三层（方案 D）：

**核心规则**（嵌入 agents/*.md，随 Plugin 分发）：
- 位置：`agents/*.md`（各 SubAgent 系统提示中直接包含）
- 内容：SubAgent 必须遵守的硬性翻译规则（类型映射、错误处理模式、命名约定、禁止模式等，约 60% 的确定性模板规则）
- 理由：嵌入 agent 系统提示可确保规则始终在上下文中，无需额外加载步骤

**参考指南**（随 Plugin 分发，按需加载）：
- 位置：`skills/migrate/references/` 目录
- 内容：跨项目通用的翻译模式、反模式、最佳实践、详细参考文档
- 格式：YAML frontmatter + Markdown（便于语义检索）
- 加载方式：SKILL.md 按需 Read（不自动注入，避免占用上下文预算）
- 示例结构：
  ```
  skills/migrate/references/
  ├── patterns/
  │   ├── async-to-tokio.md
  │   └── error-handling-migration.md
  ├── anti-patterns/
  │   ├── naive-mutex-wrap.md
  │   └── arc-everything.md
  └── type-mapping-details.md
  ```

**项目专有规则**（用户项目本地）：
- 位置：`.rust-migration/porting/` 目录
- 内容：项目特有的规则（外部依赖映射、业务逻辑处理策略、特殊模式等）
- 格式：Markdown 文件，按规则类分文件
- 保留 Changelog 机制
- 示例结构：
  ```
  .rust-migration/porting/
  ├── dependency-mapping.md       # 项目特有的依赖映射
  ├── business-logic-rules.md     # 业务逻辑翻译策略
  ├── known-workarounds.md        # 项目特有的 workaround
  └── changelog.md                # 规则变更记录
  ```

- 50KB 以下可全量加载（约 12K tokens），但多语言场景必须拆分为按语言独立的规则文件

### 26 类规则一览

| # | 规则类 | 层级 | MVP? | 说明 |
|---|--------|------|------|------|
| 1 | 迁移阶段定义 | 项目专有 | 是 | Sprint 目标和模块优先级 |
| 2 | 类型映射表 | 通用 | 是 | 源类型 → Rust 类型 |
| 3 | 错误处理模式 | 通用 | 是 | try/catch → Result<T,E>，anyhow vs thiserror |
| 4 | 内存管理/分配器策略 | 通用 | 是 | GC → 所有权模型 |
| 5 | 指针/引用映射 | 通用 | 是 | 指针 → 引用/智能指针 |
| 6 | 并发模式 | 通用 | 否 | 锁/通道/异步映射 |
| 7 | 字符串处理 | 通用 | 是 | UTF-8/UTF-16 差异 |
| 8 | 命名约定转换 | 通用 | 是 | camelCase → snake_case |
| 9 | 模块/Crate 结构 | 项目专有 | 是 | 包结构 → Cargo workspace |
| 10 | 标准库函数映射 | 通用 | 是 | 常用函数对照表 |
| 11 | 禁止模式清单 | 通用 | 是 | 禁止的反模式 |
| 12 | unsafe 使用策略 | 通用 | 是 | 何时允许、如何标注 |
| 13 | 外部依赖映射 | 项目专有 | 是 | npm/pip 包 → Rust crate |
| 14 | FFI 边界规则 | 项目专有 | 否 | 桥接层设计规范 |
| 15 | 全局状态处理 | 通用 | 是 | 全局变量 → OnceLock/lazy_static |
| 16 | 调度/热路径规则 | 项目专有 | 否 | 性能敏感路径的特殊处理 |
| 17 | 测试模式翻译 | 通用 | 是 | 测试框架映射 |
| 18 | 构建系统规则 | 项目专有 | 是 | package.json/setup.py → Cargo.toml |
| 19 | 惯用法映射表 | 通用 | 是 | 源语言惯用法 → Rust 惯用法 |
| 20 | 不确定性处理 | 通用 | 是 | 留 TODO，禁止猜测 |
| 21 | **生命周期与所有权模式** | 通用 | 否 | 引用生命周期、借用模式映射 |
| 22 | **异步运行时与并发原语** | 通用 | 否 | tokio/async-std 选择、Future 映射、取消安全性审查 |
| 23 | **序列化/反序列化兼容性** | 项目专有 | 否 | JSON/protobuf 字节级兼容 |
| 24 | **日志/可观测性映射** | 项目专有 | 否 | 日志框架 → tracing，格式兼容 |
| 25 | **平台特定行为映射** | 项目专有 | 否 | OS API → cfg 条件编译 |
| 26 | **多态/动态分发映射** | 通用 | 否 | 接口/继承/泛型 → trait/enum dispatch/泛型 |

**版本化演化**：每条规则有版本号和变更记录，每个 Sprint Review 可以修改规则，但必须记录变更原因。详见下方「规则版本管理与代码一致性」。

#### 规则版本管理与代码一致性

**规则格式与版本号**：每条规则以 `## RULE-NN: <名称> (v<N>)` 形式标注，版本号采用 `major.minor.patch`：

- `patch`：措辞/示例补充，不改变翻译结果（不触发回溯）
- `minor`：新增约束或放宽限制，向后兼容（不触发回溯，但新模块按新版翻译）
- `major`（breaking change）：与既有产出代码可能矛盾（如"禁止使用某 API"），**仅在 Plugin 主版本更新时引入**，并在 KNOWN_DIFFERENCES.md 中列出受影响范围与升级步骤

每次变更在 `porting/changelog.md` 记录：变更时间、原因、影响范围、向后兼容性（兼容/breaking）。

**冲突仲裁**：若同一规则类在通用层（核心规则/参考指南）与项目专有层（`porting/`）均定义，**项目专有规则优先适用**。

**代码回溯机制**：当 breaking change 引入与已生成代码矛盾的规则时，触发回溯检查流程：

1. **模块级版本清单**：每个模块在其 `module-learnings/` 旁生成 `_porting_manifest.json`，记录该模块翻译时各函数依据的 `RULE-NN:vX.Y.Z`（机器可读，避免靠静态分析推断"哪些模块用 v1 翻译"）。PARITY.md「模块详情」表的 **Porting Version** 列是该清单的人读摘要（如 `v1 of RULE-2,3,5`）。函数级溯源由 PORT NOTE 注释承载：`// PORT NOTE: fn handle_error [RULE-3:v2.0.0, RULE-11:v1.5.0]`。
2. **冲突检测（verifier 侧）**：verifier 系统提示新增条款——应用任一规则前先比对 `_porting_manifest.json`：若文件以 `RULE-3:v1.0.0` 翻译而当前为 `RULE-3:v2.0.0`（breaking），在 MDR 标记决策「复审 or 重译」，确保不混用相互矛盾的规则版本。
3. **核心规则版本对齐**：`agents/*.md` 内嵌的核心规则在文件头 frontmatter 标注 `version` 与 `breaking_changes`，与 `references/` 的 YAML frontmatter 一致；Plugin 升级时参考指南随核心规则的 major 同步（避免参考指南落后于核心规则的 breaking change）。
4. `/migrate review` 比对当前规则版本与各模块清单，发现不一致时将模块标记 `rule_version_stale`（migration-state.json substatus），输出待复审列表（落入 `reports/sprint-N-report.json`）；用户可经 `/migrate run --module=X --rule-upgrade-review-only` 选择性复审（而非整体重译）。
5. 关键规则（如 RULE-11 禁止模式、RULE-12 unsafe 策略）发生 breaking change 时，在 migration-state.json 记录 `affected_modules` 字段，供精准回溯。

> **CLI 边界与配置**：上述比对属确定性计算。MVP（< 50 模块）由 `/migrate review` 的 SubAgent 完成，不新增 CLI 子命令以免突破 13+5 命令清单（命令权威以 06 为准）；M2 提出确定性命令候选 `rustmigrate validate rules --check-module-versions`（解析所有 `_porting_manifest.json` 比对 `porting/changelog.md`）。**`validate rules` 为 M2「备选」，不在 [06 § 10.0.1 表](./06-plugin-structure.md#1001-cli-工具架构rustmigrate) 当前已定的 5 个 M2 扩展命令之内；是否纳入及计入方式由 M2 规划确定**（命令清单权威以 06 为准）。版本追踪开关与升级行为由 `.rustmigrate.toml` `[rules]` 段控制（`version_tracking` / `auto_regenerate_on_rule_upgrade` / `enforce_rule_version_consistency`，定义见 [06 § 11.1](./06-plugin-structure.md#111-rustmigratetoml-配置文件)）。

**行动指南**：
- MVP 阶段只生成标记"是"的规则
- 每次翻译失败且原因是规则缺失时，追加新规则并标注"由 Sprint N 失败触发"
- 规则格式：`## RULE-NN: <名称> (v<N>)` + `源语言模式 → Rust 等价物 + 注意事项 + 示例`

**跨版本冻结规则的升级检测**：项目暂停迁移（如 M1 收尾）时，在 `.rust-migration/.rule-freeze-metadata.json`（专用文件，不混入 changelog.md）记录规则快照：

```json
{
  "frozen_at_plugin_version": "0.1.0",
  "frozen_timestamp": "ISO8601",
  "core_rules_snapshot": { "RULE-3": "v1.0.0", "RULE-11": "v1.2.0" }
}
```

日后 `/migrate analyze` 运行时（如 Plugin 升级到 0.2.0）须：① 检测该文件存在；② 比对 `core_rules_snapshot` 与 `agents/*.md` 当前核心规则版本；③ 若任一规则发生 `major` 升级，输出告警，例：「检测到项目规则在 Plugin v0.1.0 冻结。当前 v0.2.0 中 RULE-3 升级至 v2.0.0 (breaking)。请复审 `.rust-migration/porting/business-logic-rules.md` 中 RULE-3 的定制版本，确认与新版 breaking change 兼容。详见 KNOWN_DIFFERENCES.md KD-NNN。」此机制让陈旧的项目专有规则在升级后显式可见，不引入 `.rustmigrate.toml` 新配置。

**规则维护责任与社区贡献**：本设计中「Plugin 维护者」「Plugin Lead」「核心维护者」指同一角色池（项目核心 committers）；「技术委员会」与「社区维护委员会」为同一治理机构的不同称谓，具体组成与选举规则在项目 GOVERNANCE.md 定义。规则库三层结构的维护职责与更新周期如下，社区贡献的本地检查要点详见本章下方「社区贡献快速参考」。

| 层级 | 维护责任 | 更新周期 | 外部对齐 |
|------|---------|---------|---------|
| 核心规则（`agents/*.md`） | Plugin 维护者，社区 PR | 每个 M 阶段 review 一次 | 与 Rust edition、clippy lint 集版本对齐 |
| 参考指南（`references/`） | Plugin 维护者 + 社区贡献 | 每个 M 阶段 review 一次 | 与所引用 crate 的主版本对齐 |
| 项目专有（`porting/`） | 迁移项目团队 | 每个 Sprint Review | 与项目依赖映射同步 |

#### 社区贡献快速参考

三类可贡献产物（规则 / 适配器 / patterns）的本地检查要点如下，让贡献者在提 PR 前能自检；提交流程与 PR 模板详见 Plugin 仓库 `CONTRIBUTING.md`。

**规则贡献**（核心规则 / 参考指南 / 项目专有）：
- 何时提交：翻译失败因规则缺失而触发（标注「由 Sprint N 失败触发」），或现有规则在新场景下不适用。
- 本地检查：规则格式为 `## RULE-NN: <名称> (v<N>)`，并附 frontmatter 元数据（`category` + `target_languages` + `ts_only` 为强制项）；版本号与变更同步写入 `porting/changelog.md`；涉及 breaking change（major）须在 KNOWN_DIFFERENCES.md 列出受影响范围（见上方「规则版本管理与代码一致性」）。规则分类元数据 frontmatter 约定（权威定义）：
  ```yaml
  ---
  id: RULE-N
  category: [TypeMapping|LanguageSemantics|ProjectPolicy|NamingConvention]
  target_languages: [ts, py, c]   # 适用的源语言集合
  source: [§9.2陷阱序号 | §7.7探测维度 | 实战发现]
  ts_only: false                  # true=仅 TS→Rust 有效（不可跨语言复用）
  ---
  ```
- 评审周期：每个 M 阶段 review 一次（核心/参考），项目专有每个 Sprint Review。
- PR 评审承诺：核心规则与参考指南的 PR 须在提交后 14 日内获得初审反馈；超期自动 escalate 至技术委员会（设计原则：给出预期反馈窗口而非精确到日，保留灵活性）。

**适配器贡献**（新语言）：
- 工作量预期：3-5 个工作日（来源 [06 § 11.2 新语言适配器的工作量拆解](./06-plugin-structure.md#112-语言扩展架构)）。
- 本地检查：`adapter.json` 通过 JSON Schema 校验；`extract-types.sh` 类型提取精度 ≥ 0.95、`extract-deps.sh` 依赖覆盖率 ≥ 0.90（阈值来自 [06 § 11.2 适配器验收标准](./06-plugin-structure.md#112-语言扩展架构)，可在 `.rustmigrate.toml` `[adapter_validation]` 调整）。
- 验收后并入发行版本。

**Patterns 贡献**（成功/失败经验）：
- 本地检查：YAML frontmatter 含 `confidence` 等级（`high`/`medium` 进入自动注入候选，`low` 仅参考）、`related_rules` 交叉索引（供影响分析）；遵循 [§6.12](#612-知识生命周期与维护政策) 的 deprecation 机制（过期移至 `anti-patterns/deprecated/` 而非删除）。
- 验收：新 pattern 在 Sprint Review 确认 `confidence` 等级后生效。

### 6.2.1 26 类规则的演化路径

26 类规则不是一次定稿，需随里程碑迭代。本节定义「非 MVP 规则何时升级」「跨适配器如何特化复用」「规则如何退役」，避免知识库变死文档。§6.12 的 deprecation 机制只覆盖 patterns/anti-patterns，本节将其延伸到核心 26 类规则。

**升级判据矩阵**（9 条 MVP=否 规则的具体升级条件，触发即纳入对应 M 阶段）：

| # | 规则类 | 升级到 MVP/启用的具体条件 |
|---|--------|--------------------------|
| 6 | 并发模式 | 项目画像检出多线程/通道使用，且 RULE-22 异步原语已就绪 |
| 14 | FFI 边界规则 | 采用 degrade(FFI) 降级策略，或 `parallel_strategy=strangler_fig` 需桥接层 |
| 16 | 调度/热路径规则 | `migration_motives` 含 `performance` 且 criterion 已提升为 Tier 1 |
| 21 | 生命周期与所有权模式 | Phase B 惯用化阶段出现借用冲突反复（同类 `LIFETIME_MISMATCH` 错误码 ≥ 3 次） |
| 22 | 异步运行时与并发原语 | 完成 M0 Spike 3 异步取消安全验证，覆盖 tokio + quinn 场景 |
| 23 | 序列化/反序列化兼容性 | 存在跨进程/持久化字节兼容需求（JSON/protobuf 与旧实现互通） |
| 24 | 日志/可观测性映射 | 源项目有结构化日志且需保持格式兼容（迁移 tracing） |
| 25 | 平台特定行为映射 | 源码含 OS API 分支，需 `cfg` 条件编译 |
| 26 | 多态/动态分发映射 | 源语言重度使用继承/接口，需 trait/enum dispatch 决策 |

**跨适配器特化示例**（同一通用规则在不同源语言下的具体化，供 M2+ 新适配器团队套用模板）：

| 规则类 | TS 特化 | Python 特化 | Go 特化 |
|--------|---------|-------------|---------|
| RULE-3 错误处理 | 禁止 `unwrap()`，`try/catch` → `Result + ?` | 禁止裸 `except`，异常 → `Result` | 禁止 `panic()`，`err != nil` → `Result + ?` |
| RULE-8 命名约定 | camelCase → snake_case | 已是 snake_case，保留 | MixedCaps → snake_case |
| RULE-22 异步原语 | `Promise` → `Future`（tokio） | `asyncio` → tokio | goroutine/channel → tokio task/mpsc |

通用规则定义"禁止什么/映射到什么"，适配器 `porting-template.md` 提供源语言侧的具体模式；项目专有层覆盖二者冲突（见上方「冲突仲裁」）。

**规则 deprecation 审批链**：核心 26 类规则退役与 patterns deprecation（§6.12）对齐，但因影响所有项目，审批更严：

| 环节 | 责任方 | 要求 |
|------|--------|------|
| 提议标记 deprecated | Plugin Lead 或社区维护委员会 | 附替代规则与"为何不再适用" |
| 评审 | ≥ 1 名核心维护者 reviewer（非提议者本人） | 确认无在用项目依赖该规则的当前 major 版本 |
| 归档位置 | `agents/*.md` 中标注 `(deprecated, vN)` + 在 `references/deprecated-rules.md` 记录退役原因 | 不直接删除，保留历史可追溯 |

deprecation 仅在 Plugin 主版本（major）更新时生效，并在 CHANGELOG「Breaking Changes」段公告（与 [06 § 10.0.2](./06-plugin-structure.md#1002-版本控制与向后兼容策略) 的兼容性窗口一致）。

**规则变更通知机制**：breaking change（major）发布时在 CHANGELOG 明确列出 `affected_modules` 范围（由 migration-state.json 的 `affected_modules` 字段自动计算，见 [§6.2 规则版本管理与代码一致性](#规则版本管理与代码一致性) 第 5 项），社区可按此评估升级成本。

---

## 6.3 PARITY.md — 迁移进度与等价深度跟踪

> **设计依据**：深度分析了 Claw-Code 的 PARITY.md 实践——它使用 9-lane checkpoint + 四级深度标签（strong/good/moderate/stub）+ commit hash 证据链，区分"工具表面一致"和"运行时行为一致"。

增强为 Sprint 级聚合 + **等价深度标签**：

```markdown
# 迁移进度

## Sprint 聚合视图
| Sprint | 目标模块数 | 已完成 | 通过率 | 覆盖率 | 阻塞项 |
|--------|-----------|--------|--------|--------|--------|
| S1     | 3         | 2      | 95%    | 82%    | 模块 C 类型复杂 |

## 模块详情
| 模块 | 状态 | 等价深度 | Sprint | Porting Version | 尝试次数 | 测试通过 | 覆盖率 | 已知差异 | 风险 |
|------|------|---------|--------|-----------------|---------|---------|--------|---------|------|
| utils/string | done | strong | S1 | v1 of RULE-2,3,8 | 1 | 24/24 | 91% | 0 | 低 |
| core/parser | testing | stub | S1 | v2 of RULE-3,11 | 2 | 18/22 | 76% | 1 | 中 |
| core/runtime | pending | — | S2 | — | 0 | — | — | — | 高 |
```

> **Porting Version 列**：记录该模块翻译时依据的规则版本，用于 breaking change 后的回溯检查（见 [§6.2 规则版本管理与代码一致性](#规则版本管理与代码一致性)）。`/migrate review` 比对当前规则版本与各模块的 Porting Version，识别需重新翻译的模块。

> **依赖图状态（Dependency Graph Status）列**（M1 人工标注）：标注该模块的依赖是否均满足 L3 FFI 参考条件（旧实现侧可构造），取值 `ready` / `blocked: {module}`，使「哪些模块当前可做 L3 FFI 对比」对人类可见（依据见 [03 § 7.6.2](./03-execution-model.md#762-l3-差异测试的依赖状态前置检查模块间依赖场景)；M2 由 `ffi_ready_dependencies` 字段自动计算）。

> **Manual Review 状态列**：取值 `done` / `pending_manual_review`，使 `requires_manual_review` 积压对人类可见（覆盖率判别规则与积压观测见 [03 § 7.5](./03-execution-model.md#75-质量评估分层评分卡) 与 [§ 4.2 Sprint Retrospective](./03-execution-model.md#42-外循环sprint-级跨会话天周)）。

> **验证工具组合列**（验证画像）：为每个模块标注实际采用的验证工具组合，使验证方法对人类透明，例：纯函数 `proptest + insta`，有状态 `insta + 手工测试`（有状态 + 跨语言 FFI 的行为录制框架属 M2，MVP 跳过见 [04 § 5.3](./04-toolchain.md#53-tier-1推荐画像自动启用)）。

> **L3 confidence 列**：以 partial-confidence 形式标注该模块导出函数的纯函数检测置信度分布（如 `67% high + 20% medium`，非离散标签），供 verifier 据 [03 § 7.6.1](./03-execution-model.md#761-库函数-ffi-对比可行性检查清单) 的「模块级 L3 FFI 置信度决策规则」判断是否降级，使 L3 等价证据的可信度对人类可见。

**等价深度标签**（借鉴 Claw-Code 的四级标签，MVP 使用 strong/stub 两级，M2 扩展）：

| 深度 | 含义 | MVP? | 判定标准 |
|------|------|------|---------|
| **strong** | 行为完全等价 | 是 | 所有测试通过 + 差异测试无偏差 + 无 TODO(port) 残留 |
| **stub** | 编译通过但行为未验证 | 是 | 编译通过但测试不完整或有 TODO(port) 残留 |
| good | 核心行为等价，边缘差异已登记 | M2 | 测试通过 + KNOWN_DIFFERENCES.md 中有已审批的差异 |
| moderate | 主要功能等价，部分功能缺失 | M2 | 部分测试通过 + 缺失功能已记录 |

> **`done` ≠ `strong`**：`done` 是状态（翻译工作完成），`strong` 是深度（行为等价性验证程度）。一个模块可以是 `done + stub`（翻译完了但验证不充分）。这种区分避免了 `done` 被误读为"完全可信"。

**管理层视图**：PARITY.md 顶部的聚合表可直接用于向管理层汇报。等价深度列帮助管理层理解"完成了多少"和"可信到什么程度"是两个不同的问题。

---

## 6.4 KNOWN_DIFFERENCES.md — 已知行为差异登记簿

记录所有已知的、经过评审确认可接受的行为差异。

```markdown
# 已知行为差异

## KD-001: HashMap 迭代顺序
- **模块**: core/config
- **差异**: 原版 JS Object.keys() 保持插入顺序，Rust HashMap 顺序随机
- **影响**: 配置输出顺序不同，不影响功能
- **决策**: 接受差异，文档记录
- **审批**: @zhangsan 2026-06-10
- **关联 MDR**: MDR-003

## KD-002: 浮点精度
- **模块**: math/statistics
- **差异**: 第 15 位小数有差异（IEEE 754 实现差异）
- **影响**: 统计计算结果微小偏差
- **决策**: 接受，epsilon 阈值 1e-12
- **审批**: @lisi 2026-06-12
```

**性能差异条目模板**（`migration_motives` 含 `performance` 时使用；退化超容忍度且未登记则自动 block 状态转移，见 [03 § 7.1](./03-execution-model.md#71-测试分层l0-l7)）：

```markdown
## KD-NNN: <module>::<function> 性能差异
- **模块/函数**: <module>::<function>
- **指标类型**: throughput | latency_p99
- **源基线值**: <source_value>
- **Rust 值**: <rust_value>
- **偏差**: <deviation_percent>%（相对源吞吐均值，criterion relative_difference）
- **可接受理由**: <acceptable_reason>
- **审批**: @<approver> <日期>
```

**已知差异类型预定义表**（确定性分类，verifier 发现差异时从此表选择类型，不自行发明）：

| 类型代码 | 差异类型 | 典型场景 |
|---------|---------|---------|
| `ITER_ORDER` | 迭代顺序差异 | HashMap vs 插入序 Map |
| `FLOAT_PREC` | 浮点精度差异 | IEEE 754 实现差异 |
| `NULL_HANDLING` | 空值处理差异 | null/undefined → Option |
| `STRING_LEN` | 字符串长度语义差异 | UTF-16 长度 vs UTF-8 字节数 |
| `INT_OVERFLOW` | 整数溢出行为差异 | wrapping vs panic vs silent |
| `ERROR_MSG` | 错误消息文本差异 | 不影响功能的消息格式变化 |
| `TIMING` | 时序/性能差异 | 执行顺序或延迟不同 |
| `PLATFORM` | 平台特定差异 | OS API 行为差异 |

**行动指南**：验证阶段发现行为差异时，verifier **立即追加**到此文件（不等到 Sprint Review），由人类决定是"修复"还是"接受"。

### 6.4.1 社区透明度与异议协议

PARITY.md 与 KNOWN_DIFFERENCES.md 是迁移质量的对外承诺，需具备开源项目的可见性与可质疑性。

**发布绑定**：每次发布 Rust 迁移版本时，将当时的 PARITY.md 与 KNOWN_DIFFERENCES.md 作为 release artifact 随 GitHub Release 一并发布（快照），并标注对应的源码 commit hash。快照是**补充**而非替代——`.rust-migration/` 下的 live 文件持续更新，发布快照提供"某版本对外承诺了哪些差异/进度"的可追溯定格，避免后续修改造成"进度幻觉"。完整的 Release 流程、版本号同步、CHANGELOG 规范、artifact 打包清单与发版前检查清单见 [06 § 10.0.2 Release 流程与产出物版本化](./06-plugin-structure.md#1002-版本控制与向后兼容策略)（权威来源）。

**异议机制**：社区用户或 code reviewer 对某条已登记差异（KD-NNN）有异议时，通过 GitHub Issue 引用该 KD 编号提出，附复现步骤。维护者在对应 KD 条目追加「异议 / 复议结论」字段（同 §6.4 已有的 `@-mention + 日期` 审批形式），决定维持、升级为待修复、或细化差异范围。

**冲突仲裁**：新发现差异与 KNOWN_DIFFERENCES.md 中"已接受"差异冲突时，以更窄、更精确的描述为准，并在两条目互相交叉引用；无法合并时由维护者按异议机制裁决。

**审批权限**：修改 KNOWN_DIFFERENCES.md 中已审批条目受 file-guard.sh 保护（见 [06 § 10.3 Hooks](./06-plugin-structure.md#103-hooks自动化门禁)），需人类审批者 `@-mention + 日期`。本节是对 §6.4 已有审批形式（KD-001/KD-002 示例）的制度化，非新增机制。

---

## 6.5 MDR — 迁移决策记录

每个重要架构决策写一条 MDR，格式：

```markdown
# MDR-001: 错误处理策略选择

- **日期**: 2026-06-08
- **状态**: 已采纳
- **Sprint**: S1
- **translated_from_source_commit**: a1b2c3d   # 翻译时锚定的源码 commit，用于双轨开发下追溯源依赖漂移（见 03 § 4.6.1）
- **背景**: 源项目使用 try/catch 异常处理，需要选择 Rust 错误处理策略
- **选项**:
  1. anyhow（简单，适合应用层）
  2. thiserror（类型化，适合库）
  3. 混合（库用 thiserror，应用层用 anyhow）
- **决策**: 选项 3
- **理由**: 迁移目标包含库和应用，分层处理最合理
- **后果**: 需要在 PORTING 规则中分别定义两种错误处理规则
```

**行动指南**：以下情况必须写 MDR：
- 异步运行时选择（tokio vs async-std）
- 错误处理策略
- Cargo workspace 结构
- FFI 桥接方案选择
- 功能裁剪决策
- 源码 bug 复刻决策（Phase A 识别的源码 bug，须人工确认修复或接受）
- **Phase B 三类允许改动**（边界定义见 [03 § 4.3 Step 5](./03-execution-model.md#43-内循环模块级单会话内-phase-ab-双阶段翻译)），按类别填必填字段，供 verifier Step 6 核对未超界：

| 改动类别 | MDR 必填字段 |
|---------|------------|
| 并发模式选择 | `原内部同步原语` → `新原语`；声明「对外接口/错误流程/可观测副作用未变」 |
| 取消安全性重构 | `涉及的 .await 点 / select!/timeout`；`Future drop 时如何保证状态一致`；声明「业务逻辑未变」 |
| 局部性能优化 | `函数名`（单函数内）；`原算法复杂度` → `新复杂度`；声明「函数签名/数据结构对外契约未变」 |
| 源码 bug 复刻 | `bug_replica: true`；`source_bug_location`；`human_decision: fix/accept_replica` |

---

## 6.6 AGENTS.md — AI 行为约束

```markdown
# AI 行为约束

## 硬性规则
- 禁止删除源项目文件
- Git 操作限制：只允许 commit/branch，禁止 force push/rebase
- 禁止引入未经 cargo-deny 审查的依赖
- 不确定时必须留 TODO（格式：TODO(migrate): 描述 [不确定原因]）
- 每个 unsafe 块必须有 // SAFETY: 注释

## 翻译规则
- 先读 PORTING 规则中相关条目，再开始翻译
- 翻译前先输出意图摘要，确认后再生成代码
- 禁止逐行直译，必须用 idiomatic Rust
- 优先使用标准库，其次用 PORTING 规则指定的 crate

## 规则编码策略（clippy.toml vs 自定义 lint，权威决策树）
- 规则 ≤ 10 条且全部可用 clippy.toml 的 disallowed_methods/types/macros 表达 → 留在 clippy.toml
- 规则 > 15 条，或 > 30% 需语义判断（禁止清单无法表达）→ 升级为 .rust-migration/lint-rules/ 自定义 lint crate
- verifier 在 /migrate analyze 末尾必须复核生成的 clippy.toml；规则数 > 20 时升级规则编码策略评审
- clippy.toml 只拦截被禁方法的直接调用，不能阻止 wrapper/unsafe/宏间接规避——须与 #![deny(unsafe_code)] 等全局属性及本反合理化表配合，不视为绝对防线
（详见 04 § 5.2）

## 测试规则
- 新代码必须有测试
- 修改已有代码必须跑相关测试
- 覆盖率不低于源模块

## 反合理化表（verification-is-non-negotiable）
以下是 agent 可能跳过验证的常见借口及反驳——SubAgent 系统提示中必须包含此表：

| 借口 | 反驳 |
|------|------|
| "这个改动太小了，不需要测试" | 小改动也可能引入行为差异，门禁脚本会自动执行，无需人工判断 |
| "编译通过就说明没问题" | 编译只检查类型，不检查语义等价性 |
| "测试太慢了，先跳过" | 使用 cargo nextest 并行执行，Tier 0 测试应在秒级完成 |
| "这个差异可以接受" | agent 无权决定差异是否可接受，必须记录到 KNOWN_DIFFERENCES.md 由人类审批 |
| "上下文窗口不够了，省略验证步骤" | 验证由独立脚本执行，不消耗上下文窗口 |
| "前面的模块都通过了，这个模式一样" | 每个模块必须独立验证，不允许类推 |
```

---

## 6.7 三级代码注释策略

| 级别 | 位置 | 内容 | 示例 |
|------|------|------|------|
| 模块级溯源 | 文件顶部 | 源文件路径 + 迁移 Sprint | `// Migrated from: src/utils/string.ts (Sprint S1)` |
| 函数级决策 | 函数上方 | 翻译决策说明 | `// 原版用 try/catch，此处改为 Result + ? 传播 (MDR-001)` |
| 行内保留 | 特殊行 | 不明确的语义保留原因 | `// 保持与原版相同的溢出行为 (wrapping_add)` |

---

## 6.8 CLAUDE.md 迁移配置

```markdown
# 迁移项目配置

## 核心规则
- 本项目正在从 [源语言] 迁移到 Rust
- 核心迁移规则嵌入 Plugin agents/*.md，参考指南见 skills/migrate/references/
- 项目专有迁移规则见 .rust-migration/porting/
- 迁移进度见 .rust-migration/PARITY.md
- 已知差异见 .rust-migration/KNOWN_DIFFERENCES.md
- AI 行为约束见 .rust-migration/AGENTS.md

## 当前状态
- Sprint [N]，当前聚焦模块：[模块名]
- 并行策略：[功能冻结/双轨/Strangler Fig]
- 源码锁定版本：[commit hash]

## 验证要求
- 每次写入 .rs：rust-analyzer LSP 自动诊断
- 每个模块完成：cargo test + clippy + 覆盖率 >= 原版
- 每个 Sprint：集成验证 + PARITY.md 更新
```

---

## 6.9 迁移产物生命周期

| 产出物 | 迁移期间 | 迁移完成后 | 说明 |
|--------|---------|-----------|------|
| PORTING 规则体系 | 活跃维护 | **长期保留** | 后续维护参考 |
| PARITY.md | 活跃更新 | 归档 | 不再需要 |
| KNOWN_DIFFERENCES.md | 即时写入 | **长期保留** | 故障排查参考 |
| AGENTS.md | 活跃使用 | 安全丢弃 | 仅迁移期间有效 |
| MDR | 决策时立即写 | **长期保留** | 架构决策溯源 |
| SPRINT_LEARNINGS.md | Sprint Review 追加 | **长期保留** | 知识沉淀 |
| DESIGN_ASSUMPTIONS.md | M0 产出 | **长期保留** | 技术假设记录 |
| patterns/ | 持续积累 | **长期保留** | 翻译模式复用 |
| anti-patterns/ | 持续积累 | **长期保留** | 避免重蹈覆辙 |
| migration-state.json | 核心状态 | 归档 | 不再需要 |
| test-fixtures/ | 活跃使用 | **长期保留** | 回归测试资产 |

---

## 6.10 GRADUATE：知识固化与迁移毕业

迁移不只是"代码转完就结束"。需要一个正式的毕业流程：

**Graduation Criteria（毕业标准）**：
- [ ] 所有模块状态为 done 或 degrade(FFI)
- [ ] KNOWN_DIFFERENCES.md 中所有差异已评审
- [ ] 测试覆盖率 >= 原项目
- [ ] P0 级 unsafe 全部消除，P1 级 unsafe 全部封装审计完毕，P4 级 unsafe 全部重新归类（[见插件结构 > unsafe 分类管理](./06-plugin-structure.md#104-unsafe-分类管理)）
- [ ] 性能基准无退化（允许 +-10%）
- [ ] CI/CD 已切换到 Rust 构建
- [ ] 团队完成 Rust 培训，能独立维护

**行动指南**：使用 `/migrate graduate` skill 评估是否满足毕业标准。满足后：
1. 移除 AGENTS.md 和 CLAUDE.md 中的迁移配置
2. 归档 PARITY.md 和 migration-state.json
3. 保留 PORTING 规则、KNOWN_DIFFERENCES.md、MDR、test-fixtures

---

## 6.11 增量知识沉淀架构

核心理念（借鉴 Compound Engineering）：**每一次工程活动都应让下一次更容易**。

### 4 层知识存储

| 层级 | 范围 | 存储位置 | 写入时机 |
|------|------|---------|---------|
| L0 会话级 | 单次 Claude Code 会话内的发现 | 会话上下文 | 实时 |
| L1 模块级 | 单个模块的翻译经验 | `.rust-migration/context/module-learnings/{module}.md` | 模块完成时 |
| L2 Sprint 级 | Sprint 内的模式总结 | `SPRINT_LEARNINGS.md` | Sprint Review 时 |
| L3 项目级 | 跨 Sprint 的项目级知识 | 通用知识：`skills/migrate/references/`；项目知识：`.rust-migration/context/` | 持续积累 |

### 6.11.1 四层知识的 MVP vs M2+ 分阶段实施

四层并非全部 MVP 强制，否则 L1 易与 L2 重复而沦为空架子，或过度设计成维护负担。按强制程度与自动化时机分阶段落地：

| 层级 | MVP 强制度 | 与相邻层关系 | M2+ 自动化 |
|------|-----------|-------------|-----------|
| L0 会话级 | 天然生效（会话上下文，无额外成本） | 是 L1/L2 的原料 | — |
| L1 模块级 | **可选**（团队有意愿建模块级经验则写，否则跳过） | 比 L2 更细：记单模块的特定坑（如某依赖的迁移技巧），L2 是跨模块共性总结 | index.json 自动生成候选 |
| L2 Sprint 级 | **强制**（Sprint Review 写入 SPRINT_LEARNINGS.md） | 汇总本 Sprint 跨模块模式 | — |
| L3 项目级 | 记录成功/失败案例（patterns/anti-patterns），不强制全覆盖 | 跨 Sprint 沉淀 | 语义检索集成（AgentMemory） |

**index.json 时机**：§6.12 的 `index.json`（pattern → related_rules 映射）为 **M2 自动生成候选**；MVP 阶段 SKILL.md 按规则类别手工 Read 相关 pattern 文件，不依赖 index.json，避免 MVP 维护未自动化的索引。

**MVP 手工 Read 的触发口径**（消除「按规则类别」无具体指导的歧义，避免 M1 pattern 库膨胀后无法扩展）：translator 在 [03 § 4.3 Step 2](./03-execution-model.md#43-内循环模块级单会话内-phase-ab-双阶段翻译) 生成意图摘要时，基于 PROFILE 已检测的 NodeData 属性（`is_async` / `has_ffi` 等）输出 3-5 个「最可能相关的 pattern 文件名」（如 async 模块 → `references/patterns/async-to-tokio.md`），但不自动 Read。SKILL 编排器据此在 `references/patterns/` 预扫描：若命中 ≤ 3 个文件且总 token < 20K，自动 Read 注入 translator 上下文；否则仅提示「未找到对应 pattern，请关注相关 RULE（如 async → RULE-22）」。此口径为 M2 的 index.json + 语义检索做铺垫，实现成本仅编排器一段逻辑。

**四层生命周期 MVP vs M2+ 成本对标**：

| 维度 | MVP 成本 | M2+ 自动化投入 | ROI 拐点 |
|------|---------|---------------|---------|
| L0/L2 | 天然产生（会话上下文 + Sprint Review 强制） | — | 立即正向 |
| L1 | 手工整理约 0.5h/模块（可选） | index.json 自动生成 | 项目 5+ 模块且后续复用时正向 |
| L3 检索 | 文件读取（Claude Code 原生） | 语义检索集成约 2-3 人天 | 跨 Sprint 经验复用频繁时正向 |

> **L1 接受/按需启用**：L1 在 MVP 为**可选**而非默认必做。团队若选择跳过 L1（小项目 / 模块少），应在项目 CLAUDE.md 记录「L1 按需启用」。此设计保持理论完整性的同时给 MVP 留灵活性，符合渐进式交付原则。

**L1 启用判据与格式**（让团队可自主决策，而非凭感觉）：

- **触发条件**（满足即值得写 L1）：模块 > 50 行 AND（含异步逻辑 OR FFI 调用 OR 生命周期/借用约束反复）。不满足则跳过，避免与 L2 重复沦为空架子。
- **强制字段模板**（防 freestyle 流水账）：`module-learnings/{module}.md` 须含「模块名 / 解决的陷阱 / 应对技巧 / 相关规则版本（RULE-NN:vX.Y.Z）」四字段。
- **SKILL.md Step 1.5 注入指令**（接 [03 § 4.3 Step 1](./03-execution-model.md#43-内循环模块级单会话内-phase-ab-双阶段翻译) 上下文加载后）：检查该模块源码特征，若匹配上述触发条件 AND `.rust-migration/context/module-learnings/{module}.md` 存在，则 Read 注入 translator 上下文；否则跳过（MVP 保持手工 Read，不引入未自动化的索引；index.json 为 M2 候选）。

### 知识分层存储

**通用知识**（随 Plugin 分发）：
- 位置：`skills/migrate/references/` 目录
- 内容：跨项目通用的翻译模式、反模式、最佳实践
- 格式：YAML frontmatter + Markdown（便于语义检索）
- 示例：
  ```
  skills/migrate/references/
  ├── patterns/
  │   ├── async-to-tokio.md
  │   └── error-handling-migration.md
  └── anti-patterns/
      ├── naive-mutex-wrap.md
      └── arc-everything.md
  ```

**项目知识**（项目本地）：
- 位置：`.rust-migration/context/` 目录
- 内容：项目特有的翻译经验、失败记录
- 格式：YAML frontmatter + Markdown
- 示例：
  ```
  .rust-migration/context/
  ├── patterns/              # 项目特有的成功模式
  ├── anti-patterns/         # 项目特有的失败教训
  └── module-learnings/      # 模块级翻译经验
  ```

- 不需要配套基础设施（检索靠 Claude Code 的文件读取即可，M2+ 可接入 AgentMemory）

### 写入时机规则

- **MDR**：决策发生时**立即记录**，不等到 Sprint Review
- **KNOWN_DIFFERENCES.md**：verifier 发现差异时**立即追加**，不批量写入
- **PORTING 规则**：每次规则变更记录原因和 Sprint 编号（changelog.md）
- **patterns / anti-patterns**：模块完成或失败时写入

### 辅助产出物格式模板

**SPRINT_LEARNINGS.md**（每次 Sprint Review 追加）：

```markdown
## Sprint S1 (2026-06-15)

### 新发现的翻译模式
- HashMap 迭代顺序差异：所有依赖遍历顺序的逻辑需改用 BTreeMap

### 工具调整
- proptest seed 需固定到 test-fixtures/，否则 CI 不可复现

### 规则变更
- PORTING 规则新增：整数溢出策略（由 Sprint S1 失败触发）

### 下一 Sprint 注意事项
- core/parser 模块的 async 模式需要 tokio 迁移指南
```

**DESIGN_ASSUMPTIONS.md**（M0 假设验证报告）：

```markdown
## 假设验证报告

| # | 假设 | Spike | 结论 | Plan B? | 说明 |
|---|------|-------|------|---------|------|
| 1 | SubAgent 3 次串行调用可靠 | Spike 1 | verified (4/5 通过) | 否 | 第 3 次失败因上下文膨胀，缩减 analyzer 输出后稳定 |
| 2 | rust-analyzer LSP 反馈秒级 | Spike 2 | verified | 否 | 小项目 <1s，中项目 ~3s |
| 3 | tree-sitter TS 精度 | Spike 3 | plan-b-triggered | 是 | 装饰器解析不完整，改用 ast-grep |
```

### 知识复利机制

每次模块迁移完成后，执行知识沉淀步骤：
1. 提取本次迁移中的可复用模式 → 写入 `patterns/`（通用→`references/`，项目专有→`context/`）
2. 记录失败尝试和原因 → 写入 `anti-patterns/`
3. 更新 PORTING 规则（如有新规则）→ 追加 changelog
4. 后续模块翻译时，translator SubAgent 的上下文注入相关模式

### Beads/AgentMemory 集成 [M2+ 评估]

建议在 M0 Spike 5 中评估以下集成可行性：
- **Beads**：用于 SubAgent 任务状态的跨会话持久化（替代部分 migration-state.json 手工管理）
- **AgentMemory**：用于翻译知识的语义检索（从 patterns/anti-patterns 中检索相关经验）

---

## 6.12 知识生命周期与维护政策

L0-L3 四层知识中的长期产出物 patterns/anti-patterns 若只增不维护，会随规则演进与时间推移变成"死文档"。本节定义其新鲜度、关联与失效机制。patterns 与 §6.2 规则同样经历版本化与 deprecation，保持两者一致。SPRINT_LEARNINGS.md 按 Sprint 编号天然带时间戳，属追加式记录，不需要 deprecation 机制；读者可交叉引用 porting/changelog.md 确认某条历史经验是否仍适用。

**Frontmatter 扩展**：pattern/anti-pattern 的 YAML frontmatter 在 §6.1 基础字段外增加：

```yaml
last_verified: 2026-06-10        # ISO 日期，最近一次确认仍适用
status: active                   # active | needs-review | deprecated
related_rules: [RULE-3, RULE-22] # 关联规则类，供影响分析
related_mdrs: [MDR-001]          # 关联决策记录
```

**新鲜度管理**：每 6 个月，verifier 在 `/migrate review` 执行时将到期 pattern 标记 `needs-review`；translator/verifier 在引用时若遇 `needs-review`，须确认仍适用（更新 `last_verified`）或标记 `deprecated`。

**关联索引**：`.rust-migration/context/` 的 `index.json` 记录 `pattern → related_rules / related_mdrs / 使用模块` 的映射，供按模块特征（如 `is_async=true`）只注入相关且 `status=active` 的 pattern，避免全量加载占用上下文预算。**实施时机**：index.json 为 M2 自动生成候选（见 [§6.11.1](#6111-四层知识的-mvp-vs-m2-分阶段实施)）；MVP 阶段 SKILL.md 按规则类别手工 Read 相关 pattern，不依赖 index.json。

**规则变更影响分析**：Sprint Review 时扫描本 Sprint 的规则变更，凡 `related_rules` 命中变更规则的 pattern 自动标记 `needs-review`，确保规则改动后相关经验被复核而非默默失效。

**验收与提交流程**：新 pattern 提交后在 Sprint Review 确认 `confidence` 等级——`high`/`medium` 进入注入候选，`confidence: low` 仅作参考、不自动注入。

**Deprecation 流程**：过期 pattern 移至 `anti-patterns/deprecated/` 保留作学习资料，不直接删除（记录"为何不再适用"）。
