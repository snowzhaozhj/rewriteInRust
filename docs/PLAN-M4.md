# M4 详细实施计划：巩固现有能力 + Go 扩语言

> 权威方向：[PLAN.md §11](PLAN.md) + [08-roadmap §M4](design/08-roadmap-and-reference.md)
> 语言扩展架构：[06-plugin-structure §11.2](design/06-plugin-structure.md)
> M3 遗留移交：[STATUS.md](STATUS.md)

> **状态：草案 v0.3（R2 修订完成，待用户拍板配比后执行）**。由两路调研（代码就绪度 + 技术可行性交叉验证）→ 分析 → 拆解 → R1 审查（设计契约 / 主线决策 / 可执行性三视角）→ 重定位优化 → R2 用户审查修正产出。R1 关键修正见文末「审查修订记录 R1」，R2 修正见「审查修订记录 R2」。

## 主线决策：为什么是「巩固 + Go」双主线

M4 在 08-roadmap 中定位「完善（持续）」。**R1 主线审查指出一个根本问题**：单纯新增 Go adapter 本质是 M3 式扩语言（巩固/硬化叙事不成立），且「11d 造适配器、4d 迁真实代码、1 模块能编译就算赢」会让验证门槛过低、迁移质量不可证伪。故 M4 修正为**两条可独立交付的线**：

- **巩固线（真正的「完善」）**：迁移质量度量框架 + 既有 TS/Python 真实项目基线 + 循环健壮性（错误恢复/断点续跑）+ MDR-013 债务清理。这条线让「迁移得好不好」可度量、可证伪——是「完善」里程碑的应有之义，也为任何语言的验收提供统一标尺。
- **Go 扩语言线（roadmap 承诺）**：兑现 `LanguageAdapter` trait 多语言可扩展——M3 仅验证 Python 一门，**Go 的包系统（目录=package）对「单文件 import 模型」构成真实架构压力测试**，有独立验证价值；架构契合（无 FFI）、tree-sitter-go 成熟、有 Discord（Go→Rust，消除 GC 尾延迟尖峰）一手案例。Go 验收**复用巩固线的质量度量框架设真实门槛**（多模块 + 度量），而非「1 模块编译」。

**被否决/推迟的方向**：

- **并行编排程序化调度器——推迟**：基础设施（`parallel.rs` / `topo.parallel_groups` / `run.md` 并行段 / `workflow.md`）M2-SCALE 已落地。程序化调度器是 M2 已推迟的大工程，当前 SKILL.md 编排满足 2-3 门语言调度需求，程序化状态机投入大、ROI 不足。M4 做增量收口：worktree 生命周期代码化（Sprint F ORCH-01）+ **循环健壮性大幅扩充**（ROB-01a/b/c 共 3d，覆盖 M3 实际遇到的 watchdog stall、额度耗尽、单模块失败扩散等可靠性问题）。
- **C/C++ adapter——推迟**：真正理由是**无类型 IR 架构下，C 的弱类型/指针/手动内存语义使「安全 Rust 翻译 + 等价审查」远难于 Go，ROI 在 35d 预算内做不下 Go+C**（不是「tree-sitter 宏一票否决」——宏仅在任意 token 位置才破，大量常规 C 可解析，用宏否决整门语言与本项目对 Go 并发走 degrade_skip 的处理双标）。roadmap 原 bindgen/cbindgen FFI 方案随 C 整体推迟。未来若做，走 LLMIGRATE 式 tree-sitter 拓扑路线（复用本架构），不引入 c2rust 平行工具链。
- **Kani——推迟（非砍）**：Kani（BMC）验证的是翻译后 Rust 代码的正确性（无 panic/溢出/越界），与 proptest 差异测试（源↔Rust 等价性）**互补不替代**——前者穷举有界输入证明安全性，后者随机采样验证等价性。M4 预算有限，Kani 的 ROI 在当前阶段（2-3 门语言、小型项目验证）不如质量度量框架高；推迟到 C 路线（unsafe 高产出）或更大真实项目验证阶段，届时 Kani 的正确性证明价值充分显现。
- **Community 节点——Tier 1 质量诊断信号纳入 Sprint B**：`NodeType::Community` 枚举已预留。Rust 生态已有可用 Leiden 实现（`graphrs` 0.11.16，4 年历史 33K 下载；`leiden-rs` 0.8.1 含 petgraph 适配器），原「无成熟纯 Rust Leiden crate」判断已过时。M4 做 **Tier 1 质量诊断**（~2d，Sprint B QUAL-04）：跑 Leiden → 与目录结构比对 → 输出结构偏离度分数 → 接入质量度量框架，作为源项目可迁移性评估维度。不做 Tier 2（辅助拆解）/ Tier 3（完整 Community 节点落地），视真实项目暴露需求再升级。
- **Strangler Fig——降文档**：本项目是离线翻译工作台，翻译产物为完整新建的 Rust 项目，旧代码和新代码不需要运行时共存。Strangler Fig 的核心价值（过渡期两套实现共存 + 逐步切换流量）在此场景下需求不强。注意：拓扑迁移解决的是翻译**顺序**问题，Strangler Fig 解决的是**过渡期共存**问题，两者正交——但在离线工作台场景下，scaffold 骨架 + 拓扑排序已覆盖编译期可用性，不做专门工具化。降文档：在 porting-template 中记录 Strangler Fig 思维（增量替换的验证策略）。

## 目标

