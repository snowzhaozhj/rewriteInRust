# M2 实施计划：质量提升

> 本文件是 M2 阶段的唯一执行计划。M1 阶段计划保留在 `PLAN.md §1-§9.5`。
> 权威设计来源：`docs/design/` 各章节（本文件引用但不重复）。

---

## §1 M2 总览

### 1.1 目标

将 rustmigrate 从 M1 MVP（串行、单 sprint、全步骤循环）提升为**可实用的迁移工具**：

1. **效率**：复杂度自适应循环（trivial/standard/full 分档），减少无效 token 消耗
2. **规模**：并行 sprint（parallel_groups 消费 + worktree 隔离 + Workflow 批量）
3. **质量**：验证管线增强（proptest/fuzz/覆盖率/行为录制）+ 降级 FFI
4. **可靠性**：类型安全重构 + 状态机加固 + schema 向后兼容
5. **可用性**：CLI 扩展（5 个新命令）+ headless 模式 + CI/CD 集成

### 1.2 验收标准（对齐 08 §M2）

| 指标 | 标准 | 测试方法 |
|------|------|---------|
| 规模验收 | 3 个 5K-20K 行 TS 项目，每项目 ≥3 模块迁移完成 | 真实项目端到端 |
| 降级验收 | ≥1 模块降级 FFI 成功 | circular-deps 或真实循环 |
| 并行吞吐 | ≥1.5 模块/小时（3 agent 并行） | Workflow 批量模式计时 |
| SQLite 并发 | 冲突率 <10%，单次锁等待 ≤20ms | 3 agent 并发写测试 |
| 性能无退化 | 相对 M1 基线波动 ≤±10% | 同项目回归对比 |
| 测试质量 | proptest 1000 次无 panic + fuzz 24h 无 crash | 自动化运行 |
| 翻译膨胀 | 膨胀率 <3.0x | tokei 源/Rust LOC 对比 |
| 全流程耗时 | <60min（含多模块） | 端到端计时 |

### 1.3 M2→M3 升级判据（充要条件）

**性能指标（08 §M2 原文）**：
(a) P50 ≥1.5 模块/小时 (b) P99 ≥0.8 模块/小时 (c) SQLite 冲突率 <10% 且锁等待 ≤20ms (d) 性能波动 ≤±10%

**质量指标（必要条件，补充）**：
(e) §1.2 验收标准全表通过（含覆盖率 ≥70%、膨胀率 <3.0x、降级 FFI 成功）

若方案 B merge 开销导致吞吐 <1.5，降 max_concurrent_agents=2 后 P50 ≥1.2 仍可接受。

### 1.4 与 M1 的关系

- M1 的 202 个测试 + 4 fixture 是 M2 的回归基线
- M2 不破坏 M1 已跑通的 linear/diamond/circular/edge-cases fixture
- M1 遗留的 3 个 deferred TODO 纳入 M2 对应任务

### 1.5 范围裁剪决策

设计文档 08 §M2 列出了 ~30 项交付物。经可行性评估，本计划做以下范围裁剪：

| 类别 | 纳入 M2 | 推迟 M2.5/M3 | 理由 |
|------|---------|-------------|------|
| 复杂度自适应 | ✅ M2-TIER-01 | — | 最高 ROI，纯增量，无锁风险 |
| 并行 sprint | ✅ M2-SCALE 全系列 | — | 规模化必需 |
| 类型重构 | ✅ M2-REFAC（去掉已完成的 08） | — | 代码质量基础 |
| 验证增强 | ✅ VER-01/02/04/05 + 覆盖率 | VER-03 行为录制框架 | 行为录制框架工作量 L 且 M2 验收不强依赖 |
| CLI 扩展 | ✅ 5 个设计文档已定命令 | — | 设计文档已冻结清单 |
| 降级 FFI | ✅ M2-ADV-02 | 降级决策学习 | 核心功能 vs nice-to-have |
| graduate | ✅ M2-ADV-03 | — | 状态机完整性 |
| 高级功能 | ✅ ADV-04/05/06/07/08/09/10 | — | 多为小功能 |
| 状态机程序化 | ❌ 推迟 | ✅ M2.5 | 工作量 XL，M2 用 SKILL.md + 加固 CLI 足够 |
| 向后兼容框架 | ✅ 基础版（version 字段 + 检测） | 完整迁移脚本 | 先做检测，完整迁移 M2.5 |
| 图 schema 扩展 | ❌ 推迟 | ✅ M3 | TypeAlias/Variable 不影响迁移顺序；Community 依赖 Leiden 无成熟 crate |
| FTS5 全文搜索 | ❌ 推迟 | ✅ M3 | <20K 行项目不需要跨字段搜索 |
| Leiden 社区检测 | ❌ 推迟 | ✅ M3 | 无成熟纯 Rust crate |
| CI/CD 集成 | ✅ 基础版 | 完整 dogfooding | M2 验收需 CI 就绪 |
| 规则治理工具化 | ❌ 推迟 | ✅ M3 | M2 规则量未达触发阈值 |
| 自定义 lint crate | ❌ 条件推迟 | 视 M2 规则统计 | M1 规则 <15 条，未触发 |
| /goal 自主循环 | ❌ 推迟 | ✅ M2.5 | 依赖状态机程序化 |
| 变异测试 | ❌ 推迟 | ✅ M2.5 | 验收不强依赖 |
| KNOWN_DIFFERENCES 自动生成 | ✅ | — | verifier 已有基础 |
| 完整错误码体系 | ✅ 基础版（P0 错误码） | 完整 30-40 条 | 先覆盖高频错误 |
| 等价深度扩展 | ✅ good/moderate 两级 | — | 小改动 |

