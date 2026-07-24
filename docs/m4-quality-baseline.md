# M4 迁移质量基线报告（M4-QUAL-02）

> 权威计划：[PLAN-M4.md](PLAN-M4.md) §Sprint B QUAL-02
> 度量框架：[03-execution-model.md §7.5](design/03-execution-model.md) 评分卡 + `rustmigrate stats quality`
> 数据来源：[m4-sprint-e-acceptance.md](m4-sprint-e-acceptance.md)（Go）+ [sprint-d-acceptance.md](sprint-d-acceptance.md)（Python，M3）

## 目的

QUAL-02 的目标是**建立「质量回归」横比起点**：用 Sprint B 落地的质量度量框架，在既有已迁真实项目上记录一组基线数值，供后续（Go 验收、未来语言）横向对比、发现质量退化。

本报告**汇编已验证的真实迁移实测数据**，不重新迁移（M3 迁移产物与 `/tmp` 工作区已清空）、不造数。每类数据显式标注来源口径与可比性边界。

## 数据分层与口径

三组已迁真实项目，按「是否有 `stats quality` CLI 实测数值」分两类：

| 项目 | 语言 | 迁移里程碑 | 有 `stats quality` 数值？ | 原因 |
|------|------|-----------|--------------------------|------|
| semver | Go | M4 Sprint E | ✅ 有 | 质量框架（QUAL-01 #58 / QUAL-05 #77）已落地，验收时直接跑 CLI |
| go-humanize | Go | M4 Sprint E | ✅ 有 | 同上 |
| jmespath | Python | M3 Sprint D | ❌ 无（有强等价实测） | M3 迁移时 QUAL-01/05 质量框架尚未落地（#58/#77 是 M4 才打通写回），产物已清空无法补跑 |
| textdistance | Python | M3 Sprint D | ❌ 无（有强等价实测） | 同上 |

## 一、Go 基线（`stats quality` CLI 实测）

M4 Sprint E 验收时由 `rustmigrate stats quality` 直接输出（[m4-sprint-e-acceptance.md](m4-sprint-e-acceptance.md) §VAL-05）。这是最硬的一档——质量框架 CLI 的端到端真实产出。

| 指标 | semver | go-humanize | 口径（§7.5） |
|------|--------|-------------|-------------|
| `avg_final_score` | 100.0 | 100.0 | `deterministic_avg × 0.7 + ai_avg × 0.3`（per-module 加权聚合） |
| `behavior_coverage` | 1.0 | 1.0 | `test_pass_rate × (1 − known_diff/(known_diff+10))` |
| `test_pass_rate` | 1.0（276/276） | 1.0（87/87） | 差异测试通过率 |
| `degrade_rate` | 0.0 | 0.0 | 降级模块 / 总模块 |
| `data_completeness` | 1.0 | 1.0 | 指标写回完整度 |