1. **巩固**：建立迁移质量度量框架，在既有 TS + Python 各 1 个真实项目上建立质量基线；补齐循环错误恢复；清理 MDR-013 三项遗留债。
2. **扩 Go**：支持 Go → Rust 迁移，在 **1-2 个真实 Go 项目**完成有意义的一片迁移（多模块、含降级决策），并以质量度量框架验收（非单模块编译）。

## 起点基线

- 测试基线 552 测试 / clippy -D / deny / fmt / shellcheck + plugin validate 全绿
- `LanguageAdapter` trait **9 方法**已抽象（`lang/mod.rs:210`），`create_adapter` 工厂（`lang/registry.rs:10`），Python adapter（`lang/python.rs`，~2200 行）为可抄板模板
- `SourceLang::Go` 已存在（`types/common.rs:135`）；tokei `Go → SourceLang::Go` 已映射（`profile/detect.rs:45`）；`EXCLUDED_DIRS` 已含 `vendor`（`common.rs:284`）
- **MDR-011 目录优先两阶段凝聚合并**已落地（`graph/decompose.rs`，带 footprint 预算 + 凸性约束）
- 质量评分 `final_score` 概念在设计 [03 §7.5](design/03-execution-model.md) 已定义（per-module，sprint 聚合推迟项），Sprint B 复用并落地
- 并行编排基础设施已落地（`parallel.rs` / `topo.rs:compute_parallel_groups`）

## 核心技术方向

**复用 M3 trait 架构接 Go**：`registry.rs` 加一行 match 臂 + 新建 `lang/go.rs`（抄 `python.rs` 板：struct + Parser + 9 方法）+ workspace 引 `tree-sitter-go`（**锁 0.21.x，与现有 tree-sitter 0.21 代兼容**）。图引擎语言无关，Go adapter 填充 `FileAnalysis` 抽象字段即复用。

**关键设计摩擦——Go 包系统需扩 trait（R1 critical 修正）**：Go `import "mod/pkg"` 解析到**目录（多文件 package）**，而 `resolve_import` 当前签名（`mod.rs:246`）只提供 `exists(path)->bool` 判定式、**不提供目录枚举能力**——Python 能返回代表文件是因探测**约定文件名** `__init__.py`，Go 包是任意命名 `.go`，adapter 无法用 `exists` 探出代表文件。**故扩 trait 暴露目录列举（或返回包级聚合）是 baseline 而非 fallback**（设计决策 D-M4-01 已据此重写）。下游靠 MDR-011 目录凝聚把同包文件合并，spike 仅验证凝聚效果（给死断言）。

**并发/隐式实现/unsafe 落 porting-template + 统一危险归类**：goroutine/channel/select → `DangerCategory::Concurrency`（→ RULE-6）；cgo → `Ffi`（→ RULE-12，枚举 doc 已含 cgo）；reflect → `DynamicReflection`；**Go `unsafe.Pointer` 钉死归 `Ffi`（→ RULE-12），detect_tier/gaps/degrade 三处口径统一**（R1 修正）。Go interface 隐式实现 tree-sitter 静态推不出，`Implements` 边**标注缺失而非强连**（D-M4-02）。

## Sprint 分解

```
巩固线：  Sprint A (债务收口) ─ Sprint B (质量度量+既有基线) ─ Sprint F (健壮性+编排收口)
Go 线：                         Sprint C (Go Adapter Core) ─ Sprint D (Plugin Go) ─ Sprint E (Go 端到端验收)
```

两线可独立推进。Go 线 C 依赖 A 的 registry 接线；E 复用 B 的质量度量框架（软依赖：B 先于 E）。预计巩固线 ~17d、Go 线 ~31d，合计 ~48d 工时（M4 为「持续」里程碑，可分批交付；Go 线因 Track 并行日历压缩见 Sprint C）。R2 修正：巩固线从 ~13d 增至 ~17d（+2d Community 诊断 QUAL-04 + +2d ROB-01 扩充）。

---

## Sprint A：债务收口 + Go 接入前置（~4.5d）

**目标**：清 MDR-013 三项遗留债，完成 Go registry 接线（不含 Go 解析逻辑）。

| 任务 ID | 内容 | 工时 | 依赖 |
|---------|------|:---:|------|
| M4-DEBT-01 | **io_side_effect 专属 RULE**：`IoSideEffect`（`mod.rs:104`）有 `concern()` 无对应 RULE。**先裁定归属**：与既有 RULE-3（错误处理）/RULE-10（标准库）重叠度高 → 决策新开 `RULE-NN` 还是并入；若新开，**同步登记 05-documentation-system §6.2 规则目录 + 分配编号**（权威目录，06:937 指向此）。在 TS/Python `porting-template.md` 补 io 副作用迁移规则 | 0.5d | — |
| M4-DEBT-02 | **DangerCategory 上移 types 层（含 serde 双向）**：枚举从 `crate::lang`（`mod.rs:58`）搬到 `types/`，`lang` 侧改 `use types::DangerCategory`（保 `lang→types` 单向依赖，MDR-013 授权正解）；`ModuleState.danger`（`state.rs:288`）`Vec<String>` → `Vec<DangerCategory>`。**关键 serde 细节**：①给枚举补 `Deserialize` + `#[serde(rename_all="snake_case")]`（当前 `mod.rs:56` 仅 derive Serialize）；②未知字符串加 `#[serde(other)]` 兜底变体或显式声明硬失败可接受；③union 去重后按 `as_str()` 重排（旧字符串字典序 ↔ 新枚举声明序 `Ord` 不同，避免快照断言挂）。**受影响文件清单**（rg 核实，必须全改）：`cli/lib.rs` 23 处（含 `danger:Vec<String>`:1883、`union_danger` 闭包:1995、JSON 输出:2109）、`state.rs`、`machine.rs`、`coverage.rs`、`validate/mod.rs`、`cli_e2e.rs` 各构造点 | 2d | — |
| M4-DEBT-03 | **RULE-6/12/15 porting-template 展开**：concurrency/ffi/shared_mutable_global 三类危险信号补完整迁移细则；**concern() 文案语言中立化**（现为 TS 口径「JS number 为 f64」，上移后被 Go 复用会失真，标注或改写）；**同步 bump 各模板 frontmatter `rule_version`**（DEBT-01/03 改了模板覆盖规则，需与 Sprint F GOV-01 校验对齐） | 1.5d | DEBT-01 |
| M4-LANG-01 | **Go registry 接线**：workspace `Cargo.toml` 引 `tree-sitter-go = "0.21"`（版本兼容硬约束）；`registry.rs:14` 加 `SourceLang::Go => Ok(Box::new(GoAdapter::new()?))` 臂；`lang/go.rs` 骨架（`new`/`language`/`can_handle(.go)`，其余 `todo!()` 占位）。注：`unsupported_language_returns_error` 测试已用 `SourceLang::C`、Go 实现后仍有效，无需改 | 0.5d | — |

