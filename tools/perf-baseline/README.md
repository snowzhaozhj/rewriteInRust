# 性能基线（M2-PERF-BASE）

为 Sprint F 的 **F6「性能无退化 ≤±10%」** 验收门提供 M1 基线。无基线则 F6 不可测。

## §1 测量什么

| 指标 | 是否脚本化 | 说明 |
|------|-----------|------|
| `graph build` 时长 | ✅ 确定性 | 各 fixture 跑 N 次取统计，本工具落盘 |
| 单模块翻译时长 | ❌ 不可重现 | LLM 驱动，见 §3 |

## §2 graph build 基线

方法学（`measure.py`）：

- **构建**：release profile（`cargo build --release`）——debug 时长无参考意义
- **采样**：每 fixture `WARMUP=5` 次预热（丢弃）+ `ITERATIONS=50` 次计时，`time.perf_counter()` 毫秒级
- **隔离**：每次在临时 cwd 运行，`graph build` 产物 `.rust-migration/` 落临时目录，不污染仓库
- **对比口径**：以 **median** 判 ±10%（抑制偶发抖动）；同时记录 min/mean/p90 + 图规模锚点（node/edge count）
- **图规模守卫**：check 时若 node_count 相对基线变化，时长不可比，单独告警

### 局限（必读）

fixture 规模仅数十行，单次 `graph build` ≈ **22ms，绝对值由进程启动 + SQLite 建库主导**，解析逻辑本身 <1ms。因此当前基线：

- ✅ 能捕捉**数量级退化**（误引 O(N²)、意外重 IO、进程级回归）
- ⚠️ 对解析热路径的**细粒度退化区分度低**（被启动开销淹没）
- 守卫：基线 median <5ms 时相对偏差不可靠，check 标 `NOISE?` 而非 FAIL

待 Sprint F 引入 5K–20K 行真实项目后，可加大项目基线提升区分度（届时刷新 baseline.json）。

## §3 单模块翻译时长——为何不测

翻译由 LLM（SubAgent）驱动，时长受模型负载、网络、上下文长度影响，**波动远超 ±10%，不可脚本化重现**；M1 阶段亦未留实测记录（无历史基线可补）。

`baseline.json` 的 `module_translation` 段记为 `not_measured`，并约定协议：

> Sprint F 端到端迁移时人工记录单模块 full 档完整循环 wall-clock，写入 `baseline.json`。F6 对该项采用**数量级/趋势**判据，而非严格 ±10%。

这与 PLAN-M2 §1.2 验收表对「吞吐为 M2 新指标、无 M1 基线」的处理一致。

## §4 用法

```bash
just perf-baseline          # release 构建 + 测量并写入 baseline.json（刷新基线，走 PR）
just perf-baseline-check    # 测量并对比基线，median 超 ±10% 退出非零（F6 回归门）

# 或直接：
python3 tools/perf-baseline/measure.py print      # 仅打印当前测量，不读写基线
python3 tools/perf-baseline/measure.py snapshot   # 写基线
python3 tools/perf-baseline/measure.py check       # 对比基线
```

## §5 文件

- `measure.py` — 测量脚本（python3，无第三方依赖）
- `baseline.json` — M1 基线快照（schema `perf-baseline/v1`，含 git_commit + 环境元数据）
- `README.md` — 本文件

刷新基线须走 PR，commit message 记录测量环境（机器、commit）。跨机器对比有系统性偏差——F6 回归应在**同一台机器**上跑 baseline 与 check。
