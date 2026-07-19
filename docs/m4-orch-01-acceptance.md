# M4-ORCH-01 验收记录：并行 + worktree 编排真实项目演练

> ORCH-01 PR-5 交付物。对齐 memory「验收≠工具自测」：用真实开源项目跑通并行翻译全链，作为「并行编排端到端可用」的里程碑证据。前 4 个 PR（#71/#73/#74/#75）已把并行/worktree 基础设施从 coded-but-dead 接活并进 CI；本演练证明真 LLM 编排器能驱动该链路。

## 演练目标

**证明性演练**（非全量迁移）：取真实开源项目的**一个拓扑层多个独立模块**，跑通并行翻译核心链路——
> 同层 N 路并行派发 → 各自 worktree 隔离翻译 → 逐层 git merge → 整组 `cargo check` 真门 → 两层 done（`batch-transition-done`）。

这是 PR-4 mock 集成测试（Rust harness 串行模拟）之上的**真 LLM 并发**验证，补 codex I1 指出的 harness 固有边界：真并发 worktree/target 锁争用 + worktree 内真自检 + 真实翻译产物合并。

## 选型

**textdistance**（github.com/life4/textdistance，MIT）——30+ 文本/序列距离算法，纯算法、无 I/O、算法文件彼此拓扑独立，天然适合展示「同层多模块并行」。工作区 `/tmp/textdistance-check`（真实 git clone，绝不动本仓库）。

> 对比 jmespath（Sprint D 已验收）：jmespath 是链式 lexer→parser→AST，并行度低；textdistance 的 `algorithms/` 下多个算法只依赖 `base`、彼此独立，是理想的并行演练目标。

## 演练配置

- **规模**：3 路并行（sprint 3 层取 simple / sequence_based / phonetic 三个算法模块）
- **模式**：headless 无人值守（`headless=true` + `auto_confirm_intent=true`）
- **分层**：`state populate-modules --no-decompose`（SCC-only，14 单文件模块 6 层）——默认 MDR-011 目录凝聚会把 `algorithms/` 压成 3 个 coupled_batch 大模块、并行度被吃掉，故用 no-decompose 暴露真实拓扑并行度
- **CLI**：`cli/target/release/rustmigrate`（Sprint D 教训 #7：改 CLI 后必重建 release）

## 分层拓扑（graph parallel-groups / no-decompose populate）

```
sprint 1: types.py, utils.py, libraries.py       （叶层）
sprint 2: base.py, benchmark.py
sprint 3: simple, sequence_based, phonetic, compression_based, edit_based, vector_based  （6 路可并行）
sprint 4: token_based（依赖 edit_based）
sprint 5: algorithms/__init__.py
sprint 6: __init__.py
```

演练取 sprint 3 的 3 路（simple/sequence_based/phonetic），前置依赖 types/utils/base。

## 执行记录

### 阶段 0：确定性前置（零 LLM 额度）

| 步骤 | 命令 | 结果 |
|------|------|------|
| clone | `git clone --depth 1 textdistance` | ✅ |
| init | `rustmigrate init` | ✅ |
| graph build | `graph build --root textdistance` | ✅ 241 节点 / 330 边（status=warning：Python 全量构建） |
| populate | `state populate-modules --root textdistance --no-decompose` | ✅ 14 模块 / 6 sprint |
| scaffold | `scaffold workspace --target rust-src --name textdistance` | ✅ |
| 项目状态机 | `transition → profile → plan → scaffold → sprint_loop` | ✅ |
| porting 规则 | 落 `adapters/python/porting-template.md` 到 `.rust-migration/porting/core-rules.md` | ✅ |
| git 基线 | 建 `rust-migration` 分支，提交骨架 + porting + state | ✅ |

### 阶段 1：前置层串行翻译（types/utils/base）

派 1 个 translator（串行，前置是并行层依赖）：
- `types.rs`：`SimFunc<T>`/`TestFunc<T>` 类型别名（`Callable`→`Box<dyn Fn>`）
- `utils.rs`：`words_combinations`/`find_ngrams`（`itertools.permutations/product` 自实现）
- `base.rs`：`Base`/`BaseSimilarity` 双 trait（ABC 拆 trait+struct）+ counter 自由函数

**编排器独立验证**（不信 subagent 自报）：`cargo check` exit=0 ✅。提交 + 推进 3 模块 → done。

