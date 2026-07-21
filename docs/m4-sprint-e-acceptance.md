# M4 Sprint E 验收记录：Go 端到端迁移验证（M4-VAL-01~08）

> Go 扩语言线收官验收。对齐 memory「验收≠工具自测」「并行是核心」：用 2 个真实开源 Go 项目跑通 analyze→build→populate→翻译→差异测试→质量度量→graduate 全链，以质量度量框架设有意义门槛（非单模块编译）。所有验证均由编排器**独立复核**——不信 subagent 自报，整组 `cargo check`/`clippy --all-targets -D warnings`/差异测试均亲自跑，并对差异测试做负向验证（篡改 fixture 立即报不一致→还原）。

## 选型（VAL-01 ✅）

2 个 <3K 行、纯计算/数据处理、有 `go test` 覆盖、MIT、0 并发/cgo/reflect 的项目（交叉验证真实存在）：

| 项目 | 规模 | 结构 | 迁移代表模块 |
|------|------|------|-------------|
| **semver**（github.com/Masterminds/semver） | 1656 行 / 4 文件单包 | 单模块 | `file:collection.go`（coupled_batch 4 文件） |
| **go-humanize**（github.com/dustin/go-humanize） | 12 文件多模块（含 math/big） | 多模块 | `file:big.go`（coupled_batch 12 成员） |

- **降级点决策**：如实记录不强造（用户拍板）。
- **工作区**：`/tmp/{semver,humanize}-migrate`，绝不动本仓库。

## 验收结果总览

| 验收项 | semver（项目 A） | go-humanize（项目 B） | 判定 |
|--------|-----------------|----------------------|------|
| VAL-02/03 ≥1 模块 done | ✅ collection.go → done | ✅ big.go → done | ✅ |
| VAL-04 差异测试逐条等价 | ✅ 276/276 | ✅ 87/87 | ✅ |
| VAL-05 质量度量 final_score | ✅ 100.0 | ✅ 100.0 | ✅ |
| VAL-06 graduate 正确识别 | ✅ | ✅ | ✅ |
| VAL-08 全量回归 `just ci`（本仓库级，非分项目） | ✅ 757 测试全绿 | ✅（同左） | ✅ |

## VAL-02：semver 迁移（项目 A）

- **Phase A 忠实翻译**：1532 行 Rust ← 1656 行 Go（全干净直译，0 降级）。
- **独立验证**：整组 `cargo check` 0 error + `clippy --all-targets -D warnings` 0 warning。
- **翻译要点**：regexp→regex crate、指针 receiver→`&self`、Go error→`Result`、sql driver/json 接口→独立方法、包级 var→`AtomicBool`。
- **模块** `file:collection.go`（4 文件 coupled_batch）→ **done**，metrics 写回 `276/276`、`known_differences=0`。

## VAL-03：go-humanize 迁移（项目 B）

- **Phase A 忠实翻译**：1074 行 Rust（12 个 `.rs`）← 12 文件 Go。
- **独立验证**：`cargo check` + `clippy --all-targets -D warnings` 全绿（本次会话复核确认）。
- **翻译要点**：`big.Int`→num-bigint、`big.Rat`→num-rational 均忠实。
- **唯一自然降级**：`BigCommaf(big.Float)`——Rust 无无系统依赖的任意精度二进制浮点忠实等价，保留明确 `TODO(port)`/`unimplemented!()`，符合「如实记录不强造」。差异测试覆盖范围已注明排除该点。
- **模块** `file:big.go`（12 成员 coupled_batch）→ **done**，metrics 写回 `87/87`、`known_differences=0`。

## VAL-04：差异测试框架（复用 M3）

Go 录制程序（`record/main.go`，`replace` 指向本地 `src`）录行为点 → Rust `tests/differential.rs` 逐条对照 fixture.json。

| 项目 | fixture 断言数 | 覆盖类别 | 结果 |
|------|--------------|---------|------|
| semver | 276 | 版本解析/比较/约束/排序等 | 276/276 等价 |
| go-humanize | 87 | bytes/ibytes/parse_bytes/comma/commaf/si/parse_si/ftoa/ordinal/format_float/format_integer/big_bytes/big_comma/plural/word_series/oxford（16 类） | 87/87 等价 |

