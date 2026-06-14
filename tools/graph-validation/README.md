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

## 结果（基于 master core · 2026-06-14）

| 仓库 | 边召回 | 环 self/oracle | 硬门 |
|------|--------|----------------|------|
| rxjs | 1.0 | 16/16 | ✅ |
| fp-ts | 1.0 | 64/64 | ✅ |
| zod | 0.9627 | 4/8 | ❌ |

### zod 不达标 —— harness 发现的真实建图 bug（已修复）

zod 漏 5 条 `*.test → index` 边、**漏报 4 个环节点**，根因是 core `resolve_import` 对
`from ".."`（解析到 src 根 barrel `index.ts`）拼出带前导斜杠的 `/index.ts` 候选，
永不匹配根下的 `index.ts`，整条 barrel 导入边丢失，进而 SCC 断裂、漏报循环依赖。

**已由 PR #4 修复**。修复后经本 harness 端到端验证：zod 边召回 `0.9627→1.0`、
环节点 `4/8→8/8` 一致；rxjs/fp-ts 保持 16/16、64/64 不回归。合并 PR #4 后
`just validate-graph` 三仓库全绿，harness 作为持续回归防线。

## 目录

| 路径 | 作用 |
|------|------|
| `repos.txt` | 钉死的校验仓库清单（url + sha + src 根） |
| `oracle/` | dependency-cruiser / dpdm 调用与解析（`parse-*.js`）+ 钉版本 `package.json` |
| `compare/` | 归一化（`normalize*.js`）+ 差分对比（`compare.js`）+ SCC 重算（`scc.js`） |
| `reports/` | 各仓库校验报告（Markdown） |
| `run.sh` | 主驱动：拉仓库 → 跑三方 → 归一化 → 对比 → 出报告 |
| `.work/` | 运行时工作目录（克隆仓库 + 中间产物，gitignore） |
