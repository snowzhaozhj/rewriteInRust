# 项目状态快照

> 每次会话结束前更新。新会话读此文件 → 找到 PLAN.md 对应任务 → 继续执行。

## 当前位置

- **Milestone**: M1 MVP
- **Phase**: Phase 1 ✅ → 源码图校验 harness ✅（M1 验收门）→ Phase 2 集成验证 ✅ → Phase 3 Plugin analyze 实现 ✅（已合并 master，Live 验证待交互会话）→ M1 收尾 3 项 ✅（PR #7 待合并）
- **下一步**: Phase 4 翻译循环（另一会话进行中）+ PR #7 合并 → M1 graduate

## 进行中的任务

- **PR #5**（`feat/m1-graph-validation-harness`）源码图校验 harness：已合并 master ✅
- **PR #3**（`feat/m1-integ-phase2`）Phase 2 集成验证：已合并 master ✅（commit bfacff1）
- **PR（待提）**（`feat/m1-finalize`）M1 收尾 3 项：实现完成，4 层门禁全过，待提 PR

## 下一步

> **M1 收尾 3 项已实现完成**（分支 `feat/m1-finalize`），4 层质量门全过、design-checker 无 MISMATCH。提 1 个收尾 PR → 审查 → 合并 → **M1 graduate**。

| 任务 | 内容 | 设计出处 | 状态 |
|------|------|----------|------|
| **M1-STATE-04** | 模块级 `transition_module`（Option<to>/substatus/reason 落盘 + blocked 恢复/degrade 重置副作用 + 合法性校验 + 原子写） | 09-appendix | ✅ 实现 + 6 单测 + 2 e2e |
| **M1-PROFILE-04** | profile 工具可用性检测（`--adapter-tools` → ADAPTER_TOOL_MISSING；cargo-nextest → RUST_TOOL_MISSING；结果入 `data.tool_checks`） | 06:90/676/677/865 | ✅ `profile/tools.rs` + 6 单测 + 2 e2e + ts adapter analysis-tools.json |
| **M1-PROFILE-05** | `stats loc` 改 tokei 源码/Rust LOC（`source`/`rust` 双侧 + by_language；路径取 CLI 参数>配置>默认） | 06:99 | ✅ `stats/loc.rs` + 2 单测 + 2 e2e |

> **M2 推迟项**（已在代码 TODO 标注，不在 M1 范围）：增量构建、`graph build --profile` 性能画像、`graph interfaces --deps-of` 批量、`stats compare` 结构对比、ErrorData structured context。
> **M2 符号级精度提升**：跨文件方法调用 `obj.method()` 解析（PLAN §10 **M2-REFAC-10**，已补 2026-06-14 调研的分档方案/recall ~70% 天花板/stack-graphs 避坑；档1 零歧义增强低成本可先做）。

## 阻塞项

- Plugin Live 验证（skill/agent/hook 实际触发）需在交互式会话中补全
  - 影响范围：仅 Phase 3（Plugin 实现），不阻塞 Phase 1-2
  - Phase 3 代码已实现（analyzer/translator/scaffolder SubAgent + SKILL.md analyze 8 步骨架 + TS porting-template）；**待 Live 验证**：① `/migrate analyze` 端到端真实执行（M1-PLG-05）② plugin 内 SubAgent `agentType` 是否需 `rust-migrate:` 命名空间前缀

## Handoff Note

**本次完成**：M1 收尾 3 项（PR #7，分支 `feat/m1-finalize`）实现 + 三方审查 + 两轮修复。

### M1 收尾 3 项审查闭环（2026-06-14）

- 实现 commit `af0cd68`；审查修复 `0fa9210`（/code-review）+ `ca3b37f`（codex + pr-review-toolkit 5 agent + /code-review 三方汇总）。
- 质量门全过：fmt + clippy -D warnings + 139 core + 25 e2e；design-checker 无 MISMATCH。
- **待 PR #7 合并 → M1 graduate**。

**M1-INTEG 接线时需处理的 deferred TODO**（见 PR #7 评论）：
1. profile 自动定位 analysis-tools.json（需 `CLAUDE_PLUGIN_ROOT` env 约定 + SKILL 接线；当前靠 `--adapter-tools` 显式传参）
2. 完整子进程超时（当前仅 stdin(null)）
3. ToolStatus 枚举化 / LocReport 派生 totals（type-design，M2 质量项）

**设计文档歧义（需团队定夺，当前实现遵循转换矩阵）**：
- `done + --force` 重做：设计行 379（暗示可重做）vs 行 209 矩阵（done 硬终态）矛盾。
- blocked 进入：设计行 206「可从任何状态进入」vs 矩阵（仅 blockable 活跃态可进）。

### Phase 1 第二轮 code-review 修复（commit `098f164`）

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
| 2026-06-14 | 源码图校验 harness（文件级硬门 + 符号级软门 + barrel/interface 修复 + 双审查）PR#5 | 已合并 |
| 2026-06-14 | Phase 2 集成验证（13 命令路由 + E2E + 契约修复 + codex/code-review 双审查 + merge master）PR#3 | 待验收 |
