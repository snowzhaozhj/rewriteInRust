# 项目状态快照

> 每次会话结束前更新。新会话读此文件 → 找到 PLAN.md 对应任务 → 继续执行。

## 当前位置

- **Milestone**: M1 MVP
- **Phase**: Phase 1 四路并行实现 ✅（含审查修复）
- **下一步**: Phase 2（集成验证）

## 进行中的任务

_无_

## 下一步

1. 执行 **Phase 2（集成验证）**
   - M1-INTEG-01: `main.rs` 全命令路由（clap subcommands）
   - M1-INTEG-02: Thin E2E: init → graph build → graph topo 链路
   - M1-INTEG-03: 所有命令输出符合 JSON 格式
   - M1-INTEG-04: `just ci` 全量通过

## 阻塞项

- Plugin Live 验证（skill/agent/hook 实际触发）需在交互式会话中补全
  - 影响范围：仅 Phase 3（Plugin 实现），不阻塞 Phase 1-2

## Handoff Note

**本次完成**：Phase 1 审查修复

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