**验收标准**：
- [ ] MDR-013 三项全落地；io RULE 归属已裁定并（若新开）登记 05 §6.2
- [ ] `ModuleState.danger: Vec<DangerCategory>`；**旧 state 文件（`Vec<String>`）可正确反序列化**（新增双向 serde 测试 + 未知值策略测试）；23 处引用全改、`just test` 无回归
- [ ] `create_adapter(SourceLang::Go)` 返回骨架 adapter
- [ ] `just ci` 全过，TS/Python 路径无回归

---

## Sprint B：迁移质量度量框架 + 既有语言真实基线（~7d，巩固线旗舰）

**目标**：让「迁移得好不好」可度量、可证伪；在既有 TS/Python 真实项目上建立质量基线（既验证框架又补既有语言真实规模化验证缺口）。

| 任务 ID | 内容 | 工时 | 依赖 |
|---------|------|:---:|------|
| M4-QUAL-01 | **迁移质量度量框架（CLI）**：定义并落地 per-module 度量——① 源行为覆盖率（差异测试通过行为点 / 总录制点，依托设计 §7.6 差异测试）；② degrade_skip 占比（降级模块数 / 总模块，依托 §4.9 止损）；③ 人工修订率（**git-diff 口径**：done 前人工改动行数 / 自动产出行数。注：`attempts` 仅记 LLM 重试轮次、不含人工编辑，故需 git 基线；无 git 基线则**降级为可选指标**）；④ `final_score` 聚合（复用设计 03 §7.5 评分卡，落地 per-module 计算）。**①②③ 为本计划新增度量（非 §7.5 评分卡输入项），需登记设计 03 §7.5**（见 VAL-07）。输出**优先扩展现有 `stats` 子命令**（不破 13+5 命令清单）的 JSON | 2.5d | — |
| M4-QUAL-02 | **既有语言真实基线**：用 M3 已迁的 jmespath（Python）+ 1 个 TS 真实项目，跑质量度量框架，产出基线报告（degrade 率/覆盖率/修订率）；建立「质量回归」对比起点（后续 Go 验收与之横比） | 1.5d | QUAL-01 |
| M4-QUAL-03 | **质量度量接线 Plugin**：`/migrate review` 仪表板展示三项度量 + final_score；verifier 输出对接质量度量 schema | 1d | QUAL-01 |
| M4-QUAL-04 | **Community 结构偏离度诊断（Tier 1）**：集成 `graphrs` Leiden 算法（workspace 引依赖，版本 0.11.x），对 `SourceGraph` 跑社区检测 → 与目录结构划分计算 NMI/ARI 一致性 → 输出结构偏离度分数（0-1，高=目录结构不反映实际耦合，迁移拆解质量风险高）。扩展 `stats` 子命令输出。**不改 MDR-011 拆解逻辑**（Tier 2 辅助拆解视真实项目需求再定） | 2d | — |

**验收标准**：
- [ ] `rustmigrate quality`（或 `stats`）对已迁模块输出 source-behavior-coverage / degrade-rate / revision-rate / final_score
- [ ] jmespath + 1 TS 项目质量基线报告落地（`docs/m4-quality-baseline.md`）
- [ ] `/migrate review` 仪表板展示质量度量
- [ ] `stats` 输出含 Community 结构偏离度分数（Leiden vs 目录划分 NMI/ARI）
- [ ] `just ci` 全过

---

## Sprint C：Go Adapter Core（~15d，Go 线核心）

**目标**：实现 `GoAdapter`，可解析 Go 源码（含 build tags/test 文件过滤）、构建依赖图（含包凝聚）、检测复杂度分档。

