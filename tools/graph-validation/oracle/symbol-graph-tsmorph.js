// 符号级 Calls/Extends/Implements oracle —— 用 ts-morph（TS 编译器类型检查器）
// 作为「真值」，提取跨文件的符号级调用与继承/实现关系。
//
// 为什么用 ts-morph 而非 dependency-cruiser/dpdm：
//   - dc/dpdm 是「文件级 import 图」工具，无法解析「某个 CallExpression 到底调到
//     哪个定义」「extends Foo 的 Foo 定义在哪个文件的哪个符号」。
//   - 符号级 Calls/Extends 必须有「类型检查器 + 符号解析」才能做到 oracle 级精度，
//     这正是 TS 编译器 API 的能力。ts-morph 是其轻量封装，API 友好、社区成熟。
//
// 用法：
//   node symbol-graph-tsmorph.js <repo_root> <src_root>
//     repo_root：仓库根（含 tsconfig.json）
//     src_root ：相对 repo_root 的源码根（输出路径相对它，与自研图口径一致）
//
// 输出（stdout，JSON）：
//   { calls: [...], extends: [...], implements: [...], stats: {...} }
//   - calls 元素   : { caller_file, caller_symbol, callee_file, callee_symbol, callee_kind }
//   - extends/implements 元素: { child_file, child_symbol, parent_file, parent_symbol }
//   路径均为「相对 src_root、posix、去 TS 扩展名」（与 normalize.js / 自研侧一致）。
//   跨文件才计入；剔除标准库 / node_modules / 解析失败的端点。
//
// 注意：本脚本是 oracle（真值侧），口径尽量「宽而准」——能被类型检查器解析到本地
// 定义的就计入。自研启发式必然是其子集（recall < 1），这正是要量化的指标。

'use strict';

const path = require('path');

let Project, Node, SyntaxKind;
try {
  ({ Project, Node, SyntaxKind } = require('ts-morph'));
} catch (e) {
  process.stderr.write(
    '[symbol-oracle] 未找到 ts-morph，请先在 oracle/ 下 npm install（见 package.json 钉版本）\n',
  );
  process.exit(2);
}

// ---- 路径归一化（与 compare/normalize.js 的 canonFile 同口径，独立内联避免跨 worktree 依赖） ----
const TS_EXTS = ['.d.ts', '.tsx', '.mts', '.cts', '.ts', '.jsx', '.mjs', '.cjs', '.js'];

// 把绝对文件路径归一化为「相对 src_root、posix、去扩展名 / 去 index」的 key；
// 非 src_root 子树（node_modules / 标准库 / 仓库外）返回 null。
function canonRelToSrc(absFile, absSrcRoot) {
  let rel = path.relative(absSrcRoot, absFile);
  if (rel.startsWith('..') || path.isAbsolute(rel)) return null; // 在 src_root 之外
  rel = rel.split(path.sep).join('/');
  for (const ext of TS_EXTS.slice().sort((a, b) => b.length - a.length)) {
    if (rel.endsWith(ext)) {
      rel = rel.slice(0, -ext.length);
      break;
    }
  }
  if (rel.endsWith('/index')) rel = rel.slice(0, -'/index'.length);
  while (rel.startsWith('./')) rel = rel.slice(2);
  return rel;
}

// 解析一个 symbol 到其定义（跟随 import alias）。返回去重后的 declaration 节点数组。
function resolveDeclarations(symbol) {
  if (!symbol) return [];
  let s = symbol;
  // 跟随 import alias（`import { foo }` 的 foo 是 alias symbol，需解析到原始定义）。
  try {
    const aliased = s.getAliasedSymbol && s.getAliasedSymbol();
    if (aliased) s = aliased;
  } catch (_) {
    /* 非 alias 时抛错，忽略 */
  }
  const decls = s.getDeclarations ? s.getDeclarations() : [];
  return decls || [];
}

// 从一个 declaration 节点取「符号名」——优先 getName()，回退到所属命名声明的名字。
function declName(decl) {
  if (decl && typeof decl.getName === 'function') {
    const n = decl.getName();
    if (n) return n;
  }
  // VariableDeclaration、方法等：向上找有名字的祖先声明
  if (decl && typeof decl.getFirstAncestor === 'function') {
    const named = decl.getFirstAncestor(
      (a) => typeof a.getName === 'function' && a.getName(),
    );
    if (named) return named.getName();
  }
  return null;
}

// 取一个节点的「外围命名符号」（caller 侧的 enclosing symbol）：
// 最近的 函数/方法/类/接口 声明名。顶层语句返回 '<module>'。
function enclosingSymbolName(node) {
  let cur = node.getParent();
  while (cur) {
    if (
      Node.isFunctionDeclaration(cur) ||
      Node.isMethodDeclaration(cur) ||
      Node.isClassDeclaration(cur) ||
      Node.isInterfaceDeclaration(cur) ||
      Node.isFunctionExpression(cur) ||
      Node.isArrowFunction(cur)
    ) {
      // 箭头/函数表达式常赋给变量：取变量名更稳定
      if (Node.isArrowFunction(cur) || Node.isFunctionExpression(cur)) {
        const varDecl = cur.getFirstAncestor((a) => Node.isVariableDeclaration(a));
        if (varDecl && varDecl.getName()) return varDecl.getName();
      }
      if (typeof cur.getName === 'function' && cur.getName()) return cur.getName();
    }
    cur = cur.getParent();
  }
  return '<module>';
}

