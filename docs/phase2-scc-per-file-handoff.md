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

### signature 提取（已落地，**采选项 B 的轻量形态**）— 见七.架构修订
- ~~选项 A：CLI 按 line_range 回读源文件 + 字符串扫描剥体~~ **已废弃**（CLI 重造 lexer + TS 语义泄漏到语言无关层 + 回读一致性风险，经用户质疑 + codex 确认重构）。
- **采选项 B**，但发现 `nodes.extra` JSON 列正是稀疏属性扩展点 → **零 schema 改动、零 migration**（非原判的「M3 级过重」）：`SourceNode.signature` + `NodeExtra.signature` round-trip + lang 层 AST 提取（`decl_signature`：function/class 剥到 body 子节点前、interface/enum 整节点）+ `structure_hash` 纳入 signature。
- CLI `collect_exported_interfaces` 直读 `node.signature`；新增 `graph interfaces <group> --members`（整组一次输出，省 N 次调用）；`signature`/`token_estimate` 透传 single/`--deps-of`/`--members` 三模式。

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
  - **实测（reexport 透传分支，根因2未修，SCC=51 文件——比 41 真环更保守）**：51 文件 / 187 导出符号 → **签名总计 ~4,297 token**（AST 精确提取，契约 agent 实际输入）。
  - **结论**：远低于 200K 上下文窗口（>40x 余量）。**「契约 agent 装得下」假设成立，无需 SCC 内子簇分契约**。契约 agent 还需输出 stub 骨架（量级与签名相当）+ contract.md，输出预算同样宽裕。
  - **签名提取架构**（审查重构后，见七.架构修订）：`signature` 由 **build 时 lang adapter 用 tree-sitter AST 提取**（function/class 取 `node` 起始到 `body` 子节点起始、剥体；interface/enum 整节点即类型定义），存入 `SourceNode.signature` → `nodes.extra` JSON 列（零 schema 改动）→ query 直读，**不回读源文件、CLI 零语言相关逻辑**。AST 子节点边界天然处理泛型/对象参数/箭头，无字符串扫描歧义。signature 已纳入 `structure_hash`。
  - **已知 scope 边界**（记 TODO，非本次）：函数重载（无 body 的 `function_signature` 当前 walk_ast 不提取）、匿名 `export default`（无 name 当前不入图）、class 方法签名未单列（方法是独立节点、通常非 exported）。mobx 核心以函数/interface/enum 为主，不受影响。
  - **复现**：`cd /tmp/sprint-f-candidates/mobx/packages/mobx && rustmigrate init && rustmigrate graph build --root . --full && rustmigrate graph interfaces core/observable.ts --members`（读 `data.total_signature_tokens`）。
- **Level 1（单测，无 LLM）✅ 已补**：core `signature_extraction_by_kind`（AST 按种类提取）+ `structure_hash_sensitive_to_signature`（增量正确性）+ `persist_round_trip_preserves_signature`；CLI e2e `smoke_graph_interfaces_members_whole_scc_group`（整组读图签名）。412 测试全过。
- **Level 2（机制验证，无 LLM）✅ 已证（PR-B）**：产物 `docs/examples/scc-stub-first-contract/`（`contract.md` 6 字段 + `stub/` 契约门 + `impl/` 实现门）。stub 骨架 `cargo check` 过=契约门成立；impl 由 stub 逐文件填 `todo!()`（签名逐字节一致，diff 仅 body 变化），整组 `cargo test`(2 passed)/`clippy -D` 过 + `Rc::strong_count(&emitter)==1`（Handler 持 `Weak` 破环）。**机制自洽可行**。
- **Level 3（LLM 端到端，仅 circular-deps 3 文件真环，M2-SCALE-SCC 已用整组方式跑通过）**：新逐文件流程重跑——契约 agent 出 stub（check 过）→ 3 member agent 并行填 → 整组 check/test/clippy 全过 + Rc::strong_count 断言。**断点续跑**：中断在 2/3 文件，重跑验证只重派第 3 个。
- 全程 `just ci`。mobx 41 文件 LLM 实跑留 Phase 2 之后（需先修根因2 + Level 0 确认契约上限）。

## 六、实施顺序（可分 PR）

1. **PR-A（CLI）**：`graph interfaces` signature_text + `--members` + 单测（Level 0/1）。独立可合。
2. **PR-B（机制）✅ 已完成**：Level 2 手写契约+stub 验证机制自洽（产物 `docs/examples/scc-stub-first-contract/`）。
3. **PR-C（提示词）⬅ 下一步**：translator/run/workflow 改造 + MDR + Level 3 LLM 端到端。

## 七、续接指引（新会话开工步骤）

