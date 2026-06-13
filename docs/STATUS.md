# 项目状态快照

> 每次会话结束前更新。新会话读此文件 → 找到 PLAN.md 对应任务 → 继续执行。

## 当前位置

- **Milestone**: M1 MVP
- **Phase**: Phase 1 四路并行实现 ✅（含两轮审查修复）
- **下一步**: P1 源码图差分校验 harness（M1 验收门）→ 然后 Phase 2（集成验证）

## 进行中的任务

_无_（`worktree-phase1-impl` 已 push commit `098f164`，code-review 修复 PR 待开）

## 下一步

> **优先级 1（下一个独立 PR，M1 验收门）**：源码图差分校验 harness。
> 应在 analyzer / translator 等下游阶段往图上盖楼**之前**完成——图是它们信任的地基。

### P1. 源码图差分校验 harness（独立 PR · M1 验收门）

- **目的**：在真实 TS 仓库上，把自研「文件级 import 图 + 环检测」与外部成熟依赖图工具**差分对比**，确认建图正确性。
- **关键决策（已定，勿改）**：
  - 只校验**文件级 import 图 + 环**（驱动拓扑序的关键层）；符号级 Calls/Extends 不纳入（tree-sitter 启发式、设计上即近似）
  - Oracle = **dependency-cruiser（主）+ dpdm（交叉验证）**；**不用 madge**（`04-toolchain.md:155` 已评估其停更并选用 dependency-cruiser 替代）
  - 绕过 CLI（graph 命令仍 `todo!("Phase 2")`）：新建 `cli/crates/core/examples/dump_import_graph.rs` 直调 core API（`build_graph_ts` + `detect_cycles`，取 `EdgeType::Imports` 边）
  - 两侧**归一化**到同一边形式（相对 src 根、posix、去扩展名、仅项目内边、type-only 两侧一致）
  - **硬门**：对双 oracle 交集的边召回 ≥ 0.98 + 环集合一致
  - 产物：`tools/graph-validation/`（run.sh + repos.txt 钉版本/sha + oracle/compare 脚本 + reports/）+ `just validate-graph`
- **时机理由**：图刚经 code-review 加固＝干净基线；它是下游所有阶段的地基，错误会向下复利；设计已留槽位（`08-roadmap`「3 个公开中型 TS 项目验收」/ `03 §`「M1 验收 3 真实项目校准」）。设计标注「非 M1 阻断」，极端时间压力下可滑至 M2 前 2 周，但不建议推后。
- **完整提示词**：已生成（见对应会话记录），新会话直接粘贴执行；产物按阶段交付流程单独提 PR。

### P2. Phase 2（集成验证）— 不依赖 P1，可并行

   - M1-INTEG-01: `main.rs` 全命令路由（clap subcommands）
   - M1-INTEG-02: Thin E2E: init → graph build → graph topo 链路
   - M1-INTEG-03: 所有命令输出符合 JSON 格式
   - M1-INTEG-04: `just ci` 全量通过

## 阻塞项

- Plugin Live 验证（skill/agent/hook 实际触发）需在交互式会话中补全
  - 影响范围：仅 Phase 3（Plugin 实现），不阻塞 Phase 1-2

## Handoff Note

**本次完成**：Phase 1 第二轮 code-review 修复（commit `098f164`）

### Phase 1 code-review 修复（2026-06-14）

`/code-review` 一轮对抗审查后修复（persist.rs 由另一会话处理；4 项 Phase-2 接线后再修）：

- **TS 提取**（`lang/typescript.rs`）：`export *`/`export * as` 产生 Imports 边；`export const` 箭头/函数表达式入图为 Function 节点；类数据字段不再误判为 Function；泛型/限定父类型 `extends Bar<T>`/`ns.Base` 归一化；`format!` 内联 NodeId 改 `NodeId::symbol`
- **跨文件解析**（`build.rs`）：成员调用/命名空间构造剥离基名正确解析；构造/extends 全局兜底改唯一匹配（消除连错文件的虚假边）；import 别名冲突按歧义处理；跨文件边排序插入
- **确定性**：`GraphStats` 改 BTreeMap；`parallel_groups` 排序；`primary_language` 确定性平局；`compute_level` 递归改迭代（消除深链栈溢出）；自导入识别为环
- **profile**：复杂度只按源语言行数
- **hooks**：`fmt.sh`/`on-rust-file-create.sh` 相对被编辑文件定位 Cargo；`verify.sh` 守护 git rev-parse

