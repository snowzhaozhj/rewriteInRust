> [返回主索引](./README.md)

# 四、执行模式（Sprint 循环模型）

## 4.1 从线性阶段到 Sprint 循环

原设计的线性阶段划分在实际执行中存在问题：
- 大项目不可能等所有测试搭好再开始迁移
- 迁移过程中会发现新规则，需要回头更新 PORTING 规则
- 不同模块可能处于不同阶段

**修订**：改为 Sprint 循环模型，分两层循环。

## 4.2 外循环：Sprint 级（跨会话/天/周）

```
Sprint N:
  1. Sprint Planning
     - 选择本 Sprint 要迁移的模块（按拓扑排序）
     - 源项目变更检测（双轨/Strangler Fig 策略）— 脚本化执行 `rustmigrate graph build` 增量重建并比对指纹
       对比源项目当前版本 vs source-ref/ 基线，STRUCTURAL 变更命中目标模块时自动告警；IDENTITY_LOST（文件消失）时暂停要求用户处理（见 § 4.6.1）
     - 源项目构建冒烟检查：重跑源项目 smoke-build（如 `npm ci && npm test -- --passWithNoTests`）确认 FFI 桥接前置条件仍满足；失败则 fail-fast 并提示修复源环境后再继续
     - 确认 PORTING 规则是否需要更新
     - 确认测试基础设施是否就绪

  2. 执行（多个 Work Unit）
     - 每个 Work Unit = 一个完整的 Claude Code 会话
     - 每个 Work Unit 迁移 1-3 个模块
     - 产出：Rust 代码 + 测试 + MDR

  3. Sprint Review（由 `/migrate review` 触发，扩展为完整 Sprint Review 流程）
     - 集成验证（Tier 0 + Tier 1 + 按需 Tier 2）— 由 `full-verify.sh` 执行
     - 更新 PARITY.md — 由 Skill 主上下文自动更新
     - 回顾 PORTING 规则，追加新发现的规则（附 changelog）— 人工 + AI 辅助
     - 更新 KNOWN_DIFFERENCES.md — 由 verifier 即时写入，Review 时人工审批
     - **知识沉淀**（[见文档体系 > 增量知识沉淀](./05-documentation-system.md#611-增量知识沉淀架构)）：提取 patterns/anti-patterns，写入 SPRINT_LEARNINGS.md — verifier 提取 + 人工审阅
     - 评估是否需要调整迁移策略 — 人工决策

  4. Sprint Retrospective
     - 哪些规则频繁触发失败？→ 补充到 PORTING 规则
     - 哪些工具信噪比低？→ 调整 Tier 级别
     - 上下文管理是否够用？→ 调整模块粒度
     - requires_manual_review 积压数量（观测指标）：> 总模块数 20% 时记录策略反思
       （不强制自动调整，由人类在下一 Sprint 决策清空或调整策略）
```

**行动指南**：每个 Sprint 以 `migration-state.json` 中的 Sprint 元数据为准，包含 Sprint 目标、已完成模块、阻塞项。