---

## §2 未决设计决策（M2 启动前定夺）

### D1: done 终态语义

**矛盾**：02 §3.4 行 170（`degrade_* → translating 通过 --force`）暗示可重做 vs 09 附录 A 矩阵（done 无出边，硬终态）。

**推荐决策**：**done 是硬终态，不可 force 重做**。
- 理由：done 意味着通过了全部验证（Tier 0/1/2），重做会破坏已建立的质量保证
- 如果用户确实想重做，应通过 `graduate --rollback`（M3 功能）回退到 reviewing
- degrade_* → translating 的 `--force` 仅适用于降级状态，不涉及 done

**影响**：M2-ADV-03 graduate 命令以 done 为前置条件，无需处理 done→重做的分支。

### D2: blocked 进入规则

**矛盾**：02 §3.4 图「可从任意状态进入」vs 09 附录 A「仅活跃状态可进入 blocked」。

**推荐决策**：**仅活跃状态（pending/translating/compile_fixing/testing/reviewing/paused）可进入 blocked**。
- 理由：done/degrade_* 是终态或半终态，进入 blocked 无意义
- M1 代码 `can_transition_to()` 已按此实现

**影响**：无代码变更。更新 02 §3.4 图的措辞以消除歧义。

### D3: M2-SCALE 写隔离方案

**矛盾**：PLAN P0-A.3 标注"两方案待定"，但 06 §10.5 已预设 worktree 方案。

**推荐决策**：**方案 B — SubAgent 产代码 + 编排器统一 merge**。
- 理由（可行性评估对比结论）：
  - worktree 方案的 `Cargo.toml` 是并发写的最大冲突点——多 worktree 同时修改同一区域几乎必然冲突
  - 方案 B 让编排器串行聚合 deps 清单，一次性写入，**彻底消除冲突**
  - SubAgent 返回结构化 JSON（rust_code + deps + test_code），编排器统一写入 rust_root/ + Cargo.toml
  - migration-state.json 更新由编排器统一做，无并发冲突
  - source-graph.db 在翻译阶段只读（analyzer 阶段已完成写入），无 SQLite 并发写问题
- 风险：SubAgent 无法直接 `cargo check`
- 缓解：编排器在每组模块 merge 后执行一次 `cargo check`，失败则逐个回滚

**影响**：M2-SCALE-01/02 实现方向确定。需同步更新设计文档：
- 06 §10.5 M2 扩展表的「多 agent 工作区」行改为「编排器统一 merge」
- 04 §5.7.3 标注 `shared_db_path` 段落为"方案 B 下不适用"
- 08 §M2 交付物中「多 agent worktree 隔离机制」改为「编排器统一 merge 机制」

**影响**：M2-SCALE-01/02 的实现方向确定，降低工作量约 30%。

### D4: NodeData.complexity vs TIER-01 分档

**现状**：两套分级体系未关联——04 §5.7.1 的 `Simple/Moderate/Complex` 按 LOC，M2-TIER-01 的 `trivial/standard/full` 按语义特征。

**推荐决策**：**新增 `tier` 字段，保留 `complexity` 原语义**。
- `complexity`（Simple/Moderate/Complex）保留原有 LOC-based 语义，用于统计/展示
- `ModuleState` 新增 `tier: Option<ModuleTier>`（Trivial/Standard/Full），由 AST 语义特征驱动
- 两者共存但职责分离：complexity 描述"代码规模"，tier 决定"翻译策略"
- 理由：避免 breaking change（已有 source-graph.db 中的 complexity 数据不需迁移）
- 设计文档需补充 TIER-01 分档定义章节（建议放在 03 或 04 中）

---

## §3 Sprint 结构

M2 分为 6 个 Sprint，预计 4-5 周：

