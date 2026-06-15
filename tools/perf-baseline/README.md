# 性能基线（M2-PERF-BASE）

为 Sprint F 的 **F6「性能无退化 ≤±10%」** 验收门提供 M1 基线。无基线则 F6 不可测。

## §1 测量什么

| 指标 | 是否脚本化 | 角色 |
|------|-----------|------|
| `graph build` — **真实项目**（rxjs/fp-ts/zod） | ✅ 确定性 | **F6 回归硬门** |
| `graph build` — fixture（linear/diamond/circular/edge） | ✅ 确定性 | 仅冒烟参考 |
| 单模块翻译时长 | ❌ 不可重现 | LLM 驱动，见 §4 |

## §2 真实项目基线（F6 硬门）

fixture 仅数十行，`graph build` ≈22ms **几乎全是进程启动 + SQLite 建库开销**，解析逻辑 <1ms，对回归区分度极低。故基线主体用**真实公开 TS 项目**——数百文件、百毫秒级、解析逻辑占主导：

| 项目 | 规模（node/edge） | median | 特点 |
|------|------|--------|------|
| fp-ts | 2365 / 5828 | ~314ms | 高度互递归（环极多），最大样本，区分度最好 |
| rxjs | 807 / 2168 | ~99ms | 含多个真实循环依赖 |
| zod | 538 / 1207 | ~99ms | 结构扁平 |

±10% 容差在 fp-ts 上 = ±31ms，远大于测量噪声 → **能可靠捕捉解析热路径退化**。

仓库清单复用图校验的 `tools/graph-validation/repos.txt`（钉死 commit SHA，可复现），克隆到 `tools/graph-validation/.work/<name>/repo`（与图校验**共享** clone，已 gitignore）。

### 方法学（`measure.py`）

- **构建**：release profile——debug 时长无参考意义
- **采样**：真实项目 `WARMUP=3 + ITERATIONS=20`（百毫秒级稳定）；fixture `5+50`（压噪声）
- **隔离**：每次在临时 cwd 运行，`.rust-migration/` 产物落临时目录，不污染仓库
- **对比口径**：以 **median** 判 ±10%；同时记录 min/mean/p90 + 图规模锚点（node/edge count）
- **图规模守卫**：check 时若 node_count 相对基线变化，时长不可比，单独告警
- **退出码**：仅**真实项目**超容差才 FAIL（退出非零）；fixture 仅打印偏差，`[冒烟·不判]`

## §3 fixture 基线（冒烟参考）

保留 4 个 fixture 的 graph build 时长，但**不纳入硬判**——绝对值 ≈22ms 被进程启动开销淹没，相对偏差不可靠。仅作快速冒烟（确认命令未崩、数量级未爆）。

## §4 单模块翻译时长——为何不测

翻译由 LLM（SubAgent）驱动，时长受模型负载、网络、上下文长度影响，**波动远超 ±10%，不可脚本化重现**；M1 阶段亦未留实测记录。

`baseline.json` 的 `module_translation` 段记为 `not_measured`，约定协议：

> Sprint F 端到端迁移时人工记录单模块 full 档完整循环 wall-clock，写入 `baseline.json`。F6 对该项采用**数量级/趋势**判据，而非严格 ±10%。

与 PLAN-M2 §1.2 对「吞吐为 M2 新指标、无 M1 基线」的处理一致。

## §5 用法

```bash
just perf-baseline          # release 构建 + 测量并写入 baseline.json（刷新基线，走 PR）
just perf-baseline-check    # 测量并对比基线，真实项目超 ±10% 退出非零（F6 回归门）

# 或直接：
python3 tools/perf-baseline/measure.py print      # 仅打印当前测量，不读写基线
python3 tools/perf-baseline/measure.py snapshot   # 写基线
python3 tools/perf-baseline/measure.py check       # 对比基线
```

### 首次准备真实项目仓库

`measure.py` 不主动联网 clone（避免与测量耦合）；仓库缺失时会打印克隆命令。最简单：

```bash
just validate-graph rxjs    # 图校验会一并 clone 到共享 .work/
# 或手动按 measure.py 的提示 git clone + checkout 钉死 SHA
```

三个仓库（rxjs/fp-ts/zod）首次 clone 后即驻留 `.work/`，后续测量仅本地 checkout，无需联网。

## §6 文件

- `measure.py` — 测量脚本（python3，无第三方依赖；schema `perf-baseline/v2`）
- `baseline.json` — M1 基线快照（含 git_commit + 环境元数据 + 各项目 SHA）
- `README.md` — 本文件

刷新基线须走 PR，commit 记录测量环境（机器、commit）。跨机对比有系统性偏差——F6 回归须在**同一台机器**上跑 baseline 与 check。
