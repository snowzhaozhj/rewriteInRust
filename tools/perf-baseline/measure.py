#!/usr/bin/env python3
"""M2-PERF-BASE：rustmigrate 性能基线测量。

测量 `graph build` 的 wall-clock 时长（确定性指标），落盘基线供
Sprint F「F6 ≤±10%」回归对比。两个维度：

- **真实项目**（rxjs/fp-ts/zod，数百文件）：~百毫秒级，解析逻辑占主导，
  区分度好 → **F6 回归硬门**（check 超 ±10% 退出非零）。
- **fixture**（linear/diamond/circular/edge，数十行）：~20ms 由进程启动
  主导、区分度低 → **仅冒烟参考**（check 打印偏差但不影响退出码）。

子命令：
  snapshot   测量并写入 baseline.json（覆盖）
  check      测量并对比 baseline.json，真实项目 median 超 ±TOL% 退出非零
  print      仅测量并打印当前结果（不读写 baseline）

真实项目仓库清单复用 `tools/graph-validation/repos.txt`（钉死 commit SHA），
克隆到 `tools/graph-validation/.work/<name>/repo`（与图校验共享，幂等）。
首次运行前需 clone，缺失时 check/snapshot 会提示。

`graph build` 之外的「单模块翻译时长」为 LLM 驱动、不可脚本化重现，
本脚本不测，见 baseline.json 的 `module_translation` 段与 README §3。
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
REPOS_TXT = REPO / "tools" / "graph-validation" / "repos.txt"
WORK_DIR = REPO / "tools" / "graph-validation" / ".work"

FIXTURE_ITER = 50    # fixture 采样次数（绝对值小，多采样压噪声）
FIXTURE_WARMUP = 5
REAL_ITER = 20       # 真实项目采样次数（~百毫秒级，稳定，少采样即可）
REAL_WARMUP = 3
TOLERANCE = 0.10     # F6 ±10% 门禁（仅真实项目硬判）


def time_build(src: Path, label: str, iterations: int, warmup: int) -> dict:
    """对 src 跑 warmup+iterations 次 graph build，返回统计 + 图规模锚点。

    在临时 cwd 运行，graph build 的 .rust-migration/ 产物落临时目录，不污染仓库。
    """
    if not src.is_dir():
        raise SystemExit(f"源码目录不存在: {src}")
    samples: list[float] = []
    node_count = edge_count = None
    with tempfile.TemporaryDirectory(prefix=f"perf-{label}-") as tmp:
        cmd = [str(BIN), "graph", "build", "--root", str(src)]
        for i in range(warmup + iterations):
            t0 = time.perf_counter()
            proc = subprocess.run(cmd, cwd=tmp, capture_output=True, text=True)
            dt = (time.perf_counter() - t0) * 1000.0  # ms
            if proc.returncode != 0:
                raise SystemExit(
                    f"graph build 失败 ({label}): {proc.stderr or proc.stdout}"
                )
            if i == warmup:  # 从首个计时样本提取图规模锚点
                data = json.loads(proc.stdout).get("data", {})
                node_count = data.get("node_count")
                edge_count = data.get("edge_count")
            if i >= warmup:
                samples.append(dt)
    samples.sort()
    return {
        "node_count": node_count,
        "edge_count": edge_count,
        "iterations": iterations,
        "min_ms": round(samples[0], 3),
        "median_ms": round(statistics.median(samples), 3),
        "mean_ms": round(statistics.fmean(samples), 3),
        "p90_ms": round(samples[int(len(samples) * 0.9)], 3),
    }


def parse_repos() -> list[dict]:
    """解析 graph-validation/repos.txt：每行 `name url sha src_root`，# 注释/空行跳过。"""
    repos = []
    for line in REPOS_TXT.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split()
        if len(parts) != 4:
            continue
        name, url, sha, src_root = parts
        repos.append({"name": name, "url": url, "sha": sha, "src_root": src_root})
    return repos