| 任务 ID | 内容 | 工时 | 依赖 |
|---------|------|:---:|------|
| M4-GO-01 | **tree-sitter-go 集成 + 骨架补全**：`GoAdapter` 完成 `detect_tier`（危险信号 goroutine/channel/select/reflect/cgo/`unsafe.Pointer`）+ `resolve_extensions(["go"])` + `detect_source_root`（含 `go.mod` 的目录为 module 根） | 0.5d | LANG-01 |
| M4-GO-02 | **Go import 解析 + 文件过滤**：提取单 import 与分组 `import (...)`；区分标准库 / 本地 module（`go.mod` module path 前缀）/ 外部依赖；别名 import（`f "fmt"`）+ 点导入 + `_` 副作用导入标注。**关键过滤（R1 补）**：排除 `*_test.go`（同包测试文件污染符号集）；按 build tag 跳过非默认平台后缀文件（`*_linux.go`/`*_windows.go` 等）+ `//go:build` 门控——至少只迁默认构建集 | 2.5d | GO-01 |
| M4-GO-03 | **Go 包 resolve + 扩 trait（R1 critical 重写）**：`resolve_import` 的 `exists`-only 签名无法探任意命名的包代表文件 → **扩 trait 暴露目录列举能力（`list_dir` 回调）或返回包级聚合**；剥离 `go.mod` module path 前缀 → 定位包目录 → build.rs 跨包 Imports/Calls 边适配到包内真实文件节点。**spike 死断言**：`go-pkg-deps` dry-run 后断言「同 package 全部 `.go` 出现在同一 `DecompUnit.members`」，否则触发回退路径 | 3.5d | GO-02 |
| M4-GO-04 | **Go 符号提取**：`func`/`method`(receiver 归属到 type)/`type struct`/`type interface`/`const`/`var` 节点 + Contains 边；**首字母大写=导出**填 `exported_names`；struct 嵌入 → `Extends`。**激活 `NodeType::Variable`**（const/var，M2 预留变体，见对账表披露） | 2d | GO-01 |
| M4-GO-05 | **Go 调用分析 + instance_type_bindings**：`pkg.Func()` 跨包调用 + `x.Method()` receiver 匹配 + composite literal `Foo{}`/`&Foo{}` 构造绑定；复用 build.rs 档 1 名称匹配 | 1.5d | GO-04 |
| M4-GO-06 | **Go signature 提取**：func 签名（参数/多返回值/receiver/可变参/泛型 `[T any]`）+ interface 方法集 + struct 字段骨架 | 1.5d | GO-04 |
| M4-GO-07 | **interface 隐式实现处理**：不强连 `Implements` 边，标注 interface 方法集供 LLM 翻译期判断（D-M4-02） | 0.5d | GO-04 |
| M4-GO-08 | **Go fixture（4 个）**：`go-linear-deps`（线性+导出约定+多返回值）/`go-diamond-deps`（菱形+interface+struct 嵌入）/`go-circular-deps`（包级环检测）/`go-pkg-deps`（多文件 package 凝聚+跨包调用，**含 `_test.go` 与 build-tag 文件做过滤回归**）+ `ground-truth.json` | 1.5d | GO-02 |
| M4-GO-09 | **Go graph 集成测试**：fixture 偏序约束；signature round-trip；跨包 `pkg.Func` 边正确。**凝聚验收限定 fixture**（R1 修正）：断言 go-pkg-deps 同包 N 文件归入同一 `DecompUnit.members`（真实项目大包超 footprint 预算会拆，不承诺单一模块） | 1d | GO-03~08 |
| M4-GO-10 | **adapter 注册收尾 + grammar 契约测试**：`ast_contract_go.rs` 测 tree-sitter-go 节点类型稳定性（`function_declaration`/`method_declaration`/`type_spec`/`import_spec` 等） | 0.5d | GO-01 |

### 执行策略（PR 拆解，对齐 M3 Sprint B）

```
Step 1: PR-C1 (Foundation)     → GO-01 + GO-10          [工时 ~1d]
Step 2: PR-C2 (Core Analysis)  → GO-02~07               [工时 ~11.5d, 内部双线并行]
         ├─ Track A: GO-02 → GO-03（扩 trait + 包凝聚 spike）  [6d，关键路径]
         └─ Track B: GO-04 → GO-05 + GO-06 + GO-07              [5.5d]
Step 3: PR-C3 (Validation)     → GO-08 + GO-09          [工时 ~2.5d]
```

- 工时合计 15d；日历时间因 Track A/B 并行压缩到 ~1 + max(6, 5.5) + 2.5 ≈ **9.5d**
- **GO-03 扩 trait 是关键路径最高风险**，PR-C2 起步即做；spike 死断言不过则当 PR 内走包级聚合回退（工时已含）

**验收标准**：
- [x] 4 个 Go fixture 偏序约束全部满足
- [x] `cargo run -- graph build --root fixtures/go-linear-deps` 输出正确依赖图
- [x] `_test.go` 与非默认平台 build-tag 文件被正确排除（go-pkg-deps 回归覆盖）
- [x] go-pkg-deps 同包多文件归入同一 `DecompUnit`（spike 断言通过，或包级聚合回退已落地）
- [x] 首字母大写导出、多返回值签名、receiver 归属正确
- [x] `just ci` 全过，Go fixture 纳入 nextest

---

## Sprint D：Plugin Go 适配（~7d，Go 线）

**目标**：Plugin 层支持 Go 项目迁移分析和翻译。

