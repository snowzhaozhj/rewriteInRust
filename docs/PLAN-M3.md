# M3 详细实施计划：多语言支持（Python 优先）

> 权威方向：[PLAN.md §11](PLAN.md) + [08-roadmap §M3](design/08-roadmap-and-reference.md)
> 语言扩展架构：[06-plugin-structure §11.2](design/06-plugin-structure.md)
> M2 遗留移交：[STATUS.md](STATUS.md)

## 目标

支持 Python → Rust 迁移。在 2 个真实 Python 项目中完成至少 1 个模块迁移。

## 起点基线

- 407 测试 / 91.96% 覆盖率 / clippy -D / deny / fmt / shellcheck 全绿
- `LanguageAdapter` trait 6 方法已抽象，`SourceLang` 枚举已含 Python
- TS 适配器 1634 行（core 最大单文件），已在 rxjs 40 文件项目验证

## 核心技术方向

**架构分层**（参考 Sourcetrail ParserClient / Snyk DepGraph 模式）：
- **adapter 层**：语言专有——AST 解析、import 到文件路径的解析、tier 分档
- **图引擎层**（build.rs）：语言无关——文件遍历、节点去重、边归一化、re-export 链跟踪、SCC、指纹、增量更新

**关键改动**：`resolve_import()` 从 build.rs 全局函数搬到 `LanguageAdapter` trait 方法。build.rs 其余跨文件边构建逻辑操作的是 FileAnalysis 的抽象字段（`is_constructor`、`constructor_bindings`、`reexport`、`SymbolKind`），Python adapter 正确填充这些字段即可复用。同时修复 `add_cross_file_edges` 函数调用分支的 alias 漏边 bug（TS 也有此 bug）。

**FFI 降级桥接取消**：模块级跨运行时桥接造成状态不同步、调试复杂、部署复杂。`degrade_skip` 为唯一降级路径，降级报告输出原因和推荐 Rust 替代 crate。`scaffold/ffi.rs` 归档。

## Sprint 分解

```
Sprint A (多语言泛化 + 遗留清理) → Sprint B (Python Adapter Core)
  → Sprint C (Plugin Python 适配) → Sprint D (端到端验收)
```

预计 8-10 周。A→B 串行（B 依赖 A 的泛化），C 依赖 B，D 依赖全部。

---

## Sprint A：多语言泛化 + 遗留清理（~2 周）

**目标**：消除 TS 硬编码，resolve_import 下沉到 adapter，关闭 M2 遗留。

| 任务 ID | 内容 | 预估 | 依赖 |
|---------|------|------|------|
| M3-LANG-01 | **adapter 工厂**：新增 `lang/registry.rs`，`create_adapter(lang: SourceLang) -> Box<dyn LanguageAdapter>`；`detect.rs` 改用工厂 | 0.5d | — |
| M3-LANG-02 | **resolve_import 下沉到 adapter**：trait 新增 `fn resolve_import(&self, specifier: &str, current_file: &str, exists: &dyn Fn(&str) -> bool) -> Option<String>`；build.rs 的 `resolve_import()` 逻辑搬入 `TypeScriptAdapter`；build.rs 改为调 `adapter.resolve_import()`；`resolve_extensions()` 和 `import_specifier_extensions()` 退役（由各 adapter 内部使用） | 2d | LANG-01 |
| M3-LANG-03 | **build_graph 泛化**：`build_graph_ts()` 等 4 个便捷函数改为 `build_graph(adapter)` + 语言参数路由 | 1d | LANG-01 |
| M3-LANG-04 | **alias 漏边修复**：`add_cross_file_edges` 函数调用分支（line ~997）补 `alias_to_original` 查找（TS/Python 共有 bug） | 0.5d | — |
| M3-LANG-05 | **FileAnalysis 泛化**：`constructor_bindings` 改名为 `instance_type_bindings`（语义不变：变量名→类型名，TS `new Foo()` 和 Python `Foo()` 都填充）；删除 `lang/mod.rs` 的 TODO(M3) 注释 | 0.5d | — |
| M3-LANG-06 | **配置泛化**：`ProjectConfig` 默认 `source_language` 改为 `None`（需显式设置）；exclude 列表按语言从配置注入（TS: `node_modules`，Python: `__pycache__`/`.venv`） | 0.5d | LANG-01 |
| M3-LANG-07 | **stats 泛化**：`collect_ts_files()` → `collect_source_files(lang)`；`ts_max_nesting()` 按 `SourceLang` 分发创建 parser + 控制流节点集 | 1d | LANG-01 |
| M3-FFI-CLOSE | **FFI 关闭 + 权威文档同步**：`scaffold/ffi.rs` 标记 `#[deprecated]`；删除 TODO(M3-FFI)；新增 MDR 记录决策；同步更新 PLAN.md §11（删 PyO3 binding）、08-roadmap §M3（删 PyO3 验收指标、Mypy→tree-sitter）、02 架构图（标注 PyO3/Mypy 取消）、04 工具链（Python 段更新） | 1d | — |
| M3-DEV-01 | **DEVIATION 4 项 MDR 补录**：fingerprint 提取范围、事务类型 DEFERRED、WAL pragma、exported_names 额外维度 | 0.5d | — |

