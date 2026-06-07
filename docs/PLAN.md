# rustmigrate 实施计划

> 本文件是项目执行的**唯一权威计划**。新会话读 CLAUDE.md → STATUS.md → 本文件即可接续执行。
> 结构:事务(Workstream) → Sprint(粗粒度目标+验收) → 任务(Sprint 开始时细化)。

---

## 一、事务(Workstream)划分

本项目的工作分为 **6 条并行事务流**,贯穿整个生命周期:

| # | 事务 | 说明 | 产出物 |
|---|------|------|--------|
| W1 | **CLI 工具开发** | Rust CLI `rustmigrate` 的所有命令实现 | `cli/` 源码 + 测试 |
| W2 | **Plugin 开发** | SKILL.md + SubAgent + Hooks + 适配器 | `plugin/` 全部内容 |
| W3 | **验证与测试** | 单元测试 + 集成测试 + 真实项目端到端 + 性能基准 | fixtures/ + CI green + 性能数据 |
| W4 | **质量保障** | code review + 交叉验证 + 回归检测 + 设计一致性 | 每 Sprint 的质量报告 |
| W5 | **知识沉淀** | 开发中发现的 learning/decision/pattern 持久化 | docs/learnings/ + docs/decisions/ |
| W6 | **项目管理** | Sprint 规划 + STATUS.md 维护 + 风险追踪 + 多会话续接 | PLAN.md + STATUS.md |

**任何 Sprint 的交付都必须同时覆盖 W3(验证) + W4(质量) + W5(知识)**,不能只有 W1/W2 的编码。

---

## 二、Sprint 总览与里程碑

```
Sprint 0 (M0 假设验证)     ─────→ GATE: 设计假设全部验证,M1 方案确定
Sprint 1 (M1 基础设施)     ─────→ GATE: CLI 核心命令 + Plugin 骨架可用
Sprint 2 (M1 核心流程)     ─────→ GATE: /migrate analyze 端到端跑通
Sprint 3 (M1 迁移循环)     ─────→ GATE: /migrate run + review 跑通单模块
Sprint 4 (M1 验收)         ─────→ GATE: 3 项目验收通过 = M1 交付
Sprint 5+ (M2 质量提升)    ─────→ GATE: 验证管线完整,翻译质量可靠
Sprint N+ (M3/M4)          ─────→ 多语言 + 完善
```

---

## 三、Sprint 详细规划

### Sprint 0: 假设验证（M0,预计 1-2 周）

**目标**:验证 6 个技术假设,产出决策,确定 M1 执行路径。

**事务覆盖**:
- W3: 每个 Spike 有明确 pass/fail 标准
- W4: Spike 结果交叉验证(同一 Spike 跑多次确认可复现)
- W5: 结论写入 `DESIGN_ASSUMPTIONS.md`
- W6: M0-GATE 决策写入 STATUS.md

| Spike | 验证内容 | 依赖 | Pass 标准 | Fail → Plan B |
|-------|---------|------|-----------|---------------|
| S0 | Plugin 加载 + crate 编译 | 无 | skill/agent/hook 三者 work；二进制 <50MB | 回退非 Plugin |
| S1 | SubAgent 3 次串行可靠性 | S0 | ≥4/5 完成全部步骤 | Plan B3 混合编排 |
| S2 | rust-analyzer LSP | S0 | 秒级诊断反馈 | PostToolUse Hook |
| S3 | tree-sitter TS 精度 | S0 | exports/imports/calls ≥90% | TS Compiler API |
| S4 | SKILL.md 长指令跟随 | S0 | >2000 字遗漏率 ≤20% | 拆短 Skill |
| S5 | Beads/AgentMemory | S0 | 评估结论 | 不用,走纯文件 |

**Sprint 0 GATE 验收**:
- [ ] `DESIGN_ASSUMPTIONS.md` 已写,每个 Spike 有结论
- [ ] M1 方案已确定(基线 or Plan B3)
- [ ] 发现的 Plan B 触发项已更新到 PLAN.md

---

### Sprint 1: M1 基础设施（预计 2-3 周）

**目标**:CLI 核心命令可用 + Plugin 基础骨架可加载 + 开发工具链就绪。这是**并行度最高的 Sprint**——CLI 和 Plugin 完全独立开发。