| 任务 ID | 内容 | 工时 | 依赖 |
|---------|------|:---:|------|
| M4-PLG-01 | **`adapters/go/` 目录**（权威路径 `plugin/skills/migrate/adapters/go/`，符合 MDR-009 两文件契约）：`analysis-tools.json`（`go test`/`go vet`）+ `porting-template.md`（frontmatter 含 `language_id: go` + `target_languages: [go]` + `rule_version`，与 TS/Python 对齐） | 1d | — |
| M4-PLG-02 | **Go porting-template.md**：类型映射（`string→String`,`int→i64`,`[]T→Vec<T>`,`map[K]V→HashMap`,`interface{}→泛型/trait object`,`struct→struct`,`error→Result/thiserror`,`chan T→mpsc/async channel`,`nil→Option`）+ 惯用法（GC→所有权、`defer→Drop/scopeguard`、goroutine→async/thread、大写导出→`pub`、多返回值→元组/Result、零值语义） | 2d | — |
| M4-PLG-03 | **translator.md Go 分支**：类型映射表按 `source_language` 条件化；Phase A Go 特化（receiver `(s *T)`→`&self`/`&mut self`、package→mod、大写导出→`pub`、`if err != nil`→`?`/`Result`、无相对 import） | 1d | PLG-02 |
| M4-PLG-04 | **analyzer.md + verifier.md Go 适配**：analyzer Go 动态特性扫描（`reflect`/`cgo`/`unsafe.Pointer`/`go:generate`）记入 `gaps.dynamic_features`（**unsafe.Pointer 口径与 GO-01/PLG-05 统一：归 Ffi/RULE-12**）；verifier Go 特化案例（int 溢出 Go wrap vs Rust panic、map 迭代随机序、nil interface 双字、goroutine 泄漏、GC vs Drop 时机、recover/panic 边界） | 1d | GO-10 |
| M4-PLG-05 | **degrade_skip Go 边界**：goroutine/select/channel/reflect/cgo/`unsafe.Pointer` 触发降级，输出原因 + 推荐 Rust crate（tokio/crossbeam/rayon）；`blocked_by_skip` 传播复用 M3 机制 | 1d | — |
| M4-PLG-06 | **Plugin Go 端到端验证**：`/migrate analyze` + `run` 对 Go fixture headless 跑通，≥1 模块推进到 `translating` | 1d | PLG-01~05 |

**验收标准**（✅ 全达标，见 [m4-sprint-d-acceptance.md](m4-sprint-d-acceptance.md)）：
- [x] `adapters/go/` 含两文件契约，frontmatter 含 `language_id`/`target_languages: [go]`/`rule_version`（PR #62）
- [x] `/migrate analyze` 对 Go 项目正确填充 `migration-state.json`
- [x] `/migrate run` 对 Go fixture headless 跑通，≥1 模块推进到 `translating`
- [x] translator 可按 `source_language=go` 切换类型映射表
- [x] goroutine/channel/unsafe.Pointer 等触发 degrade_skip 并输出推荐替代（`classify_file` danger 分类 + porting-template crate 推荐）

---

## Sprint E：Go 端到端验收（~9d，Go 线）

**目标**：真实 Go 项目迁移验证，**以质量度量框架设有意义门槛**（R1 修正：非单模块编译）。

| 任务 ID | 内容 | 工时 | 依赖 |
|---------|------|:---:|------|
| M4-VAL-01 | **真实 Go 项目选型**：2 个 <3K 行开源 Go 项目（纯计算/数据处理/CLI 类，避免重并发/cgo/网络 I/O 密集）；要求有 `go test` 覆盖 | 0.5d | — |
| M4-VAL-02 | **项目 A 迁移有意义的一片**：analyze→build→populate→sprint_loop；**迁移多模块（含至少 1 个降级决策点）**，≥1 模块到 done | 2.5d | PLG-06, VAL-01, QUAL-01 |
| M4-VAL-03 | **项目 B 迁移有意义的一片**：同上 | 2d | VAL-02 |
| M4-VAL-04 | **差异测试框架（复用 M3）**：Go 行为录制（`go test` 输出 JSON fixture）→ Rust 对比；≥1 模块行为一致 | 1.5d | VAL-02 |
| M4-VAL-05 | **质量度量验收（替代纯性能门）**：用 Sprint B 框架度量 Go 迁移质量（覆盖率/degrade 率/修订率/final_score），与既有语言基线横比；性能仅做「无明显退化」轻量 smoke（不设 ±10% 硬门，离线 CLI 非性能敏感路径） | 0.5d | VAL-02, QUAL-02 |
| M4-VAL-06 | **graduate 验证**：`/migrate graduate` 对 Go 项目正确识别完成/未完成（复用 M2/M3 逻辑） | 0.5d | VAL-02 |
| M4-VAL-07 | **设计文档同步**：08-roadmap M4（Go 完成 / C 推迟理由 / Kani 推迟 / Community Tier 1 纳入）+ 04-toolchain Go 工具链 + 02-architecture Go adapter + PLAN.md §11 + **03 §7.5 登记质量度量框架（QUAL-01 三项新增度量 + QUAL-04 Community 偏离度）** | 1d | VAL-03 |
| M4-VAL-08 | **全量回归 + 覆盖率**：`just ci` 全过；覆盖率 ≥70% | 0.5d | ALL |

**验收标准**：
- [ ] 2 个真实 Go 项目各完成多模块迁移、≥1 模块 done（cargo check+test+clippy 全过）
- [ ] 差异测试：Go 原始行为与 Rust 对齐（JSON fixture 对比通过）
- [ ] **质量度量达标**：Go 迁移 final_score 与既有语言基线同档（不显著劣化）
- [ ] `/migrate graduate` 正确识别完成/未完成
- [ ] TS/Python/Go 全量回归无退化、性能无明显退化
- [ ] 设计文档 M4 交付物全部同步

---

## Sprint F：循环健壮性 + 编排收口（~5.5d，巩固线）

