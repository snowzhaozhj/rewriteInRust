// 把 dump_import_graph 的输出归一化为统一比较形态 { edges:[[u,v]], cycles, files }。
//
// dump_import_graph 的 from/to 已是「相对 src_root」的 posix 路径（含扩展名），
// 这里只需套用与 oracle 完全相同的 canonFile（去扩展名 / 去 index）。
//
// 用法：node normalize-self.js < self-dump.json

'use strict';

const fs = require('fs');
const { canonFile } = require('./normalize.js');

const d = JSON.parse(fs.readFileSync(0, 'utf8'));

const fileSet = new Set();
const edgeSet = new Set();
for (const e of d.edges || []) {
  const from = canonFile(e.from);
  const to = canonFile(e.to);
  fileSet.add(from);
  fileSet.add(to);
  if (from !== to) edgeSet.add(`${from}\t${to}`);
}

const cycles = (d.cycles || []).map((c) => c.map(canonFile).sort());

const result = {
  edges: [...edgeSet]
    .map((e) => e.split('\t'))
    .sort((a, b) => (a[0] === b[0] ? a[1].localeCompare(b[1]) : a[0].localeCompare(b[0]))),
  cycles,
  files: [...fileSet].sort(),
};
process.stdout.write(JSON.stringify(result));
