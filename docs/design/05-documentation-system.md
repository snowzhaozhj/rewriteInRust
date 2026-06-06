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

**版本化演化**：每条规则有版本号和变更记录，每个 Sprint Review 可以修改规则，但必须记录变更原因。

**行动指南**：
- MVP 阶段只生成标记"是"的规则
- 每次翻译失败且原因是规则缺失时，追加新规则并标注"由 Sprint N 失败触发"
- 规则格式：`源语言模式 → Rust 等价物 + 注意事项 + 示例`

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
| 模块 | 状态 | 等价深度 | Sprint | 尝试次数 | 测试通过 | 覆盖率 | 已知差异 | 风险 |
|------|------|---------|--------|---------|---------|--------|---------|------|
| utils/string | done | strong | S1 | 1 | 24/24 | 91% | 0 | 低 |
| core/parser | testing | stub | S1 | 2 | 18/22 | 76% | 1 | 中 |
| core/runtime | pending | — | S2 | 0 | — | — | — | 高 |
```

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

---

## 6.5 MDR — 迁移决策记录

每个重要架构决策写一条 MDR，格式：

```markdown
# MDR-001: 错误处理策略选择

- **日期**: 2026-06-08
- **状态**: 已采纳
- **Sprint**: S1
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
- 通用迁移规则见 .claude/rules/（按 paths 条件自动加载）
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
| 1 | SubAgent 4 步编排可靠 | Spike 1 | verified (4/5 通过) | 否 | 第 3 次失败因上下文膨胀，缩减 analyzer 输出后稳定 |
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