**safe-default 关键决策**：base.py 的 `libraries.get_libs()` 外部库加速路径（`external=true` 时从 30+ 外部库取答案）→ headless safe-default 裁剪为 `external_answer` 恒返 None（等价 external=false），不翻译 libraries.py。

### 阶段 1.5：libraries.py 降级（真实演练撞出的现象）

**图静态传递依赖 vs 翻译期裁剪的偏差**：`state deps` 报 3 路目标都 blocking=`libraries.py`——因图把 `base → libraries` 记为依赖，libraries 成为所有算法的传递依赖。但 base 的 Rust 翻译已 safe-default 裁剪该依赖，Rust 侧不存在此边。

处理：libraries.py（引 30+ 外部库，headless 不直译）走合法降级链 `translating → compile_fixing → paused → degrade_skip`。降级后 3 路目标门禁全部 `ready=True`。

> 值得记录：`pending → degrade_skip` 非法（矩阵要求经 paused），且 `translating → paused` 也非法（须先 compile_fixing/testing）。降级须走 `... → compile_fixing → paused → degrade_skip`。这是状态机矩阵对「降级必经失败态」的强制。

### 阶段 2：sprint 3 三路并行翻译（核心）

依赖门禁就绪后（3 路目标 `state deps` 全 `ready=True`），按 workflow.md 步骤 2a：
1. 提交 state 基线 → 建 3 个 worktree：`git worktree add .wt/{m} -b wt/{m} HEAD`（从含前置 done 代码的 HEAD）
2. 标 3 模块 `translating` + 记派发台账（`record-subagent-call`）
3. **真并发派发 3 个 translator subagent**（单条消息 3 个 Agent 调用），各在独立 worktree、`CARGO_TARGET_DIR` 隔离编译锁

三路回传（各自 worktree 内 `cargo check` 0 error + `clippy` 0 warning + 已 commit）：

| 模块 | Python 行 | Rust 行 | 产物 | worktree 自检 |
|------|----------|---------|------|--------------|
| simple.py | 127 | 319 | Prefix/Postfix/Length/Identity/Matrix（Postfix 无继承 → 组合复用 Prefix） | ✅ 0 error |
| sequence_based.py | 186 | 415 | LCSSeq/LCSStr/RatcliffObershelp（difflib.SequenceMatcher 自实现） | ✅ 0 error |
| phonetic.py | 179 | 334 | MRA（impl BaseSimilarity）/Editex（impl Base，Levenshtein 风格 DP） | ✅ 0 error |

**并发实证**：3 路在各自 worktree（`.wt/simple` / `.wt/sequence_based` / `.wt/phonetic`）真并发翻译，各设 `CARGO_TARGET_DIR=.wt/{m}/rust-src/target` 隔离，无编译锁争用。补 PR-4 mock harness（串行模拟）覆盖不到的真并发 + worktree 内真自检（codex I1 指出的边界）。

### 阶段 3：逐层合并 + 整组 check 真门 + 两层 done

**步骤 2c 逐层 git merge（真实 reconcile）**：
- `wt/simple` → 主分支：干净合并
- `wt/sequence_based` → **lib.rs 冲突**（两分支都 append `pub mod` 到相邻行）
- `wt/phonetic` → **lib.rs 冲突**（同上）

**关键洞察——聚合文件的 append 冲突应确定性合并、非 abort 重译**：lib.rs 的 `pub mod` 声明冲突是**纯追加冲突**（每个 translator 必须 append 自己的 mod，语义独立不互斥）。workflow.md 步骤 2c 的默认「冲突 → abort + 重译」对此**不适用**——重译不会消除冲突（新 translator 仍会 append）。编排器正确处理是**确定性合并保留全部声明**（MDR-003「结构化合并」）。本演练编排器直接解决两次 lib.rs 冲突，未触发重译。

**步骤 2d 整组验证真门（唯一 done 真门）**：3 路全合并后在主 worktree 执行整组验证——

```
cargo check --quiet   → exit=0 ✅（跨并发兄弟无 orphan rule/命名冲突）
cargo clippy -D warnings → exit=0 ✅
cargo test --quiet    → exit=0 ✅（0 tests：Phase A 忠实翻译未含单测，见边界）
```

**步骤 2d 两层 done**：整组过 → 编排器推进本层成功模块 `testing → reviewing`（`agent_done` substatus 全程保留，实测钉死）→ `batch-transition-done`：

```json
{"requested": [simple, sequence_based, phonetic], "succeeded": [3 全部], "skipped": [], "duplicates": 0}
```