function main() {
  const repoRoot = process.argv[2];
  const srcRoot = process.argv[3];
  if (!repoRoot || !srcRoot) {
    process.stderr.write('用法: node symbol-graph-tsmorph.js <repo_root> <src_root>\n');
    process.exit(2);
  }
  const absSrcRoot = path.resolve(repoRoot, srcRoot);
  const tsConfigFilePath = path.join(repoRoot, 'tsconfig.json');

  // 优先用 tsconfig（拿到 paths 别名 / lib / target 等正确配置）；
  // 无 tsconfig 时退化为「按 glob 加载 src 下所有 ts」。
  let project;
  const fs = require('fs');
  if (fs.existsSync(tsConfigFilePath)) {
    project = new Project({ tsConfigFilePath, skipAddingFilesFromTsConfig: true });
    project.addSourceFilesAtPaths([
      `${absSrcRoot}/**/*.ts`,
      `${absSrcRoot}/**/*.tsx`,
      `!${absSrcRoot}/**/*.d.ts`,
    ]);
  } else {
    project = new Project({ compilerOptions: { allowJs: false } });
    project.addSourceFilesAtPaths([
      `${absSrcRoot}/**/*.ts`,
      `${absSrcRoot}/**/*.tsx`,
      `!${absSrcRoot}/**/*.d.ts`,
    ]);
  }

  const calls = new Map(); // key -> obj，去重
  const extendsRel = new Map();
  const implementsRel = new Map();
  let callExprSeen = 0;
  let callResolvedCross = 0;

  const sourceFiles = project
    .getSourceFiles()
    .filter((sf) => canonRelToSrc(sf.getFilePath(), absSrcRoot) !== null)
    .filter((sf) => !sf.getFilePath().endsWith('.d.ts'));

  for (const sf of sourceFiles) {
    const callerFile = canonRelToSrc(sf.getFilePath(), absSrcRoot);
    if (callerFile == null) continue;

    sf.forEachDescendant((node) => {
      // ---------- Calls：CallExpression + NewExpression ----------
      if (Node.isCallExpression(node) || Node.isNewExpression(node)) {
        callExprSeen += 1;
        const expr = node.getExpression();
        if (!expr) return;
        let sym;
        try {
          sym = expr.getSymbol();
        } catch (_) {
          return;
        }
        const decls = resolveDeclarations(sym);
        for (const decl of decls) {
          const declSf = decl.getSourceFile && decl.getSourceFile();
          if (!declSf) continue;
          const calleeFile = canonRelToSrc(declSf.getFilePath(), absSrcRoot);
          if (calleeFile == null) continue; // 标准库 / node_modules / 仓库外
          if (calleeFile === callerFile) continue; // 仅跨文件计入（与自研 Calls 跨文件口径一致）
          const calleeSymbol = declName(decl);
          if (!calleeSymbol) continue;
          const calleeKind = decl.getKindName ? decl.getKindName() : 'unknown';
          callResolvedCross += 1;
          const callerSymbol = enclosingSymbolName(node);
          const key = `${callerFile}\t${callerSymbol}\t${calleeFile}\t${calleeSymbol}`;
          if (!calls.has(key)) {
            calls.set(key, {
              caller_file: callerFile,
              caller_symbol: callerSymbol,
              callee_file: calleeFile,
              callee_symbol: calleeSymbol,
              callee_kind: calleeKind,
            });
          }
        }
        return;
      }

      // ---------- Extends / Implements：Class/Interface 的 heritage ----------
      if (Node.isClassDeclaration(node) || Node.isInterfaceDeclaration(node)) {
        const childName = typeof node.getName === 'function' ? node.getName() : null;
        if (!childName) return;

        const heritages = [];
        // extends：class 单基类、interface 可多基接口
        if (typeof node.getExtends === 'function') {
          const ext = node.getExtends();
          if (Array.isArray(ext)) ext.forEach((e) => heritages.push(['extends', e]));
          else if (ext) heritages.push(['extends', ext]);
        }
        // implements：仅 class
        if (typeof node.getImplements === 'function') {
          for (const impl of node.getImplements()) heritages.push(['implements', impl]);
        }

        for (const [kind, exprWithTypeArgs] of heritages) {
          const baseExpr = exprWithTypeArgs.getExpression
            ? exprWithTypeArgs.getExpression()
            : exprWithTypeArgs;
          if (!baseExpr) continue;
          let sym;
          try {
            sym = baseExpr.getSymbol();
          } catch (_) {
            continue;
          }
          const decls = resolveDeclarations(sym);
          for (const decl of decls) {
            const declSf = decl.getSourceFile && decl.getSourceFile();
            if (!declSf) continue;
            const parentFile = canonRelToSrc(declSf.getFilePath(), absSrcRoot);
            if (parentFile == null) continue; // 外部基类型（如 extends Error）不计入
            const parentSymbol = declName(decl);
            if (!parentSymbol) continue;
            const rec = {
              child_file: callerFile,
              child_symbol: childName,
              parent_file: parentFile,
              parent_symbol: parentSymbol,
            };
            const key = `${rec.child_file}\t${rec.child_symbol}\t${rec.parent_file}\t${rec.parent_symbol}`;
            (kind === 'implements' ? implementsRel : extendsRel).set(key, rec);
          }
        }
      }
    });
  }

  const toSortedArr = (m) =>
    [...m.values()].sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));

  const out = {
    calls: toSortedArr(calls),
    extends: toSortedArr(extendsRel),
    implements: toSortedArr(implementsRel),
    stats: {
      source_files: sourceFiles.length,
      call_exprs_seen: callExprSeen,
      call_resolved_cross_file: callResolvedCross,
      calls_unique: calls.size,
      extends_unique: extendsRel.size,
      implements_unique: implementsRel.size,
    },
  };
  process.stdout.write(JSON.stringify(out));
}

main();
