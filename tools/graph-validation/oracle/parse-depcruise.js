// 解析 dependency-cruiser 的 JSON 输出，提取「项目内部 import 边」。
//
// 输入：depcruise --output-type json 的结果（stdin 或文件参数）。
//   - module.source ：相对仓库根的导入方文件路径，如 src/internal/Observable.ts
//   - dep.resolved  ：被导入方。带 tsconfig 时为已解析路径；否则为原始 ./X specifier。
//   - dep.coreModule / couldNotResolve：辅助判断是否外部 / 无法解析。
//
// 输出（stdout，JSON）：{ edges: [[from,to]...], cycles: [[file...]...], files: [...] }
//   - 路径相对 src_root，已归一化（去扩展名 / posix / 去 index）。
//   - dependency-cruiser 不直接给环；环由下游 compare 从边集重算 SCC，这里 cycles 留空。
//
// 用法：node parse-depcruise.js <repo_root> <src_root> < depcruise.json

'use strict';

const fs = require('fs');
const path = require('path');
const { canonFile } = require('../compare/normalize.js');

function readStdin() {
  return fs.readFileSync(0, 'utf8');
}

function main() {
  const repoRoot = process.argv[2];
  const srcRoot = process.argv[3]; // 相对 repoRoot
  if (!repoRoot || !srcRoot) {
    process.stderr.write('用法: node parse-depcruise.js <repo_root> <src_root> < depcruise.json\n');
    process.exit(1);
  }
  const absSrc = path.resolve(repoRoot, srcRoot);

  const data = JSON.parse(readStdin());
  const modules = data.modules || [];

  // 第一遍：收集所有「位于 src_root 内」的内部文件，归一化为相对 src 的 key。
  const internalSet = new Set();
  for (const m of modules) {
    const abs = path.resolve(repoRoot, m.source);
    if (isInside(absSrc, abs)) {
      internalSet.add(canonFile(path.relative(absSrc, abs)));
    }
  }

  const edges = new Set();
  for (const m of modules) {
    const fromAbs = path.resolve(repoRoot, m.source);
    if (!isInside(absSrc, fromAbs)) continue;
    const fromKey = canonFile(path.relative(absSrc, fromAbs));
    if (!internalSet.has(fromKey)) continue;

    for (const dep of m.dependencies || []) {
      if (dep.coreModule) continue;
      const toAbs = resolveDep(repoRoot, m.source, dep);
      if (toAbs == null) continue;
      if (!isInside(absSrc, toAbs)) continue; // 外部 / 越界
      const toKey = canonFile(path.relative(absSrc, toAbs));
      if (!internalSet.has(toKey)) continue;
      if (fromKey === toKey) continue; // 自导入：边层面忽略（环检测另算）
      edges.add(`${fromKey}\t${toKey}`);
    }
  }

  out(edges, internalSet);
}

// 把 dep 解析为绝对路径。优先用 dep.resolved（若已是项目内相对/绝对路径），
// 否则按 import specifier 相对 module 目录解析；外部包返回 null。
function resolveDep(repoRoot, moduleSource, dep) {
  const spec = dep.resolved || dep.module || '';
  if (!spec) return null;

  // 已解析为仓库内相对路径（带扩展名），如 src/internal/Observable.ts
  if (!spec.startsWith('.') && !path.isAbsolute(spec)) {
    // 裸 specifier（外部包，如 'rxjs'、'tslib'）或 couldNotResolve 的裸名 → 外部
    if (dep.couldNotResolve) return null;
    // depcruise 解析成功时 resolved 是相对 repoRoot 的路径（不以 . 开头）
    const abs = path.resolve(repoRoot, spec);
    if (fileLikeExists(abs)) return abs;
    return null;
  }

  // 相对 specifier：相对导入方文件所在目录解析
  const fromDir = path.dirname(path.resolve(repoRoot, moduleSource));
  const base = path.resolve(fromDir, spec);
  return resolveWithExt(base);
}

// 给定无扩展名/可能是目录的 base，尝试补扩展名 / index 找到真实文件。
const CAND_EXTS = ['.ts', '.tsx', '.mts', '.cts', '.d.ts', '.js', '.jsx', '.mjs', '.cjs'];
function resolveWithExt(base) {
  if (fileLikeExists(base) && fs.statSync(base).isFile()) return base;
  for (const ext of CAND_EXTS) {
    if (fs.existsSync(base + ext)) return base + ext;
  }
  for (const ext of CAND_EXTS) {
    const idx = path.join(base, 'index' + ext);
    if (fs.existsSync(idx)) return idx;
  }
  return null;
}

function fileLikeExists(p) {
  try {
    return fs.existsSync(p);
  } catch {
    return false;
  }
}

function isInside(dir, file) {
  const rel = path.relative(dir, file);
  return rel === '' || (!rel.startsWith('..') && !path.isAbsolute(rel));
}

function out(edges, files) {
  const result = {
    edges: [...edges].map((e) => e.split('\t')).sort(cmpEdge),
    cycles: [], // 由 compare 从边集重算 SCC
    files: [...files].sort(),
  };
  process.stdout.write(JSON.stringify(result));
}

function cmpEdge(a, b) {
  return a[0] === b[0] ? (a[1] < b[1] ? -1 : a[1] > b[1] ? 1 : 0) : a[0] < b[0] ? -1 : 1;
}

main();
