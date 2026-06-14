// 差分对比核心：自研图 vs (dependency-cruiser ∩ dpdm)。
//
// 输入（命令行参数为 3 个归一化 JSON 文件 + 元信息）：
//   --self <self.json>        自研图 dump（dump_import_graph 输出，已转归一化形态）
//   --depcruise <dc.json>     parse-depcruise.js 输出
//   --dpdm <dpdm.json>        parse-dpdm.js 输出
//   --name <repo>             仓库名
//   --sha <sha> --src <root>  元信息（写进报告）
//   --out <report.md>         报告输出路径
//
// 硬门：对「depcruise ∩ dpdm」的边，自研图召回率 ≥ 0.98 且环集合一致。
//
// 报告写 Markdown；同时把机器可读结果打到 stdout（JSON 一行），供 run.sh 汇总。

'use strict';

const fs = require('fs');
const { tarjanScc } = require('./scc.js');

const RECALL_GATE = 0.98;

function parseArgs() {
  const a = {};
  const argv = process.argv.slice(2);
  for (let i = 0; i < argv.length; i += 2) a[argv[i].replace(/^--/, '')] = argv[i + 1];
  return a;
}

function loadEdges(file) {
  const d = JSON.parse(fs.readFileSync(file, 'utf8'));
  const set = new Set();
  for (const [u, v] of d.edges) set.add(`${u}\t${v}`);
  return { set, files: new Set(d.files || []), raw: d };
}

function main() {
  const args = parseArgs();
  const self = loadEdges(args.self);
  const dc = loadEdges(args.depcruise);
  const dpdm = loadEdges(args.dpdm);

  // Oracle 交集（双 oracle 都认可的边）= 校验基准。
  const oracleIntersect = new Set();
  for (const e of dc.set) if (dpdm.set.has(e)) oracleIntersect.add(e);

  // 召回：交集中有多少被自研图覆盖。
  const missing = [];
  let hit = 0;
  for (const e of oracleIntersect) {
    if (self.set.has(e)) hit += 1;
    else missing.push(e);
  }
  const total = oracleIntersect.size;
  // oracle 有效性：任一 oracle 边集为空（dpdm/depcruise 静默失败）或交集为空时，
  // 校验基准不可信，不得判过——否则 M1 验收门会假绿（见 code-review 发现）。
  const oracleValid = dc.set.size > 0 && dpdm.set.size > 0 && total > 0;
  const recall = oracleValid ? hit / total : 0;

  // 自研图多出的边（相对交集），按是否被任一 oracle 认可分类。
  const extraVsIntersect = [];
  for (const e of self.set) {
    if (!oracleIntersect.has(e)) {
      const inDc = dc.set.has(e);
      const inDpdm = dpdm.set.has(e);
      extraVsIntersect.push({ e, inDc, inDpdm });
    }
  }
  // 真正「两个 oracle 都不认」的多余边（最值得关注）
  const extraNeither = extraVsIntersect.filter((x) => !x.inDc && !x.inDpdm);

  // 环：三方各自从边集重算 SCC，比较「参与环的节点集合」。
  const selfScc = tarjanScc([...self.set].map((e) => e.split('\t')));
  const dcScc = tarjanScc([...dc.set].map((e) => e.split('\t')));
  const dpdmScc = tarjanScc([...dpdm.set].map((e) => e.split('\t')));

  // Oracle 环节点交集（双 oracle 都判为环上的节点）。
  const oracleCyclicNodes = new Set();
  for (const n of dcScc.cyclicNodes) if (dpdmScc.cyclicNodes.has(n)) oracleCyclicNodes.add(n);

  const cycNodeMissing = [...oracleCyclicNodes].filter((n) => !selfScc.cyclicNodes.has(n)).sort();
  const cycNodeExtra = [...selfScc.cyclicNodes]
    .filter((n) => !dcScc.cyclicNodes.has(n) && !dpdmScc.cyclicNodes.has(n))
    .sort();
  // 环集合一致 = 自研图的环上节点集合 ⊇ oracle 交集，且无双 oracle 都不认的多余环节点。
  const cyclesConsistent = cycNodeMissing.length === 0 && cycNodeExtra.length === 0;

  const recallPass = oracleValid && recall >= RECALL_GATE;
  const gatePass = oracleValid && recallPass && cyclesConsistent;

  const summary = {
    name: args.name,
    sha: args.sha,
    src: args.src,
    self_files: self.files.size,
    self_edges: self.set.size,
    dc_edges: dc.set.size,
    dpdm_edges: dpdm.set.size,
    oracle_intersect_edges: total,
    oracle_valid: oracleValid,
    recall: Number(recall.toFixed(4)),
    recall_pass: recallPass,
    missing_count: missing.length,
    extra_neither_count: extraNeither.length,
    self_cycle_nodes: selfScc.cyclicNodes.size,
    oracle_cycle_nodes: oracleCyclicNodes.size,
    cycle_nodes_missing: cycNodeMissing.length,
    cycle_nodes_extra: cycNodeExtra.length,
    cycles_consistent: cyclesConsistent,
    gate_pass: gatePass,
  };

  writeReport(args.out, summary, {
    missing,
    extraNeither,
    cycNodeMissing,
    cycNodeExtra,
    selfCycles: selfScc.cycles,
    dcScc,
    dpdmScc,
  });

  process.stdout.write(JSON.stringify(summary) + '\n');
}

