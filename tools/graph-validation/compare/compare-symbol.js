// 符号级精度对比：自研启发式 Calls/Extends/Implements vs ts-morph oracle（类型检查器真值）。
//
// 输入（命令行）：
//   --self <self-symbol-dump.json>  dump_symbol_graph 输出
//   --oracle <tsmorph.json>         symbol-graph-tsmorph.js 输出
//   --name <repo> --sha <sha> --src <root>
//   --out <report.md>
//
// === 口径设计（关键） ===
// 跨系统「符号 ID」对齐难：自研 caller 侧只有文件（不追踪 enclosing 函数），
// callee 符号名可能带命名空间前缀 / 别名；ts-morph caller 侧有 enclosing 符号、
// callee 是解析后的定义名。两侧符号名空间不一致，直接逐符号比 = 噪声淹没信号。
//
// 故采用「文件级聚合先行」：
//   - Calls   → (caller_file, callee_file)，忽略符号名，忽略构造/普通区分。
//   - Extends → (child_file, parent_file)；Implements 同理。
// 这把跨系统对齐难度降到与已验证的 import 图同一量级（纯文件对），快速得到
// 「自研启发式相对类型检查器真值的文件级 precision/recall/F1」。
//
// 符号级精确对比（caller_symbol/callee_symbol 全匹配）作为 stretch：
//   - 自研 caller_symbol 恒为文件（无符号），故 caller 侧符号对齐天然不可能 → 只能比 callee 符号。
//   - 下面给出「callee 符号名集合（按文件分组）」的弱对比作为参考，不计入软门。
//
// === 软门（非硬门，不阻断）===
//   对 Calls / Extends / Implements 三类各算 precision/recall/F1（以 oracle 为真值）。
//   F1 < WARN_F1 时在报告里标注「⚠️ 启发式效果偏低」，但退出码恒 0（spike 不阻断 CI）。

'use strict';

const fs = require('fs');

const WARN_F1 = 0.7; // 软门警示阈值

function parseArgs() {
  const a = {};
  const argv = process.argv.slice(2);
  for (let i = 0; i < argv.length; i += 2) a[argv[i].replace(/^--/, '')] = argv[i + 1];
  return a;
}

// ---- 自研侧：路径已含扩展名，需归一化到与 oracle 同口径（去扩展名 / 去 index）----
const TS_EXTS = ['.d.ts', '.tsx', '.mts', '.cts', '.ts', '.jsx', '.mjs', '.cjs', '.js'];
function canonFile(p) {
  if (p == null) return null;
  let s = String(p).split('\\').join('/');
  for (const ext of TS_EXTS.slice().sort((a, b) => b.length - a.length)) {
    if (s.endsWith(ext)) {
      s = s.slice(0, -ext.length);
      break;
    }
  }
  if (s.endsWith('/index')) s = s.slice(0, -'/index'.length);
  while (s.startsWith('./')) s = s.slice(2);
  return s;
}

// 文件级边集（忽略自环：自研 Calls 不产同文件边，oracle 也已剔同文件，双保险）。
function selfCallFilePairs(selfDump) {
  const set = new Set();
  for (const c of selfDump.calls || []) {
    const from = canonFile(c.caller_file);
    const to = canonFile(c.callee && c.callee.file);
    if (from && to && from !== to) set.add(`${from}\t${to}`);
  }
  return set;
}
function oracleCallFilePairs(oracle) {
  const set = new Set();
  for (const c of oracle.calls || []) {
    const from = canonFile(c.caller_file);
    const to = canonFile(c.callee_file);
    if (from && to && from !== to) set.add(`${from}\t${to}`);
  }
  return set;
}
function selfHeritageFilePairs(selfDump, field) {
  const set = new Set();
  for (const h of selfDump[field] || []) {
    const from = canonFile(h.child && h.child.file);
    const to = canonFile(h.parent && h.parent.file);
    if (from && to && from !== to) set.add(`${from}\t${to}`);
  }
  return set;
}
function oracleHeritageFilePairs(oracle, field) {
  const set = new Set();
  for (const h of oracle[field] || []) {
    const from = canonFile(h.child_file);
    const to = canonFile(h.parent_file);
    if (from && to && from !== to) set.add(`${from}\t${to}`);
  }
  return set;
}