**最终状态**: 121 测试全过 | clippy `-D warnings` 零警告 | fmt | shellcheck 全过

### Phase 1 审查修复（2026-06-07）

对照 PLAN.md 全面审查后修复的问题：

**图构建 bug 修复**：
- re-export（`export { x } from './module'`）现在正确生成 import 边
- `export type` re-export 正确标记 `is_type_only`
- 跨文件 calls 边通过 import 映射正确解析（`service.ts:clamp → utils.ts:clamp`）
- extends 边跨文件查找修复（`AuthService implements Serializable` 目标在不同文件时）
- extends 边的添加时序改为两阶段（先添节点后修正边，避免 add_edge 静默丢弃）

**测试覆盖补全**：
- 新增 ground-truth 通用验证 harness（19 个测试），自动读取 ground-truth.json 验证节点/边/拓扑约束
- 覆盖全部 4 个 fixture（linear-deps、diamond-deps、circular-deps、edge-cases）
- 补充 save_to_db / scaffold_project_with_bin 负例测试
- 已知限制标注在 ground-truth.json（泛型调用 `f<T>()` / 方法调用类型推断）

**设计一致性**：
- 补全 Community NodeType（M2 预留，12 种对齐设计文档）
- 更新 schema.sql、persist.rs 映射

**代码质量**：
- 全部 pub 类型添加 doc comment
- precision benchmark ground-truth 更新（re-export import 的正确处理）
- 修复 query.rs unused import 警告

**最终状态**: 105 测试全过 | clippy 零警告 | fmt 通过

### Phase 1 四路并行实现 ✅

4 个 Worker 并行完成：

| Worker | 模块 | 文件 | 测试数 |
|--------|------|------|--------|
| A | graph | build.rs, query.rs, topo.rs, persist.rs, mod.rs | 28 |
| B | state + validate | machine.rs, mod.rs, rules.rs | 27 |
| C | profile + scaffold + stats | detect.rs, template.rs, coverage.rs | 23 |
| D | plugin hooks | hooks.json, on-rust-file-create.sh, post-build.sh, file-guard.sh | shellcheck ✅ |

**之前完成**：M0 Sprint 0 + Phase 0 冻结合约

### M0 Sprint 0 ✅

- Spike S0: Plugin 结构验证 + Release 二进制 743K
- Spike S3: tree-sitter TS 精度 F1=1.0（三维度全满分）
- 决策文档: 001-plugin-viability.md + 002-parser-choice.md
- Sprint 0 GATE 全部通过

### Phase 0 冻结合约 ✅

定义了 Phase 1 并行开发所需的全部公共类型：

| 文件 | 内容 |
|------|------|
| `types/common.rs` | NodeId, Span, SourceLang, Complexity, RiskLevel |
| `types/graph.rs` | NodeType(9种), EdgeType(8种), SourceNode, Dependency, Provenance |
| `types/state.rs` | ProjectState(6种), ModuleStatus(11种), MigrationStateFile 全结构 |
| `types/config.rs` | MigrateConfig (.rustmigrate.toml 5 个配置段) |
| `error.rs` | MigrateError (thiserror, 12 种错误变体) |
| `response.rs` | Response<T> 统一 JSON 输出 |
| `schema.sql` | nodes/edges/metadata/schema_versions 四表 |

### Phase 0 完成标志

- [x] `cargo check` 通过，无 warning
- [x] `cargo clippy -D warnings` 通过
- [x] 所有 pub struct/enum 有 `///` 文档注释
- [x] `schema.sql` 包含 nodes/edges/metadata/schema_versions 四表
- [x] 各类型与 `docs/design/09-appendix-schemas.md` 一致

## 最近完成

| 时间 | 任务 | commit |
|------|------|--------|
| 2026-06-07 | 项目脚手架初始化 | 559da00 |
| 2026-06-07 | PLAN.md v1 + CLAUDE.md + STATUS.md | fd18544 |
| 2026-06-07 | 实施蓝图重构（PLAN.md v2 + fixtures + tooling） | c3acc34 |
| 2026-06-07 | M0 Sprint 0 Spike S0+S3 完成 | 777da76 |
| 2026-06-07 | Phase 0 冻结合约 | b3922c2 |
| 2026-06-14 | code-review 修复（图提取/跨文件解析/确定性/hooks，121 测试） | 098f164 |