**目标**：补迁移循环错误恢复（呼应额度韧性/watchdog stall），编排做增量收口。**与 Go 线解耦，可独立 PR。**

> **ROB-01 扩充理由（R2 修正）**：M3 实际反复遇到 watchdog stall（agent 静默超时判失败）、额度耗尽中断、单模块失败扩散等**可靠性问题**——这不是「过早优化」而是可靠性基础设施缺口。原 1d 仅够做表面的「失败后状态不丢」，覆盖不了 watchdog 恢复和额度韧性。拆分为三个子项共 3d。

| 任务 ID | 内容 | 工时 | 依赖 |
|---------|------|:---:|------|
| M4-ROB-01a | **checkpoint 硬化 + 幂等重试**：每个模块完成后立即原子持久化状态（migration-state.json 原子写 + 全字段不丢）；重跑失败模块不腐蚀已有状态（状态回退 + 产物清理），保证幂等 | 1d | — |
| M4-ROB-01b | **watchdog stall 检测 + 恢复路径**：识别 agent 静默超时（后台长命令 stdout 静默超 600s）→ 标记当前模块失败 → 跳过或重试（可配置策略）→ 不阻塞后续无依赖模块推进；输出明确的失败原因和恢复建议 | 1d | ROB-01a |
| M4-ROB-01c | **额度耗尽优雅暂停 + 续跑**：检测 token 预算/API 额度逼近上限 → 保存当前进度（含进行中模块的中间状态）→ 输出续跑指令（断点位置 + 恢复命令）→ 下次从断点恢复，已完成模块不重跑 | 1d | ROB-01a |
| M4-ORCH-01 | **worktree 生命周期代码化**：当前 worktree 创建/合并/清理散在 run.md 文字约定 → 落为代码层管理（防泄漏/冲突）。**不含程序化状态机调度器**（当前 SKILL.md 编排满足 2-3 门语言调度需求，程序化状态机投入大、当前 ROI 不足，推迟） | 1d | — |
| M4-GOV-01 | **规则版本陈旧检测**：CLI 校验各 `porting-template.md` frontmatter `rule_version` vs 权威清单一致性；落地设计已留的 `[rules].enforce_rule_version_consistency` 开关；不一致返回明确错误（复用 `profile/tools.rs` JSON 框架）。**砍 index.json 自动生成**（3 门语言规模未到回本点，数据模型投机，YAGNI） | 1.5d | — |

**验收标准**：
- [ ] 模拟单模块失败/中断，续跑不丢状态、可重入、幂等（重跑不腐蚀已有产物）
- [ ] 模拟 watchdog stall（agent 静默超时），系统自动标记失败 + 跳过/重试 + 后续模块不阻塞
- [ ] 模拟额度耗尽，系统优雅暂停 + 输出续跑指令 + 下次断点恢复
- [ ] worktree 生命周期有代码层管理（创建/合并/清理），不再纯文字约定
- [ ] `rule_version` 与权威清单不一致时 CLI 报错（非静默）
- [ ] TS/Python/Go 既有路径无回归

---

## 任务总览

| Sprint | 线 | 任务数 | 工时 | 关键交付 |
|--------|----|:---:|:---:|---------|
| A 债务收口 + Go 前置 | 巩固/Go | 4 | 4.5d | MDR-013 三项清债（含 DangerCategory serde 双向）+ Go registry 接线 |
| B 质量度量 + 既有基线 | 巩固 | 4 | 7d | 质量度量框架 + TS/Python 真实基线 + Community 结构偏离度诊断（巩固线旗舰） |
| C Go Adapter Core | Go | 10 | 15d | GoAdapter 全方法 + 扩 trait 包凝聚 + 文件过滤 + 4 fixture |
| D Plugin Go 适配 | Go | 6 | 7d | adapters/go/ + 提示词 Go 分支 + degrade 边界 |
| E Go 端到端验收 | Go | 8 | 9d | 2 真实 Go 项目多模块 + 质量度量验收 |
| F 健壮性 + 编排收口 | 巩固 | 5 | 5.5d | checkpoint 硬化 + watchdog stall 恢复 + 额度韧性续跑 + worktree 生命周期 + 版本陈旧检测 |
| **合计** | | **37** | **~48d** | 巩固线 ~17d + Go 线 ~31d |

## 明确不做（含理由）

| 项目 | 理由 |
|------|------|
| C/C++ adapter（含 bindgen/cbindgen FFI 方案） | 无类型 IR 下 C 弱类型/指针/手动内存语义使安全翻译+等价审查远难于 Go、ROI 不划算；**推迟**，未来走 LLMIGRATE 式 tree-sitter 拓扑路线（D-M4-03） |
| Kani 形式化验证 | Kani（BMC）验证翻译后 Rust 代码正确性（无 panic/溢出/越界），与 proptest 差异测试（等价性）互补不替代。M4 预算有限，ROI 在当前阶段（2-3 门语言、小型项目）不如质量度量框架高；**推迟**到 C 路线（unsafe 高产出）或更大真实项目验证阶段（D-M4-04） |
| Community 社区检测（Tier 2/3） | Tier 1 质量诊断信号已纳入 Sprint B（QUAL-04，~2d）：Leiden 跑社区检测 → 与目录结构比对 → 输出偏离度分数。Tier 2（辅助 MDR-011 拆解）和 Tier 3（完整 `NodeType::Community` 落地）视真实项目暴露需求再定；**推迟** |
| Strangler Fig 工具化 | 本项目是离线翻译工作台，旧代码和新代码不需要运行时共存，Strangler Fig 的核心价值（过渡期共存+逐步切流量）在此场景下需求不强。注：拓扑迁移管翻译**顺序**、Strangler Fig 管过渡期**共存**，两者正交——但离线场景下 scaffold+拓扑已覆盖编译期可用性；**降文档** |
| 跨文件方法调用档 2（receiver 类型环境） | 质量增量、非阻塞，档 1 名称匹配够用；视真实漏边率未来立项；**推迟** |
| TypeAlias 节点产出 | 枚举已预留，非 Go 前置；Go fixture 暴露需求再补；**推迟** |
| 并行编排程序化调度器 | 当前 SKILL.md 编排满足 2-3 门语言调度需求，程序化状态机投入大、当前 ROI 不足。M4 做 worktree 生命周期收口（ORCH-01）+ **循环健壮性大幅扩充**（ROB-01a/b/c 共 3d：checkpoint 硬化 + watchdog stall 恢复 + 额度韧性续跑）；**推迟**程序化调度器 |
| index.json 自动生成 | 3 门语言规模未到回本点，数据模型投机，**YAGNI 砍** |
| 混合语言 monorepo / namespace packages | 单项目单语言假设不变，超 M4 范围 |