```
Sprint A (基础加固)      ▓▓░░░░░░░░░░░░░░░░░░░░░░  2-3 天
Sprint B (类型+图精度)   ░░▓▓▓▓▓░░░░░░░░░░░░░░░░░  5-6 天
Sprint C (核心功能双线)  ░░░░░░░▓▓▓▓░░░░░░░░░░░░░  4-5 天
Sprint D (并行+高级)     ░░░░░░░░░░░▓▓▓▓▓▓▓░░░░░░  7-9 天
Sprint E (验证+CLI扩展)  ░░░░░░░░░░░▓▓▓▓░░░░░░░░░  3-4 天（与D并行）
Sprint F (M2 验收)       ░░░░░░░░░░░░░░░░░░▓▓▓▓▓▓  7-10 天
                         ├── 关键路径 ~25-33 天 ──────┤
                         ├── 日历约 5-7 周（含 PR 审查间隙）──┤
```

### 关键路径

```
路径 A（自适应循环）: A(VER-04:1d) → B(REFAC-09:1d → REFAC-10:3-4d) → C(TIER-01:3d + ADV-07:2d) → F(7-10d)
  总长: ~17-22d

路径 B（并行 sprint，关键路径）: A(REFAC-06:0.5d) → C(SCALE-P:1.5d → SPRINT:1d) → D(SCALE-02:5-7d → SCALE-01:2-3d) → F(7-10d)
  总长: ~17-23d

关键路径 = max(A, B) + Sprint E 并行 ≈ 25-33d（纯开发时间，不含 PR 审查间隙）
```

> 注：SCALE 编号沿用 PLAN.md 原始编号，实际执行序为 SCALE-02 → SCALE-01（先写隔离再 Workflow 编排）。

---

## §4 Sprint A：基础加固（2-3 天）

**目标**：清理 M1 技术债，为后续功能提供可靠基础。解决未决设计决策。

### 任务清单

| 任务 ID | 内容 | 文件 | 工作量 | 依赖 |
|---------|------|------|--------|------|
| M2-DESIGN-01 | 统一 done 终态语义（更新 02 §3.4 图 + 09 附录 A 措辞） | docs/design/ | 0.5d | — |
| M2-DESIGN-02 | 统一 blocked 进入规则（更新 02 §3.4） | docs/design/ | 0.5d | — |
| M2-VER-04 | populate 数据卫生：孤儿 pending 清理 + docstring 修正 + record-subagent-call 无 init e2e | machine.rs, cli e2e | 1d | — |
| M2-VER-05 | Timestamp ISO8601 校验：非法格式返回明确错误 | common.rs, machine.rs | 0.5d | — |
| M2-REFAC-05 | parse_node_extra 错误可见化 | persist.rs | 0.5d | — |
| M2-REFAC-06 | MigrationSequence 字段私有化 + getter | topo.rs, 调用方 | 0.5d | — |
| M2-REFAC-07 | StateMachine.load() 后置校验 | machine.rs | 0.5d | — |
| M2-REFAC-13 | ToolStatus 枚举化 + LocReport 派生 totals | types/state.rs, stats/ | 0.5d | — |
| M2-REFAC-14 | ErrorData structured context（details: Option<Value>） | response.rs, lib.rs:659 | 0.5d | — |
| M2-COMPAT-01 | migration-state.json 版本检测基础：init 时写入 version 字段 + validate state 版本检查 | machine.rs, validate/ | 1d | — |
| M2-ADV-06 | stats compare 非占位实现（tokei + tree-sitter 函数计数结构对比） | lib.rs:1143, stats/ | 1d | — |
| M2-CTX-01 | 上下文预算实证校验：用真实项目实测 02 §3.5.1 预算表准确度，结果反馈规则库改进（08 §M2 标注前 2 周优先） | docs/design/, DESIGN_ASSUMPTIONS.md | 1d | — |

### 并行策略

- DESIGN-01/02 + VER-04/05 + REFAC-05/06/07/13/14 全部独立，可 3 路并行
- ADV-06 和 COMPAT-01 独立于上述任务

### 完成标志

- [ ] 2 个设计决策写入 docs/decisions/
- [ ] populate 孤儿清理 + 幂等测试通过
- [ ] Timestamp 非法格式返回 `INVALID_TIMESTAMP` 错误
- [ ] stats compare 非占位，输出源/Rust 结构对比 JSON
- [ ] migration-state.json 含 `version` 字段，validate state 检查版本
- [ ] `just ci` 全绿 + M1 fixture 回归通过

---

## §5 Sprint B：类型安全 + 图精度提升（5-6 天）

**目标**：完成核心类型重构，提升图构建精度（TIER-01 分档的数据基础）。

### 任务清单

