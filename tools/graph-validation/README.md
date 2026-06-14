# 源码图差分校验 harness（M1 验收门）

在真实中型 TS 仓库上，将自研「文件级 import 图 + 环检测」与外部成熟依赖图工具
**差分对比**，确认建图正确性。源码图是下游 analyzer / translator 信任的地基，
建图错误会沿迁移流程向下复利。

## 校验范围与关键决策

- **只校验文件级 import 图 + 环**（驱动拓扑序的关键层）。符号级 Calls/Extends 不纳入
  （tree-sitter 启发式、设计上即近似）。
- **Oracle = dependency-cruiser（主）+ dpdm（交叉验证）**，取双方**交集**为校验基准
  （不用 madge —— 已停更，见 `docs/design/04-toolchain.md`）。
- **绕过 CLI**：`graph` 子命令在 M1 仍为占位，故新建
  `cli/crates/core/examples/dump_import_graph.rs` 直调 core API
  （`build_graph_ts` + `detect_cycles`，只取 `EdgeType::Imports` 边）。
- **归一化口径**（两侧同一套规则，见 `compare/normalize.js`）：相对 src 根、posix 分隔符、
  去 TS 扩展名与 `/index`、仅保留项目内部边、type-only 两侧一致计入。
- **dependency-cruiser 必须加 `--ts-pre-compilation-deps`**：否则默认丢弃「编译后消失」的
  type-only import，边数严重偏少（fp-ts 503 vs 1246）且算不出 type-only 参与的环，
  与 dpdm/自研口径不一致。

## 硬门

- 对「dependency-cruiser ∩ dpdm」交集的边，自研图召回率 **≥ 0.98**
- **环集合一致**：双 oracle 都判为环上的节点，自研图必须全覆盖，且无双方都不认的多余环节点
- **oracle 有效性前置**：任一 oracle 边集为空或交集为空（多为 oracle 工具静默失败）时，
  校验基准不可信，强制判不达标并在报告标注 —— 防止验收门假绿（空交集时召回率虚高为 1）

## 用法

```bash
just validate-graph          # 跑 repos.txt 全部仓库
just validate-graph rxjs     # 只跑指定仓库（调试）
```

首次运行自动 `npm install` oracle 工具（版本钉死于 `oracle/package.json`）并编译 example，
报告输出到 `reports/<name>.md`。

## 校验仓库（钉死 commit，详见 repos.txt）

| 仓库 | sha | 文件数 | 特点 |
|------|-----|--------|------|
| rxjs | `72bc921` | 243 | 大样本、含真实循环依赖 |
| fp-ts | `669cd3e` | 123 | 高度互递归（环极多） |
| zod | `ca42965` | 80 | barrel re-export、扁平结构 |

## 结果（2026-06-14 · 含本 PR 的 barrel 修复）

| 仓库 | 边召回 | oracle 有效 | 环 self/oracle | 硬门 |
|------|--------|:----------:|----------------|------|
| rxjs | 1.0 | ✅ | 16/16 | ✅ |
| fp-ts | 1.0 | ✅ | 64/64 | ✅ |
| zod | 1.0 | ✅ | 8/8 | ✅ |

**三仓库全部达标。**

### 开发中发现并修复的建图 bug

harness 首次在 zod 上跑出召回 `0.9627`（漏 5 条 `*.test → index` 边、**漏报 4 个环节点**），
定位到 core `resolve_import` 对 `from ".."`（解析到 src 根 barrel `index.ts`）拼出带前导
斜杠的 `/index.ts` 候选，永不匹配根下的 `index.ts`，整条 barrel 导入边丢失 → SCC 断裂 →
**漏报循环依赖**（直接污染拓扑序）。本 PR 一并修复（`build.rs` + 单元测试），修复后
zod 边召回 `0.9627→1.0`、环节点 `4/8→8/8`，rxjs/fp-ts 不回归。harness 作为持续回归防线。

## 符号级精度（软门观测）—— 启发式效果

文件级 import 图是**硬门**（上表，驱动拓扑序）。**符号级 Calls/Extends/Implements** 是
tree-sitter 启发式（非确定性），用 **ts-morph（TS 编译器类型检查器）** 作真值做**软门观测**
（F1 < 0.7 警示、**不阻断**——启发式精度必然低于类型系统）。口径：文件级聚合
（caller_file→callee_file）。入口 `just validate-graph-symbol`，详见 [SYMBOL-PRECISION.md](./SYMBOL-PRECISION.md)。

### 结果（2026-06-14，含本 PR 的 interface extends 修复）

| 仓库 | Calls (P/R/F1) | Extends (P/R/F1) | Implements |
|------|----------------|------------------|------------|
| rxjs | 1.0 / 0.65 / 0.79 | 1.0 / 1.0 / **1.0** | 1.0 / 1.0 / **1.0** |
| fp-ts | 1.0 / 0.45 / 0.62 ⚠️ | 1.0 / 1.0 / **1.0** | — |
| zod | 1.0 / 0.39 / 0.56 ⚠️ | — | — |

- **precision 恒 = 1.0**：自研启发式建的符号边**零误报**——精确但不全。对迁移用途（图可信、不误导）是最优属性。
- **Extends/Implements F1 = 1.0**：继承图完整准确（含本 PR 修复的 interface extends）。
- **Calls F1 0.56~0.79（⚠️ 软门警示）**：recall 偏低，根因 tree-sitter 启发式**不解析方法调用 `obj.method()`**（只认顶层函数 + `new`）。这是启发式根本边界，需类型系统才能解 —— 记 `TODO`，迁移主靠 import 拓扑（已达硬门），Calls 是辅助信号。

### harness 发现并修复的第二个建图 bug：interface extends 全漏

fp-ts（继承几乎全是 `interface X extends Y`）首跑 Extends **0/80**。根因：`extract_heritage`
仅覆盖 class 的 `class_heritage`，**未处理 interface 的 `extends_type_clause`**。本 PR 修复
（`typescript.rs` + 单元测试），fp-ts `0/80→80/80`、rxjs `22/28→28/28`（额外补回 6 条混用的 interface extends）。

## 目录

| 路径 | 作用 |
|------|------|
| `repos.txt` | 钉死的校验仓库清单（url + sha + src 根） |
| `oracle/` | 文件级 `parse-*.js`（dependency-cruiser/dpdm）+ 符号级 `symbol-graph-tsmorph.js`（ts-morph）+ 钉版本 `package*.json` |
| `compare/` | 文件级 `normalize*.js`/`compare.js`/`scc.js` + 符号级 `compare-symbol.js` |
| `reports/` | 各仓库校验报告（`<name>.md` 文件级 / `<name>-symbol.md` 符号级） |
| `run.sh` | 文件级主驱动：拉仓库 → 三方 → 归一化 → 对比 |
| `run-symbol.sh` | 符号级驱动：自研 vs ts-morph，文件级聚合软门 |
| `SYMBOL-PRECISION.md` | 符号级方法/口径/软门 rationale/已知局限 |
| `.work/` | 运行时工作目录（克隆仓库 + 中间产物，gitignore） |