// precision/recall/F1：self 为预测，oracle 为真值。
function prf(selfSet, oracleSet) {
  let tp = 0;
  for (const e of selfSet) if (oracleSet.has(e)) tp += 1;
  const fp = selfSet.size - tp; // 自研有、oracle 无
  const fn = oracleSet.size - tp; // oracle 有、自研无
  const precision = selfSet.size ? tp / selfSet.size : oracleSet.size ? 0 : 1;
  const recall = oracleSet.size ? tp / oracleSet.size : selfSet.size ? 0 : 1;
  const f1 = precision + recall ? (2 * precision * recall) / (precision + recall) : 0;
  return {
    tp,
    fp,
    fn,
    self_count: selfSet.size,
    oracle_count: oracleSet.size,
    precision: Number(precision.toFixed(4)),
    recall: Number(recall.toFixed(4)),
    f1: Number(f1.toFixed(4)),
  };
}

function diffSamples(selfSet, oracleSet, n = 20) {
  const missing = []; // oracle 有、自研无（漏报）
  const extra = []; // 自研有、oracle 无（误报）
  for (const e of oracleSet) if (!selfSet.has(e)) missing.push(e);
  for (const e of selfSet) if (!oracleSet.has(e)) extra.push(e);
  missing.sort();
  extra.sort();
  return { missing: missing.slice(0, n), extra: extra.slice(0, n), missingTotal: missing.length, extraTotal: extra.length };
}

function main() {
  const args = parseArgs();
  const self = JSON.parse(fs.readFileSync(args.self, 'utf8'));
  const oracle = JSON.parse(fs.readFileSync(args.oracle, 'utf8'));

  const blocks = {
    calls: {
      label: 'Calls（函数调用）',
      self: selfCallFilePairs(self),
      oracle: oracleCallFilePairs(oracle),
    },
    extends: {
      label: 'Extends（继承）',
      self: selfHeritageFilePairs(self, 'extends'),
      oracle: oracleHeritageFilePairs(oracle, 'extends'),
    },
    implements: {
      label: 'Implements（接口实现）',
      self: selfHeritageFilePairs(self, 'implements'),
      oracle: oracleHeritageFilePairs(oracle, 'implements'),
    },
  };

  const results = {};
  for (const k of Object.keys(blocks)) {
    const m = prf(blocks[k].self, blocks[k].oracle);
    const d = diffSamples(blocks[k].self, blocks[k].oracle);
    results[k] = { metrics: m, diff: d, label: blocks[k].label };
  }

  // stretch：callee 符号集合弱对比（仅 Calls；按 (callee_file) 分组比符号名集合）。
  const calleeSymbolNote = stretchSymbolNote(self, oracle);

  const summary = {
    name: args.name,
    sha: args.sha,
    src: args.src,
    level: 'file-aggregated',
    calls: results.calls.metrics,
    extends: results.extends.metrics,
    implements: results.implements.metrics,
    warn_f1: WARN_F1,
    calls_warn: results.calls.metrics.f1 < WARN_F1,
    extends_warn: results.extends.metrics.oracle_count > 0 && results.extends.metrics.f1 < WARN_F1,
    implements_warn:
      results.implements.metrics.oracle_count > 0 && results.implements.metrics.f1 < WARN_F1,
    soft_gate: true,
  };

  writeReport(args.out, summary, results, calleeSymbolNote);
  process.stdout.write(JSON.stringify(summary) + '\n');
  // 软门：恒 0 退出，不阻断（spike 性质）。
  process.exit(0);
}

// callee 符号名重合度（stretch，参考用）：两侧各取 callee 符号名集合，算 Jaccard。
function stretchSymbolNote(self, oracle) {
  const selfSyms = new Set();
  for (const c of self.calls || []) {
    if (c.callee && c.callee.symbol) {
      // 剥离命名空间前缀（自研可能输出 `ns.foo`），取最后一段
      const s = String(c.callee.symbol).split('.').pop();
      if (s) selfSyms.add(s);
    }
  }
  const oracleSyms = new Set();
  for (const c of oracle.calls || []) if (c.callee_symbol) oracleSyms.add(c.callee_symbol);
  let inter = 0;
  for (const s of selfSyms) if (oracleSyms.has(s)) inter += 1;
  const union = new Set([...selfSyms, ...oracleSyms]).size;
  return {
    self_callee_symbols: selfSyms.size,
    oracle_callee_symbols: oracleSyms.size,
    intersect: inter,
    jaccard: union ? Number((inter / union).toFixed(4)) : 0,
  };
}

