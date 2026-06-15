#!/usr/bin/env python3
"""M2-PERF-BASE：rustmigrate 性能基线测量。

测量 `graph build` 在各 fixture 上的 wall-clock 时长（确定性指标），
落盘基线供 Sprint F「F6 ≤±10%」回归对比。

子命令：
  snapshot   测量并写入 baseline.json（覆盖）
  check      测量并对比 baseline.json，median 超 ±TOL% 时退出非零
  print      仅测量并打印当前结果（不读写 baseline）

设计说明见同目录 README.md。`graph build` 之外的「单模块翻译时长」
为 LLM 驱动、不可脚本化重现，本脚本不测，见 baseline.json 的
`module_translation` 段与 README §3。
"""
from __future__ import annotations

import json
import platform
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
BIN = REPO / "cli" / "target" / "release" / "rustmigrate"
BASELINE = Path(__file__).resolve().parent / "baseline.json"
FIXTURES = ["linear-deps", "diamond-deps", "circular-deps", "edge-cases"]

ITERATIONS = 50      # 计时采样次数
WARMUP = 5           # 丢弃的预热次数（冷启动/磁盘缓存）
TOLERANCE = 0.10     # F6 ±10% 门禁

# fixture 规模小（数十行），graph build 时长由进程启动主导（~20ms），
# 绝对值小、噪声相对大。median 用于对比以抑制偶发抖动；min 反映理论下界。


def measure_one(fixture: str) -> dict:
    """对单个 fixture 跑 ITERATIONS 次 graph build，返回统计 + 图规模锚点。"""
    src = REPO / "fixtures" / fixture
    if not src.is_dir():
        raise SystemExit(f"fixture 不存在: {src}")

    samples: list[float] = []
    node_count = edge_count = None
    # 在临时 cwd 运行，graph build 的 .rust-migration/ 产物落在临时目录，不污染仓库
    with tempfile.TemporaryDirectory(prefix=f"perf-{fixture}-") as tmp:
        cmd = [str(BIN), "graph", "build", "--root", str(src)]
        for i in range(WARMUP + ITERATIONS):
            t0 = time.perf_counter()
            proc = subprocess.run(
                cmd, cwd=tmp, capture_output=True, text=True
            )
            dt = (time.perf_counter() - t0) * 1000.0  # ms
            if proc.returncode != 0:
                raise SystemExit(
                    f"graph build 失败 ({fixture}): {proc.stderr or proc.stdout}"
                )
            if i == WARMUP:  # 从首个计时样本提取图规模锚点
                data = json.loads(proc.stdout).get("data", {})
                node_count = data.get("node_count")
                edge_count = data.get("edge_count")
            if i >= WARMUP:
                samples.append(dt)

    samples.sort()
    return {
        "node_count": node_count,
        "edge_count": edge_count,
        "iterations": ITERATIONS,
        "min_ms": round(samples[0], 3),
        "median_ms": round(statistics.median(samples), 3),
        "mean_ms": round(statistics.fmean(samples), 3),
        "p90_ms": round(samples[int(len(samples) * 0.9)], 3),
    }


def measure_all() -> dict:
    if not BIN.exists():
        raise SystemExit(
            f"release 二进制不存在: {BIN}\n请先运行 `just perf-baseline` 或 "
            "`cargo build --release --workspace`"
        )
    git = subprocess.run(
        ["git", "-C", str(REPO), "rev-parse", "--short", "HEAD"],
        capture_output=True, text=True,
    ).stdout.strip()
    return {
        "schema": "perf-baseline/v1",
        "metadata": {
            "git_commit": git,
            "binary": str(BIN.relative_to(REPO)),
            "profile": "release",
            "iterations": ITERATIONS,
            "warmup": WARMUP,
            "machine": f"{platform.system()} {platform.machine()}",
            "note": "时间戳由 git_commit 锚定；刷新基线请走 PR 并在 commit 记录环境",
        },
        "graph_build": {f: measure_one(f) for f in FIXTURES},
        "module_translation": {
            "status": "not_measured",
            "reason": (
                "单模块翻译为 LLM 驱动，时长受模型负载/网络/上下文影响，"
                "波动远超 ±10%，不可脚本化重现；M1 亦未留实测记录。"
            ),
            "protocol": (
                "Sprint F 端到端迁移时人工记录单模块 full 档循环 wall-clock，"
                "F6 对该项采用『数量级/趋势』而非严格 ±10%，见 README §3。"
            ),
            "median_ms": None,
        },
    }


def cmd_print() -> int:
    print(json.dumps(measure_all(), ensure_ascii=False, indent=2))
    return 0


def cmd_snapshot() -> int:
    result = measure_all()
    BASELINE.write_text(
        json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(f"✓ 基线已写入 {BASELINE.relative_to(REPO)} (commit {result['metadata']['git_commit']})")
    for f, m in result["graph_build"].items():
        print(f"  {f}: median {m['median_ms']}ms (min {m['min_ms']}, {m['node_count']}N/{m['edge_count']}E)")
    return 0


def cmd_check() -> int:
    if not BASELINE.exists():
        raise SystemExit(f"基线不存在: {BASELINE}，请先 `python3 measure.py snapshot`")
    base = json.loads(BASELINE.read_text(encoding="utf-8"))["graph_build"]
    cur = measure_all()["graph_build"]
    print(f"F6 graph build 回归对比（容差 ±{int(TOLERANCE * 100)}%，基于 median）")
    failed = False
    for f in FIXTURES:
        b, c = base.get(f, {}).get("median_ms"), cur[f]["median_ms"]
        if b is None:
            print(f"  {f}: 基线缺失，跳过")
            continue
        delta = (c - b) / b
        # 绝对值过小时（<5ms 基线）相对偏差无意义，仅提示
        flag = "OK"
        if abs(delta) > TOLERANCE:
            if b < 5.0:
                flag = "NOISE?(基线<5ms，相对偏差不可靠)"
            else:
                flag = "FAIL"
                failed = True
        # 图规模变了则时长不可比，单列警告
        if cur[f]["node_count"] != base.get(f, {}).get("node_count"):
            flag += " ⚠图规模变化"
        print(f"  {f}: {b}ms → {c}ms ({delta:+.1%}) {flag}")
    return 1 if failed else 0


def main() -> int:
    cmd = sys.argv[1] if len(sys.argv) > 1 else "print"
    dispatch = {"print": cmd_print, "snapshot": cmd_snapshot, "check": cmd_check}
    if cmd not in dispatch:
        raise SystemExit(f"未知子命令: {cmd}（可用: {', '.join(dispatch)}）")
    return dispatch[cmd]()


if __name__ == "__main__":
    sys.exit(main())
