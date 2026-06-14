// 从边集计算强连通分量（Tarjan 迭代版），用于在统一粒度上比较环。
//
// 输入：edges = [[from,to], ...]（已归一化的节点 key）。
// 输出：cycles = 含 >1 节点的 SCC（每个 SCC 内节点已排序），以及参与任一环的节点集合。
//
// 三方（自研图 / depcruise / dpdm）都用同一函数从各自边集重算 SCC，
// 消除「自研图给 SCC、dpdm 给环路径」的粒度差异。

'use strict';

function tarjanScc(edges) {
  const adj = new Map();
  const nodes = new Set();
  for (const [u, v] of edges) {
    nodes.add(u);
    nodes.add(v);
    if (!adj.has(u)) adj.set(u, []);
    adj.get(u).push(v);
  }

  let index = 0;
  const idx = new Map();
  const low = new Map();
  const onStack = new Set();
  const stack = [];
  const sccs = [];

  // 迭代式 Tarjan，避免大图递归栈溢出。
  for (const start of nodes) {
    if (idx.has(start)) continue;
    const work = [[start, 0]];
    while (work.length) {
      const frame = work[work.length - 1];
      const [v, pi] = frame;
      if (pi === 0) {
        idx.set(v, index);
        low.set(v, index);
        index += 1;
        stack.push(v);
        onStack.add(v);
      }
      const succ = adj.get(v) || [];
      if (pi < succ.length) {
        frame[1] = pi + 1;
        const w = succ[pi];
        if (!idx.has(w)) {
          work.push([w, 0]);
        } else if (onStack.has(w)) {
          low.set(v, Math.min(low.get(v), idx.get(w)));
        }
      } else {
        if (low.get(v) === idx.get(v)) {
          const comp = [];
          let w;
          do {
            w = stack.pop();
            onStack.delete(w);
            comp.push(w);
          } while (w !== v);
          sccs.push(comp);
        }
        work.pop();
        if (work.length) {
          const parent = work[work.length - 1][0];
          low.set(parent, Math.min(low.get(parent), low.get(v)));
        }
      }
    }
  }

  // 含自环的单节点 SCC 也算环（文件自导入）。
  const selfLoop = new Set();
  for (const [u, v] of edges) if (u === v) selfLoop.add(u);

  const cycles = sccs
    .filter((c) => c.length > 1 || (c.length === 1 && selfLoop.has(c[0])))
    .map((c) => [...c].sort());

  const cyclicNodes = new Set();
  for (const c of cycles) for (const n of c) cyclicNodes.add(n);

  return { cycles, cyclicNodes };
}

module.exports = { tarjanScc };