1. 读本文 + `docs/STATUS.md`（Sprint F 段含本轮记录）。
2. 确认 Phase 1 PR #26 是否已合并、`feat/m2-graph-reexport-transparency` 是否已开 PR/合并；据此决定基线分支（建议从 reexport 分支或其合并后的 master 切新分支 `feat/m2-scc-per-file-translation`）。
3. ~~先做 Level 0~~ **✅ 已完成**：mobx 51 文件 SCC 签名 ~4.3K token，>40x 余量（见五.Level 0）。
4. ~~PR-A signature_text~~ **✅ 已落地**（#27/#28 合并）；~~PR-B Level 2 机制验证~~ **✅ 已证**（`docs/examples/scc-stub-first-contract/`）。**下一步 PR-C**（提示词改造 + Level 3 LLM 端到端）。
5. 改提示词前重读 `docs/learnings/agent-skill-prompt-guide.md`。

## 八、关键风险（Plan agent 对抗结论）
- ~~契约 agent 上限未量化 → Level 0 先验。~~ **✅ 已量化解除**：mobx 51 文件 SCC 签名 ~4.5K token，>40x 余量（见五.Level 0）。最大盲点已闭合。
- 契约缺编译保证 → **stub-first 解决**（最重要加固，别退回纯 .md 契约）。
- 共享写面并发撕裂 → 全冻结解决。
- 多候选组合爆炸 → 上移契约层。
- Phase B 撞上限 → 契约增量。

## 八.5 架构修订（signature 进图，Level 0 落地时确立）

> 初版按交接文档「选项 A」在 CLI 回读源文件 + 字符串扫描剥体。用户质疑「为何在 CLI 重造
> lexer + TS 语义泄漏到语言无关层」，codex 异构确认后重构为下述正确架构。

**反模式（已废弃）**：build 时 tree-sitter AST 信息齐全却只存 line_range（丢文本）；query 时
为补签名回读源文件 + 手写括号扫描剥 body。三病：重复 IO + 重造 lexer；一致性风险（build 后源
改/删、行号错位）；TS 语义泄漏到语言无关 CLI 层（M3 接 Python/C 必错）。

**正确架构**：signature 是 AST 提取的符号静态属性，与 line_range 同级——
1. **lang 层提取**（`typescript.rs` `decl_signature`）：function/class 取 `node` 起始到
   `body` 子节点起始（剥体）；interface/enum 整节点（body 即类型）。AST 边界天然处理
   泛型/对象参数/箭头，无字符串歧义。提取点：extract_function/class/interface/enum + 箭头 const。
2. **存图**：`SourceNode.signature: Option<String>` → `NodeExtra.signature` → `nodes.extra`
   JSON 列。**零 schema 改动、零 migration**（extra 列即稀疏属性扩展点，`#[serde(default)]` 前向兼容）。
3. **增量正确性（codex 抓的致命点）**：signature 纳入 `structure_hash`，否则改返回类型时
   content_hash 变但 structure_hash 不变 → 增量判 COSMETIC → 不重写节点 → **DB signature 过期**。
4. **query 直读**：CLI `collect_exported_interfaces` 读 `node.signature`，**零源文件回读、零语言逻辑**。

**scope 边界（TODO，非本次）**：函数重载（无 body 的 `function_signature` 当前 walk_ast 不提取）、
匿名 `export default`（无 name 当前不入图）、class 方法签名未单列。这些是已有解析 gap，独立于本次重构。

## 九、参考文件位置速查
- `cli/crates/cli/src/lib.rs`：`collect_exported_interfaces`（直读 node.signature）/ `cmd_graph_interfaces_members` / populate / `state deps`
- `cli/crates/core/src/lang/typescript.rs`：`decl_signature`（AST 签名提取）/ extract_function/class/interface/enum
- `cli/crates/core/src/graph/persist.rs`：`NodeExtra.signature`（extra JSON round-trip）
- `cli/crates/core/src/graph/fingerprint.rs`：`structure_hash`（含 signature）
- `cli/crates/core/src/types/state.rs`：`ModuleState`:207 / `member_files`:233 / substatus:212
- `cli/crates/core/src/graph/topo.rs`：`SccGroup`:45 / `build_scc_groups`:164
- `cli/crates/core/src/graph/build.rs`：`build_reexport_map` / `resolve_reexport_origin` / `add_cross_file_edges`
- `cli/crates/core/src/lang/typescript.rs`：`analyze_file`:72（根因2：缺 has_error 检查）
- `plugin/agents/translator.md`:75-85（SCC 翻译）/ `run.md`:116-134（worktree 协议）/ `workflow.md`:47-145（并行编排+真门）