**验收标准**：
- [ ] `build_graph()` 接收任意 `LanguageAdapter`，TypeScript 走原路径不回归
- [ ] `resolve_import` 在 adapter 里，build.rs 不含语言特有解析逻辑
- [ ] alias 漏边 bug 已修，补测试覆盖
- [ ] `just ci` 全过（407+ 测试），无新 clippy 警告
- [ ] M2 遗留全部关闭（FFI MDR + DEVIATION MDR）

---

## Sprint B：Python Adapter Core（~3 周）

**目标**：实现 `PythonAdapter`，可解析 Python 源码、构建依赖图、检测复杂度分档。

| 任务 ID | 内容 | 预估 | 依赖 |
|---------|------|------|------|
| M3-PY-01 | **tree-sitter-python 集成**：workspace Cargo.toml 加 `tree-sitter-python`；`PythonAdapter` 结构体 + `language()`/`can_handle(.py)`/`detect_tier()` 基础方法 | 0.5d | LANG-01 |
| M3-PY-02 | **Python import 解析**：`analyze_file()` 提取 `import x` / `from x import y` / `from . import z`（相对导入）/ `__init__.py` 包检测；识别 `if TYPE_CHECKING:` 块并标记 `ImportKind::StaticType`；输出 `FileAnalysis.imports` | 2d | PY-01 |
| M3-PY-03 | **Python resolve_import 实现**：`PythonAdapter::resolve_import()` 处理相对导入（dot count → 向上 N 级）+ 绝对导入（点分包名 → 逐级查 `__init__.py` → 查 `module.py`）+ 外部包返回 None | 2d | PY-02 |
| M3-PY-04 | **Python 符号提取**：function/class/global variable 节点 + Contains/Extends 边 + `__all__` 导出名（列表字面量）+ `@property`/`@staticmethod`/`@classmethod` decorator 标注 | 2d | PY-01 |
| M3-PY-05 | **Python 调用分析 + instance_type_bindings**：顶层函数调用 + `self.method()` 粗粒度名称匹配 + `obj = Foo()` 填充 `instance_type_bindings`；`Foo()` 设 `is_constructor: true` | 1d | PY-04 |
| M3-PY-06 | **Python signature 提取**：函数签名含 type annotation + decorator；class 签名含 bases + 方法列表骨架 | 1d | PY-04 |
| M3-PY-07 | **Python fixture 创建**：3 个 fixture（py-linear-deps / py-diamond-deps / py-circular-deps）+ `ground-truth.json` + 含 `__init__.py` 的包结构 fixture | 1.5d | PY-02 |
| M3-PY-08 | **Python graph 集成测试**：端到端验证 fixture 偏序约束满足；signature round-trip；`__init__.py` re-export 透传正确 | 1d | PY-03~07 |
| M3-PY-09 | **adapter 注册 + grammar 契约测试**：`registry.rs` 注册 Python adapter；新增 `ast_contract_python.rs` 测试 tree-sitter-python 节点类型稳定性 | 0.5d | PY-01 |

### 执行策略（PR 拆解 + 并行）

