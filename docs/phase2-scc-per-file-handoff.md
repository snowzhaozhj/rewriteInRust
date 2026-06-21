# Phase 2 交接：SCC 组「逐成员文件翻译 + 整组编译门禁」

> 续接文档。新会话先读本文 → 读 `docs/STATUS.md` → 按「续接指引」开工。
> 由 Sprint F 真实项目（mobx）验收引出的架构改造。

## 一、背景与根因链（为什么做）

Sprint F 用本项目 CLI dogfood 真实 TS 库做验收，mobx（57 文件）卡死，挖出三个独立根因：

1. **barrel 假环**（✅ 已修，Phase 1）：图层把 `export * from './x'` 建边 internal→x、`import from './internal'` 建边 consumer→internal，barrel（mobx `internal.ts`：54 个 `export *`，52/57 文件 import 它）→ 双向边 → 全库坍缩成虚假巨型 SCC。
2. **解析健壮性**（⏳ 未做，独立 PR）：`analyze_file`（`cli/crates/core/src/lang/typescript.rs:72`）只在 `parser.parse()` 返回 None 时报错，**不查 `root.has_error()`**。tree-sitter-typescript 0.21.2 对真实文件（mobx `api/action.ts` 的 call-signature 泛型 `<T extends Function>(fn: T): T`）部分解析失败 → ERROR 节点后的符号（`action`/`isAction`/`autoAction`）静默丢失 → exported_names 不全 → 少数边无法透传回退到 barrel。
3. **真环 + 翻译单元误用**（⏳ 本 Phase 2 要解决）：barrel 修掉后，mobx 反应式核心**仍是 41 文件真环**（core/types/api/utils 互递归，如 `utils/utils.ts` import `globalState`→`core/globalstate.ts`，core 又依赖 utils helper）。这不是 barrel 假象，是 mobx 真实架构。而现状把整个 SCC 折叠成**一个翻译单元、一次 LLM 调用**（`plugin/agents/translator.md:79`）→ 41 文件吃不下 → 卡死。

**根本认知**：把图论 SCC（排序工具）当成了「原子翻译单元」。但 **Rust 是整 crate 名称解析，文件书写顺序无关**——`use crate::core::derivation::Derivation` 在 derivation.rs 未写完时也能引用，只要整 crate 编译时存在。**SCC 应是「编译门禁单元」，不是「翻译单元」**。这两个概念被混为一谈。

**Phase 2 目标**：解耦。翻译粒度=单文件（上下文=该文件+组契约），SCC 仅作整组编译门禁。这是真环库可用的唯一路径，也是「按 LLM 自由翻译反推」的架构。

证据复现：
```bash
cd /tmp/sprint-f-candidates/mobx/packages/mobx   # depth-1 clone（若不在，重新 clone microsoft/mobx）
<rustmigrate> graph build --full && <rustmigrate> graph cycles   # 当前仍报 1 个大 SCC（受根因2残留影响 51；模拟解析无缺口后实测真环=41）
```

## 二、已完成（Phase 1）+ 分支/PR 状态