**循环依赖处理（破环：SCC 折叠为 composite，M2-SCALE-SCC，见 [MDR-004](../decisions/004-scc-fold-break-cycle.md)）**：源码循环依赖（含 re-export 间接环）**不再**拒绝填充 + 人工破环/降级。`state populate-modules` 用 Tarjan SCC 把每个强连通分量**缩点折叠为一个 composite 模块组**（`ModuleState.member_files` 列出组内全部互引源文件，module key=组内字典序最小成员），在缩点后无环的 DAG 上排 sprint 层级。**核心论据**：Rust 同一 crate 内 mod 之间允许互相 `use`（mod 间循环引用合法，只有 crate 间禁环），故源码环不是翻译障碍，无需破环/shared-types/FFI。**折叠后翻译粒度=单文件**（SCC 是编译门禁单元≠翻译单元，整组一次会撑爆上下文）：契约+stub→契约门（stub `cargo check`）→逐文件填空（签名锁）→整组编译门，详见 [MDR-006](../decisions/006-scc-per-file-stub-first.md)。FFI 切分退化为「单个 SCC 大到超上下文预算」时的兜底（当前 TODO，未实现触发路径）。
> **语义分叉**：`graph topo-sort` 命令（纯排序原语，Kahn 算法）对有环图**仍**返回 E002 `CyclicDependency`（见 [04 § 5.7.6](./04-toolchain.md#576-图查询能力清单)）——破环收口在 `populate-modules` 的缩点，而非 `topo-sort`，是有意为之。完整 SCC 检测能力（`graph cycles`）为 M2 交付。

### 4.2.1 执行模式分层

| 模式 | 入口 | 适用场景 | 并行度 | 阶段 |
|------|------|---------|--------|------|
| **Skill 交互式** | `/migrate analyze`、`/migrate run`、`/migrate review` | 分步调试、学习流程、小项目 | 串行 | MVP (M1) |
| **Workflow 批量** | `ultracode` workflow 定义文件 | 多模块并行迁移、CI/CD 集成 | 多 agent 并行 | M2+ |
| **/goal 自主循环** | `/goal "迁移 module X, Y, Z"` | 自主迁移循环（analyze→run→review 自动串联） | 串行但自主 | M2+ |

**Workflow 批量模式要点**（M2 阶段设计）：
- 每个 agent 在独立 worktree 中工作（避免文件冲突）
- 按拓扑排序分批，同一批内可并行
- 每个 agent 执行 `/migrate run` 的内循环
- 汇总阶段合并结果到主分支

## 4.3 内循环：模块级（单会话内）— Phase A/B 双阶段翻译

> **步骤编号映射**：本节（03）从 Step 0 开始编号（含源文件保留、上下文加载两步前置），09 附录 B SKILL.md 骨架将前置步骤合并，Phase A 起编为 Step 2。对应关系：03 Step 3 = 09 Step 2（Phase A），03 Step 4 = 09 Step 3（对抗审查），03 Step 5 = 09 Step 4（Phase B），03 Step 6 = 09 Step 5（测试验证）。

```
Work Unit（一个 Claude Code 会话）:

  Step 0: 源文件保留（首次迁移时执行一次）
    - 将待迁移模块的源文件复制到 `.rust-migration/source-ref/` 作为参考副本
    - 锁定源码版本（记录 commit hash 到 migration-state.json）
    - 原则：源文件保留直到 `/migrate graduate` 毕业（借鉴 Bun 的 .zig/.rs 并存模式）

  Step 1: 上下文加载
    - 读取 migration-state.json 确认当前任务
    - 读取 PORTING 规则中相关条目
    - 读取目标模块源码 + 依赖接口（优先读 source-ref 中的锁定版本）
    - **外部依赖可映射性预检**：解析模块 import 语句（tree-sitter/adapter extract-deps.sh），
      将外部包名与 `porting/dependency-mapping.md` 交叉核验；存在无映射条目时输出警告，
      用户可选：(a) 继续翻译、(b) 预降级为 FFI/skip、(c) 中止并先更新 dependency-mapping
      （MVP 为信息性警告，不阻塞）

  Step 2: 语义解构
    - AI 生成意图摘要（纯文本，不含源语言语法）
    - 意图摘要必须逐项覆盖 7 个核心字段（translator 系统提示要求，模板见 [09 附录 E](./09-appendix-schemas.md#附录-e意图摘要module-intentmd内容规范)）：
      ① 标题/目的 ② 公开接口签名 ③ 前置/后置条件 ④ 错误处理方案 ⑤ 并发模型 ⑥ 关键边界值处理 ⑦ 可观测副作用（纯函数填「无」）
      （RULE-3 整数溢出 / RULE-22 异步运行时等核心规则涉及的维度须展开说明）
    - 识别关键语义点：错误处理、并发、状态管理
    - **依据范围（消除"行为语义 vs 实现 bug"歧义）**：意图摘要捕获**源码实现的实际行为**（不是"应有的正确语义"），
      因为 Phase A 1:1 忠实翻译的目标是最大化兼容性而非修复原项目 bug——复现 bug 本身不违设计。
      但若识别出源码 bug（如越界未检查、整数溢出未处理），不得静默复刻：translator 须在 MDR 标记
      `bug_replica: true`，列出源码 bug 的具体位置与内容，供后续**人工确认是否应修复**——
      使"复现 bug 还是修复"成为有据可查的人工决策点，而非隐蔽的"意图摘要不准"。

  Step 2.5: 意图确认门禁（人类决策点，MVP 默认开启）
    - 向人类展示意图摘要全文，暂停等待确认后才进入 Phase A
    - 意图摘要是翻译的语义契约，错误意图会污染整个内循环，故须前置拦截
    - 与 § 7.4 Approval Token / 不自动宣布成功属同类人类决策点
    - 确认方式沿用 § 4.2.1 Skill 交互式模式（不新增 CLI 命令）
    - 交互规范（详见下方「§ 4.3.1 意图确认交互规范」）：摘要长度指南、用户应拒绝的 5 个特征、
      修订流程（最多 2 轮自动修订，第 3 轮升级 manual_review）、三模式 auto_confirm_intent 默认值
    - power-user 可在 .rustmigrate.toml 设 auto_confirm_intent=true 跳过
      （各模式默认值见下方 § 4.3.1 三模式表）

  Step 3: Phase A — 忠实翻译
    - 生成 Rust 代码，优先保持与源码的 1:1 对应（便于 diff 对照审查）
    - Phase A 不做优化：不得删除死代码、辅助函数、冗余字段或内联未使用常量
      （惯用化优化全部留到 Phase B）
    - 非平凡函数加 PORT NOTE 注释，标注源码行号范围或等价锚点（供 Step 4.5 结构校验）
    - Private 方法默认翻译（不省略），保持结构完整性
    - 标记系统（借鉴 Bun 的 TODO(port) 纪律——累积 2,327 个标记后统一清理）：
      - TODO(port) — 标记未完成项（仅 Phase A 翻译期间允许累积；进入 Phase B 后禁止新增）
      - PERF(port) — 标记已知性能问题
      - PORT NOTE — 标记翻译决策（说明为什么选择这种翻译方式）
    - TODO(port) 清理纪律：
      - 阶段约束：Phase A 遗留的 TODO(port) 须在 Phase B + 测试阶段（Step 5/6）全部解决；
        模块到达 `done` 的前置条件是 TODO(port) 计数 = 0（由 verifier 在 Step 6 检查点保证）
      - 趋势量化：每个 Sprint Review 统计当前 TODO(port) 总数，与前一 Sprint 同模块对比，
        净增数必须 ≤ 0；累计消除率（本 Sprint 消除数 / 上次 Sprint 总数）每轮 ≥ 10%
      - 报告口径：`/migrate review` 报告按模块分解 TODO(port) 计数（非仅总数）
    - F1 反馈：rust-analyzer LSP 自动诊断（秒级）

  Step 4: 对抗性审查
    - verifier SubAgent 对 Phase A 产出物执行对抗性审查
    - 逐维度比对源码与翻译结果（使用 7.7 节探测维度清单，含第 9 维「意图一致性」）
    - 须读取 `{module}-intent.md`，核对 Phase A 代码是否符合意图摘要 7 字段（偏离记为「意图漂移风险」）
    - 产出物：审查报告（差异列表 + 修正建议）

  Step 4.5: Phase A 结构校验门禁（进入 Phase B 前）
    - Skill 主上下文在 verifier 返回后执行 `rustmigrate stats compare`（确定性脚本门禁，与 F2 verify.sh 同级）校验 Phase A 是否保持 1:1 结构（确认翻译器未提前优化）：
      函数数量比 0.9x–2.0x、代码行数比 1.2x–3.0x、
      主控制流（循环/条件分支）数量与嵌套层级按源码 AST 大致对应
    - 门禁阈值＝§ 7.5 评分卡的**告警阈值上界**（函数数 > 2.0x、行数 > 3.0x 触发告警）：
      门禁在「越过告警线」时才拦截，留出 §7.5 健康范围（函数 0.9x–1.3x / 行数 1.2x–2.0x）
      与告警线之间的缓冲，避免高频误拦正常翻译。阈值依据与 M1 校准计划见 § 7.5 阈值说明
    - 比例越界 → 标记「Phase A 疑似已优化」，要求 translator 以忠实保留模式重做 Phase A
      再进入 Step 5（门禁，非软提示）。若重做后仍越界，标记 `requires_manual_review` 并进入 paused 状态（与 Phase B 耗尽路径对齐）
    - **跨侧口径限制（M2-ADV-06 实现确定）**：`stats compare` 源侧用 tree-sitter AST 精确计数、
      Rust 侧用轻量词法扫描（M2 刻意不引 `tree-sitter-rust` 以省依赖），两侧口径无法严格对齐——
      源侧函数含箭头函数/方法、Rust 侧数 `fn` 不含闭包；源侧控制流集 `if/for/while/do/switch`、
      Rust 侧词法集含 `else/loop/match`；且 AST 精确深度 vs `{}` 配平启发式算法本身不同。加之跨语言
      「函数数/嵌套」可比性天然弱。**结论**：`函数数量比` / `控制流嵌套比` 仅作**粗粒度告警信号**
      （配合上述宽缓冲带使用，看膨胀方向），不作精确判定；`代码行数比`（tokei 跨语言同口径）是门禁
      **主依据**。输出 JSON 以 `method` 字段（`tree-sitter` / `lexical-scan`）透明标注每侧计数手段供判读。
      要让函数/嵌套比达到精确可比，需 Rust 侧改走 AST（引 `tree-sitter-rust` + 统一控制流节点映射），
      推迟到 M3 多语言前端时一并评估。
    - **源语言分派**（M3-VAL-02 更新）：`stats compare` 源侧解析按 `source_language` 分派——TypeScript 与
      **Python 已支持**（Python 走 `tree_sitter_python` + `is_py_control_flow`，控制流节点集
      `if/for/while/with/try/match`，与 `lang/python.rs` 机械性判定一致）。C/Go 尚未实现，
      `source_max_nesting` 返回 `NotImplemented`，由 CLI 层冒泡兜底**优雅降级为 warning + `data:null`**
      （不报错、不静默半残 0 比值，M2-ADV-06 保护仍在）。

  Step 5: Phase B — 编译修正 + 惯用化优化
    - 基于审查报告修正语义偏差
    - 允许重写（非直译）的范围**仅限**以下三类，且各有精确边界（verifier 据此判超界）：
      ① 并发模式选择：仅改**内部**同步原语（Arc/Mutex vs channel vs 其他），不改对外接口、
         错误流程或可观测副作用——`fn foo()→async fn foo()` 即改签名，属超界
      ② 取消安全性重构：仅调整代码结构以保证 Future 被 drop（取消）时状态一致，不改业务逻辑
      ③ 局部性能优化：仅**单函数内**算法改进（如 O(n²)→O(n log n)），不跨函数/不改数据结构对外契约
    - 三类重写**均不得改变**函数签名、错误语义或可观测副作用顺序，边界定义如下：
      - **「可观测副作用顺序」定义**：相同输入下，函数对外部状态（文件系统、网络、共享数据结构的可见性）
        的修改**相对顺序**不变。改内部同步原语（如 Mutex→channel）仅在保证数据一致性
        （happens-before 关系等价）的前提下允许——若改为 channel 导致原本同步的消息顺序变成异步乱序，属超界。
      - **取消安全重构**：仅调整代码结构以满足 Rust 对 Drop 的要求，不改变前置/后置条件或错误传播路径
        （可重排代码块以确保资源在 drop 前释放，但不改资源获取/释放的**因果关系**）——
        若改变了错误处理时机（如先释放资源再检查错误），属超界。
    - 任何此类重写须记录 MDR（在 Step 7 写入），按改动类别填对应必填字段（MDR schema 见 [05 § 6.5](./05-documentation-system.md#65-mdr--迁移决策记录)）
    - 惯用 Rust 优化（消除翻译腔）——边界定义：消除翻译腔 = 不改变 Step 6 四项审查维度（入参/出参类型、返回时机、错误处理流程、可观测副作用顺序）的表达方式变更；改变任一项即归入上述三类重写并须记 MDR
    - 编译失败 → 先跑 `cargo fix --allow-dirty`（确定性自动修复）→ 剩余错误交给 AI 修复（最多 3 轮）；3 轮后仍失败 → 暂停，触发降级流程（生成降级分析报告，等待人类通过 `/migrate run --degrade=ffi` 确认，详见 [09 附录 B Step 4](./09-appendix-schemas.md#附录-bmvp-skill-的-skillmd-骨架)）
    - 断点续传：若 Phase B translator SubAgent 失败或超时，translator 将当前 Phase B 状态持久化到
      `.../attempts/{module}-phase-b-partial.rs`，并记 substatus=`phase_b_failed_at_round_N`；
      `/migrate run --retry` 从同一检查点重入 Phase B（不回退到 Phase A）
    - F1（rust-analyzer 诊断）反馈循环（此步骤只做编译验证和惯用化优化，不生成测试）
    - **loom/shuttle 插桩**（仅并发模块）：translator Phase B 完成时按原语类型分流——std::sync 并发添加 cfg-gated 导入（`#[cfg(loom)] use loom::sync::Mutex; #[cfg(not(loom))] use std::sync::Mutex;`）使 L7 测试激活 loom 状态空间探索；tokio::sync/async 并发编写 shuttle executor 测试（`shuttle::check_random(|| { rt.block_on(async { ... }) })`）激活 shuttle 调度探索。选型见 [04 § 5.3](./04-toolchain.md#53-tier-1推荐画像自动启用)
    - 注意：意图摘要的生成依据是**源语言语义**（接口契约 + 行为 spec），而非 Phase A 代码；
      因此 Phase B 的合规重写不应使意图摘要失效。verifier 在 Step 6 须按 § 7.7 维度 9
      校验 MDR 中的语义变更是否仍符合意图摘要 7 字段（偏离则按下方 Step 6 流程处理）

  Step 6: 测试生成与验证（针对 Phase B 最终代码）
    - 测试在 Phase B 完成后生成，**以 Phase B 最终 Rust 代码为目标**（非 Phase A）
    - 由 verifier SubAgent 生成测试（非 translator 同步生成）
    - Phase B 改动审查清单（verifier 系统提示要点，逐项核对，任一改变即标记「超界」并要求回退/降级）：
      入参/出参类型、返回时机（同步↔异步）、错误处理流程、可观测副作用顺序——
      这四项是「意图摘要 vs Phase B」的一致性对比维度，均须保持不变
    - 语义影响复核：若 Phase B 的 MDR 涉及错误处理策略、返回值语义或并发可见性的改变，
      verifier 须先确认该改写与意图摘要保持一致（parity）——一致则照常生成测试；
      不一致则视为「语义漂移」：**标记为 super-boundary 并记录偏差原因到 MDR**，确认该改写仍符合
      源语言原意后，在 MDR 明确说明语义变更与原意的映射，方可更新 `{module}-intent.md` 并生成测试。
      **同一模块的意图摘要内容更新超过 1 次则标记该模块 `requires_manual_review`**（防止意图摘要被随意改动失效）
    - 针对性测试（避免「用 Phase B 验证 Phase B」的循环）：对 MDR 标记的三类允许改动，
      verifier 在本步生成有针对性的测试——并发改动跑 loom/shuttle、性能改动跑 criterion；
      不做事前 proptest 重放（proptest 对比本身是 L2 等价测试，不是超界改动检测手段）。
      **Phase B 针对性测试自检**：verifier 须在 `test-fixtures/{module}-phase-b-coverage.yaml` 中记录每个 Phase B MDR 对应的针对性测试文件路径；MDR 数 > 0 但该 yaml 为空或条目数 < MDR 数时，阻塞 `done` 转移
    - F2 反馈：cargo test + clippy（分钟级）
    - 测试失败 → 分析原因 → 修复或记录到 KNOWN_DIFFERENCES.md

  Step 7: 产出物更新
    - 更新 PARITY.md（模块状态）
    - 写 MDR（如有架构决策）
    - 更新 migration-state.json
```

### 4.3.1 意图确认交互规范（Step 2.5 落地细节）

意图摘要长度无硬性上限（由编排器按源码行数动态控制，见 [02 § 上下文加载](./02-architecture.md)），下表给出**参考目标**，过长（远超目标）本身是「冗余」拒绝信号：

| 源码规模 | 意图摘要目标长度 |
|---------|----------------|
| < 100 行 | < 200 词 |
| 100–500 行 | < 500 词 |
| 500–2K 行 | < 1000 词 |

**用户应拒绝/修订意图摘要的 5 个特征**：① 缺 § 4.3 Step 2 的 7 个核心字段之一；② 含源语言语法/术语（应为纯语义）；③ 与 porting/ 规则冲突；④ 过度冗余（远超上表目标）；⑤ 跳过了错误处理或并发模型说明。

**修订流程**：「确认 / 修订」二选一。修订时用户补充约束 → translator 重新生成摘要 → 再次暂停确认。最多 **2 轮自动修订**；第 3 轮仍不满意则升级 `requires_manual_review`（避免无限循环确认）。

**三模式 `auto_confirm_intent` 默认值**：

| 模式 | 默认值 | 说明 |
|------|-------|------|
| Skill 交互式 | `false`（强制） | MVP 主路径，必须人工确认 |
| /goal 自主循环 | `false`（可配，强烈建议保持） | 关闭则错误意图无人审视，污染内循环 |
| Workflow 批量 | `false`（建议批前由用户对本批模块一次性签名授权） | M2 范围 |

> **可靠性假设（M0/M1 验证，写入 DESIGN_ASSUMPTIONS.md）**：① 「translator 意图摘要生成可靠性 ≥ 90%」——由 M0 Spike 采样验证；② 「/goal 模式下 `auto_confirm_intent=false` 时最终代码质量仍达标（§ 7.5 `final_score` ≥ 75）」——由 M1 实测项目验证。假设不成立时的降级路径：关闭对应模式的自动化或强制人工审查。

**降级路径与灰色情况处理**：
- **假设不成立时的降级优先级**：若 M0 实测可靠性 < 85%，按模式分级收紧默认值——Skill 保持强制确认；/goal 强制 `auto_confirm_intent=false`；Workflow 改为批前人工签名（不退回 LLM 驱动完整翻译，意图摘要中间步骤保留）。
- **灰色情况处理权限**：Phase B 期间 verifier 发现摘要与代码不符，视为「意图漂移」（区别于「摘要表述不清」），触发更新 `{module}-intent.md` 并由 verifier 记入 MDR；该修改**不计为违反 Step 2.5 人工确认**——因其源自后续代码生成的新发现，而非原始摘要问题（更新频率上限见 § 4.3 Step 6）。
- **验收标准映射**（取代独立质量配置项）：意图摘要质量以「§ 4.3.1 用户应拒绝的 5 个特征 ↔ § 4.3 Step 2 的 7 字段完整性 ↔ [09 附录 E](./09-appendix-schemas.md#附录-e意图摘要module-intentmd内容规范) JSON Schema」三者映射判定，不引入 `intent_summary_min_quality` 配置项。

### 4.3.2 复杂度自适应分档（TIER-01，M2）

> M2 引入。M1 对所有模块跑完整 11 步内循环；M2 按模块复杂度分档，让简单模块走短路径，减少无效 token 消耗（决策见 [PLAN-M2 §2 D4](../PLAN-M2.md)）。分档结果记入 `ModuleState.tier`（`Trivial`/`Standard`/`Full`），由 `rustmigrate detect` 在 per-module AST 语义特征上自动评估（M2-TIER-01a），analyzer 输出语义信号供复核。

**与 `NodeData.complexity` 的关系（消歧）**：两者是**正交的两套分级**，不可混用——

- `NodeData.complexity`（[04 § 5.7.1](./04-toolchain.md#571-图数据模型)，`Simple`/`Moderate`/`Complex`）按 **LOC** 估算，是图层面的规模标注，用于批次划分/优先级提示。
- `ModuleState.tier`（本节，`Trivial`/`Standard`/`Full`）按 **AST 语义特征**驱动，**决定翻译循环路径**。**短 ≠ 低风险**（`utils.clamp` 几行但含 NaN 陷阱 → `Full`），故 tier 不从 complexity 推导。

> 实现注：M2 同处**删除 M1 起恒为 `Low` 的死字段 `ModuleState.risk`/`default_risk()`**（零读取点），由「填 risk」改为「填 tier」，详见 [PLAN-M2 §2 D4](../PLAN-M2.md)。分档理由（危险信号，如 `["async","try-catch"]`）写入 run 日志 + `AttemptRecord` 供失败升档定位，**不**新增持久化 `tier_signals` 字段（过度设计）。

**分档判据（基于 AST 可见语义特征，非 LOC）**：

| 档位 | 判据 | 循环 | 测试 |
|------|------|------|------|
| **Trivial** | 纯类型文件（仅 interface/type/enum 导出）或常量文件或 barrel（仅 re-export） | 批量直翻 + 编译 + 签批，**跳 Phase B** | 仅验编译 + 导出可见性 |
| **Standard** | 无下列任何危险信号的普通模块 | 保留意图摘要 + Phase A + 审查 + 测试 | 语义等价测试 |
| **Full** | 含**任一危险信号**：副作用 I/O / 并发(Promise.all/async) / 错误路径(try-catch) / 数值计算 / 全局状态 / 动态类型操作 / 条件类型/泛型约束 / unknown·never 不可判定 | 完整 11 步（见 § 4.3） | 完整 Tier 0+1+2 |

**关键原则**：
- 任一危险信号或 `unknown` → **不降档**（默认 `Full` 兜底）；分档可观测（state 记录结果）+ 失败自动升档重跑（M2-TIER-01d）。
- **维度 9 意图一致性永不跳过**（见 § 7.7）。`Trivial` 无运行时行为可比对，其维度 9 **退化为「导出符号 + 类型签名集合一致性」核对**——源导出的 `interface/type/enum/const` 名称与签名 ⇿ Rust 侧 `pub` 类型/常量逐项对应（核对类型契约而非行为契约，故「永不跳过」成立，`Trivial` 不生成完整 7 字段 intent.md）；`Standard`/`Full` 仍为完整 7 字段核对。

## 4.4 三层反馈循环

| 层级 | 触发时机 | 延迟 | 内容 | 处理方式 |
|------|---------|------|------|---------|
| F1 编译反馈 | 每次写入 .rs 文件 | 秒级 | **rust-analyzer LSP 自动诊断** | 自动反馈给 LLM 重试 |
| F2 测试反馈 | 模块翻译完成 | 分钟级 | 测试失败 + clippy 警告 | AI 分析修复或标记差异 |
| F3 集成反馈 | Sprint Review | Sprint 级 | 集成测试 + 覆盖率 + 性能基准 | 团队决策是否通过 |

**行动指南**：F1 由 rust-analyzer LSP 自动提供，无需 Hook 配置；F2 在 Skill 的 SKILL.md 中通过分步指令要求"翻译步骤完成后执行 `verify.sh` 验证命令"；F3 由 `/migrate review` Skill 手动触发。

**权衡**：rust-analyzer 在超大项目中可能有性能问题；保留 `verify.sh` 中的 cargo check 作为 F2 的一部分（确定性兜底）。

## 4.5 问题前移矩阵

目标：尽可能在早期阶段发现问题，降低修复成本。

| 问题类型 | 能在 F1(编译反馈) 发现？ | 能在 F2(测试反馈) 发现？ | 必须到 F3(集成反馈)？ |
|---------|------------------------|------------------------|---------------------|
| 类型不匹配 | 是 | — | — |
| 借用/生命周期 | 是 | — | — |
| 逻辑错误 | — | 是（单元测试） | — |
| 行为不等价 | — | 是（差异测试） | — |
| 性能退化 | — | — | 是（benchmark） |
| 并发 bug | 部分（Send/Sync 编译期检查） | 部分（loom/shuttle 基础验证） | 是（完整状态空间探索 + 集成测试） |
| FFI 边界问题 | — | — | 是（集成测试） |
| 翻译膨胀 | — | 是（tokei 对比） | — |
| unsafe 安全性 | — | 否 | 是（Miri 需 Tier 2 启用 + cargo-geiger 全局） |

## 4.6 并行开发策略

迁移期间源项目可能还在演化，需要选择并行策略：

| 策略 | 适用场景 | 操作 | 风险 |
|------|---------|------|------|
| **功能冻结** | 小项目、短期迁移 | 迁移期间源项目不接受新功能 | 业务停滞 |
| **双轨开发** | 中型项目 | 源项目继续开发，每个 Sprint 同步变更到 Rust | 同步成本高 |
| **Strangler Fig** | 大型项目、长期迁移 | 通过路由层逐模块切换，新旧并行运行 | 架构复杂 |

**行动指南**：
- 在 PROFILE 阶段（项目画像）中决定并行策略
- 功能冻结：在 `migration-state.json` 中锁定源码 commit hash
- 双轨开发：每个 Sprint 开始前检查源项目变更（见 § 4.6.1），必要时更新迁移规则
- Strangler Fig：需要额外配置 FFI 桥接层和路由层

### 4.6.1 源项目变更追踪（双轨 / Strangler Fig 场景必需）

功能冻结策略下源码不变，无需追踪；但**双轨开发**与 **Strangler Fig** 策略下，源项目在迁移期间持续演化（bug 修复、功能变更），若 Rust 侧无法感知这些变更，会导致等价性验证失效或行为不一致。

**机制：复用既有结构指纹体系，不新建独立追踪文件**。源项目变更检测直接重用 [04 § 5.7.5 增量更新策略](./04-toolchain.md#575-增量更新策略)的三级变更检测（`NONE` / `COSMETIC` / `STRUCTURAL`）与结构指纹——图分析、增量更新、源码变更追踪共用同一套指纹，避免重复设计：

1. **基线指纹存储**：变更监控基线在 § 4.8 录制（baseline commit 的结构指纹写入 `file_fingerprints` 表，详见 § 4.8 第 5 点）。指纹存储于 `source-graph.db` 的 `file_fingerprints` 表（schema 权威见 [04 § 5.7.1](./04-toolchain.md#571-图数据模型)），**不引入 `source-tracking.json` 等并行结构**。
2. **变更检测**：在每个 Sprint Planning（§ 4.2 第 1 步）与 `/migrate run` 前执行，对比源项目当前版本与 `source-ref/` 基线版本，按模块输出变更级别与影响范围 `{module_path: {level: NONE|COSMETIC|STRUCTURAL|IDENTITY_LOST, detail: {changed_functions, impact_depth}}}`。`IDENTITY_LOST`：基线中存在但当前源项目中已不存在的文件（重命名/删除/拆分），Sprint Planning 检测到时暂停并要求用户更新 `migration-state.json` 模块路径映射或标记 `requires_manual_review`。`impact_depth` 通过 `imports` 边反向 BFS 计算（同 § 5.7.5 传递性更新）。
3. **执行方式（不新增 MVP CLI 命令）**：M1 阶段功能冻结为默认并行策略，双轨/Strangler Fig 场景直接复用既有 `rustmigrate graph build` 增量重建——它本就计算上述三级变更并更新 `file_fingerprints`，对比基线与当前指纹即得变更报告，无需新建命令。是否将「变更报告」封装为独立 M2 子命令 `rustmigrate source check-changes`，归入 [06 § 10.0.1](./06-plugin-structure.md#1001-cli-工具架构rustmigrate) 的 M2 扩展候选评估（不突破 13+5 命令清单，命令权威以 06 为准）；MVP 阶段由 Sprint Planning 脚本调用 `graph build` + 指纹比对承担。
4. **STRUCTURAL 变更告警**：若检测到 `STRUCTURAL` 级变更命中目标模块或其依赖链，Sprint Planning（§ 4.2 第 1 步脚本化检测）自动向用户告警，提示需重新翻译受影响模块或同步迁移规则。**全局影响处理**：若反向 BFS 涉及 > 30% 模块（如修改了顶层 export），自动建议 full re-graph 并提示用户重新评估继续双轨开发的成本效益（避免高频变更下逐 Sprint 重建指纹却收益递减）。受影响模块的源依赖一致性由 MDR 的 `translated_from_source_commit` 字段（见 [05 § 6.5](./05-documentation-system.md#65-mdr--迁移决策记录)）追溯——比对翻译时锚定的 source commit 与当前 commit 即知哪些 Rust 模块的源依赖已漂移。当 STRUCTURAL 变更触发模块 M 的重新翻译时，Sprint Planning 须同步执行：(a) 重新执行 Step 0 更新 M 的 `source-ref/` 至当前 commit，(b) 重新执行 § 7.2 Step 2 行为录制——M 的既有 test-fixtures（golden/ops-recording/proptest-regressions）视为过期，由重新录制覆盖；过期判定复用 MDR 已有 `translated_from_source_commit` 字段与当前 commit 的比对，不新增字段。

## 4.7 异步翻译策略

> **设计依据**：Claw-Code（TS→Rust）采用"边界异步、核心同步"策略，大幅降低了翻译复杂度；Pingora（C→Rust）则在 Tokio 之上构建了定制运行时。不同项目需要不同策略。

翻译时的异步处理不应硬性规定通用规则，而是按项目特征决定：

| 源项目特征 | 推荐策略 | 理由 |
|-----------|---------|------|
| 纯计算/CLI 工具 | **核心同步**，仅 I/O 边界异步 | 降低翻译复杂度，避免不必要的 async 传染 |
| Web 服务（Express/Flask） | **按需异步**，路由层 async，业务逻辑可同步 | 匹配 axum/actix 的异步模型 |
| 高并发运行时（事件循环） | **全栈异步**，须选择运行时并写 MDR | 必须用 tokio/async-std 重新设计并发模型 |

**行动指南**（与 PROFILE/PLAN 边界对齐，见 [02 § 3.2.3](./02-architecture.md#323-profile-与-plan-边界清晰化)）：
- **PROFILE（自动）**：analyzer SubAgent **客观检测**异步模式（扫描 Promise/async-await 语法、事件循环、回调风格），在画像摘要 stdout JSON 中输出 `async_pattern_summary: { detected_async_patterns: [...], recommended_strategy: "...", needs_user_decision: bool }`。检测是事实采集，不做决策。
- **PLAN（人类决策）**：PLAN 阶段依据上述检测结果**确认 `async_strategy` 选择**，写入 `.rustmigrate.toml` 的 `async_strategy` 字段，作为 translator Phase A 代码生成规则的上下文。
- 如果选择"全栈异步"，须写 MDR 记录运行时选择（tokio vs async-std）和取消安全性审查计划。

## 4.8 PROFILE → PLAN 中间步骤：原项目可复现基线

在 PROFILE（画像）和 PLAN（规划）之间，插入一个关键步骤：

1. 锁定源项目版本（git tag/commit hash）
2. 确认源项目能在本地完整构建和测试
3. 录制基线行为（CLI 输出、API 响应、测试结果）
4. 记录基线指标（测试覆盖率、性能数据、代码行数）
5. **录制变更监控基线**（支撑 § 4.6.1 源项目变更追踪）：在 Step 0 源文件保留（见 § 4.3 Step 0）后，为 `source-ref/` 中所有文件计算结构指纹，写入 `source-graph.db` 的 `file_fingerprints` 表，关联 baseline commit；这些指纹标注来源为 `source-baseline-fingerprint`（区别于图分析的常规指纹，也区别于 § 7.1 性能基线 `source_baseline.json`），作为后续变更检测（§ 4.6.1）的对比基准。复用既有指纹体系，不新建独立追踪文件。

**执行者**：此步骤由 `/migrate analyze` 的分析阶段末尾执行（analyzer SubAgent 完成画像后，Skill 主上下文执行基线录制脚本）。`source_commit` 写入 `.rustmigrate.toml`。

**行动指南**：如果源项目本地构建失败，**停止迁移**——先修复源项目。

## 4.9 项目级止损标准

当迁移进展持续不佳时，需要及时止损而非无限投入。以下阈值为参考值，可在 `.rustmigrate.toml` 的 `[stop_loss]` 节中配置覆盖。

| 指标 | 阈值（默认） | 触发动作 |
|------|------------|---------|
| DEGRADE 比例（全项目） | >40% 模块降级为 FFI 桥接 | 暂停迁移，评估是否继续——大面积降级意味着 AI 翻译能力不足以处理该项目 |
| FFI 降级占比（链路级） | 单链路 >30% / 多链路 >50% 节点降级 | 触发链路评审——深链被迫全降级会严重削弱迁移价值；此为链路粒度的提前预警，与上行「全项目 40%」是不同粒度（链路级先于全项目触发） |
| LLM API 成本 | 超出预算 2x | 暂停迁移，评估是优化 prompt/模块粒度还是放弃 |
| Sprint 停滞 | 连续 3 个 Sprint 未完成任何模块 | 召集团队评审，分析阻塞原因，决定是否继续 |
| 质量评分回归 | 当前 Sprint 评分（§7.5 `final_score`）< 前 3 个 Sprint 均值 − 10%，连续 2 个 Sprint | 标记团队评审；若持续 3 个 Sprint，触发升级评审后再继续 |

```toml
# .rustmigrate.toml 中的止损配置（可选覆盖）
[stop_loss]
degrade_ratio_threshold = 0.4        # 降级模块比例阈值
cost_multiplier_threshold = 2.0      # LLM API 成本超预算倍数
stalled_sprint_threshold = 3         # 连续停滞 Sprint 数
score_regression_threshold = 0.10    # 质量评分回归阈值（相对前 3 Sprint 均值的下降比例）
```

**行动指南**：`/migrate review` 在仪表板中展示止损指标的当前值和阈值距离。接近阈值时提前预警。若 `migration_motives` 含 `performance`，额外输出性能基线对标报告，汇总各模块 throughput / p99 latency 变更，并给出总体 ROI =（总收益 token 数）/（迁移成本 token 数）。

## 4.10 性能与并行转换指南

M1（MVP）用 §4.2.1 的「Skill 交互式」串行模式，M2+ 用「Workflow 批量」并行模式。本节给出两者的吞吐预期、并发度依据和升级决策点。

**(1) M1 串行吞吐预期（理论值，M1 验收时用实测项目补充）**

以「单模块 1K 行需约 1 天（含 Phase A/B + 测试搭建 + 人工审查）」为反推基准：

| 模块数 × 平均规模 | 单模块耗时 | 总耗时预期 |
|------------------|-----------|-----------|
| 5 × 500 行 | ~0.5 天 | ~2-3 个工作日（1-2 Sprint） |
| 10 × 1000 行 | ~1 天 | ~5-7 个工作日（每 Sprint 2-3 天，含人工审查） |
| 20 × 1000 行 | ~1 天 | ~3-4 Sprint |

> 上表为**理论预期**，非实测；M1 验收时须用 3 个真实项目补充实际数据校准。

**(2) M2 并发度计算依据**

`max_concurrent_agents`（[06 § 11.1](./06-plugin-structure.md#111-rustmigratetoml-配置文件) 默认 3）取以下约束的最小值：

```
max_concurrent_agents = min(LLM_API_并发限制, 磁盘 I/O 饱和点, 可用 worktree 数)
```

参考配置：激进 = 5 / 平衡 = 3（默认）/ 保守 = 1。`.rustmigrate.toml` 注释中应说明「3」是平衡配置默认值。

> **M2 吞吐 ≥ 1.5 模块/小时（[08 § M2 验收](./08-roadmap-and-reference.md#m2-质量提升8-12-周)）的假设条件**：同批模块平均 1-2K 行、LLM 处理延迟 P50 ≤ 5 分钟、worktree 启动 + target 冷编开销 < 5% 总耗时。**（D5 定稿：翻译期图只读、编排器集中 writer，无 SQLite 并发写竞争，原「SQLite 写竞争占用 / 冲突率」假设已移除，见 [04 § 5.7.3](./04-toolchain.md#573-持久化存储)）** 任一实际超出该假设将影响可达成性，须在 M2 验收时按实测分布（P50/P95/P99）重新校准（验收口径见 08 § M2）。

**(3) 性能瓶颈分层**

| 环节 | 量级 | 说明 |
|------|------|------|
| Skill 调用延迟 | 秒级 | 编排开销，可忽略 |
| LLM 翻译处理 | 分钟级 | 主要耗时来源 |
| cargo 编译/测试 | 分钟级 | 大项目可能更久 |

> rust-analyzer 在超大项目的性能问题已在 §4.4 权衡段说明（保留 cargo check 兜底）。

**(4) M1 → M2 升级决策（可观测指标，非硬性阈值）**

`/migrate review` 仪表板展示以下指标，供用户判断是否升级到 M2 并行模式：
- 单次 Sprint 耗时持续 > 10 个工作日；**或**
- LLM 等待时间占 Sprint 总耗时 > 60%（表明 API 并发度为主要提升轴，与 §4.10(3) 瓶颈分析一致）。

满足任一时，提示「考虑切换 Workflow 批量模式」。是否升级由用户决策（M1/M2 并存，不强制）。

> 此处为「**何时考虑** M1→M2 升级」的启动条件（可观测、非硬性）；M2 验收通过并进入 M3 的**充要判据**（P50/P99/**WAL 配置回归**/波动四项；D5 定稿：原「SQLite 锁等待」门禁经集中 writer 架构降级为 N/A）见 [08 § M2 → M3 升级判据](./08-roadmap-and-reference.md#m2-质量提升8-12-周)，两者勿混淆。

## 4.11 CI/CD 集成（M2 范围）

M2 交付物「CI/CD 集成」的落地设计。区分两类关注点：**用户集成模板**（rustmigrate 如何在用户项目 CI 中被调用，M2 范围）与**项目自验证 dogfooding**（本项目 CI 是否用自身工具，M2 概念设计、不要求实现）。

### 4.11.1 用户集成模板（GitHub Actions）

rustmigrate 在用户项目 CI 中作为**验证门禁**调用（非自动迁移）。触发与分级策略：

| CI 事件 | 运行内容 | 失败动作 |
|---------|---------|---------|
| PR 打开 / 更新 | Tier 0（`cargo check` + `clippy` + `cargo test`）+ `rustmigrate review --json` 解析迁移状态 | status != "ok" 则 fail job |
| merge 到主干 | Tier 0 + Tier 1（黄金文件 + 属性测试） | 任一 Tier 失败则 fail |
| tag / release | Tier 0+1+2（含模糊测试差异对比、覆盖率门禁） | 完整管线，产出 KNOWN_DIFFERENCES 报告 |

模板要点：
- **缓存策略**：`source-graph.db` 通过 `actions/cache` 按源码 hash 持久化，避免每次重建图（图构建耗时门禁见 [08 § M1 MVP](./08-roadmap-and-reference.md#m1-mvp6-8-周)）。
- **产物上传**：`rustmigrate review` 生成的报告（PARITY.md、KNOWN_DIFFERENCES.md）通过 `actions/upload-artifact` 上传供审阅。
- **JSON 解析 + 失败判定**：CI 步骤解析 CLI 统一 JSON 输出（`{status, data, warnings}`，格式见 [06 § 10.0.1 CLI 与 Plugin 交互](./06-plugin-structure.md#1001-cli-工具架构rustmigrate)），`status != "ok"` 时令 job 失败。示例：

```yaml
# .github/workflows/migration-check.yml（用户项目，示意）
- name: rustmigrate review gate
  run: |
    rustmigrate review --json > review.json
    status=$(jq -r '.status' review.json)
    if [ "$status" != "ok" ]; then
      jq -r '.warnings[]' review.json   # 输出机器可读的失败详情
      exit 1
    fi
```

### 4.11.2 错误信息标准化

CLI 失败时输出机器可读的统一结构，便于 CI 提取。例如 `rustmigrate validate state` 在状态非法时返回：

```json
{ "status": "error", "data": null, "warnings": ["migration-state.json: 字段 current_state 取值 'FOO' 不在状态枚举内"] }
```

CI 步骤据 `status` 决定 job 成败，据 `warnings` 在日志中打印可定位详情。

### 4.11.3 可复现性保证

CI 集成要求确定性，避免"本地通过 / CI 失败"：
- `source-graph.db` 构建必须确定性：tree-sitter 解析顺序固定、vendored `Cargo.lock` 锁定依赖版本；**tree-sitter 版本须锁定**（`.rustmigrate.toml` 的 `[reproducibility]` 段，schema 权威见 [06 § 11.1](./06-plugin-structure.md#111-rustmigratetoml-配置文件)），跨用户版本差异会导致图构建不一致。
- 属性测试 / 模糊测试用**固定 seed**（proptest regression 文件入库），回归用例可复现。
- HashMap 迭代顺序等非确定源已在验证层处理（见 [§ 7.7 探测维度清单](#77-不等价证据探测维度清单)）。
- **LLM 输出非确定性**：翻译用固定 `temperature`/`top_p`（`[reproducibility]` 段配置），叠加确定性规则注入，把 LLM 自由度收窄到可复现范围。注意：Claude API 的 seed 参数支持取决于 API 可用性——不可用时，可复现性依赖固定 temperature/top_p + 确定性规则集，而非逐 token 复现。
- **中间产物 schema 版本化**：`migration-state.json`、`source-graph.db`、`type-map.json` 须含 `schema_version`（必填）字段；断点续传（checkpoint）恢复时校验版本——同版本直接恢复，跨版本走 schema 迁移（版本化与迁移工具权威见 [06 § 10.0.2 版本控制与向后兼容策略](./06-plugin-structure.md#1002-版本控制与向后兼容策略)），迁移失败则需人工恢复。
- **M1 可复现性验收标准（单机单环境，更可控）**：在**一台 CI 机器上连续两次** `analyze→run→review`，产出的 Rust 源代码哈希一致（改为单机两次而非跨用户，剔除环境差异这一不可控变量；跨用户哈希一致作为 M2 目标）。
- **CI 操作与故障诊断**：`.rust-migration/ci/verify-reproducibility.sh`（脚本框架与完整 Schema 作为 M0 spike / M1 早期交付物，非新增整章）实现 ——(1) **排除规则**：比对前过滤生成时间戳注释（如 `// Generated at ...`）、行尾空格、跨平台行尾符差异（CRLF vs LF），具体脚本见 [06 § 10.6 产出物目录结构](./06-plugin-structure.md#106-产出物目录结构)；(2) **记录环境快照**到 `.rust-migration/.ci-env-snapshot`，格式为 JSON `{rustmigrate_version, tree_sitter_version, cargo_version, rust_version, os, arch, cpu_count}`，用于区分环境漂移 vs 真实非确定性；两次运行间须清除 `.rust-migration/intermediate/`、缓存与编译产物（详见 M0 spike 验收清单）；(3) **哈希比对**：按上述规则过滤后用 `sha256sum -b`（二进制模式，忽略行尾格式差异）计算并与上次基线对比——「5 字节差异」定义为生成时间戳相关元数据的字节偏差（如 UUID/时间戳值的轻微差异），不含源码语义变化；为便于自动化判定，建议改用相对容限（< 0.1% 源文件大小）；(4) 在 GitHub Actions 中 PR 提交时哈希偏离则输出警告。reproduce 失败时先比对 `.ci-env-snapshot` 区分「环境/版本漂移」与「真实非确定性」，前者锁版本后重试，后者查 `[reproducibility]` 配置是否生效。此为 M1 验收门槛之一（工作量归属见 [08 路线图](./08-roadmap-and-reference.md#m1-mvp6-8-周)）。上述第 (4) 点哈希偏离警告在 `ci.yml` PR workflow 中作为独立 job 运行（路径变更触发，工作量见 [08 § M1 '可复现性脚本与 CI 集成' 行](./08-roadmap-and-reference.md#m1-mvp6-8-周)）。

### 4.11.4 项目自验证（dogfooding，M2 概念设计）

> 本节为 M2 概念设计，**不要求 M2 实现**。

设想：本项目可在自身 CI 中用 rustmigrate 验证一个内置的 TS→Rust 微型 fixture（M1 已建的自建微型项目，见 [08 § M1 MVP 工作量分解](./08-roadmap-and-reference.md#m1-mvp6-8-周)），作为工具端到端回归的活体测试。落地时机与范围待 M2 后评估，避免过早投入。

具体化（仍 M2 落地，M1 仅备料）：
- **fixture 范围**：TS 输入 = 50-100 行的纯工具模块（如字符串格式化 / 数学辅助）+ 对应的**手写**期望 Rust 输出（用于比对，非自动生成）——保证 fixture 本身可理解，不至复杂到触发工具自身边界。
- **验证深度**：dogfooding **仅验证到 Tier 0**（`cargo check` + clippy + `cargo test`），**不**做语义等价（语义等价是用户责任）——它是回归门禁，不是完整迁移演示，避免范围蔓延；额外对 fixture 输出运行 § 7.5 确定性指标子集（行数比、函数数比、clippy 警告数，基线存于 `test-fixtures/benchmarks/`），任一指标相对基线退化超 15% 则 CI 告警（纯确定性，复用 tokei+clippy 已有输出）。
- **CI 集成**：`dogfooding.yml`（独立 GitHub Actions workflow）；初期为**信息性状态检查**（不阻塞），M1.5 无失败后升级为 required，把它定位为内部回归覆盖而非负担。

**Dogfooding 验收标准**（量化「信息性状态检查」判定）：
- **Fixture 规范**：50-100 行纯工具模块，涵盖纯函数、有状态对象、错误处理三类语言特性。
- **成功判定**：`all([cargo check, clippy, cargo test]) == passed`，单个工具失败即判失败。
- **M1.5 升级条件**：M2 前检验连续 3 个开发周期内 dogfooding 无失败，或首次失败的根本原因已修复（3 周期对应 M1/M2 开发节奏）。
- **故障隔离**：在 `dogfooding.yml` 中分离「rustmigrate 转译输出」与「Tier 0 工具验证」两步，各自将失败原因报告到不同 artifact，以区分 rustmigrate 工具问题 vs Tier 0 工具问题。

---

# 七、测试与验证策略

## 7.1 测试分层（L0-L7）

| 层 | 名称 | 工具 | Tier | 目标 |
|----|------|------|------|------|
| L0 | **单元测试** | cargo test | 0 | 基础正确性 |
| L1 | **快照/黄金文件测试** | insta | 1 | 锁定输入/输出对（合并原 L1+L3，消除重叠） |
| L2 | 属性测试 | proptest | 1 | `for all x: old(x) == new(x)` |
| L3 | **E2E 差异测试** | 自建差异框架 | 1（M2+ 完整版）/ M1 使用简化脚本对比 | 整体行为等价 |

> **M1 阶段 L3 轻量替代**：M1 不构建完整的自建差异测试框架（G3 组件）。改用简单的 shell 脚本做输入输出对比——对 CLI 工具运行相同输入，`diff` 比较 stdout/stderr/exitcode；对库函数通过 FFI 桥接调用新旧实现并比较输出；对有状态对象/服务，shell 脚本从 Step 2 录制的操作序列（`test-fixtures/{module}-ops-recording.jsonl`）出发，依次向新旧实现发送相同操作，逐操作 diff 可观测响应（返回值/stdout/HTTP response body），内部状态一致性由条件强制的 L1 操作序列快照（insta）承担，L3-简化不验证内部状态。完整差异测试框架在 M2 阶段实现。
>
> **依赖链路限制**：M1 的 shell 脚本 L3 方案仅适用于**叶子模块**（无内部消费者，黑盒输入输出自包含）。若模块 A 的输出是模块 B 的输入（内部链路依赖），源语言版 A 无法与 Rust 版 B 直接交互（类型不兼容），黑盒等价假设失效。此时应在迁移规划（PLAN）时三选一：(1) **整体迁移依赖链**（A+B 同 Sprint 一起迁），(2) **A 降级为 FFI 桥接**供 B 消费，或 (3) **B 的 L3 验收延后到 M2** 的完整 G3 框架。依赖关系由 §7.2 Step 2.1 记录。

| L4 | 模糊测试 | cargo-fuzz | 2 | 随机输入差异对比 |
| L5 | 变异测试 | cargo-mutants | 2 | 验证测试真正保护了行为 |
| L6 | **性能回归** | criterion | 2（性能动机时提升为 1） | 无性能退化 |
| L7 | **并发正确性** | loom / shuttle | 2 | 并发模型正确性 |

**proptest 回归集管理策略**（界定写入/回滚边界）：(1) **初始化**：Phase A 翻译完成后，verifier 从源项目测试 suite 提取/生成初始 proptest case，写入 `test-fixtures/proptest-regressions/{module}.txt` 作为「基准」；(2) **Phase B 对标**：Phase B 完成后对标基准跑 proptest，新增失败 case 须记录失败原因与是否属「允许不等价」范畴；(3) **更新权限**：失败仅当落在 § 4.3 Step 5 三类允许重写范畴内时，verifier 在 MDR 标注「proptest case X updated due to [并发模式改进]」并经**人类审批**后更新基准，否则标记 `requires_manual_review`（verifier 无权为掩盖回归而改基准）；(4) **基线传递**：回归集入库后，后续迭代的回归基线是**上一版本的集合**（非源项目），支撑 § 4.9 质量评分回归检测的趋势追踪。同等原则适用于 insta 快照（L1 黄金文件）：verifier/translator 不得执行 `cargo insta accept` 自行接受变更快照；pending snapshots 须在审查报告中列出 diff 并经人类审批后更新。

**测试执行确定性保障**：
- proptest：固定 seed 记录到 `test-fixtures/proptest-regressions/`
- cargo-fuzz：corpus 持久化到 `test-fixtures/fuzz-corpus/`
- criterion：基线数据持久化到 `test-fixtures/benchmarks/`
  - **基线时机**：Phase A 后采集源项目基线（`source_baseline.json`，仅参考）；Phase B 优化后采集 Rust 基线（`rust_baseline.json`，回归对比的**验收基准**）。**若 Phase A 的 Rust 代码编译失败**：在最近的上一个可编译版本、或经 FFI 回调的源实现上采集基线，标注为 `source-baseline-reference`（区别于正常 `source_baseline.json`），避免因 Phase A 不可运行而无基线。
  - **回归容忍度**：`.rustmigrate.toml` 的 `[testing] benchmark_tolerance = 0.10` 表示接受 ≤10% 的相对性能偏差。**语义明确**：偏差是相对源项目基线**吞吐均值**的百分比，用 criterion 的 relative_difference 指标计算，并**同时报告 p99 延迟**（不同统计法结论差异可达 30%，故须固定口径）。超出容忍度的性能回归须 MDR 经维护者批准方可合并；性能退化超容忍度**且未在 KNOWN_DIFFERENCES.md 登记**时自动 block 状态转移。详细 CI 执行与目录结构见 [04 § 5.3](./04-toolchain.md#53-tier-1推荐画像自动启用) 与 [06 § 10.6](./06-plugin-structure.md#106-产出物目录结构)。

### 7.1.1 模块类型 × 测试层要求矩阵

L0-L7 分层定义了「能做什么」，但模块要达到 `done` 还需明确「哪些层是强制的」。下表按模块类型规定 M1（MVP）下的**强制（必须）/ 条件（满足条件时必须）/ 可选**层组合。层选择由检测到的模块类型决定（PROFILE/analyzer 输出），**不由 verifier 自由裁量**。

| 模块类型 | 强制（M1 必须） | 条件强制 | 可选（M2+ 提升） |
|---------|---------------|---------|----------------|
| 纯工具函数（无状态/无副作用） | L0 + L1 + L3-简化 + L2 | — | L4/L5 |
| 有状态对象/服务 | L0 + L1 + L3-简化 | 有持久化状态 → L1 操作序列快照 | L4/L5 |
| 异步/并发代码 | L0 + L1 + L3-简化 | **L7（loom/基础并发验证）必须** | L4 |
| FFI 绑定 | L0 + L1（快照 insta 必须）+ 手工审阅 | FFI 边界 → L3-简化（跨边界 I/O 对比） | L4 |
| 性能敏感模块（`performance` 动机） | L0 + L1 + L3-简化 | L6（criterion）必须 | L4/L5 |

> **M1 最低门槛**：**所有模块**至少 L0 + L1 + L3-简化；纯函数额外要求 L2（proptest）；异步/并发模块额外要求 L7。L4-L6 在 M1 默认不强制（除非对应迁移动机触发，见 §8）。verifier 在 Step 6 生成测试时按此矩阵选择测试层（其系统提示中纳入本矩阵）；`/migrate run` SKILL 按检测到的模块类型注入对应层要求，不留给 verifier 自由决定。
>
> **FFI 模块的覆盖率约束**：FFI 降级模块的 LLVM 覆盖率只统计 Rust 侧（跨边界源实现不计入，覆盖率虚高，见 [04 § 5.3](./04-toolchain.md#53-tier-1推荐画像自动启用)）。因此 FFI 模块**不得仅凭「覆盖率 ≥ 源」判定 `done`**——必须同时通过 L1 快照（insta）+ 手工审阅；若仅覆盖率达标，该模块状态降级为 `manual_review_only`（待人工复核，不计入 done）。

## 7.2 测试基础设施搭建（SCAFFOLD 阶段修正）

原设计存在循环依赖：Rust 代码不存在时不能写 Rust 测试。

**修正方案**：

```
Step 1: 评估原项目测试质量
  → 有测试：标记为"黄金测试集来源"
  → 测试不足：补充测试（在源语言中）

Step 2: 行为录制（不依赖 Rust 代码）
  ├── CLI 工具：录制 args → stdout/stderr/exitcode
  ├── HTTP 服务：mitmproxy 录制请求/响应
  ├── 库/SDK：录制函数调用的 input/output 对
  └── 有状态服务：录制操作序列和状态变更

Step 2.1: 模块依赖关系记录（支撑 L3 链路场景）
  ├── 在 PROFILE/SCAFFOLD 阶段通过图查询（已有 imports 边或 tree-sitter 模块分析）
  │   识别模块间出入依赖
  ├── 写入 .rust-migration/module-deps.json（如 {"module_a": {"depends_on": ["module_b","module_c"]}}）
  └── 对存在依赖的模块做"链式录制"（**检测到链路依赖时为强制**，否则该链路在 verifier 复审时
      触发 requires_manual_review 降级）：按拓扑顺序跑一遍完整链路 A→B→C，录制中间产物——
      (a) A 迁移前/后的输出、(b) B 消费 A 输出后的可观测状态变更、(c) C 的最终输出；
      迁移后逐层比对，偏差超容忍度即标记不等价（防止 B 消化掉 A 的微妙语义偏差，如浮点精度/集合排序）
      容忍度由 .rustmigrate.toml `[testing] chain_tolerance`（float_epsilon / collection_order_sensitive）控制
      这是 [§ 7.6.2](#762-l3-差异测试的依赖状态前置检查模块间依赖场景) L3 适用于链式模块的前提条件

Step 3: 接口契约定义（不依赖 Rust 代码）
  ├── 函数签名 + 输入输出类型
  ├── 前置条件 / 后置条件
  └── 副作用描述

Step 4: 模块迁移完成后，生成 Rust 测试（确定性 + AI 混合）
  ├── 测试骨架从录制数据**确定性生成**（脚本模板化输入输出对 → Rust #[test] 函数）
  ├── 将黄金测试集翻译为 Rust（确定性模板）
  ├── 将行为录制转为 insta 快照测试（确定性模板）
  └── 基于接口契约生成 proptest（AI 辅助，需理解语义）
  注意：测试骨架优先由确定性脚本生成，仅语义复杂的测试逻辑交给 verifier SubAgent。
  时序：translator 完成翻译 → F1 编译验证通过 → 确定性测试骨架生成 → verifier 补充语义测试 → F2 测试验证。
```

**行动指南**：测试搭建的核心是**行为录制和接口契约**，这两者不依赖 Rust 代码的存在。测试生成职责归属 verifier SubAgent，与翻译（translator）解耦。

## 7.3 验证管线（DAG 结构，非线性）

```
                      ┌─────────────┐
                      │ 源码分析     │
                      │ (tree-sitter)│
                      └──────┬──────┘
                             ▼
                      ┌─────────────┐
                      │  AI 翻译    │
                      │ (多候选)     │
                      └──────┬──────┘
                             ▼
              ┌──────────────┼──────────────┐
              ▼              ▼              ▼
        ┌──────────┐  ┌──────────┐  ┌──────────┐
        │cargo check│  │cargo deny│  │ tokei    │
        └─────┬────┘  └─────┬────┘  │ 膨胀检测 │
              │              │       └─────┬────┘
              ▼              ▼             │
        ┌──────────┐  ┌──────────┐         │
        │  clippy   │  │cargo audit│        │
        └─────┬────┘  └──────────┘         │
              │                             │
              ▼                             ▼
        ┌──────────┐                 ┌──────────┐
        │cargo test │                │ 复杂度   │
        │(nextest)  │                │ 对比报告 │
        └─────┬────┘                └──────────┘
              │
     ┌────────┼────────┐
     ▼        ▼        ▼
┌────────┐┌────────┐┌────────┐
│llvm-cov││ Miri   ││geiger  │
└────────┘└────────┘└────────┘
              │
     ┌────────┼────────┐
     ▼        ▼        ▼
┌────────┐┌────────┐┌────────┐
│proptest ││fuzz    ││mutants │
└────────┘└────────┘└────────┘
```

**关键点**：验证管线中的独立节点可以并行执行，不必等待其他节点完成。

## 7.4 安全护栏（借鉴 RustLift）

| 机制 | 说明 | 实现方式 |
|------|------|---------|
| **Approval Token** | 批量执行前需要人类预览并授权令牌 | `/migrate run` 在执行 Sprint 批量翻译前，先展示待翻译模块列表和预估成本，用户确认后生成一次性令牌 |
| **Preview-before-spend** | AI 调用前预估 token 成本 | 编排器在调度翻译任务前，根据源码大小和上下文预算预估 token 消耗，超出阈值需用户确认 |
| **不自动宣布成功** | 翻译成功后停在 `needs_review` 而非自动标 `done` | 模块状态流转增加 `reviewing` 状态（已有），verifier 通过后仍需人类最终确认 |

## 7.5 质量评估分层评分卡

翻译质量评估使用确定性指标（工具可直接计算）+ AI 辅助指标（需语义理解）的分层评分卡：

**确定性指标（工具自动计算，权重 70%）**：

| 指标 | 健康范围 | 告警阈值 | 工具 |
|------|---------|---------|------|
| 编译通过 | 是 | 否 → 阻塞 | cargo check |
| 测试通过率 | 100% | < 95% | cargo nextest |
| 代码行数比 | 1.2x - 2.0x | > 3.0x | tokei |
| 圈复杂度比 | 0.8x - 1.2x | > 1.5x | scc |
| 函数数量比 | 0.9x - 1.3x | > 2.0x | tokei |
| clippy 警告数 | 0 | > 0 | cargo clippy |
| unsafe 块数 | 0（理想） | 按 P0-P4 分类 | cargo-geiger |

> **M4-QUAL-05 数据接线**：verifier 完成行为测试后，编排器用
> `rustmigrate state record-metrics --module <M> --test-pass-rate <rate> --known-differences <n>`
> 把测试结果写入 `ModuleState`（通过/失败结果都记录，不能只保存成功样本）；并行路径由主 worktree
> 的集中 writer 消费 `TranslationResult` 可选度量后写回，机械 batch 无行为测试时保持空值、不伪造通过率。
> `stats quality --source <src> --rust <rust>` 通过 tokei `count_loc` 计算项目级 `rust/source` 行数比，
> 仅作为 `QualityReport.project_loc_ratio` 的**项目级近似值**输出（warning 明示粒度），**不下沉到
> per-module `deterministic.loc_ratio` 或 `final_score`**，避免大型模块掩盖小模块膨胀或反向误罚。不复用完整 `stats compare`：后者还计算函数数/控制流嵌套，Go/C 尚未实现嵌套分析时会
> 整体返回 `NotImplemented`，不应因此丢失语言无关的 LOC 比。真正的 per-module LOC 比需按
> `member_files` 切分源/目标作用域，推迟到有真实需求时再实现。

> **阈值依据与 M1 校准（行数比 / 函数数比 / 复杂度比）**：当前膨胀比例区间为**初始保守估计**，尚无大样本真实迁移数据支撑（Bun/Pingora 公开数据为整体行数级别，未细分到模块函数比）。这些阈值同时被 [§ 4.3 Step 4.5 Phase A 结构校验门禁](#43-内循环模块级单会话内-phase-ab-双阶段翻译)复用（门禁取本表告警阈值的上界）。M1 验收须在 ≥3 个真实 TS 项目上实测函数数比与行数比，若实测中位数与初始估计偏差 > 20%，按实测数据调整本表与 Step 4.5 门禁（验收检查点见 [08 § M1](./08-roadmap-and-reference.md#m1-mvp6-8-周)）。在校准完成前，本阈值不作为「成熟开源项目级」的硬性证据。

> **覆盖率作为等价代理的边界**：测试通过率是必要不充分条件。覆盖率本身只证明「代码被执行过」，不证明「行为等价」（源实现有 bug 时高覆盖率仍可能掩盖错误）。当 Rust 覆盖率低于源码时，需 L2 属性测试（proptest）或 L3 差异测试补充；若覆盖率 ≥ 源但 L2/L3 失败则**阻塞**。覆盖率作参考指标，不作充分条件。覆盖率门槛配置见 [06 § 11.1](./06-plugin-structure.md#111-rustmigratetoml-配置文件) `[testing] coverage_threshold`。
>
> **verifier 覆盖率判别规则（三级响应级联，纳入其系统提示，直接影响 done/reviewing → blocked/requires_manual_review 状态转移）**：
> 1. **源码 ≥ 覆盖率**（达标）：覆盖率 ≥ 源码且对新增代码路径做 case study（针对性测试）即可继续——case study 须确认新增行是**有意的错误处理强化**（依 § 5 porting 规则与 Phase B MDR）而非死代码。**上界约束**：当行数比 > 1.5x **且** 覆盖率 ≥ 源时，verifier 须交叉核对 Phase B MDR，确认新增行对应已记录的重写（§ 4.3 Step 5 三类允许改动）；新增行缺 MDR 依据则标记 `requires_manual_review`（防「覆盖率 100% 但新增行全是无据膨胀」）。Phase A 结构门禁（§ 4.3 Step 4.5）是抵御无据膨胀的第一道防线；
> 2. **源码-10% ≤ 覆盖率 < 源码**（须补充，不可跳过）：**必须**补 L2 属性测试（优先 proptest）或 L3 差异测试，补充成功后达标；若 proptest 无法编写（有状态库函数，见 § 7.6.1 三选一）或 L3 因依赖链断裂不可执行（见 § 7.6.2），**才**标记 `requires_manual_review`；
> 3. **覆盖率 < 源码-10%**（或绝对值 < 40%）：无论如何须 `requires_manual_review`。
>
> **与 `coverage_threshold` 的关系**：规则 3 中 40% 为硬编码下限（不可配）；`coverage_threshold`（[06 § 11.1](./06-plugin-structure.md#111-rustmigratetoml-配置文件)，默认 80）为 `done` 状态的独立绝对门槛——即使三级规则判定达标，覆盖率低于 `coverage_threshold` 仍标记 `requires_manual_review`。两者取严格并集。
>
> **标记权与状态转移的责任边界**：`requires_manual_review` 由 **verifier 标记**（标记行为），但 `requires_manual_review → done` 的状态转移须**人类确认**——对标 § 4.3 Step 2.5 意图确认门禁的交互模式（[09 附录 B Step 6](./09-appendix-schemas.md#附录-bmvp-skill-的-skillmd-骨架) 更新状态前显示确认对话），不新增独立流程步骤。这堵住了「verifier 声称覆盖率 ≥ 源码即 done，但新路径实际未被 proptest 覆盖」的虚假推进。

**AI 辅助指标（verifier 评估，权重 30%）**：三项均归一化到 0-100，评分锚点以**客观信号**为主、主观判断为辅，避免沦为印象分。verifier 系统提示须内嵌本表（CSV 格式评分表）。

| 指标 | 客观锚点（确定性信号，应自动采集） | 主观判断（仅在客观信号不决定时介入） |
|------|----------------------------------|----------------------------------|
| 惯用性 | `cargo clippy --message-format=json` 警告数（0 警告→满分基线）；裸 `unwrap`/`expect` 计数（用户代码区，**例外**：`#[test]`/初始化常量/`OnceLock` 内允许，由 PORT NOTE 标注后不计扣分） | idiomatic 模式选择是否恰当（如 `?` vs 手写 match） |
| 语义保真度 | 意图摘要 7 字段逐项核对结果（§ 7.7 维度 9，每偏离 1 字段扣固定分）；proptest seed 回归是否新增失败（`test-fixtures/proptest-regressions/` 比对） | 等价重新实现是否在意图约定的「允许不等价」范围内 |
| 可维护性 | 圈复杂度比（scc，与源码偏差）；公开 API 缺注释项计数（`cargo doc` 缺失项） | 注释对陌生读者是否「足够」（仅当客观计数达标后做定性补充） |

> 「裸 unwrap」的判定：用户代码区（非测试、非初始化、非 PORT NOTE 标注的合理场景）中的 `unwrap`/`expect` 计为扣分项；测试代码、`const`/`OnceLock` 初始化、以及 PORT NOTE 注明理由的场景不计扣分。「一致」指语义等价（允许等价重新实现），非逐字符匹配；偏离意图约定的实现才扣分。

**评分公式**：每个指标归一化到 0-100（确定性指标按健康范围映射；布尔指标取 0/100）。AI 三项**强制项**用加权聚合（突出「三项都须达标」，避免一高两低被均值掩盖）而非简单算术平均：

```
ai_avg       = min(idiom, fidelity, maint) × 0.34
             + median(idiom, fidelity, maint) × 0.33
             + max(idiom, fidelity, maint) × 0.33
final_score  = deterministic_avg × 0.7 + ai_avg × 0.3
```

> **AI 指标缺失时的降级口径（既有实现登记）**：verifier 尚未提供三项 AI 指标时，
> `stats quality` 以已有的 ≥2 项确定性指标均值输出临时 `final_score`（确定性权重 100%），并以
> `ai_indicators: null` 明示证据不完整；这用于迁移过程中的可计算基线，**不得**当作完整 70/30
> 成熟评分。AI 指标接线后自动切回上式 70/30。机械 batch 无行为测试时不伪造
> `test_pass_rate`，在 per-module 指标不足时 `final_score` 保持 null；项目级 LOC 比也不用于凑模块分。
> `data_completeness` 仅表示“可计算分数的模块占比”，不代表 AI 证据完整度。

verifier 须以**结构化 JSON 输出**评估结果（含三项各自分值、客观信号原始值、置信度 0-100），不输出自由文本评语，便于跨 Sprint 对标与回归检测。每 Sprint 的评分快照（含趋势）持久化为 `sprint-N-report.json`（schema 见 [09 附录 F](./09-appendix-schemas.md#附录-f评分报告-sprint-n-reportjson-schema)）。

> **M1/M2 时序划分**：M1 仅实现 **per-module 质量评分**（确定性指标自动化 + verifier AI 指标）和 `sprint-N-report.json` 基础格式；**跨 Sprint 趋势检测与回归告警推至 M2**（依赖多 Sprint 数据），届时由独立 CLI 子命令 `rustmigrate stats quality-trends --sprint-start 1 --sprint-end N`（M2 扩展命令，归入 [06 § 10.0.1](./06-plugin-structure.md#1001-cli-工具架构rustmigrate) 评估，不突破 13+5 命令清单）实现，供 verifier/Skill 调用——CLI 独立可测、结果可复现，而非内嵌于 SKILL.md 指令。§ 4.9 的「质量评分回归阈值 10%」是 M2 趋势检测的判定规则。

> **M1 评分校准（AI 指标主观性收敛）**：AI 三项评分的可靠性须在 M1 验收时校准。选取 3 个具代表性的参考模块（覆盖三类形态：一个纯函数库、一个有状态服务、一个异步代码），由 ≥2 名评审独立按上表打分，计算组内相关系数（ICC）；ICC ≥ 0.75（成熟 OSS 一致性门槛）方视为评分规则可用，< 0.75 则收紧客观锚点定义（增加可自动采集的信号、缩小主观判断区间）后重测。校准数据写入 `DESIGN_ASSUMPTIONS.md`（M0/M1 产出）。

**M4 新增项目级度量**（M4-QUAL-01 登记，非 per-module 评分卡输入项）：

| 度量 | 定义 | 数据源 | 说明 |
|------|------|--------|------|
| `degrade_rate` | 降级模块数 / 总模块数 | `migration-state.json` modules | 项目级止损信号 |
| `behavior_coverage` | `test_pass_rate × (1 - known_diff / (known_diff + 10))` | verifier 产出 → `state record-metrics` 写 `ModuleState.test_pass_rate` + `known_differences` | Per-module，差异测试全量覆盖前的过渡口径 |
| `revision_rate` | 人工修改行数 / 自动产出行数（git-diff 口径） | git baseline（optional） | 当前无 git baseline 时标为 N/A |

> 这三项独立于 `final_score` 评分卡（不参与加权），由 `rustmigrate stats quality` 输出。`deviation_score`（社区结构偏离度，Louvain vs 目录分区 NMI）由 `rustmigrate stats community` 输出，用于源项目可迁移性评估。

**阈值按迁移动机调整**：上表确定性指标的告警阈值非一刀切，按主要动机调整。调整规则的权威定义在 [§8 动机路由行动指南](#八迁移动机驱动的策略路由)（如内存安全放宽膨胀容忍、性能保持严格并新增 benchmark 必须项）。

## 7.6 行为等价性验证

> **FFI 降级模块验证策略（多模块链路下的缺口收口）**：模块降级为 FFI 桥接时，等价验证分三层，避免「降级即放弃验证」——(a) **验证范围**：rustmigrate 只对 **Rust 侧** wrapper 做验证（L1 快照 + 手工审阅，覆盖率仅计 Rust 侧，见 § 7.1.1）；FFI 包装暴露的源实现侧语义由人工负责，wrapper 的类型映射正确性纳入手工审阅项。(b) **链路标记**：链式依赖中若 A 降级 FFI 供 B 消费，B 的 L3 沿用 § 7.6.2 的 `L3_blocked` / `requires_manual_review` 标记机制（不新增字段）。(c) **止损联动**：链路降级占比超 § 4.9 阈值（单链路 30% / 多链路 50%）触发链路评审。

| 项目类型 | 录制方式 | 对比方式 |
|---------|---------|---------|
| CLI 工具 | args → stdout/stderr/exitcode | 黄金文件逐字节对比 |
| HTTP 服务 | mitmproxy 录制请求/响应 | JSON diff + header 对比 |
| 库/SDK | FFI(PyO3/napi-rs) 调用原实现 | proptest 生成输入对比输出（**仅适用于纯函数**，见 §7.6.1） |
| 有状态服务 | 共享数据库 schema | 操作后状态 snapshot 对比 |

> **注：FFI 调用方向不对称**：PyO3 支持 Rust 嵌入 Python 解释器（`Python::with_gil`）实现 Rust→Python 调用；napi-rs 仅支持 Node.js→Rust 方向，L2/L3 对比的 TS 原实现调用须通过反转测试运行器（Node.js 加载 .node addon 调 Rust 新实现 + 原生调 TS 旧实现，结果输出 JUnit XML 供 cargo nextest 消费）或子进程桥接（proptest 序列化输入→node 子进程执行 TS 函数→反序列化输出）实现。

### 7.6.1 库函数 FFI 对比可行性检查清单

FFI 桥接（napi-rs/PyO3）调用原实现做 proptest 对比，**只对纯函数可靠**。实施前 verifier 须按下表判定。

**纯函数识别标准（4 个条件全满足）**：
1. 无全局/静态状态修改；
2. 无 I/O 操作（文件、网络、stdout）；
3. 无系统时间/随机源依赖；
4. 输出确定性（同输入恒同输出）。

**TS 纯函数检测分层（MVP，analyzer SubAgent 负责）**：
1. **静态 AST 规则（tree-sitter，TS 上下文）**：函数体内 ① 无 `this` 绑定/无对 `this.` 的写、② 无对自由变量（闭包外部变量）的赋值/`push`/`Object.assign` 等变更、③ 无 `async`/`await`/Promise 构造、④ 无 I/O 调用（`fs.*`/`fetch`/`console.*`/`process.*`）、⑤ 无 `Date.now`/`Math.random` 等非确定源。命中全部 → `purity_confidence: high`。
2. **动态兜底（简化，不做 code2vec 之类的过度工程）**：静态规则有歧义时（如经第三方库间接 I/O），由 translator 在 `{module}-intent.md` 标注函数为 pure，verifier 在对抗审查时 sign-off 确认 → `purity_confidence: medium`。
3. **保守回退**：无法确定 → `purity_confidence: low`，**不**自动启用 FFI proptest 等价，转入「有状态库函数三选一」。

**输出契约**：analyzer 在 `source-graph.db` 的函数节点上标注 `purity_confidence: {high|medium|low}`（可经图 API 查询），并产出汇总 JSON 列出 `{function_id, purity_status, detection_method, confidence_score}`。仅 `high`/经 sign-off 的 `medium` 函数才用于 FFI proptest 等价对比。

> **非 JSON-safe 纯函数的 L2 路径**：`purity_confidence=high` 但参数含 Map/Set/TypedArray 的函数，不走子进程桥接，改由反转测试运行器直接在 Node.js 进程内做等价对比（无需 JSON 序列化）——Node.js 端原生构造 Map/Set 实例调用 TS 旧实现，napi-rs 侧通过 JsMap/JsObject 映射传递给 Rust 新实现，结果对比输出 JUnit XML；PARITY.md L2 confidence 列标注 `reverse-runner`。含真正不可序列化类型（闭包、Symbol、带状态 RegExp）的函数标记 `requires_manual_review`。

**模块级 L3 FFI 置信度决策规则（纳入 Step 6 verifier 系统提示，按决策树行动、不留自由判断空间）**：置信度枚举与 [04 § 5.3](./04-toolchain.md#53-tier-1推荐画像自动启用) 的 `tier1_exceptions` confidence（low/medium/high）对齐。verifier 统计模块内导出函数的 purity_confidence 分布——若 `purity_confidence < high` 的导出函数占比 **> 30%**，进入降级路径：(a) 该模块的高/medium 置信函数仍跑 FFI proptest，low 置信函数转「有状态库函数三选一」；(b) 若降级后仍无足够高置信函数支撑等价证据，标记 `requires_manual_review`。模块的 L3 置信度以「partial-confidence」形式记入 PARITY.md L3 confidence 列（如 `67% high + 20% medium`），而非离散标签。

> **M0 Spike 1 验收点**：analyzer 的 TS 纯函数检测在 **≥2 个项目 × ≥5 函数/项目（合计 20+）** 上，与人工标注的一致率 ≥ 80%。**「一致」定义为二值**：`purity_confidence` 等级匹配（high/medium/low 各自相等才算一致；等级不同即不一致，即便二者都倾向「应做 FFI proptest」）。按效应类别（I/O 调用 / 全局状态 / 间接副作用）**分别**统计假阳/假阴率；任一类别准确率 < 75% 则**先收紧该类静态规则再进 M1**。基线写入 `DESIGN_ASSUMPTIONS.md`，但**不作 M1 阻塞门槛**——若真实项目分布不同，作为 M1 增量改进项延后处理（避免 M0 过度规约）。

**有状态库函数的处理方案（三选一，记录权衡）**：

| 方案 | 做法 | 成本/代价 |
|------|------|----------|
| 部分纯化 | 在隔离环境（重置状态）重跑 FFI 对比 | 需构造可重置桩，中等成本 |
| 接口契约验证 | 仅按接口契约断言（documented but unverified） | 低成本，但等价性未真正验证 |
| 降级 manual/skip | 标记 `requires_manual_review` 或跳过 FFI 对比 | 失去自动等价保证 |

**FFI 调用性能指导**：行为等价对比需在每条 proptest 用例中额外调用一遍原实现（FFI），在默认 `proptest_cases = 256`（见 [06 § 11.1](./06-plugin-structure.md#111-rustmigratetoml-配置文件)）下可能使测试耗时翻倍。
- **阈值来源**：单次 FFI 延迟阈值 `ffi_sampling_threshold_ms`（默认 10，配置见 [06 § 11.1](./06-plugin-structure.md#111-rustmigratetoml-配置文件) `[testing]`）以 M0 实测的 PyO3/napi-rs 延迟 p95 为单位。M0 在现有 Spike 1（SubAgent 编排）/ Spike 3（tree-sitter 精度）中**顺带**对 3 个验收项目的纯函数各测 1-2 个函数，记录 p50/p95/p99 写入 `DESIGN_ASSUMPTIONS.md`「FFI 性能基准」节（成本 < 1 人天，不新增独立 Spike），同时验证所选 Rust→TS 调用机制的端到端可行性（子进程桥接或反转运行器）。
- **自动决策（取代三值策略）**：verifier 在生成测试前先用当前函数的 FFI 延迟估算单函数测试耗时（`proptest_cases × 测得延迟`）；若预计 > 30s，**自动**逐级下调 `proptest_cases`（256→64→32，每级记日志），而非让用户选采样策略。
- **延迟未知时**：先对当前函数做微基准（10 次 warmup + 10 次采样，< 10s），结果写入 `.rust-migration/ffi-baselines.json`，据此决定是否下调用例数——避免盲目采样，决策链可追溯（实现细节见 [09 附录 B § /migrate run Step 5](./09-appendix-schemas.md#附录-bmvp-skill-的-skillmd-骨架)）。

> 本清单作为 verifier 对抗审查的一个维度纳入其系统提示。

### 7.6.2 L3 差异测试的依赖状态前置检查（模块间依赖场景）

L3 差异测试通过 FFI 桥接调用**旧实现**做对比（库/SDK 场景）。其隐含假设是「被测模块的旧实现可独立运行」。但当模块 A 依赖尚未迁移完的模块 B 时，调用 A 的旧实现需要 B 的旧实现——在模块间部分迁移状态下，若 B 已被部分改写，`source-ref/` 中可能找不到 B 的完整可运行旧实现，FFI 对比的旧实现侧将无法构造，等价性验证失效。

**前置要求（纳入 verifier SubAgent 系统提示，执行 L3 FFI 对比前必须先检查）**：

1. **依赖状态检查**：执行 L3 FFI 对比前，verifier 先查询被测模块的依赖（通过 `rustmigrate graph deps <module>` 或 migration-state.json），判定每个依赖在 FFI 对比中能否提供稳定的旧实现侧。此逻辑与 [09 附录 B § /migrate run Step 0.6 依赖就绪检查](./09-appendix-schemas.md#附录-bmvp-skill-的-skillmd-骨架)的依赖遍历同构，仅判定目标不同（前者判翻译就绪，此处判 FFI 参考就绪）。
2. **可对比条件**：依赖 B 满足以下任一时，A 的 L3 FFI 对比可执行——(a) B 的 `source-ref/` 旧实现完整保留且未被改写（源文件保留纪律见 § 4.3 Step 0）；(b) B 已 `done` 且 Rust 侧可经 FFI 反向被旧实现调用（罕见，需双向桥接）；(c) B 已迁移为 Rust 但 `status ≠ done`（跨边界类型不匹配）时——(c-i) Rust 版可经 marshaling 适配器被调用侧 FFI 消费（M2 特性，MVP 不实现），或 (c-ii) B 降级为 `type_conversion_pending` 状态，A 的 L3 验证标记 `blocked`，记入 KNOWN_DIFFERENCES.md 为 `L3_blocked: pending_upstream B`，等待 B 转 `done` 后重新触发。
3. **执行时机与决策归属**：本检查在 Step 6 测试生成、执行 L3 FFI 对比**之前**完成。verifier **只做标记不改编译状态**——统计「无法提供旧实现侧」的依赖数，若 > 0 则在 KNOWN_DIFFERENCES.md 标记 `L3_blocked: depends on {module}`，并将降级决策权**交回 translator/用户**（verifier 无权改变模块编译/done 状态）。当发现某依赖处于 `type_conversion_pending` 时，verifier 向 translator 返回建议并列出三选一（整体迁移链 / 该依赖降级 FFI / 本模块 L3 延后），由 translator/用户确认，**不得由 verifier 自动决策**。
4. **不满足时的处理**：若依赖链上存在无法提供旧实现侧的模块，沿用 § 7.1「依赖链路限制」的三选一策略——(1) 整体迁移依赖链（A+B 同 Sprint），(2) B 降级为 `degrade_ffi` 供对比复用，或 (3) A 的 L3 验收延后到完整 G3 框架。选 (3) 延后时，须在 `.rustmigrate.toml` 明确记录推迟到哪个 sprint/M 版本，避免「延后验收变永久跳过」。

**可观测性**：依赖状态由 PARITY.md 既有「依赖图状态（Dependency Graph Status）」列承载（取值 `ready` / `blocked: {module}`，schema 权威见 [05 § 6.3](./05-documentation-system.md#63-paritymd--迁移进度与等价深度跟踪)），逐一检查模块依赖：均 `done` 或 `source-ref/` 完整保留则 `ready`；存在 `type_conversion_pending` 或其他中间态则 `blocked: {module}`，使「哪些模块当前可做 L3 FFI 对比」对人类可见。

> **M2+ 自动化预留（设计建议）**：M2 时可向 migration-state.json 模块元数据（schema 权威见 [09 附录 A](./09-appendix-schemas.md#附录-amigration-statejson-schema)）增加可选布尔字段 `ffi_ready_dependencies`，由 `rustmigrate validate state` 程序化计算并标注，把 M1 的人工检查升级为自动判定。M1 阶段不引入该字段，依赖状态由 verifier 按上述前置要求人工核查。

## 7.7 不等价证据探测维度清单

verifier SubAgent 在对抗性审查阶段，应在以下维度系统性探测新旧实现的行为差异。此清单作为 verifier 的"检查表"，确保不遗漏常见差异点：

| # | 探测维度 | 具体探测点 | 典型差异来源 |
|---|---------|-----------|-------------|
| 1 | **边界值** | 空输入、最大值、零、负数、最小值 | 不同语言对边界值的默认处理不同 |
| 2 | **类型边界** | null/undefined/NaN、整数溢出点（i32::MAX+1）、类型强制转换边界 | JS number 是 f64，Rust 有严格整数类型 |
| 3 | **集合操作** | 空集合、单元素、大集合（>10K 元素）、迭代顺序依赖 | HashMap 迭代顺序随机化 |
| 4 | **时间/日期** | 时区边界（UTC+-12）、夏令时切换、闰秒、epoch 前日期 | 时区库实现差异 |
| 5 | **字符串** | 空串、Unicode 多字节字符、emoji（多码点）、超长字符串（>1MB）、特殊字符（\0, \r\n） | UTF-8 vs UTF-16 长度语义 |
| 6 | **并发** | 多线程竞态、取消/超时、死锁场景、共享状态一致性 | GC vs 所有权模型差异 |
| 7 | **错误路径** | 所有 catch/except 分支、嵌套错误、错误链传播、panic vs Result | 异常模型 vs Result 模型 |
| 8 | **浮点精度** | 累积误差（长链计算）、比较精度（epsilon）、NaN 传播、+-Inf、-0.0 | IEEE 754 实现/优化差异 |
| 9 | **意图一致性（Semantic Contract Fidelity）** | 读取 `{module}-intent.md`，逐字段核对 Phase A 代码与意图摘要 7 字段（接口签名/前后置条件/错误模型/并发模型/边界值处理/可观测副作用）；Phase B 后比对 MDR 语义变更是否仍符合意图约定 | 翻译偏离语义契约（意图漂移），非源码↔代码语义差异 |

**维度 → proptest Strategy 映射（确定性规则，纳入 verifier 系统提示，不由 LLM 临时发挥）**：维度 1-8 各有固定的 proptest `Strategy` 模板，维度 9 为契约核对（非属性测试）：

| 维度 | proptest Strategy 模板（示例） |
|------|------------------------------|
| 1 边界值 | `prop_oneof![Just(T::MIN), Just(T::MAX), Just(0), Just(-1), any::<T>()]` |
| 2 类型边界 | 整数溢出点（`i32::MAX as i64 + 1`）、`f64::NAN`/`INFINITY`、强制转换边界 |
| 3 集合操作 | 空集合 / 单元素 / `>10K` 元素；HashMap/BTreeMap 遍历顺序对比 |
| 4 时间/日期 | 时区边界（UTC±12）、DST 切换点、epoch 前时间戳 |
| 5 字符串 | 空串、emoji（多码点）、`\0`/`\r\n`、`>1MB` 串；UTF-8/16 长度对比 |
| 6 并发 | 多线程竞态、取消/超时序列（配合 loom/shuttle，见 § 7.1 L7） |
| 7 错误路径 | 覆盖每个 catch/except 分支、错误链传播、panic vs Result |
| 8 浮点精度 | epsilon 对比、NaN 传播、±Inf、-0.0、长链累积误差 |

**§7.7 维度 proptest 与 §7.1.1 L2 proptest 的关系**：§7.7 维度 proptest 定义的是强制输入策略（边界值/类型边界/集合等），对所有导出函数适用；其断言因模块类型而异——纯函数：断言为 `old(x)==new(x)`（与 L2 叠加，共享 `proptest-regressions` 回归集）；非纯函数（有状态/异步）：断言为 Rust 实现在边界输入下不 panic 且满足函数后置条件（自正确性验证，无需 FFI），不受 §7.1.1「L2 非强制」约束。两者共用 Strategy 模板但互不替代。

**行动指南**：verifier SubAgent 的系统提示中应包含上述两表。对每个导出函数，verifier **必须**按其涉及的数据类型和操作从维度 1-8 中选出适用维度，**为每个适用维度生成 ≥ 1 个 proptest case**（断言类型按上段规则：纯函数用 FFI 等价断言，非纯函数用自正确性断言；执行时序：维度选定在 Step 4 对抗审查时完成；proptest 生成在 Step 6 执行，时序详见 § 7.1 proptest 回归集管理策略第 1-2 点），并将选择结果记录到 `test-fixtures/{module}-dimension-coverage.yaml`（使「哪些维度被覆盖、哪些被判定不适用」可审计）。适用维度数下限由 `.rustmigrate.toml` `[testing] min_applicable_dimensions_per_function` 控制（默认按函数复杂度：1-2 个参数 ≥ 3 维，3+ 个参数 ≥ 5 维——多参数函数涉及更多类型交互，天然跨越更多探测维度）；生成完成后 verifier 自校验，不满足则标记模块 incomplete 并阻塞 `done` 转移。维度 9（意图一致性）对所有模块强制执行——读取意图摘要并逐字段核对，确保对抗审查不只检查「源码 vs Phase A」语义一致性，也检查「意图摘要 vs Phase A」契约一致性。

**测试生成目标（verifier 系统提示要点）**：测试生成针对 **Phase B 最终代码**，非 Phase A。Phase B 允许的语义重写（仅限三类：并发模式/取消安全/局部性能优化，见 §4.3 Step 5 边界）须与意图摘要保持 parity；若 MDR 显示某重写改变了错误处理策略、返回值语义或并发可见性，verifier 须先校验更新后的语义是否仍符合意图摘要，不符则先更新 `{module}-intent.md`（标记语义漂移并在 MDR 中说明）再生成测试。

---

# 八、迁移动机驱动的策略路由

不同动机决定不同的优先级和验收标准：

| 动机 | 迁移顺序 | 额外工具 | 验收标准 | 允许"不等价"？ |
|------|---------|---------|---------|---------------|
| 性能 | profiling 驱动，热路径优先 | criterion 必须 | benchmark >= 原版 | 是（更快的算法） |
| 内存安全 | unsafe 密集区优先 | cargo-geiger + Miri 必须 | CVE 消除 | 否 |
| 部署简化 | 整体迁移 | cross 交叉编译 | 单二进制部署成功 | 否 |
| 并发安全 | 并发热点优先 | loom/shuttle 推荐 | 编译通过 = 无数据竞争 | 否 |
| 合规 | 外部要求驱动 | cargo-deny 必须 | 审计报告通过 | 否 |

**行动指南**：PROFILE 阶段画像时确认迁移动机（支持多动机，`.rustmigrate.toml` 中 `migration_motives` 数组，首项为主要动机），据此自动配置 Tier 1/2 工具和验收标准。多动机场景下取各动机工具和验收标准的并集。

**§7.5 确定性指标阈值的动机调整**：质量评分卡（§7.5）的告警阈值按主要动机调整——`内存安全` 动机下「代码行数比」告警阈值放宽（3.0x → 3.5x，安全检查/边界处理会引入额外代码）；`性能` 动机保持严格阈值且新增 benchmark 必须项；多动机取最宽松的阈值并集。调整后的阈值作为 verifier 评分的输入。