```
Step 1: PR-B1 (Foundation)     → PY-01 + PY-09         [~1d]
Step 2: PR-B2 (Core Analysis)  → PY-02~06              [~5d, 内部双线并行]
         ├─ Track A: PY-02 → PY-03   (import 解析 → resolve)
         └─ Track B: PY-04 → PY-05 + PY-06  (符号 → 调用 + 签名)
Step 3: PR-B3 (Validation)     → PY-07 + PY-08         [~2.5d]
```

- PR-B1 是所有后续前置，先做先合
- PR-B2 两条 Track 只共享 PY-01 的 adapter 结构，互不依赖
- PR-B3 作为验收层独立 PR，发现问题不回溯改前面的 PR
- 日历时间：~7-8d（PR-B2 内部并行压缩）

**验收标准**：
- [ ] 3 个 Python fixture 的 `ground-truth.json` 偏序约束全部满足
- [ ] `cargo run -- graph build --root fixtures/py-linear-deps` 输出正确依赖图
- [ ] `__init__.py` 包结构正确解析，re-export 透传工作
- [ ] `if TYPE_CHECKING` 块识别为 `StaticType` import
- [ ] `just ci` 全过，Python fixture 测试纳入 nextest

---

## Sprint C：Plugin Python 适配（~2 周）

**目标**：Plugin 层支持 Python 项目迁移分析和翻译。

| 任务 ID | 内容 | 预估 | 依赖 |
|---------|------|------|------|
| M3-PLG-01 | **`adapters/python/` 目录**：~~`adapter.json` + `detect.sh`~~ + `analysis-tools.json` + `porting-template.md`（**修正见 MDR-009**：shell 脚本模式从未落地，语言检测在 analyze.md Step2、提取由 CLI tree-sitter 完成；adapter 目录契约 = analysis-tools.json + porting-template.md，与 TS 对齐，不建 adapter.json/detect.sh） | 1d | — |
| M3-PLG-02 | **Python porting-template.md**：Python → Rust 类型映射规则（`str→String`, `int→i64`, `float→f64`, `list→Vec`, `dict→HashMap`, `Optional→Option`, `dataclass→struct`, `abc.ABC→trait`）+ 惯用法差异（GIL/动态类型/duck typing/decorator/metaclass） | 2d | — |
| M3-PLG-03 | **translator.md 多语言分支**：类型映射表按 `source_language` 条件化；Phase A Python 特有规则（`self` 参数转换 / `__init__.py` 处理 / 无 type-only import 区分） | 1d | PLG-02 |
| M3-PLG-04 | **analyzer.md + verifier.md 适配**：Python 项目分析指令 + 验证规则 | 1d | PY-09 |
| M3-PLG-05 | **degrade_skip 降级报告增强**：输出降级原因 + 推荐 Rust 替代 crate；skip 后标记上游模块为 `blocked_by_skip`，翻译时注入 context | 1d | — |
| M3-PLG-06 | **Plugin 端到端验证**：`/migrate analyze` + `run` 对 Python fixture live 跑通 | 1d | PLG-01~05 |

**验收标准**：
- [x] ~~`adapters/python/adapter.json` 符合 06 §11.2 JSON Schema 契约~~（**作废，MDR-009**）→ `adapters/python/` 含 `analysis-tools.json`（格式匹配 `AnalysisTool`）+ `porting-template.md`（frontmatter 与 TS 对齐）
- [ ] `/migrate analyze` 产出 `migration-state.json`（Python 项目正确填充模块状态）
- [ ] `/migrate run` 对 Python fixture headless 跑通，至少 1 模块状态推进到 `translating`
- [ ] translator 可根据 `source_language` 切换类型映射表
- [ ] 降级报告输出原因和推荐替代；`blocked_by_skip` 标记传播到上游模块

---

## Sprint D：端到端验收（~2 周）

**目标**：真实 Python 项目迁移验证 + 质量门禁全过。