**事务覆盖**:
- W1: CLI 13 个命令的基础实现(graph build 是关键路径)
- W2: Plugin hooks 实战化 + SKILL.md 框架
- W3: CLI 每个命令有 insta 快照测试；CI green
- W4: code review(对照 guppy/ast-grep 源码交叉验证实现质量)
- W5: 记录 crate 集成中的坑到 `docs/learnings/`

**关键交付**:
| 事务 | 交付物 | 验收标准 |
|------|--------|---------|
| W1 | CLI 13 命令全部实现 | `cargo nextest` 全过；insta 快照覆盖 |
| W2 | Plugin hooks 实战 + SKILL.md 框架 | hook fire 可观测；SKILL.md 可加载 |
| W3 | CI pipeline green | `.github/workflows/ci.yml` 全过 |
| W4 | 实现质量 review 报告 | 无 blocker/high |

**Sprint 1 GATE 验收**:
- [ ] `rustmigrate graph build` 对真实 TS 项目产出有效 `source-graph.db`
- [ ] `rustmigrate graph topo-sort` 输出正确迁移顺序
- [ ] CI 全绿(check + test + clippy + deny)
- [ ] Plugin 安装后 hook 可 fire

---

### Sprint 2: M1 核心流程——`/migrate analyze`（预计 2 周）

**目标**:`/migrate analyze` 端到端跑通(从空项目到产出 source-graph + 规则 + 测试基础设施)。

**事务覆盖**:
- W1: CLI 无新增(Sprint 1 已完成)
- W2: analyzer + translator(规则) + scaffolder SubAgent 实现 + SKILL.md analyze 完整版
- W3: 对 fixture TS 项目跑 analyze,验证产出物完整性
- W4: SubAgent 产出物 Schema 校验 + 交叉验证(多次跑看一致性)
- W5: 记录 SubAgent 编排中的 learning

**关键交付**:
| 事务 | 交付物 | 验收标准 |
|------|--------|---------|
| W2 | 3 个 SubAgent(analyzer/translator-rule/scaffolder) | 各产出物符合 06 §10.2 接口表 |
| W2 | SKILL.md analyze 完整版 | 7 步序列可靠执行(≥80%) |
| W2 | TS 语言适配器 | detect/extract-types/extract-deps 对 fixture 项目 work |
| W3 | fixture 项目 analyze 通过 | source-graph.db + porting/ + test-fixtures/ 非空 |

**Sprint 2 GATE 验收**:
- [ ] `/migrate analyze` 对 fixture 项目产出完整的 `.rust-migration/` 目录
- [ ] `migration-state.json` 状态正确(scaffold 完成)
- [ ] 5 次独立执行,成功率 ≥80%

---

### Sprint 3: M1 迁移循环——`/migrate run` + `/migrate review`（预计 2-3 周）

**目标**:单模块迁移内循环(Phase A/B + 验证)跑通；review 仪表板可用。

**事务覆盖**:
- W2: translator(翻译模式) + verifier SubAgent + SKILL.md run/review 完整版
- W3: 对 fixture 项目的一个纯函数模块完成迁移 + Tier 0 通过
- W4: verifier 对抗审查质量验证(人工抽检是否真的在找不等价)
- W5: 翻译过程中发现的 pattern/anti-pattern 写入 references/

**关键交付**:
| 事务 | 交付物 | 验收标准 |
|------|--------|---------|
| W2 | translator(翻译) + verifier SubAgent | Phase A/B 双阶段 + 对抗审查 |
| W2 | SKILL.md run + review 完整版 | 断点续传 work；降级路径 work |
| W3 | 1 个模块迁移完成 | Rust 代码 + Tier 0 通过 + PARITY.md 更新 |
| W3 | `/migrate review` 输出 | 仪表板可读,覆盖率数据正确 |

**Sprint 3 GATE 验收**:
- [ ] 至少 1 个纯函数模块从 TS 迁移到 Rust,Tier 0 全过
- [ ] 断点续传验证:中断后恢复不丢状态
- [ ] `/migrate review` 正确展示 PARITY.md + 覆盖率

---

### Sprint 4: M1 验收（预计 1-2 周）