| 任务 ID | 内容 | 文件 | 工作量 | 依赖 |
|---------|------|------|--------|------|
| M2-REFAC-01 | SourceNode 构造器 + pub(crate) 封装 | types/graph.rs, build.rs, 所有构造点 | 1d | — |
| M2-REFAC-02 | ImportInfo 枚举化（ImportKind + SymbolKind enum） | lang/mod.rs, build.rs | 1d | — |
| M2-REFAC-03 | sub_kind 类型化 → EdgeSubKind enum | types/graph.rs, build.rs | 0.5d | — |
| M2-REFAC-04 | migration_status/rust_kind → enum | types/graph.rs, persist.rs | 0.5d | — |
| M2-REFAC-09 | Arrow function 提取（lexical_declaration 处理） | lang/typescript.rs | 1d | REFAC-01 |
| M2-REFAC-10 | 跨文件方法调用解析（档 1：唯一方法名连边 + 局部 receiver 绑定） | graph/build.rs | 3-4d | REFAC-09 |
| M2-REFAC-11 | fixup_extends 名称索引（HashMap 替代 O(N)） | graph/build.rs | 0.5d | — |
| M2-REFAC-12 | walk_ast class 递归（嵌套 dynamic import 捕获） | lang/typescript.rs | 0.5d | — |
| M2-REFAC-15 | module key 人类友好归一化（`--human` 显示映射） | types/state.rs, lib.rs | 0.5d | — |

### 并行策略

- **Wave 1（独立）**：REFAC-01、REFAC-02、REFAC-03、REFAC-04、REFAC-11、REFAC-12、REFAC-15 全部独立
- **Wave 2（串行）**：REFAC-09 → REFAC-10（依赖完整函数节点）
- Wave 1 和 Wave 2 可并行推进

### 回归验证

所有 REFAC 完成后必须跑：
```bash
just ci                    # 全量 CI
cargo nextest run          # 202 测试回归
# fixture Live 验证（至少 linear + diamond）
```

### 完成标志

- [ ] 所有 `Option<String>` 类型化完成（sub_kind/migration_status/rust_kind）
- [ ] SourceNode 构造器强制使用，外部无法构造非法组合
- [ ] ImportInfo 5 种非法布尔组合在编译期消除
- [ ] Arrow function `export const f = () => {}` 生成 Function 节点
- [ ] 跨文件方法调用档 1 上线（唯一方法名连边 + 局部 receiver）
- [ ] `just ci` 全绿 + ground-truth 偏序约束 100% 满足

---

## §6 Sprint C：核心功能双线（4-5 天）

**目标**：同时推进自适应循环和并行 sprint 基础，两线独立互不阻塞。

### 线 1：复杂度自适应循环

| 任务 ID | 内容 | 文件 | 工作量 | 依赖 |
|---------|------|------|--------|------|
| M2-TIER-01a | CLI：per-module 复杂度评估 + ModuleState.risk 自动填充 | detect.rs, machine.rs | 1d | Sprint B REFAC-09/10 |
| M2-TIER-01b | analyzer.md 扩展：per-module 语义信号输出 | plugin/agents/analyzer.md | 0.5d | TIER-01a |
| M2-TIER-01c | run.md 分档逻辑：trivial/standard/full 循环路径 | plugin/skills/migrate/run.md | 1d | TIER-01a |
| M2-TIER-01d | 降档/升档机制 + 可观测日志 | run.md, state machine | 0.5d | TIER-01c |
| M2-ADV-07 | 默认 TODO 决策策略（headless 模式） | run.md, translator.md | 1-2d | TIER-01c |

**M2-TIER-01 分档判据**：

| 档位 | 判据（AST 语义特征） | 循环 | 测试 |
|------|---------------------|------|------|
| **trivial** | 纯类型文件（仅 interface/type/enum 导出）或常量文件或 barrel（仅 re-export） | 批量直翻 + 编译 + 签批，跳 Phase B | 仅验编译 + 导出可见性 |
| **standard** | 无以下任何危险信号的普通模块 | 保留意图摘要 + Phase A + 审查 + 测试 | 语义等价测试 |
| **full** | 含任一危险信号：副作用 I/O / 并发(Promise.all/async) / 错误路径(try-catch) / 数值计算 / 全局状态 / 动态类型操作 / 条件类型/泛型约束 / unknown/never 不可判定 | 完整 11 步 | 完整 Tier 0+1+2 |

**关键原则**：
- 判据基于 **AST 可见语义特征**，非 LOC
- 任一危险信号或 unknown → 不降档（默认 full 兜底）
- 短 ≠ 低风险（`utils.clamp` 几行但含 NaN 陷阱 → full）
- **维度 9 意图一致性永不跳过**
- 降档可观测（state 记录分档结果）+ 失败自动升档重跑