| 任务 ID | 内容 | 预估 | 依赖 |
|---------|------|------|------|
| M3-VAL-01 | **真实项目选型**：选 2 个 <3K 行开源 Python 项目（纯计算/数据处理类，避免 I/O 密集/框架绑定）；要求有 pytest 测试覆盖 | 0.5d | — |
| M3-VAL-02 | **项目 A 端到端迁移**：analyze → graph build → state populate → sprint_loop → 至少 1 模块 done（cargo check + test + clippy 过） | 2d | PLG-06, VAL-01 |
| M3-VAL-03 | **项目 B 端到端迁移**：同上，第二个项目至少 1 模块 done | 2d | VAL-02 |
| M3-VAL-04 | **差异测试框架**：Python 原始行为录制（pytest 输出 JSON fixture）→ Rust 迁移后对比（cargo test 消费同一 fixture）；验证至少 1 个迁移模块的输入/输出行为一致 | 1.5d | VAL-02 |
| M3-VAL-05 | **性能回归验证**：TS 路径性能不退化（±10%）；Python 路径建立基准 | 0.5d | VAL-02 |
| M3-VAL-06 | **graduate 验证**：`/migrate graduate` 对 Python 项目正确识别"已完成"vs"未完成"状态（复用 M2 已有 graduate 逻辑，验证 Python 路径兼容） | 0.5d | VAL-02 |
| M3-VAL-07 | **设计文档同步**：08-roadmap M3 验收标记 + 04-toolchain Python 工具链 + 02-architecture Python 适配器 + PLAN.md §11 PyO3/Mypy 方向更新 | 1d | VAL-03 |
| M3-VAL-08 | **全量回归 + 覆盖率**：`just ci` 全过；覆盖率 ≥70% | 0.5d | ALL |
**验收标准**：
- [ ] 2 个真实 Python 项目中 ≥1 模块迁移到 done
- [ ] 迁移产物 `cargo check` + `cargo test` + `clippy -D` 全过
- [ ] 差异测试：Python 原始行为与 Rust 迁移后行为对齐（JSON fixture 对比通过）
- [ ] `/migrate graduate` 对 Python 项目正确识别完成/未完成状态
- [ ] TS 路径性能不退化（±10%）
- [ ] TS 路径全量回归无退化
- [ ] 设计文档 M3 交付物全部同步（含 PLAN.md §11 方向更新）

---

## 任务总览

| Sprint | 任务数 | 预估工时 | 关键交付 |
|--------|--------|---------|---------|
| A 多语言泛化 + 遗留清理 | 9 | 7.5d | resolve_import 下沉 + adapter 工厂 + M2 遗留关闭 + 权威文档同步 |
| B Python Adapter Core | 9 | 11.5d | PythonAdapter 全方法 + 3 fixture + grammar 契约 |
| C Plugin Python 适配 | 6 | 7d | adapters/python/ + 提示词多语言 + 降级报告增强 |
| D 端到端验收 | 8 | 8.5d | 2 个真实项目 + 差异测试 + graduate 验证 |
| **合计** | **32** | **~34.5d** | |

## M2 遗留关闭清单（M3 全部处理，不再拖）

| 项目 | Sprint | 处理方式 |
|------|--------|---------|
| FFI 方向不匹配 (TODO(M3-FFI)) | A | 归档 ffi.rs + MDR 记录 degrade_skip 决策 |
| DEVIATION 4 项待 MDR | A | 补录到 docs/decisions/ |
| constructor_bindings 泛化 (TODO(M3)) | A | 改名 instance_type_bindings |
| F2-FFI 验收缺口 | A | MDR 标记为"设计变更取消"而非"推迟" |

## 明确不做（M4 scope，不是遗留）

| 项目 | 理由 |
|------|------|
| C/Go LanguageAdapter | M4 核心目标，非 M3 范围 |
| namespace packages 支持 | Python 3.3+ 隐式包，M3 只支持标准包 |
| mypy 集成 | M3 纯 tree-sitter，mypy 是精度优化 |
| 混合语言 monorepo | M3 单项目单语言 |
| wildcard re-export 覆盖语义 | 精度微调，不影响迁移正确性 |
| extends 候选类型参数化 | 不出错，仅多余搜索 |

## 风险与缓解