function sample(arr, n) {
  return arr.slice(0, n);
}

function writeReport(out, s, detail) {
  const L = [];
  L.push(`# 源码图差分校验报告 — ${s.name}`);
  L.push('');
  L.push(`- 仓库 SHA：\`${s.sha}\``);
  L.push(`- src 根：\`${s.src}\``);
  L.push(`- 自研图：${s.self_files} 文件 / ${s.self_edges} import 边`);
  L.push(`- dependency-cruiser：${s.dc_edges} 边 · dpdm：${s.dpdm_edges} 边`);
  L.push(`- Oracle 交集（dc ∩ dpdm）：${s.oracle_intersect_edges} 边`);
  L.push('');
  L.push('## 硬门结果');
  L.push('');
  L.push(`| 指标 | 值 | 门槛 | 结果 |`);
  L.push(`|------|----|------|------|`);
  L.push(
    `| 边召回率（自研 ∩ vs oracle 交集） | **${s.recall}** | ≥ 0.98 | ${s.recall_pass ? '✅ 达标' : '❌ 不达标'} |`,
  );
  L.push(
    `| 环节点一致 | missing ${s.cycle_nodes_missing} / extra ${s.cycle_nodes_extra} | 双向为 0 | ${s.cycles_consistent ? '✅ 一致' : '❌ 不一致'} |`,
  );
  L.push('');
  L.push(`**综合硬门：${s.gate_pass ? '✅ 达标' : '❌ 不达标（见下方根因分析）'}**`);
  L.push('');
  if (!s.oracle_valid) {
    L.push(
      '> ⚠️ **oracle 无效**：dependency-cruiser 或 dpdm 边集/交集为空（很可能 oracle 工具静默失败），',
    );
    L.push('> 校验基准不可信，已强制判不达标，请检查 oracle 运行日志。');
    L.push('');
  }

  L.push('## 边召回明细');
  L.push('');
  L.push(`- oracle 交集边数：${s.oracle_intersect_edges}`);
  L.push(`- 自研图命中：${s.oracle_intersect_edges - s.missing_count}`);
  L.push(`- 缺失（oracle 交集有、自研图无）：${s.missing_count}`);
  if (detail.missing.length) {
    L.push('');
    L.push('缺失边样本（最多 30 条，`from -> to`）：');
    L.push('');
    L.push('```');
    for (const e of sample(detail.missing, 30)) L.push(e.replace('\t', ' -> '));
    L.push('```');
  }
  L.push('');
  L.push(
    `- 自研图多出且双 oracle 都不认的边：${s.extra_neither_count}（可能是自研误报或 oracle 漏报）`,
  );
  if (detail.extraNeither.length) {
    L.push('');
    L.push('多余边样本（最多 30 条）：');
    L.push('');
    L.push('```');
    for (const x of sample(detail.extraNeither, 30)) L.push(x.e.replace('\t', ' -> '));
    L.push('```');
  }
  L.push('');

  L.push('## 环对比');
  L.push('');
  L.push(`- 自研图环上节点数：${s.self_cycle_nodes}`);
  L.push(`- oracle 交集环上节点数：${s.oracle_cycle_nodes}`);
  L.push(`- 自研缺失的环节点：${s.cycle_nodes_missing}`);
  L.push(`- 自研多出（双 oracle 都不认）的环节点：${s.cycle_nodes_extra}`);
  if (detail.cycNodeMissing.length) {
    L.push('');
    L.push('缺失环节点样本：`' + sample(detail.cycNodeMissing, 30).join('`, `') + '`');
  }
  if (detail.cycNodeExtra.length) {
    L.push('');
    L.push('多出环节点样本：`' + sample(detail.cycNodeExtra, 30).join('`, `') + '`');
  }
  L.push('');
  L.push(`- 自研图检测到的 SCC（环）数：${detail.selfCycles.length}`);
  L.push('');

  if (!s.gate_pass) {
    L.push('## 根因分析（待人工核对填充）');
    L.push('');
    L.push('> 自动判定未达标。请按以下方向核对，区分「自研 bug / 归一化口径差 / oracle 噪声」：');
    L.push('> - 缺失边：检查是否为 barrel re-export、动态 import、tsconfig paths 别名；');
    L.push('> - 多余边：检查是否为 type-only / 注释内 import 误提取；');
    L.push('> - 环差异：检查是否因个别边缺失导致 SCC 断裂。');
    L.push('');
  }

  fs.writeFileSync(out, L.join('\n') + '\n');
}

main();
