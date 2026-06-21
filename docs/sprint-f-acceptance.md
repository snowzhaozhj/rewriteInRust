# Sprint F 验收记录（M2）

> 推进路径（用户定）：**先跑不依赖 FFI 的验收**——无环项目跑 F1/F2/F3/F4/F6，FFI 降级验收（F2-FFI）留到无环验收跑完后。

## F1 项目选取 + 环境准备 ✅

候选项目通过本项目 CLI 自身 dogfood 筛选（`graph build` + `graph cycles` + `graph topo-sort`）：

| 项目 | 文件数 | LOC | 复杂度 | 有环 | 最大fan-out | 用途 |
|------|-------|-----|-------|------|------------|------|
| **io-ts** | 16 | 6202 | moderate | 否 | 6 (TaskDecoder) | B路径主项目 |
| **zustand** | 15 | 1450 | moderate | 否 | 6 (middleware) | B路径副项目 |
| **mobx** | 57 | 8462 | complex | **是(51文件SCC)** | 54 (internal barrel) | 留 F2-FFI 降级验收 |
| ts-pattern | 18 | 4021 | — | 是 | — | 备用 |
| date-fns | — | — | — | — | — | 函数式扁平，文件过多，弃用 |

选型：`/tmp/sprint-f-candidates/` 下 clone（depth 1）。无环项目 io-ts + zustand 满足"≥2项目、每项目≥3模块"。

### 已知 gap（诚实标注）

- **缺"15-25 依赖模块"覆盖**：F1 要求"≥1 个含 15-25 依赖的模块（覆盖中段上下文预算）"。但无环候选最高 fan-out 仅 6（io-ts/zustand）；mobx 的 54 是 barrel re-export（`internal.ts`）且项目有环。**这批中小型库都缺中段上下文预算模块。** 待补：再选一个结构更耦合的大型无环 TS 项目，或在 F2-FFI（mobx）阶段间接覆盖高 fan-out 翻译。

## M2-PERF-BASE 性能基线 ✅

graph build 全量重建耗时（3 次稳定取值）：

| 项目 | graph build real(s) | F11 门槛 |
|------|--------------------|---------| 
| io-ts (16文件) | 0.33 | <10s ✅ |
| zustand (15文件) | 0.23 | <10s ✅ |
| mobx (57文件) | 0.37 | <10s ✅ |

- 单模块翻译时长基线（F6 ≤±10%）：待 F2 翻译循环测得。

## F2 自适应循环验证 — full 档端到端 PoC ✅（io-ts 2 模块）

scaffold workspace → 翻译（Phase A 忠实）→ cargo check/test/clippy → 状态机推进，全管道打通。

| 模块 | tier | 首轮编译 | 测试 | clippy -D | 膨胀率 | 状态机 |
|------|------|---------|------|-----------|--------|--------|
| FreeSemigroup | full | ✅ 通过 | 4/4 | 零 | 1.03x | done |
| DecodeError | full | ❌ E0072 递归→修复后通过 | 2/2 | 零 | 0.87x | done |

### 真实迁移发现

- **TS `unknown` → Rust 无直接对应**：以 `Box<dyn Any>` 作桩，代价是不能派生 `Eq`/`Clone`（io-ts 的 `DecodeError.Leaf.actual`）。Phase B 可按场景泛型化。
- **递归 ADT 跨模块需在引用点 `Box`**：`DecodeError` 经 `FreeSemigroup<DecodeError<E>>` 递归，而 `FreeSemigroup::Of(A)` 内联存储 → 首轮直译触发 E0072（infinite size）。TS 引用语义无此问题，Rust 必须 `Box` 打破。属典型 Phase A `compile_fixing` 修复点。
- **组感知依赖门禁正确**：`state deps DecodeError` 返回 `FreeSemigroup ready=done, all_ready=True`。
- **D3 首轮编译通过率数据点**（小样本）：2 模块中 1 通过 / 1 失败（递归类型）= 50%。说明 worktree 内自检环节对真实项目有价值（首轮普遍带编译错误的假设在递归 ADT 上成立）。

### 流程纠错记录

- 误把 `cargo test`（失败）与 `state transition` 放在同批命令，导致 DecodeError 在编译失败时已被推进 done。修复编译后终态成立，但暴露**编排器应以 check 通过为 transition 前置**（plugin run.md 协议需强调此序）。

## F2 降级机制 ✅（io-ts Schemable，HKT 触发）