| 风险 | 影响 | 缓解 |
|------|------|------|
| Python 动态类型导致依赖图不精确 | graph 漏边/多边 | tree-sitter 仅做语法级提取；tier 分档标注动态类型密度 |
| Python 项目结构多样（flat/src-layout） | resolve_import 路径解析复杂 | 先支持标准 src-layout + flat layout |
| tree-sitter-python grammar 版本兼容性 | AST 节点类型名变更 | 新增 grammar 契约测试（复用 M2 模式） |
| degrade_skip 传递性阻塞 | 关键模块 skip 导致上游翻译失败 | skip 后标记上游 blocked_by_skip + 翻译 context 注入推荐替代 |

## 设计决策

| 编号 | 问题 | 决策 | 理由 |
|------|------|------|------|
| D-M3-01 | Python import 解析精度 vs 速度 | **(a) 纯 tree-sitter** | 与 TS adapter 一致；动态 import 由 tier 标注 |
| D-M3-02 | FFI 桥接 | **取消——degrade_skip 为唯一降级路径** | 模块级跨运行时桥接造成状态不同步、调试/部署复杂；翻不了的模块用 Rust crate 替代或标记 out-of-scope |
| D-M3-03 | Python type annotation 提取深度 | **(a) 仅 AST 注解（PEP 484/526）** | 不走统一 IR，LLM 负责映射；mypy 是 M4 优化 |
| D-M3-04 | 图引擎与 adapter 分界 | **resolve_import 下沉到 adapter，build.rs 其余逻辑保持语言无关** | 业界共识：import 解析是语言内部事务（Dependabot/Snyk/Sourcetrail 验证）；图引擎价值在一致性保障而非解析逻辑 |

## 08-roadmap M3 交付物对账表

> 08-roadmap-and-reference.md §M3 列出的交付物与 PLAN-M3 的对应关系。确保无遗漏。

| 08-roadmap M3 交付物 | PLAN-M3 状态 | 说明 |
|---------------------|-------------|------|
| Python LanguageAdapter（Mypy 类型提取 + PyO3 桥接） | **DEVIATION** → M3-PY-01~09 | 工具链变更：tree-sitter 替代 Mypy（D-M3-01）；PyO3 取消（D-M3-02）。Sprint A 同步更新 08-roadmap |
| Python 专用迁移规则模板（porting-template.md） | ✅ M3-PLG-02 | 覆盖 |
| 统一差异测试框架 | ✅ M3-VAL-04 | 覆盖（Python 行为录制 + JSON fixture 对比） |
| `/migrate graduate` 毕业评估 | ✅ M3-VAL-06 | M2 已有基础逻辑，M3 验证 Python 路径兼容 |
| 性能基准对比自动化（criterion 集成） | **M2 已完成** | M2 Sprint E 已有 `graph build --profile` 性能基准（ADV-04）；M3 做 ±10% 回归验证（VAL-05），不重复 criterion 集成 |
| 并发测试（loom/shuttle 集成） | **M2 已完成** | M2 Sprint E 已有 proptest 7 属性 × 1000 + cargo-fuzz（VER-01/02）；loom/shuttle 为 08 早期规划，M2 用 proptest 替代 |
| 依赖图可视化（Mermaid 自动生成） | **M2 已完成** | M2 Sprint E 已实现 `graph export --format mermaid`（CLI-03） |

## PLAN-M2 推迟到 M3 的候选项对账

> PLAN-M2.md §11 推迟项与 PLAN-M3 的对应关系。

| PLAN-M2 推迟项 | PLAN-M3 状态 | 说明 |
|---------------|-------------|------|
| 图 schema 扩展（TypeAlias/Variable/Community） | **部分覆盖** | M3-PY-04 新增 global variable 节点；TypeAlias/Community 推到 M4 |
| FTS5 全文搜索 | **不做** | M4 scope，非 Python 支持的前置条件 |
| 规则治理工具化 | **不做** | M4 scope |
| 降级决策学习 | **不做** | M4 scope |
| 类型复杂度前置降级信号 | **不做** | M4 scope |
| 跨文件方法调用档 2（receiver 类型环境） | **不做** | M4 scope，M3 用粗粒度名称匹配 |
| 适配器规则版本陈旧检测 | **不做** | M4 scope |
| index.json 自动生成 | **不做** | M4 scope |