## 风险与缓解

| 风险 | 影响 | 缓解 |
|------|------|------|
| **Go 包系统需扩 trait（已确认非可选）** | build.rs 跨包边接线改动、Sprint C 关键路径滑期 | GO-03 工时已上修至 3.5d 含 build.rs 接线；spike 死断言（同包 .go 同 DecompUnit）pass/fail 明确；包级聚合回退工时已含 |
| DangerCategory serde 双向兼容 | 旧 state 文件反序列化失败 / 数组顺序断言挂 | 补 Deserialize+snake_case、`#[serde(other)]` 兜底、union 后 `as_str()` 重排；新增双向 serde 测试；23 处引用清单进任务防漏改 |
| build tags / `_test.go` 污染依赖图 | 真实项目（Sprint E）符号/边错误 | GO-02 显式过滤 `_test.go` + 非默认平台 build-tag；go-pkg-deps fixture 回归覆盖 |
| Go interface 隐式实现推不出 | Implements 边缺失 | 不强连，标注方法集供 LLM 判断（D-M4-02），宁缺勿错 |
| goroutine/channel 并发语义无法 1:1 映射 async | 翻译错误 | degrade_skip 边界前置（PLG-05），并发构造默认降级+推荐 crate |
| 质量度量框架本身不可靠 | 验收标尺失真 | QUAL-02 先在既有 TS/Python 已迁模块建基线校准，框架先于 Go 验收落地 |
| tree-sitter-go 版本错配 | 编译失败 / AST 节点漂移 | 锁 0.21.x；GO-10 grammar 契约测试 |

## 设计决策

| 编号 | 问题 | 决策 | 理由 |
|------|------|------|------|
| D-M4-01 | Go 包系统映射 | **扩 trait 暴露目录列举（baseline，非 fallback）+ MDR-011 凝聚合并同包** | `resolve_import` 的 `exists`-only 签名无法探任意命名包代表文件（R1 critical）；Go「目录=package」靠目录凝聚合并 |
| D-M4-02 | Go interface 隐式实现 | **不强连 Implements 边，标注方法集** | tree-sitter 静态推不出实现关系；强连制造假边，宁缺勿错 |
| D-M4-03 | 是否 M4 做 C adapter | **推迟** | 无类型 IR 下 C 语义翻译+等价审查难度+ROI 低（非「宏一票否决」） |
| D-M4-04 | Kani 集成 | **推迟（非砍）** | Kani 验证正确性（无 panic/溢出/越界）、proptest 验证等价性，互补不替代。当前 ROI 不如质量度量；推迟到 C 路线（unsafe 高产出）或更大项目验证阶段 |
| D-M4-05 | goroutine/channel/unsafe degrade 边界 | **并发默认 degrade_skip+推荐 crate；unsafe.Pointer 归 Ffi/RULE-12** | 并发 1:1 映射 async 不可靠；unsafe 三处口径统一 |
| D-M4-06 | M4 旗舰定位 | **双主线：巩固（质量度量）+ Go 扩语言** | 纯 Go 不符「完善」语义且验证门槛过低（R1 critical）；质量度量同时解决可证伪性与 Go 验收标尺 |

## 08-roadmap M4 交付物对账表

| 08-roadmap §M4 方向 | PLAN-M4 状态 | 说明 |
|---------------------|-------------|------|
| C/C++ LanguageAdapter（bindgen+cbindgen） | **推迟**（D-M4-03） | FFI 方案随 C 整体推迟；roadmap 同步更新 |
| Go LanguageAdapter | ✅ **Go 线旗舰** | Sprint C/D/E |
| Kani 集成 | **推迟**（D-M4-04） | Kani（正确性）与 proptest（等价性）互补；当前 ROI 不足，推迟到 C 路线或更大项目 |
| 社区反馈驱动规则库积累 | **部分** | Sprint F GOV-01 版本检测；Sprint B QUAL-04 Community 结构诊断；社区运营非代码范围 |
| 多 agent 并行编排优化 | **收口（大幅扩充健壮性）** | Sprint F ORCH-01 worktree 生命周期 + ROB-01a/b/c 循环健壮性 3d（checkpoint/watchdog/额度韧性）；程序化调度器推迟 |
| Strangler Fig 模式工具支持 | **降文档** | 离线工作台场景下运行时共存需求不强；scaffold+拓扑已覆盖编译期可用性 |
| （新增）迁移质量度量 + 巩固 | ✅ **巩固线旗舰** | Sprint B；R1 审查驱动新增，非 roadmap 原列，已显式标注 |