| 项 | 分支 | PR | 状态 |
|----|------|-----|------|
| ESM `.js` 扩展名 import 修复 + 测试文件排除 + 分层重构（扩展名清单下沉 adapter） | `fix/graph-js-ext-import` | [#26](https://github.com/snowzhaozhj/rewriteInRust/pull/26) | OPEN，mergeable，CI 跑过；4 视角审查全过 |
| **re-export 透传转发**（消除 barrel 假环，根因1） | `feat/m2-graph-reexport-transparency`（栈在 #26 上） | 未开 PR | 已 commit `fa62a60` + push；347 测试/clippy/fmt 全过 |

Phase 1 关键实现（`cli/crates/core/src/graph/build.rs`）：
- `ImportInfo.reexport` 标记（`lang/mod.rs`）+ 提取层区分 re-export（`lang/typescript.rs`）。
- `build_reexport_map` + `resolve_reexport_origin`：把 `import {X} from './barrel'` 透传到 X 真正定义处；支持通配 `export *` 展开、具名 re-export 链、循环 barrel visited 保护、memo。全量+增量两条构建路径都接线。
- 单测 `reexport_barrel_does_not_create_false_cycle` 验证假环消解。

**注意**：Phase 1 在 mobx 上只把 internal 入边从 52 降到 ~14（残留受根因2解析缺口影响）。barrel 透传逻辑本身正确（单测+atom 直连 derivation/observable 验证），mobx 未完全消环是因为根因2+3。

## 三、Phase 2 设计（核心：stub-first 契约）

逐文件独立翻译的难题：各文件对跨引用符号必须用**一致的 Rust 类型表示**（含所有权 Rc/Weak/RefCell）。整组一次翻译能保证一致，拆开有风险。解法不是「文档约定」，而是**编译器强制**：

### 机制三步
1. **契约步骤**（组级一次，读全组 TS 签名——紧凑，签名非函数体）：
   - 产出 `intermediate/{group}-contract.md`（6 字段，见下）
   - 产出 `rust_root/<group>/` **可编译的 Rust 签名 stub 骨架**：struct/enum/trait/fn 签名齐全、所有权类型已定、函数体全 `todo!()`；`mod.rs` 写全 `mod` 声明；Cargo.toml 依赖一次写全
   - **契约门**：stub 骨架必须 `cargo check` 通过才算 valid。一致性由编译器保证，不靠 LLM 自觉。
2. **逐文件翻译**（文件级并行）：每个成员文件一个 agent，输入=该文件源码+契约+stub，产出=把对应 mod 的 `todo!()` 填成实现，**签名锁定不许改**。从「自觉遵守文档」降为「填空，禁改签名」。
3. **实现门**：全部填完后整组 `cargo check`/`test`（=现有「真门」）→ compile_fixing → done。

契约 `.md` 6 字段：`module_map`（源文件→Rust mod名+路径）、`exported_symbols`（跨引用符号完整 Rust 签名）、`ownership_graph`（对象引用环边表+每条边 Rc/Weak/Box 决策，**显式标 Weak 回边**——单文件视角看不到的图级决策）、`error_model`（组共享 Error enum 完整定义或独立声明）、`visibility`、`cross_file_calls`（依赖索引）。

**共享写面全冻结**：契约步骤后 Cargo.toml / mod.rs / Error enum / 全部跨文件签名冻结，逐文件 agent 纯填空零共享写 → 同 worktree 内无冲突并行。比现有 `append_only`（`translator.md:184`）更严。

### Phase A/B 映射
```
组级 步骤1   意图摘要（整组一次，记跨文件拓扑）            [现有不变]
组级 步骤1.5 Rust 契约 + stub 骨架（整组一次）→ 契约门(stub check)  [新增]
文件级 步骤2 Phase A 忠实翻译（逐文件并行填 todo!，签名锁定）  [改造]
组级        整组 cargo check（实现门 = 现有真门）          [现有]
组级 步骤3   Phase B 惯用化（契约增量修订 + 逐文件 apply）   [改造]
```
- **多候选（M2-ADV-01）上移契约层**：standard/full 出 2 套所有权/类型策略契约，verifier 选优契约，选定后逐文件只译 1 套（避免逐文件×候选爆炸）。
- **Phase B 不退回整组**（否则又撞上下文上限）：审查要改签名 → 先改契约+stub → 逐文件 apply。

## 四、改动清单

### CLI（先做，纯 Rust 可单测）— `cli/crates/cli/src/lib.rs`
- `collect_exported_interfaces`（:1013）加 `signature_text` 字段：按 `line_range`（Span 1-based 闭区间）从源文件读签名行。**选项 A，不改 SourceNode/SQLite schema**（选项 B 加字段+build.rs 提取+migration 是 M3 级，过重）。
- 新增 `graph interfaces <group> --members` 模式：一次输出整组所有成员导出签名（给契约 agent，省 N 次 CLI 调用）。
- `cmd_graph_interfaces` / `_deps_of`（:924/:946）透传新字段。

### 断点续跑 — orchestrator 管 intermediate 文件（**不改 core/state**）
- `intermediate/{group}-progress.json`：`{contract_valid, stub_check_passed, members:{file:{phase_a,phase_b}}}`。每文件完成即更新（细粒度 checkpoint）。原子写（temp+rename），**仅编排器写**（SubAgent 不写，同「编排器持锁」哲学）。
- `ModuleState.substatus`（自由 String，`types/state.rs:212`）记组级里程碑 `contract_ready`/`phase_a_in_progress`。**substatus 不在状态机转换矩阵约束内 → core/state 零改动**（已确认 `can_transition_to` 只管 status enum）。

### 提示词（遵 `docs/learnings/agent-skill-prompt-guide.md`：自包含、不引用设计文档、Step 号跨文件一致）
- `plugin/agents/translator.md`：改写「SCC 模块组翻译」（:75-83，删「整组一次翻译」改「契约→逐文件→整组门」，所有权决策上移契约 `ownership_graph`）；新增「步骤1.5 组 Rust 契约+stub」小节；Phase A（:105）加 SCC 逐文件分支（填 todo!、签名锁、禁碰 Cargo.toml/mod.rs/Error enum）；多候选（:113）加 SCC 例外；共享写面约束（:179）升级「全冻结」。
- `plugin/skills/migrate/run.md`：新增「步骤4.5 契约门」（L1 校验=stub check 过+contract 6 字段）；步骤6 Phase A（:68）加 SCC 逐文件并行分支+progress checkpoint+签名锁校验；步骤9 Phase B（:87）加契约增量分支；断点表（:42）加 `contract_ready`/`phase_a_in_progress` 路由行。
- `plugin/skills/migrate/workflow.md`：§2a 派发（:47）SCC 组改「先契约 agent，再 N 个 member agent 并行同 worktree」，`dependency_interfaces`（:60）对组内互引不再需要（契约已含）只对跨组用；§2d（:107）明确两道门（契约门/实现门）。

### 设计文档/MDR
- 新增 MDR：记录「SCC=编译门禁单元≠翻译单元」「stub-first 契约」「LLM-first 反推」决策。同步 `docs/design/` SCC 翻译相关章节。

## 五、验证（先轻后重）

- **Level 0（先量天花板，read-only）✅ 已量（假设成立，>40x 余量）**：`graph interfaces --members` 实现 + mobx 实测。
  - **实测（reexport 透传分支，根因2未修，SCC=51 文件——比 41 真环更保守）**：51 文件 / 187 导出符号 → **签名总计 ~4,477 token**（body-stripped，契约 agent 实际输入）；含完整函数体上界 ~37,134 token；51 文件全源码绝对硬上界 ~64,635 token（258 KB÷4）。
  - **结论**：三档全部远低于 200K 上下文窗口——realistic 4.5K（>40x 余量），即便喂全部原始源码（65K）也 ~3x 余量。**「契约 agent 装得下」假设成立，无需 SCC 内子簇分契约**。契约 agent 还需输出 stub 骨架（量级与签名相当）+ contract.md，输出预算同样宽裕。
  - **度量法/已知近似**（不影响结论，均被上界兜底）：(a) 签名按 `line_range` 取行，body-bearing 种类（Function/Class/Variable）截断到首个 `{` 剥离函数体，Interface/Enum/TypeAlias 保留全文（类型定义本身）；(b) 类方法签名未单列（class 截断到 `{` 丢方法签名，mobx 核心以函数/interface/enum 为主故影响小，class 密集 SCC 的 realistic 值会升高，但 65K 原始源码绝对上界封顶）；(c) 对象类型参数 `f(o:{...})` 的内联 `{` 会被提前截断（略低估）。
  - **复现**：`cd /tmp/sprint-f-candidates/mobx/packages/mobx && rustmigrate init && rustmigrate graph build --root . --full && rustmigrate graph interfaces core/observable.ts --members`（读 `data.total_signature_tokens` / `total_fullrange_tokens`）。
- **Level 1（CLI 单测，无 LLM）✅ 已补**：`smoke_graph_interfaces_members_whole_scc_group`（circular-deps 三向环整组、3 成员、`signature_text` 剥离函数体断言、token 合计）入 nextest，409 测试全过。`signature_text`/`token_estimate` 已透传 single/`--deps-of`/`--members` 三模式。
- **Level 2（机制验证，无 LLM，最高价值先做）**：人工写 circular-deps 的 contract.md + stub 骨架 → 验 stub `cargo check` 过（契约门成立）→ 按契约填实现 → 整组 check/test 过 + `Rc::strong_count==1`（破环正确）。手写都跑不通则提示词无意义。
- **Level 3（LLM 端到端，仅 circular-deps 3 文件真环，M2-SCALE-SCC 已用整组方式跑通过）**：新逐文件流程重跑——契约 agent 出 stub（check 过）→ 3 member agent 并行填 → 整组 check/test/clippy 全过 + Rc::strong_count 断言。**断点续跑**：中断在 2/3 文件，重跑验证只重派第 3 个。
- 全程 `just ci`。mobx 41 文件 LLM 实跑留 Phase 2 之后（需先修根因2 + Level 0 确认契约上限）。

## 六、实施顺序（可分 PR）

1. **PR-A（CLI）**：`graph interfaces` signature_text + `--members` + 单测（Level 0/1）。独立可合。
2. **PR-B（机制）**：Level 2 手写契约+stub 验证机制自洽（产出 circular-deps 契约样例进 fixtures/docs）。
3. **PR-C（提示词）**：translator/run/workflow 改造 + MDR + Level 3 LLM 端到端。

## 七、续接指引（新会话开工步骤）

1. 读本文 + `docs/STATUS.md`（Sprint F 段含本轮记录）。
2. 确认 Phase 1 PR #26 是否已合并、`feat/m2-graph-reexport-transparency` 是否已开 PR/合并；据此决定基线分支（建议从 reexport 分支或其合并后的 master 切新分支 `feat/m2-scc-per-file-translation`）。
3. **先做 Level 0**：clone mobx → `graph build --full` → `graph interfaces --members`（实现后）量 41 文件签名 token，确认契约方案上限。
4. 从 PR-A 开始：CLI `graph interfaces` 加 signature_text（选项 A）+ `--members`。
5. 改提示词前重读 `docs/learnings/agent-skill-prompt-guide.md`。

## 八、关键风险（Plan agent 对抗结论）
- ~~契约 agent 上限未量化 → Level 0 先验。~~ **✅ 已量化解除**：mobx 51 文件 SCC 签名 ~4.5K token，>40x 余量（见五.Level 0）。最大盲点已闭合。
- 契约缺编译保证 → **stub-first 解决**（最重要加固，别退回纯 .md 契约）。
- 共享写面并发撕裂 → 全冻结解决。
- 多候选组合爆炸 → 上移契约层。
- Phase B 撞上限 → 契约增量。

## 九、参考文件位置速查
- `cli/crates/cli/src/lib.rs`：`collect_exported_interfaces`:1013 / `cmd_graph_interfaces`:924 / populate:1429-1568 / `state deps`:1613-1729
- `cli/crates/core/src/types/state.rs`：`ModuleState`:207 / `member_files`:233 / substatus:212
- `cli/crates/core/src/graph/topo.rs`：`SccGroup`:45 / `build_scc_groups`:164
- `cli/crates/core/src/graph/build.rs`：`build_reexport_map` / `resolve_reexport_origin` / `add_cross_file_edges`
- `cli/crates/core/src/lang/typescript.rs`：`analyze_file`:72（根因2：缺 has_error 检查）
- `plugin/agents/translator.md`:75-85（SCC 翻译）/ `run.md`:116-134（worktree 协议）/ `workflow.md`:47-145（并行编排+真门）