- **决定性发现：io-ts 是 HKT（高阶类型）密集库**。`Schemable` 模块约 90% 是基于 fp-ts HKT（`HKT<S,A>`/`Kind<S,A>`）的 typeclass 接口；**Rust 无 HKT，无法忠实翻译**，需重设计为具体 trait + GAT。仅 `Literal`/`memoize` 可迁移（已翻译，8 测试通过）。
- 降级路径验证：`Schemable` 经 `translating→compile_fixing→paused→degrade_skip` 正确降级。**暴露状态机约束：降级（DegradeSkip/Ffi/Manual）只能从 `Paused` 进入**（state.rs:113-116），不能从 translating 直接跳（E004）。语义合理（翻译遇阻→暂停→人类决策降级），但 plugin 编排须知此路径。

## F2 三档覆盖 — 结论：trivial/standard 缺干净样本（选型教训）

| 档 | 状态 | 说明 |
|----|------|------|
| full | ✅ 充分 | io-ts ADT（FreeSemigroup/DecodeError）端到端 + 真实编译修复 |
| 降级 | ✅ | Schemable HKT → degrade_skip |
| trivial/standard | ⚠️ 阻塞 | **两个无环候选都无干净低档样本** |

- **io-ts 全判 full**（HKT 类型库，每模块泛型/接口密集，分档合理）——无 trivial/standard。
- **zustand 有 trivial/standard 但对 Rust 不友好**：barrel re-export（依赖未就绪）、React hooks（无对应）、动态浅比较（`Symbol.iterator`/`Map`/`Object.is` 语义鸿沟）。
- **选型教训**：F1 选项目只评了规模/环/fan-out，未评"可翻译性 / HKT 密度 / 框架耦合"——这是经实际翻译才暴露的维度。

## F3 并行吞吐 — 阻塞：当前素材凑不齐独立可翻译模块

- io-ts 下游（Eq/Encoder/Guard/Schema/Decoder）**全是 Schemable 的 HKT instance**，依赖已降级的 Schemable → 无法翻译。除 FreeSemigroup/DecodeError 两个 ADT 外，**无第 3 个独立可翻译模块**。
- 并行**机制**本身在 M2 已实现 + 测试（workflow.md / parallel.rs / 全局锁改造），此处缺的是**翻译素材**。
- **要验证 F3**：需结构均匀、低抽象、模块独立的中型 TS 项目（解析器 / 算法库 / CLI 工具），或用合成 fixture。**待用户定：重选项目 or 接受当前结论。**

## 待补

- F1 达标（≥3 模块/项目）：受 HKT 阻塞，io-ts 停在 2 done + 1 degrade_skip。
- F3 并行吞吐 + D3 target 冷编开销实测（需合适项目）。
- F2-FFI：mobx 51 文件 SCC 降级（阻塞于 FFI 兜底实现）。

## 🔴 重大 bug 发现 + 修复：graph 漏解析 ESM `.js` 扩展名 import

> 分支 `fix/graph-js-ext-import`，全量回归 406/406 通过。

- **现象**：jsonc-parser（现代 NodeNext TS 项目）依赖图 imports 边 = 0，被**误判"全独立模块、无环"**。
- **根因**：`resolve_import`（`graph/build.rs`）对 `import { x } from './scanner.js'` 找不到 `.ts` 源——TS ESM（NodeNext/Node16）规范要求相对 import **带 `.js`/`.mjs`/`.cjs`/`.jsx` 扩展名，但实际指向同名 `.ts`/`.tsx` 源**。原解析器无此映射 → **任何 ESM TypeScript 项目依赖图全错**（sprint 排序 / 门禁 / SCC 折叠 / 并行编排全部连带失效）。这是 R3（真实项目暴露 AST 边界）的实锤。
- **修复**：strip JS 扩展名后按源扩展名重试；新增 3 个单元测试（js/mjs/cjs/jsx）。扩展名清单由 `LanguageAdapter::import_specifier_extensions()` 声明（TS 返回 `.js/.jsx/.mjs/.cjs`），graph 层 `resolve_import` 据此 strip——语言知识留在 adapter，graph 层不内嵌任何扩展名字面量（与 `resolve_extensions` 同构）。全量 + 增量两条构建路径均已接线。
- **验证**：jsonc-parser `--full` 重建后 imports **0→18**，正确检出 `scanner→format→main→parser→edit` **5 文件循环**；`cargo nextest` 406/406、clippy `-D`、fmt 全过。
- **连带教训**：① 验证解析类改动须 `graph build --full`（增量 build 按指纹跳过未变文件，会掩盖修复效果）；② populate 把 `test/*.test.ts` 当迁移模块（应排除测试文件，待修）。

## 给 M2 验收口径的建议

真实项目验收应**预筛"可翻译性"**：HKT/typeclass 密度、框架运行时耦合、动态类型依赖。io-ts（HKT 库）、zustand（前端框架库）这类超出"模块化忠实翻译"边界，宜归类为"需人工重设计"而非纳入自动迁移吞吐指标。
