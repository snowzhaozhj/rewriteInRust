# MDR-006: SCC 组逐文件翻译 + stub-first 契约（SCC=编译门禁单元≠翻译单元）

> 状态：已定稿（2026-06-21，Phase 2 PR-C 落地）。精化 [MDR-004](./004-scc-fold-break-cycle.md)「SCC 折叠为 composite **翻译单元**」中「translator 整组一次翻译」的部分——折叠机制不变，**折叠后如何翻译**改为逐文件。Level 0/1/2 已实证（见 STATUS Phase 2 段 + `docs/examples/scc-stub-first-contract/`）。

## 背景

MDR-004 把源码强连通分量（SCC）缩点折叠为一个 composite 模块组（`ModuleState.member_files` 列成员），论据是「Rust 同 crate 内 mod 间循环 `use` 合法，源码环不是翻译障碍」。这步**正确且保留**。但 MDR-004 顺带把整组当成「一个翻译单元、一次 LLM 调用」（早期 translator.md「整组一次翻译」），Sprint F 用 mobx（真实 TS 库）dogfood 时撞墙：mobx 反应式核心是 **41 文件真环**，整组源码 + 上下文塞进一次翻译会撑爆窗口 → 卡死。

**根因**：把图论 SCC（拓扑排序 / 编译门禁工具）误当成「原子翻译单元」。这两个概念被混为一谈。

## 决策

**解耦**：翻译粒度 = **单文件**（上下文 = 该文件 + 组契约），SCC **仅作整组编译门禁**。这是真环库可用的唯一路径。

机制 = **stub-first 契约**（一致性靠编译器强制，不靠 LLM 自觉）：

1. **契约步（组级一次）**：读全组导出签名（`graph interfaces <group> --members` 一次取整组，紧凑——签名非函数体），产
   - `intermediate/{group}-contract.md`：6 字段 = `module_map` / `exported_symbols` / `ownership_graph`（环边表 + 每条边 Rc/Weak/Box 决策，**显式标 Weak 回边**）/ `error_model` / `visibility` / `cross_file_calls`。
   - `rust_root/<group>/` 可编译 stub 骨架：struct/enum/trait/fn 签名齐全、所有权类型已定、body 全 `todo!()`、`mod.rs` 全 `mod` 声明、`Cargo.toml` 依赖一次写全。
   - **契约门**：stub `cargo check` 通过才 valid。跨文件签名一致、`Rc`/`Weak`/`RefCell` 所有权类型可解析，**由编译器在「填空之前」一次性锁死**。
2. **填空步（文件级并行）**：每成员文件一个 agent，输入 = 该文件源码 + 契约 + stub，产出 = 填对应 mod 的 `todo!()`。**签名锁定**：`diff` stub 与 impl 仅 body 变化。从「自觉遵守文档」降为「填空、禁改签名」。
3. **实现门（整组 check）**：全部填完整组 `cargo check`/`test`（= 并行编排既有「真门」）→ compile_fixing → done。

## 三个核心认知

1. **SCC = 编译门禁单元 ≠ 翻译单元**。Rust 是**整 crate 名称解析、文件书写顺序无关**——`use crate::core::X` 在 `core` 还是 `todo!()` 时也能引用，只要整 crate 编译时 X 的签名存在。所以「环内文件必须一起翻译」是伪命题；它们只需**一起编译**。
2. **stub-first：一致性是编译器强制的，不是文档约定的**。纯 `.md` 契约靠 N 个 agent 各自「自觉遵守」，跨文件类型不一致要等全部填完整组 check 才暴露。stub 骨架先 check 过 = 把签名一致性前移到填空之前由编译器锁死，这是本决策最重要的加固（**别退回纯 .md 契约**）。
3. **LLM-first 反推架构**。粒度 / 上下文 / 写隔离都按「LLM 单次能可靠翻译什么」反推：单文件 + 紧凑契约装得下（Level 0 实测 mobx 51 文件 SCC 签名 ~4.3K token，>40x 余量），共享写面全冻结后逐文件零冲突可并行。

## 与 MDR-004 的关系