## 图 schema 预留变体状态对账

| 变体（types/graph.rs，标「M2 预留」） | M4 状态 | 说明 |
|------|---------|------|
| `NodeType::Variable` | ✅ **激活**（Sprint C GO-04） | Go const/var 模块级变量节点 |
| `NodeType::TypeAlias` | **仍推迟** | 非 Go 前置 |
| `NodeType::Community` | **Tier 1 诊断纳入 Sprint B** | Leiden crate 已可用（graphrs/leiden-rs）；QUAL-04 做质量诊断信号；Tier 2/3 推迟 |

## PLAN-M2/M3 推迟项 M4 处置对账

| 推迟项（来源） | M4 处置 |
|---------------|---------|
| 规则治理工具化 / 适配器规则版本陈旧检测（M3 对账） | ✅ Sprint F GOV-01 |
| index.json 自动生成（M3 对账） | **砍**（YAGNI） |
| 图 schema 扩展 TypeAlias / Community（M3 对账） | TypeAlias 推迟 / Community Tier 1 纳入 Sprint B QUAL-04 |
| FTS5 全文搜索 / 降级决策学习 / 类型复杂度前置降级信号 / 跨文件方法调用档 2（M3 对账） | 不做 / 推迟 |
| MDR-013 三项遗留 TODO（STATUS） | ✅ Sprint A DEBT-01/02/03 |

---

## 审查修订记录（R1 → v0.2，历史）

| 来源视角 | 严重度 | 修订 |
|---------|:---:|------|
| 主线决策 | critical | M4 重定位为「巩固+Go」双主线；新增 Sprint B 质量度量框架；提高 Sprint E 验收门槛（多模块+质量度量，非单模块编译） |
| 主线决策 | critical/事实 | 删 Discord「内存~8×」捏造精度，改方向性表述（消除 GC 尾延迟尖峰） |
| 主线决策 | important | C 推迟理由从「宏命门」改为「语义难度+ROI」（避免与 Go degrade 处理双标） |
| 主线决策 | important | Sprint E（原规则治理+编排）重砍：砍 index.json + 动态并发，仅留 worktree 生命周期 + 版本检测（移入 Sprint F）；补循环错误恢复（Sprint F ROB-01） |
| 可执行性 | critical | GO-03 重写：扩 trait 为 baseline（非 fallback），工时 2d→3.5d，spike 给死断言 |
| 可执行性 | important | Sprint C 工时算术修正（逐项 15d，区分工时/日历）；总计 ~44d |
| 可执行性 | important | DEBT-02 补 DangerCategory Deserialize+snake_case+`#[serde(other)]`+union 重排+23 处文件清单 |
| 可执行性 | important | GO-02 补 `_test.go` + build tags 过滤；GO-09 凝聚验收收紧到 fixture |
| 设计契约 | important | io RULE 登记 05 §6.2 + 裁定归属；Go unsafe 三处口径统一（Ffi/RULE-12）；对账表披露 `NodeType::Variable` 激活 |
| 设计契约 | nit | rule_version bump、concern() 文案中立化、target_languages:[go]、删 LANG-01 错误子项、tree-sitter-go 锁 0.21.x |

## 审查修订记录（R2 → v0.3）

| 修订项 | 严重度 | 修订内容 |
|--------|:---:|---------|
| Kani 处置 | important | 从「砍」改为「推迟」。原理由「proptest 更直接」偷换概念（Kani 验正确性、proptest 验等价性，互补不替代）；改为「M4 预算有限、ROI 在当前阶段不如质量度量，推迟到 C 路线或更大项目」 |
| Strangler Fig 理由 | nit | 结论不变（降文档），理由从「已被拓扑迁移覆盖」改为「离线工作台场景下运行时共存需求不强」——拓扑管顺序、Strangler Fig 管共存，两者正交 |
| Community 检测 | important | 原「无成熟纯 Rust Leiden crate」判断已过时（graphrs 0.11.16 / leiden-rs 0.8.1 均可用）。新增 Sprint B QUAL-04（~2d）：Tier 1 质量诊断信号（Leiden 社区检测 → 与目录结构比对 → 输出结构偏离度分数） |
| Sprint F ROB-01 扩充 | important | 原 1d 不够覆盖 M3 实际遇到的 watchdog stall、额度耗尽等可靠性问题。拆为 ROB-01a（checkpoint 硬化+幂等重试 1d）+ ROB-01b（watchdog stall 检测+恢复 1d）+ ROB-01c（额度耗尽优雅暂停+续跑 1d），共 3d |
| 并行编排推迟理由 | nit | 去掉「过早优化」措辞（错贴在可靠性问题上），改为「当前 SKILL.md 编排满足调度需求，程序化状态机投入大、ROI 不足」 |
| 工时总计 | — | 巩固线 ~13d → ~17d（+2d QUAL-04 + +2d ROB 扩充）；合计 ~44d → ~48d；任务数 34 → 37 |

> 参考案例：Discord Go→Rust（消除 GC 尾延迟尖峰，https://discord.com/blog/why-discord-is-switching-from-go-to-rust）；Go→Rust 迁移指南（https://corrode.dev/learn/migration-guides/go-to-rust/）；tree-sitter-c 宏限制 issue #108（https://github.com/tree-sitter/tree-sitter-c/issues/108）。