### 线 2：并行 sprint 基础

| 任务 ID | 内容 | 文件 | 工作量 | 依赖 |
|---------|------|------|--------|------|
| M2-SCALE-P | populate 改消费 parallel_groups（组索引→sprint 号）+ 增加 `--single-sprint` flag 兼容 M1 回归 | machine.rs, lib.rs | 1.5d | Sprint A REFAC-06 |
| M2-SCALE-SPRINT | sprint 推进：sprint N 全终态→current=N+1 + history 回填。**必须与 SCALE-P 同 PR 交付**（避免 populate 改了但推进没做的窗口期） | machine.rs | 1d | SCALE-P |
| M2-ADV-03 | graduate 命令：项目级毕业评估（所有模块 done/degrade_* 时可执行）+ graduation-report.json 产出 + 源文件归档标记。**不新增 `graduated` 模块状态**（设计文档 09 附录 A 将 GRADUATE 映射为项目级概念） | lib.rs, SKILL.md | 1d | Sprint A DESIGN-01 |

### 并行策略

- 线 1 和线 2 **完全独立**，可同时推进
- 线 1 内部：TIER-01a → TIER-01b/c → TIER-01d → ADV-07
- 线 2 内部：SCALE-P → SCALE-SPRINT（ADV-03 独立）

### 完成标志

- [ ] trivial 模块批量翻译跳过 Phase B，编译通过即 done
- [ ] full 模块行为等价于 M1 现有 11 步循环
- [ ] 降档/升档日志可观测（state 记录 `tier: trivial/standard/full`）
- [ ] headless 模式撞 TODO(port) 按 safe-default 自动决策
- [ ] populate 按 parallel_groups 分配 sprint（sprint=1 为叶节点组）
- [ ] sprint 推进逻辑测试通过
- [ ] graduate 命令可将 done 模块标记 graduated
- [ ] M1 fixture 回归通过（full 档行为不变）

---

## §7 Sprint D：并行编排 + 高级功能（7-9 天）

**目标**：完成并行翻译全栈（核心 M2 价值），补齐高级功能。

### 并行编排（关键路径）

| 任务 ID | 内容 | 文件 | 工作量 | 依赖 |
|---------|------|------|--------|------|
| M2-SCALE-02 | 写隔离实现：SubAgent 产代码到隔离目录 + 编排器统一 merge + compile_fixing 子流程 + dep 版本冲突解析 | run.md, translator.md | 5-7d | Sprint C SCALE-SPRINT |
| M2-SCALE-01 | Workflow 批量翻译：Agent tool 并发编排 + 错误恢复 | run.md, Workflow 定义 | 2-3d | SCALE-02 |
| M2-SCALE-LOCK | 全局锁改造：编排器持锁，SubAgent 不取锁 | run.md, SKILL.md | 0.5d | SCALE-01 |
| M2-PETGRAPH-01 | petgraph 副本验证 + SQLite WAL 并发合规测试 | 测试代码 | 1d | SCALE-01 |

**写隔离实现方案（D3 决策执行）**：

```
编排器（SKILL.md run）
  ├── 获取 sprint N 的模块组
  ├── 为每个模块创建隔离目录 .rust-migration/workspace/{module}/
  ├── Agent tool 并发启动 SubAgent（max_concurrent_agents=3）
  │   ├── SubAgent 翻译代码写入隔离目录
  │   ├── SubAgent 返回：{rust_files: [...], cargo_deps: [...], test_files: [...]}
  │   └── SubAgent 不直接写 rust_root/ 或 Cargo.toml
  ├── 编排器收集所有 SubAgent 产出
  ├── 统一 merge：
  │   ├── 复制 rust_files → rust_root/src/
  │   ├── 合并 cargo_deps → Cargo.toml
  │   └── 复制 test_files → rust_root/tests/
  ├── cargo check（整组验证）
  │   ├── 成功 → 更新 migration-state.json（编排器统一写，无并发）
  │   └── 失败 → compile_fixing 子流程（见下方）
  └── sprint 推进检查
```

**compile_fixing 子流程（方案 B 特有）**：
```
merge 后 cargo check 失败
  ├── 编排器解析 rustc 错误输出，提取失败文件→模块映射
  ├── 尝试定位失败模块（错误发生在哪个模块的代码中）
  │   ├── 可定位：仅回滚该模块代码（从隔离目录恢复），其他模块保留
  │   └── 不可定位（跨模块类型冲突等）：整组回滚
  ├── 对失败模块重新启动 SubAgent 进入 compile_fixing
  │   ├── SubAgent 收到：原代码 + rustc 错误输出 + 当前 Cargo.toml
  │   ├── SubAgent 在隔离目录中修复 → 返回修正代码
  │   └── 编排器重新 merge 修正代码 → 再次 cargo check
  ├── 最多重试 3 次（M1 翻译循环的 max_compile_retries）
  └── 3 次失败后 → 标记模块 paused，继续处理其他模块
```