- **编排器独立负向验证**（两项目均做）：篡改 fixture 首条期望值 → 差异测试立即报 `1/N 不一致`（打印 `Go=... Rust=...`）→ 还原恢复绿。证明断言非空跑、真实生效。
- **断言下限守卫**：go-humanize 的 differential.rs 末尾有 `assert!(total >= 80)` 防 fixture 未加载导致空跑假绿；semver 的 differential.rs 无 total 下限断言，靠 `failures.is_empty()` + 负向验证保证非空跑。

## VAL-05：质量度量验收（替代纯性能门，复用 Sprint B 框架）

`rustmigrate stats quality` 输出，两项目 final_score 均与既有语言基线同档：

| 指标 | semver | go-humanize | 口径 |
|------|--------|-------------|------|
| `avg_final_score` | 100.0 | 100.0 | §7.5 加权公式 |
| `behavior_coverage` | 1.0 | 1.0 | `test_pass_rate × (1 - known_diff/(known_diff+10))` |
| `test_pass_rate` | 1.0（276/276） | 1.0（87/87） | 差异测试通过率 |
| `degrade_rate` | 0.0 | 0.0 | 降级模块/总模块 |
| `data_completeness` | 1.0 | 1.0 | 指标写回完整度 |

- **性能**：仅做「无明显退化」轻量 smoke（离线 CLI 非性能敏感路径，PLAN-M4 不设 ±10% 硬门）。

## VAL-06：graduate 验证

`/migrate graduate` 对 Go 项目正确识别完成/未完成（复用 M2/M3 逻辑）。两项目均已推进到 `graduate` 状态（sprint_loop→graduate 转换成功）；在 `graduate` 态再次调用正确返回「需在 sprint_loop 状态执行」的配置错误（幂等守护生效）。

## VAL-07：设计文档同步

- **08-roadmap-and-reference.md §M4**：Go LanguageAdapter 完成登记 + C 推迟（D-M4-03）+ Kani 推迟（D-M4-04）+ Community Tier 1 纳入 + 巩固线各项状态。
- **04-toolchain.md**：tree-sitter 绑定列显式补 tree-sitter-go；新增 Go 工具链脚注（0.21 锁版本、包系统映射、danger 降级边界、interface 不强连、无独立外部工具）。
- **02-architecture.md**：LanguageAdapter 已落地适配器注记（TS/Python/Go 共享 trait 契约 + configure_project 钩子 + C 推迟）。
- **PLAN.md §11**：M4 表加状态列（Go ✅ / C ⏸️ / Kani ⏸️ / 社区部分）。
- **03 §7.5 质量度量框架**：QUAL-01 三项新增度量 + QUAL-04 Community 偏离度已在 QUAL-05（PR #77）登记。

## VAL-08：全量回归 + 覆盖率

- 本仓库 `just ci` 全绿（757 测试 + fmt-check + clippy -D + deny + shellcheck）。
- 迁移工作区各自 `cargo test` + `clippy --all-targets -D warnings` 全绿。

## 暴露的真实工具缺口

| # | 缺口 | 状态 |
|---|------|------|
| 1 | `stats quality --source/--rust` 的 `project_loc_ratio` 把源侧 Go `_test.go` 也计入 tokei LOC，稀释比率（humanize 0.465、semver 0.316，真实非测试比约 0.9）。 | **记 TODO（[issue #78](https://github.com/snowzhaozhj/rewriteInRust/issues/78)）**：`count_loc` 应支持排除测试文件后再算翻译对照 LOC 比。当前该值仅 `project_loc_ratio` 项目级近似（已有 warning 声明「不参与模块评分」），不影响 final_score，但横比会误导。 |

> 缺口 #1 不阻断验收——loc_ratio 已按设计不进评分卡（#77 已加 warning）；final_score=100 由 behavior_coverage + compile_pass + test_pass_rate 驱动，未受影响。独立 PR 修复。

## 范围边界（非缺陷）

- Phase A 忠实翻译按等价测试深度定为差异测试逐条对照（Sprint D 已验证的能力），非穷举 property test。
- 每项目取一个有意义的代表模块迁到 done（PLAN-M4 要求 ≥1），非全项目 graduate。
- go-humanize `BigCommaf(big.Float)` 自然降级，不在差异测试覆盖范围（已注明）。
