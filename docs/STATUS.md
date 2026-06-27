# 项目状态快照

> 每次会话结束前更新。新会话读此文件 → 找到 PLAN.md 对应任务 → 继续执行。

## 当前位置

- **Milestone**: M1 ✅ → M2 ✅ → **M3 多语言支持（Python 优先）**
- **阶段**: M3 Sprint B ✅（PR-B1/B2/B3 全合并）→ **Sprint C（Plugin Python 适配）进行中**
- **测试基线**: 471 测试 / clippy -D / deny / fmt / shellcheck 全绿
- **CI 覆盖率**: 待更新
- **最新 PR**: [#37](https://github.com/snowzhaozhj/rewriteInRust/pull/37)（PR-B3 Python 验收层，已合并）

### M2 遗留（Sprint A 已全部关闭）

| 项目 | 处理 |
|------|------|
| FFI 方向不匹配 | ✅ MDR-007：取消 FFI，degrade_skip 唯一路径 |
| TS 特有概念泛化 | ✅ LANG-05：constructor_bindings → instance_type_bindings |
| DEVIATION 4 项待 MDR | ✅ MDR-008：4 项偏差补录 |
| F2-FFI 验收缺口 | ✅ MDR-007 标记为"设计变更取消" |

### Sprint A 完成清单

| 任务 | 状态 | 说明 |
|------|------|------|
| LANG-01 adapter 工厂 | ✅ | `lang/registry.rs` + `create_adapter()` |
| LANG-02 resolve_import 下沉 | ✅ | trait 新增方法，build.rs 通过 adapter 调用 |
| LANG-03 build_graph 泛化 | ✅ | 4 个便捷函数改用工厂 + `build_graph_for_lang` |
| LANG-04 alias 漏边修复 | ✅ | 函数调用分支补 alias_to_original 查找 |
| LANG-05 instance_type_bindings | ✅ | constructor_bindings 改名 + 删 TODO(M3) |
| LANG-06 配置泛化 | ✅ | source_language: Option + default_excludes_for_lang |
| LANG-07 stats 泛化 | ✅ | collect_source_files(lang) + source_max_nesting |
| FFI-CLOSE | ✅ | ffi.rs deprecated + MDR-007 |
| DEV-01 DEVIATION MDR | ✅ | MDR-008 补录 4 项偏差 |

### 当前工作：Sprint B（Python Adapter Core）

**目标**：实现 `PythonAdapter`，可解析 Python 源码、构建依赖图、检测复杂度分档。

**PR 拆解（3 步走）**：

| PR | 任务 | 预估 | 并行策略 |
|----|------|------|---------|
| **PR-B1 Foundation** | PY-01 + PY-09 | ~1d | 串行，所有后续前置 |
| **PR-B2 Core Analysis** | PY-02 + PY-03 + PY-04 + PY-05 + PY-06 | ~5d | 内部双线并行：Track A (import→resolve) ∥ Track B (symbol→call+signature) |
| **PR-B3 Validation** | PY-07 + PY-08 | ~2.5d | 串行，验收层 |

**依赖图**：
```
PY-01 ─┬→ PY-02 → PY-03 ─────┐
        ├→ PY-04 → PY-05/06 ──┼→ PY-08
        └→ PY-09               │
                    PY-07 ─────┘
```

**进度**：
- [x] PR-B1：PY-01 adapter 骨架 + PY-09 注册/契约
- [x] PR-B2：PY-02 import 解析 + PY-03 resolve + PY-04 符号 + PY-05 调用 + PY-06 签名
- [x] PR-B3：PY-07 fixture（4 个）+ PY-08 集成测试（23 测试）+ CLI graph build 语言检测泛化

**PR-B3 交付**：
- 4 个 Python fixture：`py-linear-deps`（线性+`__all__`+async+构造调用）/ `py-diamond-deps`（菱形+继承 extends）/ `py-circular-deps`（环检测+shared 不在环）/ `py-pkg-deps`（`__init__.py` 包+re-export 透传偏序+`TYPE_CHECKING` StaticType）
- `python_ground_truth.rs`：24 测试，节点/边**双向严格校验**（含 sub_kind，防多余/缺失/标注错误漏检）+ 拓扑偏序 + Python 特有断言（extends 无 Implements、signature round-trip、StaticType import、构造 sub_kind、循环 SCC 精确同环）
- CLI `cmd_graph_build`：源语言优先取 config（避免热路径重复全树扫描），未配置才 `detect_language` 探测，失败显式告警回退 TS；非 TS 强制全量并提示降级；新增 `build_graph_full(root, lang, profile)`；TS 增量路径不回归
- `cli_e2e.rs` 新增 Python graph build 端到端用例（探测→降级→status=warning）
- `cargo run -- graph build --root fixtures/py-linear-deps` 输出 node=12/edge=15 ✓
- **审查**：4 视角全跑（主审/设计契约/专项/异构交叉）；6 项测试保真+CLI 健壮性问题已修，无遗留 important

### 当前工作：Sprint C（Plugin Python 适配）

**目标**：Plugin 层支持 Python 项目迁移分析和翻译（PLG-01~06）。

**PR 拆解（修正 PLAN-M3 偏离后）**：

| PR | 任务 | 说明 |
|----|------|------|
| **PR-C1** | PLG-01修正 + PLG-02 | Python adapter 资产：`analysis-tools.json` + `porting-template.md` |
| **PR-C2** | PLG-03 + PLG-04 | translator.md / analyzer.md / verifier.md 多语言分支 |
| **PR-C3** | PLG-05 + PLG-06 | degrade_skip 降级报告增强 + Plugin Python 端到端验证 |

> **PLG-01 偏离修正**：PLAN-M3 字面要求建 `adapter.json` + `detect.sh`，但实际架构中
> TS adapter 目录仅 `analysis-tools.json` + `porting-template.md`——语言检测在 `analyze.md`
> Step 2（读特征文件）、依赖分析由 CLI `graph build`（tree-sitter）完成，设计文档 06 §11.2
> 的 shell 脚本模式从未落地。Python adapter 对齐 TS 实际结构，不建 adapter.json/detect.sh。

**进度**：
- [x] PR-C1：Python adapter 资产（[#38](https://github.com/snowzhaozhj/rewriteInRust/pull/38)，审查必修全落实，待合并）
  - 审查：迁移规则正确性 + 设计契约 2 视角全跑；2+1 项 important 已修（regex 反向引用/环视、dict 插入顺序、PLG-01 偏离落 MDR-009）+ 多项 nit
  - MDR-009：适配器 shell 脚本模式取消，adapter 目录契约 = analysis-tools.json + porting-template.md
- [ ] PR-C2：translator.md/analyzer.md/verifier.md 多语言分支（PLG-03 + PLG-04）
- [ ] PR-C3：degrade_skip 降级报告增强 + 端到端验证（PLG-05 + PLG-06）

> **遗留待办**（M3-VAL-07 设计文档同步时处理）：设计文档 06 §11.2 正文仍描述废弃的 shell adapter 契约，需按 MDR-009 改写为两文件契约。

### M3 多语言扩展点（调研结论，2026-06-24）

**已就绪**：
- `LanguageAdapter` trait 6 方法已抽象（`language/can_handle/resolve_extensions/import_specifier_extensions/analyze_file/detect_tier`）
- `SourceLang` 枚举已预定义 TypeScript/Python/C/Go
- `profile/detect.rs` tokei 映射已含 Python/C
- Plugin 层 `SKILL.md` / `analyze.md` 已考虑多语言分发
- 设计文档 06 §11 有完整的语言扩展架构设计

**需泛化（TS 硬编码）**：
- `detect.rs`: 直接实例化 `TypeScriptAdapter`（需 adapter 工厂）
- `graph/build.rs`: `build_graph_ts()` 等 4 个便捷函数硬编码 TS adapter
- `stats/compare.rs`: `collect_ts_files()` / `ts_max_nesting()` / 独立创建 TS parser（绕过 adapter 抽象）
- `types/config.rs`: 默认 `source_language: TypeScript` / exclude 含 `node_modules`
- Plugin `translator.md`: 类型映射表以 TS 为基线
- Plugin `adapters/`: 仅有 `typescript/` 目录

## 历史归档

- **M1 详细记录**：[STATUS-M1-archive.md](STATUS-M1-archive.md)
- **M2 详细记录**：[STATUS-M2-archive.md](STATUS-M2-archive.md)（Sprint D/E/F 任务清单、PR 记录、审查修复、已知问题处理状态）
- **M2 计划**：[PLAN-M2.md](PLAN-M2.md)（55 项任务 + 5 项验收，Sprint A→F）
- **M2 Sprint F 验收**：[sprint-f-acceptance.md](sprint-f-acceptance.md)