### 高级功能

| 任务 ID | 内容 | 文件 | 工作量 | 依赖 |
|---------|------|------|--------|------|
| M2-ADV-02 | 降级 FFI：基于 napi-rs 的 FFI binding 生成 + degrade_ffi 状态处理 + 环断点选择算法 | translator.md, machine.rs, scaffold/ | 3-4d | — |
| M2-ADV-04 | graph build --profile 性能画像 JSON | lib.rs:578 | 0.5d | — |
| M2-ADV-05 | graph interfaces --deps-of 批量输出 | lib.rs:718 | 0.5d | — |
| M2-ADV-08 | profile 自动定位 analysis-tools.json（CLAUDE_PLUGIN_ROOT） | profile/, SKILL.md | 0.5d | — |
| M2-ADV-09 | 完整子进程超时（tokio timeout / nix kill） | 相关进程调用点 | 0.5d | — |
| M2-ADV-10 | [persistence] 配置段实装（backup_on_write/retention_days） | machine.rs, config.rs | 0.5d | — |
| M2-ADV-01 | 多候选生成：同模块 ≥2 种翻译方案 + verifier 选优 | translator.md, verifier.md | 2d | Sprint C TIER-01c |

### 并行策略

- SCALE-02 → SCALE-01 → SCALE-LOCK → PETGRAPH-01 串行（关键路径）
- ADV-02/04/05/08/09/10 全部独立，可与 SCALE 并行
- ADV-01 需等 TIER-01c（分档逻辑完成后在 standard/full 档执行）

### 完成标志

- [ ] 3 agent 并行翻译 diamond fixture 5 模块无冲突
- [ ] 写隔离 merge 逻辑通过测试
- [ ] 降级 FFI 在 circular-deps fixture 上成功
- [ ] petgraph 副本隔离验证 + SQLite WAL 合规
- [ ] headless 全流程（init→analyze→run→review）无人工干预可完成

---

## §8 Sprint E：验证增强 + CLI 扩展（3-4 天，与 Sprint D 并行）

**目标**：增强测试覆盖，实现 5 个 CLI 新命令。

### 验证增强

| 任务 ID | 内容 | 文件 | 工作量 | 依赖 |
|---------|------|------|--------|------|
| M2-VER-01 | proptest 属性测试：图操作不变量 | graph/ 测试 | 1d | — |
| M2-VER-02 | cargo-fuzz：解析器健壮性 | fuzz/ 目录 | 1d（+24h fuzz） | — |
| M2-COV-01 | 覆盖率门禁：cargo-llvm-cov 集成 + ≥70% target | CI 配置 | 0.5d | — |
| M2-SCALE-03 | 增量图更新：file_fingerprints 跳过未变更文件 | build.rs, persist.rs | 2d | — |

### CLI 扩展（设计文档 06 §10.0.1 已定义的 5 个命令）

| 任务 ID | 内容 | 文件 | 工作量 | 依赖 |
|---------|------|------|--------|------|
| M2-CLI-01 | graph rdeps：反向依赖查询 | lib.rs, query.rs | 0.5d | — |
| M2-CLI-02 | graph cycles：完整 SCC 环检测 + 环路径输出 | lib.rs, topo.rs | 0.5d | — |
| M2-CLI-03 | graph export：导出为 JSON/DOT/Mermaid | lib.rs, 新 export.rs | 1d | — |
| M2-CLI-04 | validate config：校验 .rustmigrate.toml | lib.rs, validate/ | 0.5d | — |
| M2-CLI-05 | state update：乐观锁状态更新（--cas-version CAS） | lib.rs, machine.rs | 1d | — |

### 其他

| 任务 ID | 内容 | 文件 | 工作量 | 依赖 |
|---------|------|------|--------|------|
| M2-ERR-01 | P0 错误码枚举化（~15 条高频错误码 + CI 重试建议） | error.rs, 09-appendix | 1d | — |
| M2-PARITY-01 | PARITY.md 等价深度扩展 + KNOWN_DIFFERENCES 自动生成 | verifier.md | 0.5d | — |
| M2-CICD-01 | CI/CD 基础：GitHub Actions workflow + dogfooding fixture | .github/, fixtures/ | 1d | — |

### 并行策略

- Sprint E 与 Sprint D **完全并行**
- VER 系列、CLI 系列、ERR/PARITY/CICD 互相独立

### 完成标志