def ensure_repo(repo: dict) -> Path:
    """确保仓库 clone 到 .work/<name>/repo 并 checkout 钉死 sha（幂等）。返回 src 路径。"""
    repo_dir = WORK_DIR / repo["name"] / "repo"
    if not (repo_dir / ".git").is_dir():
        raise SystemExit(
            f"真实项目仓库未克隆: {repo_dir}\n"
            f"请先克隆：\n"
            f"  mkdir -p {repo_dir.parent} && \\\n"
            f"  git clone {repo['url']} {repo_dir} && \\\n"
            f"  git -C {repo_dir} checkout {repo['sha']}\n"
            f"（或运行 `just validate-graph {repo['name']}` 一并克隆）"
        )
    # 幂等校验 sha（本地操作，无需网络）
    subprocess.run(
        ["git", "-C", str(repo_dir), "checkout", "--quiet", repo["sha"]],
        check=True, capture_output=True, text=True,
    )
    return repo_dir / repo["src_root"]


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

    real = {}
    for repo in parse_repos():
        src = ensure_repo(repo)
        m = time_build(src, repo["name"], REAL_ITER, REAL_WARMUP)
        m["sha"] = repo["sha"][:12]
        m["src_root"] = repo["src_root"]
        real[repo["name"]] = m

    return {
        "schema": "perf-baseline/v2",
        "metadata": {
            "git_commit": git,
            "binary": str(BIN.relative_to(REPO)),
            "profile": "release",
            "fixture_iterations": FIXTURE_ITER,
            "real_iterations": REAL_ITER,
            "machine": f"{platform.system()} {platform.machine()}",
            "note": "时间戳由 git_commit 锚定；刷新基线请走 PR 并在 commit 记录环境。"
                    "跨机对比有系统偏差，F6 须同机跑 baseline 与 check。",
        },
        # 真实项目：F6 回归硬门（百毫秒级，区分度好）
        "graph_build_real": real,
        # fixture：仅冒烟参考（~20ms 由进程启动主导，不纳入硬判）
        "graph_build_fixture": {
            f: time_build(REPO / "fixtures" / f, f, FIXTURE_ITER, FIXTURE_WARMUP)
            for f in FIXTURES
        },
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


def _print_group(title: str, group: dict) -> None:
    print(title)
    for name, m in group.items():
        tag = f" @{m['sha']}" if "sha" in m else ""
        print(f"  {name}{tag}: median {m['median_ms']}ms "
              f"(min {m['min_ms']}, p90 {m['p90_ms']}, {m['node_count']}N/{m['edge_count']}E)")


def cmd_print() -> int:
    print(json.dumps(measure_all(), ensure_ascii=False, indent=2))
    return 0


def cmd_snapshot() -> int:
    result = measure_all()
    BASELINE.write_text(
        json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(f"✓ 基线已写入 {BASELINE.relative_to(REPO)} (commit {result['metadata']['git_commit']})")
    _print_group("真实项目（F6 硬门）:", result["graph_build_real"])
    _print_group("fixture（冒烟参考）:", result["graph_build_fixture"])
    return 0


def _compare(group_base: dict, group_cur: dict, names: list, hard: bool) -> bool:
    """对比一组，返回是否有硬门 FAIL。"""
    failed = False
    for name in names:
        b = group_base.get(name, {}).get("median_ms")
        c = group_cur[name]["median_ms"]
        if b is None:
            print(f"  {name}: 基线缺失，跳过")
            continue
        delta = (c - b) / b
        flag = "OK"
        if abs(delta) > TOLERANCE:
            if not hard:
                flag = "[冒烟·不判]"
            elif b < 5.0:
                flag = "NOISE?(基线<5ms，相对偏差不可靠)"
            else:
                flag = "FAIL"
                failed = True
        if group_cur[name]["node_count"] != group_base.get(name, {}).get("node_count"):
            flag += " ⚠图规模变化（时长不可比）"
        print(f"  {name}: {b}ms → {c}ms ({delta:+.1%}) {flag}")
    return failed


def cmd_check() -> int:
    if not BASELINE.exists():
        raise SystemExit(f"基线不存在: {BASELINE}，请先 `python3 measure.py snapshot`")
    base = json.loads(BASELINE.read_text(encoding="utf-8"))
    cur = measure_all()
    print(f"F6 graph build 回归对比（容差 ±{int(TOLERANCE * 100)}%，基于 median）\n")
    print("真实项目（硬门，超容差 FAIL）:")
    failed = _compare(base.get("graph_build_real", {}), cur["graph_build_real"],
                      list(cur["graph_build_real"]), hard=True)
    print("\nfixture（冒烟参考，不影响退出码）:")
    _compare(base.get("graph_build_fixture", {}), cur["graph_build_fixture"],
             FIXTURES, hard=False)
    print()
    if failed:
        print("✗ 真实项目存在超容差退化")
    else:
        print("✓ 真实项目均在容差内")
    return 1 if failed else 0


def main() -> int:
    cmd = sys.argv[1] if len(sys.argv) > 1 else "print"
    dispatch = {"print": cmd_print, "snapshot": cmd_snapshot, "check": cmd_check}
    if cmd not in dispatch:
        raise SystemExit(f"未知子命令: {cmd}（可用: {', '.join(dispatch)}）")
    return dispatch[cmd]()


if __name__ == "__main__":
    sys.exit(main())