| | MDR-004 | MDR-006（本决策） |
|---|---|---|
| SCC 缩点折叠为 composite 组 | **定**（保留） | 不变 |
| `member_files` / 组代表 key / 缩点 DAG 排 sprint | **定**（保留） | 不变 |
| 组感知依赖门禁（`state deps`） | **定**（保留） | 不变 |
| 折叠后**如何翻译** | 整组一次翻译为一组 mod | **精化**：契约+stub → 逐文件填空 → 整组门 |

MDR-004 未被推翻，只是其「翻译」一环被本 MDR 细化。FFI 切分仍是「单 SCC 超上下文预算」兜底（TODO）——但本决策把这个触发点大幅推后：现在撑爆的是单文件而非整组，41 文件环逐文件后远在预算内。

## 连带决策

- **多候选（M2-ADV-01）上移契约层**：SCC 组多候选 = 契约步产 2 套所有权/类型策略契约，verifier 选优契约，选定后逐文件只填 1 套——避免「逐文件 × 候选」组合爆炸。
- **Phase B 不退回整组**（否则又撞上下文上限）：逐文件惯用化；需改跨文件签名 → 先改契约 + stub（契约门复验）再逐文件 apply，不允许成员单方面改签名。
- **共享写面全冻结**：契约步后 `Cargo.toml` / `mod.rs` / Error enum / 全部跨文件签名冻结，逐文件 agent 纯填空、连 append 都不做 → 同 worktree 内并行无冲突（比 D3 `append_only` 更严）。
- **断点续跑细粒度化**：编排器管 `intermediate/{group}-progress.json`（`{contract_valid, stub_check_passed, members:{file:{phase_a,phase_b}}}`，原子写、仅编排器写），`ModuleState.substatus` 记组级里程碑 `contract_ready` / `phase_a_in_progress`。**core/state 零改动**——`substatus` 是自由 String、不在状态机转换矩阵约束内，progress.json 由编排器管不入图。

## 落地与验证

- **提示词**（PR-C）：`plugin/agents/translator.md`（「SCC 模块组翻译」改契约→逐文件→整组门 + 新增「契约步」小节 + Phase A SCC 填空分支 + 多候选 SCC 例外 + 共享写面全冻结）、`plugin/skills/migrate/run.md`（断点表加 `contract_ready`/`phase_a_in_progress` 行 + 步骤 6 SCC 组 Phase A 的 6a 契约门 / 6b 逐文件填空 + 步骤 9 Phase B 契约增量）、`plugin/skills/migrate/workflow.md`（§2a 派发「先契约 agent 再 N 成员并行同 worktree」+ `dependency_interfaces` 只对跨组 + §2d 两道门）。
- **设计正文同步**：02 § 3.4 / 03 § 4.2 / 04 § 5.7.6「整组翻译为一组 mod」加本 MDR 指针、点明翻译粒度 = 单文件。
- **验证**（详见 STATUS Phase 2 段）：
  - **Level 0**（read-only）：mobx 51 文件 SCC 签名 ~4,297 token，>40x 余量，「契约 agent 装得下」成立。
  - **Level 1**（单测）：signature 进图 round-trip + structure_hash 增量正确性 + `--members` 整组读图。412 测试。
  - **Level 2**（机制，无 LLM）：`docs/examples/scc-stub-first-contract/`——stub `cargo check` 过 = 契约门；impl 由 stub 逐文件填（签名逐字节一致，diff 仅 body）+ 整组 test/clippy 过 + `Rc::strong_count(&emitter)==1` 破环断言。**机制自洽可行**。
  - **Level 3**（LLM 端到端，circular-deps 3 文件真环）：新逐文件流程重跑 + 断点续跑验证。

## scope 边界（非本决策）

- 单个 SCC 超上下文预算的 FFI 切分兜底仍 TODO（本决策大幅推后其触发点，未消除）。
- 解析健壮性（`analyze_file` 缺 `has_error` 检查导致少数边丢失）是独立根因，独立 PR，不在本决策。
- 函数重载 / 匿名 `export default` / class 方法签名未单列 = 已有解析 gap（见 [MDR-005](./005-signature-in-graph.md) scope 边界），独立于本决策。