- [ ] proptest 1000 次无 panic
- [ ] cargo-fuzz 24h 无 crash
- [ ] 覆盖率 ≥70%
- [ ] 5 个 CLI 命令可用 + 测试覆盖
- [ ] 增量图更新：变更 1 文件 < 全量构建时间 50%
- [ ] P0 错误码枚举化完成
- [ ] CI workflow 在 PR 上自动运行

---

## §9 Sprint F：M2 验收（7-10 天）

**目标**：在 3 个真实 TS 中型项目上验证 M2 全部功能。

### 验收流程

1. **F1 项目选取 + 环境准备**（1-2 天）：
   - 3 个公开 TS 项目，5K-20K 行
   - 至少 1 个含 15-25 依赖的模块（覆盖中段上下文预算）
   - 至少 1 个含循环依赖（验证降级 FFI）
   - 环境搭建 + 初始 analyze 通过

2. **F2 自适应循环验证**（1-2 天）：
   - trivial 模块批量翻译正确
   - standard 模块翻译 + 测试通过
   - full 模块完整循环通过
   - 降档/升档机制触发并正确执行

3. **F3 并行 sprint 验证**（2-3 天）：
   - 3 agent 并行翻译，写隔离 merge 无冲突
   - sprint 自动推进
   - 性能门禁：≥1.5 模块/小时

4. **F4 质量验证**（1 天）：
   - proptest + fuzz 在验收项目上通过
   - 翻译膨胀率 <3.0x
   - cargo check + clippy 零 error
   - 覆盖率 ≥70%
   - 翻译正确性：人工审阅 + verifier 对抗审查覆盖

5. **F5 修复 + 复验**（2-3 天缓冲）：
   - 真实项目暴露的 AST 边界问题修复
   - 性能门禁未达标时的调优
   - 回归验证：M1 fixture 全通过 + 202 + M2 新增测试全绿

### 验收标准（完整版）

| # | 指标 | 标准 | 来源 |
|---|------|------|------|
| F1 | 规模 | 3 个 5K-20K 行项目，每项目 ≥3 模块完成 | PLAN §10 |
| F2 | 降级 | ≥1 模块降级 FFI 成功 | PLAN §10 |
| F3 | 并行吞吐 | P50 ≥1.5 模块/小时（3 agent） | 08 §M2 |
| F4 | P99 吞吐 | P99 ≥0.8 模块/小时 | 08 §M2→M3 升级判据 |
| F5 | SQLite 并发 | 冲突率 <10%，锁等待 ≤20ms | 08 §M2 |
| F6 | 性能无退化 | 相对 M1 基线 ≤±10% | 08 §M2 |
| F7 | 测试质量 | proptest 1000 无 panic + fuzz 24h 无 crash | PLAN §10 |
| F8 | 翻译膨胀 | <3.0x | 08 §M2 |
| F9 | 全流程耗时 | <60min 含多模块 | PLAN §10 |
| F10 | 覆盖率 | ≥70% | 08 §M2 |
| F11 | 图构建 | <10s（500 文件） | PLAN §10 |
| F12 | 回归 | M1 202 测试 + fixture 全通过 | 新增 |

---

## §10 任务全量索引

### 任务总数：53 项（+ 5 项验收活动）

> 注：M2-REFAC-08（ModuleStatus 转换表 can_transition_to）已在 M1 完成，不纳入 M2 任务。

| Sprint | 任务数 | 估时 |
|--------|--------|------|
| A 基础加固 | 12 | 2-3d |
| B 类型+图精度 | 9 | 5-6d |
| C 核心功能双线 | 8 | 4-5d |
| D 并行+高级 | 11 | 7-9d |
| E 验证+CLI | 12 | 3-4d（与D并行） |
| F 验收 | 5 项验证 | 7-10d |
| **合计** | **52+验收** | **~25-33d** |

### 完整任务 ID 索引

```
Sprint A: M2-DESIGN-01/02, M2-VER-04/05, M2-REFAC-05/06/07/13/14, M2-COMPAT-01, M2-ADV-06, M2-CTX-01
Sprint B: M2-REFAC-01/02/03/04/09/10/11/12/15
Sprint C: M2-TIER-01a/b/c/d, M2-ADV-07, M2-SCALE-P, M2-SCALE-SPRINT, M2-ADV-03
Sprint D: M2-SCALE-02/01, M2-SCALE-LOCK, M2-PETGRAPH-01, M2-ADV-02/04/05/08/09/10/01
Sprint E: M2-VER-01/02, M2-COV-01, M2-SCALE-03, M2-CLI-01/02/03/04/05, M2-ERR-01, M2-PARITY-01, M2-CICD-01
Sprint F: 验收
```

### 依赖关系总览