3 模块 `reviewing → done`。这是 PR-3/PR-4 接活的 `batch-transition-done` 在**真实项目**上的首次运用。

**步骤 2e 清理**：`git worktree remove` × 3 + `git branch -D wt/{m}` × 3，worktree 列表只剩主。

## 演练撞出的真实工具缺口（记 TODO）

真实演练的价值正在于暴露 mock 测试和自测覆盖不到的缺口：

| # | 缺口 | 现象 | 建议 |
|---|------|------|------|
| 1 | **scaffold 不生成 `.gitignore`** | `cargo init --vcs none` 显式禁 VCS 文件，worktree 内自检产生的 `target/` 被 `git add -A` 吞进提交，merge 时污染（每路 30+ target 文件）+ 前置层提交也被污染（主分支 31 个 target 文件） | **本 PR 已修**：`scaffold::template` 两个 scaffold 函数补 `write_gitignore`——**后置条件式幂等**（无文件则建、有文件但缺 `/target` 有效规则则追加保留用户内容、已有则不动），且 `Cargo.toml` 已存在的早返回路径也确保 `.gitignore`（修 codex 审查的失败重试语义漏洞）。5 个回归测试 |
| 2 | **图静态传递依赖 vs 翻译期 safe-default 裁剪偏差** | base.py `import libraries` 使 libraries 成所有算法的传递依赖、卡住 `state deps` 门禁；但 base 的 Rust 翻译已 safe-default 裁剪该依赖，Rust 侧此边不存在 | 编排器判断：被裁剪的可选依赖应能解除下游门禁。当前靠人工 degrade libraries.py 绕过。可考虑 translator 回传「实际裁剪的依赖」供编排器更新图 |
| 3 | **workflow.md 步骤 2c 对聚合文件 append 冲突的指引不足** | lib.rs 的 `pub mod` 追加冲突被默认「abort + 重译」逻辑误导；实际应确定性合并 | workflow.md 2c 补：聚合文件（lib.rs/mod.rs）的纯 append 冲突走结构化合并（保留全部声明），不 abort 重译 |

> 澄清一个**排除的伪缺口**：曾疑「`state transition --substatus agent_done` 不持久化」，用全新模块干净复现证明**为脚本假象**（for 循环变量替换错误 + `>/dev/null` 吞错），CLI 行为正确——`agent_done` substatus 在 `--to testing/reviewing` 全程保留、落盘正常，PR-4 断言无误。未写入缺口清单。

## 结论

**ORCH-01 并行 + worktree 编排端到端跑通**。真实开源项目（textdistance）的 sprint-3 拓扑层 **3 路真并发翻译**全链验证：

```
分层(no-decompose 14模块6层) → 前置串行(types/utils/base→done)
  → libraries degrade 解门禁 → sprint3 三路真并发(独立worktree+隔离target)
  → 逐层 git merge(2 次真实 lib.rs 冲突，结构化合并解决)
  → 整组 cargo check/clippy/test 真门全绿
  → 推进 reviewing → batch-transition-done 两层 done
  → worktree 清理
```

- **3 个算法模块（simple/sequence_based/phonetic）真实迁移到 `done`**，迁移产物整组 `cargo check` + `clippy -D warnings` + `test` 全过（1536 行 Rust ← 719 行 Python）。
- **补齐 PR-4 mock harness 的固有边界**（codex I1）：真并发 worktree/target 隔离、worktree 内真自检、真实翻译产物合并 + 真实 merge 冲突 reconcile——均为确定性 CI harness 覆盖不到、只有真 LLM 演练能验证的环节。
- **PR-3/PR-4 接活的 `batch-transition-done` 首次真实项目运用**：3 模块一条命令 `reviewing → done`，`agent_done` substatus 语义验证正确。
- **暴露 3 项真实工具缺口**（见上表），对齐 memory「验收≠自测」——真实场景撞出 mock 测试照不到的问题。

**范围边界（本演练不覆盖，非缺陷）**：
- Phase A 忠实翻译未产单元测试（整组 `cargo test` 0 tests）——等价测试深度（差异测试录制源行为逐条断言）是 Sprint D 已验证的能力（jmespath 901/902），本演练聚焦并行编排链路，未重复。
- 未跑全量 6 层 + graduate——按「证明性:1 层并行到 done」的既定范围，取最能证明并行的 sprint-3 层，非全量迁移。
- headless safe-default 裁剪了 base 的 libraries 外部库加速路径（等价 `external=false`）——忠实翻译下的合理降级，非行为缺失。