**目标**:3 个真实 TS 项目端到端验收,满足所有 M1 验收指标。

**事务覆盖**:
- W3: 3 项目 × 各 ≥1 模块迁移 + 全量验收指标跑通
- W4: 性能门禁(图构建 <10s,全流程 <40min) + 边界测试(上下文溢出、降级)
- W5: 总结 M1 开发经验写入 `docs/learnings/m1-retrospective.md`
- W6: 更新 PLAN.md 标记 M1 完成,规划 M2 Sprint

**Sprint 4 GATE 验收(= M1 交付)**:
- [ ] 3 个 TS <5K 行项目,每项目 ≥1 模块迁移完成
- [ ] 含 ≥1 个 15-25 依赖的模块
- [ ] Tier 0 全过 + insta 快照 100%
- [ ] 图构建 <10s (100 文件)；全流程 <40min
- [ ] 断点续传验证通过
- [ ] 上下文溢出触发拆分路径(边界验证)

---

### Sprint 5+: M2 质量提升（粗线条,Sprint 4 后细化）

| Sprint | 目标 | 关键交付 |
|--------|------|---------|
| 5 | 验证管线增强 | proptest 完整版 + cargo-fuzz + 行为录制框架 |
| 6 | 高级功能 | 多候选生成 + 降级路径(FFI) + graduate |
| 7 | 并行与规模 | Workflow 批量 + worktree 隔离 + M2 CLI 命令 |
| 8 | M2 验收 | 3 个 5K-20K 项目多模块迁移 |

### Sprint N+: M3/M4（M2 后细化）

- M3: Python 适配器 + PyO3 + 统一差异测试
- M4: C/C++ + Go + Kani + 社区生态

---

## 四、质量保障机制（W4,每个 Sprint 必须）

| 机制 | 做什么 | 频率 |
|------|--------|------|
| **CI 全绿** | check + clippy + nextest + deny + shellcheck | 每次 push |
| **Code Review** | 重要模块完成后跑多维度 review(正确性+惯用性+设计一致性) | 每个 PR/每个模块 |
| **对照验证** | 对照 guppy/ast-grep/codegraph 源码验证实现是否合理 | CLI 核心模块完成时 |
| **设计一致性检查** | 实现是否与 docs/design/ 一致(接口/命名/行为) | 每个 Sprint GATE |
| **回归测试** | insta 快照 + CI nextest | 每次 push |
| **性能基准** | 图构建/全流程/上下文预算 三个硬指标 | Sprint 1+ 每个 GATE |
| **Dogfooding** | 用我们的工具迁移 fixture 项目 | Sprint 2+ |

---

## 五、多会话续接协议

### 写入方(会话结束前)

1. 更新 `docs/STATUS.md`:当前 Sprint / in-progress 任务 / 下一步 / 阻塞项
2. 已完成的任务在本文件标 `[x]`
3. commit message 引用任务 ID(如 `feat(M1-CLI-03): graph build`)

### 读取方(新会话开始)

1. 读 `CLAUDE.md` → 了解项目+约束+开发命令
2. 读 `docs/STATUS.md` → 知道"我在哪"
3. 读本文件对应 Sprint 段 → 知道"要做什么,怎么算完"
4. 如果任务需要细节 → 读 `docs/design/` 对应章节

### 粒度规则

- **粗粒度(本文件)**:Sprint 目标 + GATE 验收(全局视角,不经常变)
- **细粒度(STATUS.md)**:具体当前任务 + 进度(每次会话更新)
- **设计细节(docs/design/)**:按需查阅,不重复搬运到本文件

---

## 六、风险追踪

| 风险 | 影响 | 缓解 | 状态 |
|------|------|------|------|
| Plugin API 不如预期 | 全部 | Spike 0 验证 | 待验证 |
| SubAgent 编排不可靠 | Sprint 2+ | Plan B3 备选 | 待验证 |
| tree-sitter 精度不够 | CLI graph build | Spike 3 + OXC 备选 | 待验证 |
| 真实 TS 项目太复杂 | Sprint 4 验收 | 先选简单项目,逐步升级 | 待选 |
| 上下文预算溢出 | Sprint 3 | 拆分策略已设计 | 待验证 |