```
Sprint A（无依赖）
  ↓
Sprint B（REFAC-09 依赖 REFAC-01; REFAC-10 依赖 REFAC-09）
  ↓
Sprint C 线1（TIER-01 依赖 Sprint B REFAC-09/10）
Sprint C 线2（SCALE-P 依赖 Sprint A REFAC-06; ADV-03 依赖 DESIGN-01）
  ↓
Sprint D（SCALE-02 依赖 C 线2; ADV-01 依赖 C 线1 TIER-01c）
Sprint E（独立，与 D 并行）
  ↓
Sprint F（依赖 D + E 全部完成）
```

---

## §11 M2 推迟项（M2.5/M3 候选）

以下功能在设计文档中标注 M2 但经评估推迟：

| 功能 | 推迟到 | 理由 |
|------|--------|------|
| 状态机程序化（独立 orchestrator 二进制） | M2.5 | 工作量 XL，SKILL.md + CLI 加固足够。**注意：08 §M2 将此列为首要排序项，本计划有意偏离——M2 通过加固 CLI（VER-04/COMPAT-01/SCALE 系列）+ 保留 SKILL.md 编排实现同等可靠性，完整程序化推迟到 M2.5 积累更多编排经验后再做** |
| /goal 自主迁移循环 | M2.5 | 依赖状态机程序化 |
| 行为录制框架（VER-03） | M2.5 | 工作量 L，验收不强依赖 |
| 变异测试（cargo-mutants） | M2.5 | 验收不强依赖 |
| 图 schema 扩展（TypeAlias/Variable/Community） | M3 | 不影响迁移顺序 |
| FTS5 全文搜索 | M3 | <20K 行不需要 |
| Leiden 社区检测 | M3 | 无成熟 Rust crate |
| 规则治理工具化 | M3 | 规则量未达阈值 |
| 自定义 lint crate | 条件（M2 规则>15 条时） | |
| 降级决策学习（degrade_decision_history） | M3 | nice-to-have |
| 类型复杂度前置降级信号 | M3 | nice-to-have |
| migration-state.json 完整迁移脚本 | M2.5 | M2 先做版本检测 |
| 完整错误码体系（30-40 条） | M2.5 | M2 先覆盖 P0 |
| 跨文件方法调用档 2（per-file 类型环境） | M3 | 跨语言需求更明确时再建 |
| release-meta.json | M2.5 | 发版时补 |
| 适配器规则版本陈旧检测程序化 | M3 | |
| index.json 自动生成 | M3 | |
| Beads/AgentMemory 集成 | M3+ | |

---

## §12 质量门（同 M1，增加 M2 特有检查）

### Level 1-4 继承 PLAN §13

额外增加：

### Level 5：M2 特有

- [ ] 分档结果可观测（state 中记录 tier）
- [ ] 并行翻译无 data race（SQLite WAL 合规）
- [ ] 增量构建正确（变更文件重建，未变更跳过）
- [ ] 回归：M1 202 测试 + fixture 全通过

---

## §13 风险与缓冲

| # | 风险 | 概率 | 影响 | 缓解 |
|---|------|------|------|------|
| R1 | REFAC-10 跨文件方法调用工作量超预期 | 中 | 中 | 档 1 先做（低成本高收益），档 2 推 M3 |
| R2 | 并行 merge 冲突解决不可靠 | 中 | 高 | 方案 B 简化：编排器统一 merge + 逐个回滚 |
| R3 | 真实 5K-20K 项目暴露 M1 未覆盖的 AST 边界 | 高 | 中 | 图精度提升（Sprint B）+ 宽松验收 |
| R4 | 并行吞吐 <1.5 模块/小时 | 中 | 中 | 降 max_concurrent_agents=2 后重验 |
| R5 | TIER-01 分档判据误判（full 误判为 trivial） | 低 | 高 | 默认 full 兜底 + 失败自动升档 |

### 缓冲

- Sprint F 验收预留 5-7 天，含修复时间
- 若 Sprint D 超期 >3 天，砍 ADV-01（多候选生成）推 M2.5
- 若 SCALE 路径阻塞，优先保证 TIER-01（单机效率提升 ROI 更高）

---

## §14 执行协议

### 每个 Sprint 的交付流程

1. 质量门：`/gate` 4 层全过
2. 更新 `docs/STATUS.md`
3. commit 引用任务 ID（如 `feat(M2-TIER-01): 复杂度自适应循环`）
4. 独立分支提 PR
5. PR 审查：`/pr-review-toolkit:review-pr` + `design-checker` + `/code-review`
6. 修复 critical/important issues 后通知用户审阅

### 续接协议

同 PLAN §3 续接协议。新会话读：CLAUDE.md → STATUS.md → 本文件对应 Sprint 段。
