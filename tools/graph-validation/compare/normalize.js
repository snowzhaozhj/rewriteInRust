// 归一化规则 —— 自研图与 oracle 两侧共用同一套实现，避免口径漂移。
//
// 一条「import 边」归一化为 `from\tto` 字符串，其中 from/to 均为：
//   - 相对 src_root 的路径
//   - posix 分隔符（/）
//   - 去掉 TS 扩展名（.ts/.tsx/.mts/.cts/.d.ts）与 /index 后缀
//   - 仅保留项目内部文件（剔除 node_modules / 外部包 / 无法解析为本地文件的）
//
// type-only import 口径：两侧都「计入」（dpdm/dependency-cruiser 默认跟踪类型导入，
// 自研 tree-sitter 提取也对 `import type` 产生 Imports 边），保持一致。

'use strict';

const path = require('path');

const TS_EXTS = ['.ts', '.tsx', '.mts', '.cts', '.d.ts', '.js', '.jsx', '.mjs', '.cjs'];

// 去扩展名 + 去 /index + posix 化。输入已是相对 src_root 的路径。
function canonFile(relPath) {
  let p = relPath.split(path.sep).join('/');
  // 去最长匹配扩展名（.d.ts 优先于 .ts）
  for (const ext of TS_EXTS.slice().sort((a, b) => b.length - a.length)) {
    if (p.endsWith(ext)) {
      p = p.slice(0, -ext.length);
      break;
    }
  }
  if (p.endsWith('/index')) p = p.slice(0, -'/index'.length);
  // 去重复前导 ./
  while (p.startsWith('./')) p = p.slice(2);
  return p;
}

// 把「相对 src_root 的文件路径」归一化为节点 key；非内部文件返回 null。
// internalSet：归一化后的内部文件 key 集合（用于剔除外部/解析失败的边端点）。
function nodeKey(relPathFromSrc, internalSet) {
  if (relPathFromSrc == null) return null;
  const key = canonFile(relPathFromSrc);
  if (internalSet && !internalSet.has(key)) return null;
  return key;
}

// 边 key（用于集合比较）。
function edgeKey(from, to) {
  return `${from}\t${to}`;
}

module.exports = { canonFile, nodeKey, edgeKey, TS_EXTS };