- **迁移规模**：semver 1532 行 Rust ← 1656 行 Go（4 文件单包，0 降级）；go-humanize 1074 行 Rust ← 12 文件多模块（唯一自然降级 `BigCommaf(big.Float)`，Rust 无无系统依赖任意精度二进制浮点忠实等价）。
- **`project_loc_ratio`**：验收时 semver 0.316 / humanize 0.465 曾因 `_test.go` 计入分母而被稀释（[issue #78](https://github.com/snowzhaozhj/rewriteInRust/issues/78)）；已由 PR #82 修复（`count_loc_excluding_tests` 排除源侧测试）。该值按设计**不进评分卡**（仅项目级近似、带 warning），不影响上表 final_score。

## 二、Python 基线（M3 强等价实测，无 CLI 数值）

M3 Sprint D 迁移，验收深度是**源引擎录制 → Rust 逐条差异断言**的真实强等价（[sprint-d-acceptance.md](sprint-d-acceptance.md)）。当时质量框架 CLI 尚未落地，故无 `stats quality` 数值，但等价实测本身可换算 `test_pass_rate` 口径。

| 项目 | 迁移规模 | 差异测试等价 | 独立复核 | graduate |
|------|---------|-------------|---------|----------|
| jmespath | 1675 行 / 迁 2 模块 | **901/902 等价 + 1 豁免**（D-10，mismatch=0） | `cargo test` 114 lib + 2 golden 全过、`clippy --all-targets -D` 清零 | ✅ 2/2 done → graduate 成功 |
| textdistance | 2657 行 / 迁 base.py 组 | `golden_edit_seq` **70/70 等价** | `cargo test` golden_harness 2 passed、`clippy --all-targets -D` 清零 | ✅ 负向验证正确拒绝（未完成模块不予毕业） |

- **换算口径**：jmespath 若按 M4 框架计，`test_pass_rate` = 901/902 ≈ 0.9989（1 例为已登记 KNOWN_DIFFERENCES 豁免、非失败）；textdistance `test_pass_rate` = 70/70 = 1.0。
- **豁免 D-10**（jmespath `foo[8:2:0]`）：源抛 ValueError（slice step==0），headless safe-default 返 `Ok([])`，登记 KNOWN_DIFFERENCES；测试仍锁定该 safe-default 行为断言（豁免不掩盖回归）。

## 三、TS 真实项目基线——**缺口（未落地）**

QUAL-02 字面要求含「1 个 TS 真实项目」基线，但 M3/M4 的真实迁移验收全部用 Python（jmespath/textdistance）与 Go（semver/go-humanize），**TS 只有 fixture 级验证（linear/diamond/circular/edge-cases），真实项目基线从未落地**。

- **影响**：TS 是本项目最早支持、回归测试最密的语言（M1/M2 全程），路径正确性由 fixture ground-truth + `just ci` 持续守护；缺的是「真实 TS 项目端到端迁移的质量数值」这一横比样本。
- **不阻断**：质量框架本身已由 Go 两项目 CLI 实测充分验证；横比起点已由 Go（CLI 数值）+ Python（强等价实测）两档建立。TS 真实基线是**锦上添花的第三档样本**，需要一次真实 TS 项目迁移（~1.5d 工程），留待有真实需求或专门排期时补。

## 横比起点结论

- **final_score 基线档位 = 100**（Go 两项目 CLI 实测，§7.5 加权公式）。后续任何语言迁移的 final_score 若显著低于此、或 degrade_rate 明显高于 0，即触发质量回归审视（对齐 03 §4.9 项目级止损标准的「当前 Sprint 评分 < 前 3 个 Sprint 均值 −10%，连续 2 个 Sprint 触发评审」机制；§7.5 引用该规则）。
- **test_pass_rate 基线 ≈ 1.0**（四项目一致：Go 276/276 + 87/87，Python 901/902 + 70/70），差异测试全量逐条对照是既有语言的等价验证深度基准。
- **degrade_rate 基线 = 0**（除 go-humanize 的 1 个自然降级 `BigCommaf`，均无强造降级——对齐「降级点如实记录不强造」原则）。

## 已知局限（如实记录）

1. **Python 侧无 CLI 数值**：jmespath/textdistance 的质量数据是 M3 强等价实测换算，非 `stats quality` 直接输出（迁移早于框架落地，产物已清空无法补跑）。若需 Python 侧的完整 CLI 度量，须重新迁移一个 Python 项目并在 M4 框架下跑 `stats quality`。
2. **TS 真实基线缺失**：见 §三，需一次真实 TS 项目迁移补齐。
3. **样本量**：4 个真实项目（2 Go + 2 Python）+ fixture 级 TS。作为回归起点足够，但不构成大样本统计基准（final_score 全 100 反映的是「取有意义代表模块迁到 done、忠实翻译 + 全量等价」的验收标准，而非项目全量 graduate 的平均质量）。
