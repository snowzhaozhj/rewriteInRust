# M2 质量提升 — 执行记录归档

> M2 计划详见 [PLAN-M2.md](PLAN-M2.md)。Sprint F 验收详见 [sprint-f-acceptance.md](sprint-f-acceptance.md)。
> 本文件归档 M2 各阶段的执行细节、PR 记录、审查修复要点，从 STATUS.md 迁出以保持主文件简洁。

## M2 完成总结

- **Milestone**: M2 质量提升 ✅（2026-06-23 真实迁移验收补强通过）
- **测试基线**: 407 测试 / clippy -D / deny / fmt / shellcheck 全绿
- **CI 覆盖率**: 91.96%
- **Sprint**: M2 全 Sprint 完成 + rxjs scheduled 子树(40 文件/31 单位/89 测试)端到端迁移通过，含 10 文件 SCC stub-first 契约门

### Phase 2 Level 0 ✅ 天花板假设已证（最大盲点闭合）

- **量了再信**：`graph interfaces --members`（整组 SCC 导出签名一次输出）已实现。`signature` 由 **build 时 lang adapter 用 tree-sitter AST 提取**（function/class 剥到 body 子节点前、interface/enum 整节点），与 `line_range` 同级**持久化进图**（`nodes.extra` JSON 列，零 schema 改动），query 直读 `node.signature`。
- **mobx 实测（SCC=51 文件 / 187 导出，比 41 真环保守）**：签名总计 **~4,297 token**（AST 精确提取）。**远低于 200K 窗口 → 「契约 agent 装得下」成立，>40x 余量，无需 SCC 子簇分契约**。
- **架构重构（审查驱动）**：初版在 CLI 层回读源文件 + 手写括号扫描（mini-lexer）剥函数体——经用户质疑「为何在 CLI 重造 lexer + TS 语义泄漏到语言无关层」+ codex 异构确认，重构为「build 时 AST 提取、存图、query 直读」。codex 抓到**致命点**：signature 必须纳入 `structure_hash`，否则改返回类型时增量判 COSMETIC→不重写节点→DB signature 过期（已修 + 回归测试）。删除 read_source_lines/find_body_brace/missing_source warning 整套回读补丁。
- **Level 1**：core 单测 `signature_extraction_by_kind`（AST 按种类提取）+ `structure_hash_sensitive_to_signature`（增量正确性）+ `persist_round_trip_preserves_signature`；CLI e2e `--members` 读图签名。**412 测试全过** + clippy -D + fmt。
- **PR-A ✅ 已合并**（#27 re-export 透传消除 barrel 假环 + Level 0 度量；#28 signature 进图设计契约同步 + MDR-005）。
- **PR-B ✅ Level 2 手写契约+stub 机制自洽已证**：[PR #29](https://github.com/snowzhaozhj/rewriteInRust/pull/29)。
- **PR-C ✅ 提示词改造 + MDR-006 + Level 3 LLM 端到端跑通**：[PR #30](https://github.com/snowzhaozhj/rewriteInRust/pull/30)。

### Sprint F 进行中：破环（M2-SCALE-SCC）✅

- **设计变更**：源码循环依赖不再拒绝填充，改为 **SCC 缩点折叠为翻译单元**——每个强连通分量成为一个 composite 模块组（`ModuleState.member_files`），在缩点 DAG 上排 sprint 层级，translator 整组翻译为一组互引 Rust mod。
- **实现**：`topo.rs` 新增 `scc_groups`/`SccGroup` + 缩点层级；`lib.rs` populate 删拒绝改折叠；`state.rs` ModuleState 加 `member_files`。
- **真实项目验证**：zod（82 文件）→ 75 模块，8 文件核心环折叠为 1 个 full-tier composite 组，4 sprint 层级。
- **PR [#25](https://github.com/snowzhaozhj/rewriteInRust/pull/25)**（7 commit），GitHub CI 全绿，本地 404 测试全过。

### Sprint F 验收

详细记录见 `docs/sprint-f-acceptance.md`。

- **F1 第二轮 ✅**：3 个项目 8 模块全部 done（es-toolkit/chevrotain/yaml），15 tests pass。
- **F2 full 档端到端 PoC ✅**：io-ts FreeSemigroup/DecodeError 两模块全打通。
- **F2 降级 ✅**：io-ts Schemable 正确降级。
- **F3 并行吞吐 ✅**：8 模块并发，远超 ≥1.5 模块/小时基准。
- **F5/F7/F10/F11/F12 全过 ✅**：WAL/proptest/fuzz/覆盖率/性能/回归全绿。
- **验收补强 ✅（2026-06-23）**：rxjs scheduled 子树 40 文件端到端迁移完成。
- **遗留（移交 M3）**：F2-FFI / F4/F6 数据缺口。

### 🔴 重大 bug 修复：graph 漏解析 ESM `.js` 扩展名 import

- **PR [#26](https://github.com/snowzhaozhj/rewriteInRust/pull/26)**。
- **根因**：现代 NodeNext TS 项目相对 import 带 `.js` 但指向 `.ts` 源，`resolve_import` 无此映射。
- **修复**：strip JS 扩展名后按源扩展名重试 + `can_handle` 排除测试文件。
- **重构**：扩展名清单下沉到 `LanguageAdapter::import_specifier_extensions()`。

### Sprint D/E 完成总结（3 个 PR，3 波并行执行）

| 波次 | PR | 测试 | 任务 |
|------|-----|------|------|
| 波次 1 | [#22](https://github.com/snowzhaozhj/rewriteInRust/pull/22) ✅ | 269→291 | VER-01/02, CICD-01+COV-01, PARITY-01, ADV-04/05/09/10 |
| 波次 2 | [#23](https://github.com/snowzhaozhj/rewriteInRust/pull/23) ✅ | 291→353 | CLI-01~06, ERR-01, SCALE-02 |
| 波次 3 | [#24](https://github.com/snowzhaozhj/rewriteInRust/pull/24) ✅ | 353→399 | SCALE-03, SCALE-01, SCALE-LOCK, PETGRAPH-01, ADV-01/02/08 |

### Sprint D 任务清单

| 任务 ID | 状态 | 内容 |
|---------|------|------|
| M2-SCALE-02 | ✅ | 写隔离：types/parallel.rs + run.md 通信协议 |
| M2-SCALE-01 | ✅ | Workflow 批量翻译：新建 workflow.md |
| M2-SCALE-LOCK | ✅ | 全局锁改造：编排器持锁，SubAgent 不取锁 |
| M2-PETGRAPH-01 | ✅ | petgraph 副本隔离验证 + WAL 回归 |
| M2-ADV-01 | ✅ | 多候选生成：translator 多策略 + verifier 选优 |
| M2-ADV-02 | ✅ | 降级 FFI：binding 桩 + 降级报告 + 环断点选择 |
| M2-ADV-04 | ✅ | graph build --profile 性能画像 |
| M2-ADV-05 | ✅ | graph interfaces --deps-of 批量输出 |
| M2-ADV-08 | ✅ | profile 自动定位 analysis-tools.json |
| M2-ADV-09 | ✅ | 子进程超时（wait-timeout） |
| M2-ADV-10 | ✅ | persistence 配置段 |

### Sprint E 任务清单

| 任务 ID | 状态 | 内容 |
|---------|------|------|
| M2-VER-01 | ✅ | proptest 图操作不变量（7 个属性测试） |
| M2-VER-02 | ✅ | cargo-fuzz 解析器健壮性（2 fuzz target） |
| M2-COV-01 | ✅ | 覆盖率门禁（cargo-llvm-cov CI 集成） |
| M2-CICD-01 | ✅ | GitHub Actions CI（5 并行 job） |
| M2-PARITY-01 | ✅ | PARITY.md 等价深度扩展 |
| M2-SCALE-03 | ✅ | 增量图更新：三级变更检测 + 反向 BFS + 熔断 |
| M2-CLI-01 | ✅ | graph rdeps 反向依赖 |
| M2-CLI-02 | ✅ | graph cycles SCC 环检测 |
| M2-CLI-03 | ✅ | graph export JSON/DOT/Mermaid |
| M2-CLI-04 | ✅ | validate config |
| M2-CLI-05 | ✅ | state update --cas-version CAS 乐观锁 |
| M2-CLI-06 | ✅ | validate state --check-blocked --auto-unblock |
| M2-ERR-01 | ✅ | 错误码枚举化（E001-E015） |

### 审查修复要点（波次 3）

- profile 参数透传（增量模式下 --profile 全零 → 修复）
- remove_stale_fingerprints 事务保护
- structure_hash 纳入 calls 摘要（防 Calls 边过期）
- FFI 桩参数名 sanitize + Rust 关键字 r# 转义
- cmd_graph_build 全量路径指纹代码消重（-26 行）
- skip.effort 按 downstream_count 分档
- 全量构建一次遍历同时产出图和指纹（消除双遍历）

### M2 已知问题 / TODO（M3 处理状态）

- **TODO(M3-FFI)**: scaffold/ffi.rs → **M3 Sprint A 归档，决策：degrade_skip 为唯一降级路径**
- **DEVIATION 4 项待 MDR** → **M3 Sprint A 补录**
- **TODO(M3) constructor_bindings 泛化** → **M3 Sprint A 改名 instance_type_bindings**
- **TODO(perf) 多源 BFS** → M4
- **TODO(refactor) 层级计算抽共用** → M4