function fmtRow(label, m) {
  const warn = m.f1 < WARN_F1 && m.oracle_count > 0 ? ' ⚠️' : '';
  return `| ${label} | ${m.self_count} | ${m.oracle_count} | ${m.tp} | ${m.precision} | ${m.recall} | **${m.f1}**${warn} |`;
}

function writeReport(out, s, results, symNote) {
  const L = [];
  L.push(`# 符号级精度差分报告（文件级聚合）— ${s.name}`);
  L.push('');
  L.push(`- 仓库 SHA：\`${s.sha}\`　src 根：\`${s.src}\``);
  L.push(`- oracle：ts-morph 类型检查器（真值）　预测：自研 tree-sitter 启发式`);
  L.push(`- 对比口径：**文件级聚合**（caller_file→callee_file，忽略符号名）`);
  L.push(`- 软门：F1 < ${s.warn_f1} 标注「⚠️ 启发式效果偏低」，**不阻断**（退出码恒 0）`);
  L.push('');
  L.push('## 文件级 precision / recall / F1（以 ts-morph 为真值）');
  L.push('');
  L.push('| 关系 | 自研边数 | oracle 边数 | 命中(TP) | precision | recall | F1 |');
  L.push('|------|---------|------------|---------|-----------|--------|----|');
  L.push(fmtRow(results.calls.label, results.calls.metrics));
  L.push(fmtRow(results.extends.label, results.extends.metrics));
  L.push(fmtRow(results.implements.label, results.implements.metrics));
  L.push('');
  L.push('> precision = 自研边中被 oracle 认可的比例（误报越少越高）；');
  L.push('> recall = oracle 边中被自研覆盖的比例（漏报越少越高）。');
  L.push('');

  for (const k of ['calls', 'extends', 'implements']) {
    const r = results[k];
    const m = r.metrics;
    if (m.self_count === 0 && m.oracle_count === 0) continue;
    L.push(`## ${r.label} 明细`);
    L.push('');
    L.push(`- 自研 ${m.self_count} 边 / oracle ${m.oracle_count} 边 / 命中 ${m.tp}`);
    L.push(`- 漏报（oracle 有、自研无）：${r.diff.missingTotal}　误报（自研有、oracle 无）：${r.diff.extraTotal}`);
    if (r.diff.missing.length) {
      L.push('');
      L.push('漏报样本（最多 20，`from -> to`）：');
      L.push('```');
      for (const e of r.diff.missing) L.push(e.replace('\t', ' -> '));
      L.push('```');
    }
    if (r.diff.extra.length) {
      L.push('');
      L.push('误报样本（最多 20）：');
      L.push('```');
      for (const e of r.diff.extra) L.push(e.replace('\t', ' -> '));
      L.push('```');
    }
    L.push('');
  }

  L.push('## 符号级 stretch（参考，不计入软门）');
  L.push('');
  L.push('自研 caller 侧无 enclosing 符号（Calls 边 source 是文件节点），caller 符号无法对齐；');
  L.push('此处仅给 callee 符号名集合的 Jaccard 重合度作弱参考：');
  L.push('');
  L.push(`- 自研 callee 符号名：${symNote.self_callee_symbols}　oracle callee 符号名：${symNote.oracle_callee_symbols}`);
  L.push(`- 交集：${symNote.intersect}　Jaccard：${symNote.jaccard}`);
  L.push('');
  L.push('> 符号级精确对比的口径对齐难点见 `tools/graph-validation/SYMBOL-PRECISION.md`。');
  L.push('');

  const anyWarn = s.calls_warn || s.extends_warn || s.implements_warn;
  L.push('## 软门结论');
  L.push('');
  if (anyWarn) {
    L.push('⚠️ 存在 F1 < 阈值的关系类别（属预期：启发式精度必然低于类型系统）。');
    L.push('请按下列方向区分「自研可改进 / 口径差异 / 启发式固有局限」：');
    L.push('- Calls 漏报：跨文件方法调用 `obj.method()`（自研只解析顶层函数/构造）、');
    L.push('  re-export 链、命名空间深层调用、回调/高阶传递；');
    L.push('- Calls 误报：同名不同模块的兜底匹配命中错误文件；');
    L.push('- Extends/Implements 漏报：跨多层 barrel 的基类型、泛型基类、外部基类型（已剔）。');
  } else {
    L.push('✅ 各关系类别 F1 均达警示阈值以上。');
  }
  L.push('');
  fs.writeFileSync(out, L.join('\n') + '\n');
}

main();
