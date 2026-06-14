// 解析 dpdm 的 JSON 输出，提取「项目内部 import 边」+ 环。
//
// 输入：dpdm -o <file> 的结果（stdin 或文件）。dpdm 从 cwd（=repo 根）运行，
//   - tree[file] = [{ issuer, request, kind, id }]
//       id：已解析的相对 cwd 路径（如 src/internal/Observable.ts）；外部包为 null。
//   - circulars：[[file, file, ...], ...] 每个是一条环路径（非 SCC，可能重叠）。
//
// 输出（stdout，JSON）：{ edges:[[from,to]...], cycles:[[file...]...], files:[...] }
//   路径相对 src_root，已归一化。
//
// 用法：node parse-dpdm.js <repo_root> <src_root> < dpdm.json

'use strict';

const fs = require('fs');
const path = require('path');
const { canonFile } = require('../compare/normalize.js');

function main() {
  const repoRoot = process.argv[2];
  const srcRoot = process.argv[3];
  if (!repoRoot || !srcRoot) {
    process.stderr.write('用法: node parse-dpdm.js <repo_root> <src_root> < dpdm.json\n');
    process.exit(1);
  }
  const absSrc = path.resolve(repoRoot, srcRoot);

  const data = JSON.parse(fs.readFileSync(0, 'utf8'));
  const tree = data.tree || {};
  const circulars = data.circulars || [];

  // 内部文件集合：tree 的 key 中位于 src_root 内的。
  const internalSet = new Set();
  for (const f of Object.keys(tree)) {
    const key = toSrcKey(repoRoot, absSrc, f);
    if (key != null) internalSet.add(key);
  }

  const edges = new Set();
  for (const [file, deps] of Object.entries(tree)) {
    const fromKey = toSrcKey(repoRoot, absSrc, file);
    if (fromKey == null) continue;
    for (const dep of deps || []) {
      if (!dep || dep.id == null) continue; // 外部包 / 未解析
      const toKey = toSrcKey(repoRoot, absSrc, dep.id);
      if (toKey == null) continue;
      if (fromKey === toKey) continue; // 自导入边层面忽略
      edges.add(`${fromKey}\t${toKey}`);
    }
  }

  // 环：把每条 circular 路径的成员归一化为 src 内 key（剔除越界成员）。
  const cycles = [];
  for (const cyc of circulars) {
    const members = cyc
      .map((f) => toSrcKey(repoRoot, absSrc, f))
      .filter((k) => k != null && internalSet.has(k));
    if (members.length >= 2) cycles.push([...members].sort());
  }

  const result = {
    edges: [...edges].map((e) => e.split('\t')).sort(cmpEdge),
    cycles: dedupCycles(cycles),
    files: [...internalSet].sort(),
  };
  process.stdout.write(JSON.stringify(result));
}

// dpdm 的路径相对 repoRoot；转为相对 src_root 的归一化 key，越界返回 null。
function toSrcKey(repoRoot, absSrc, relToRepo) {
  const abs = path.resolve(repoRoot, relToRepo);
  const rel = path.relative(absSrc, abs);
  if (rel.startsWith('..') || path.isAbsolute(rel)) return null;
  return canonFile(rel);
}

function dedupCycles(cycles) {
  const seen = new Set();
  const out = [];
  for (const c of cycles) {
    const k = c.join('|');
    if (!seen.has(k)) {
      seen.add(k);
      out.push(c);
    }
  }
  return out.sort((a, b) => a.join('|').localeCompare(b.join('|')));
}

function cmpEdge(a, b) {
  return a[0] === b[0] ? (a[1] < b[1] ? -1 : a[1] > b[1] ? 1 : 0) : a[0] < b[0] ? -1 : 1;
}

main();
