# 符号级 Calls / Extends 精度差分校验（spike）

本文档说明「符号级 Calls/Extends/Implements 精度对比」组件的方法、口径与预期难点。
这是 **spike**：目标是验证可行性并量化「自研 tree-sitter 启发式」相对「类型检查器真值」
的精度，**不是**要让自研启发式达到类型系统级精度。

现有 import 图 harness（文件级 import 边 + 环，`dependency-cruiser ∩ dpdm` 双 oracle）
已覆盖**文件级 import 关系**。本组件正交扩展到**符号级调用与继承/实现关系**。

## 组件清单

| 文件 | 角色 |
|------|------|
| `cli/crates/core/examples/dump_symbol_graph.rs` | 自研侧：导出图的 Calls + Extends/Implements 边 |
| `oracle/symbol-graph-tsmorph.js` | oracle 侧：ts-morph 类型检查器解析真值 |
| `oracle/package.symbol.json` | ts-morph 钉版本（独立于 import oracle 的 package.json） |
| `compare/compare-symbol.js` | 差分对比 + 软门 + Markdown 报告 |

## 为什么用 ts-morph 作 oracle

`dependency-cruiser` / `dpdm` 只产**文件级 import 图**，无法回答：
- 某个 `CallExpression` 到底解析到哪个**定义**（哪个文件的哪个符号）？
- `class Foo extends Bar` 的 `Bar` 定义在哪个文件、是 class 还是 interface？

符号级关系必须有「TypeScript 类型检查器 + 符号解析」才能确定。`ts-morph` 是 TS 编译器
API（`typescript` 包）的轻量封装：API 友好、社区成熟、可直接吃 `tsconfig.json`（拿到 paths
别名 / lib / target）。它解析 import alias、re-export、跨文件定义的能力即「真值」来源。
版本钉死（`ts-morph@23.0.0`，对应 TS 5.x，与 import oracle 的 `typescript@5.6.3` 同主线），
避免类型检查器行为漂移导致真值不可复现。

**oracle 幻觉防护**：ts-morph 是确定性工具（非 LLM），不存在虚构；但其结果仍是「带启发
（alias 跟随、定义去重）的近似」，故软门只警示不阻断，差异需人工判读三类归因。

## 自研侧边形态（口径对齐的基础）

读 `graph/build.rs::add_cross_file_edges` 与 `lang/typescript.rs::extract_heritage` 确认：

- **Calls 边**：`source = file:{rel}`（**文件节点**），`target = function:{rel}:{name}`
  或 `class:{rel}:{name}`（构造调用，`sub_kind="constructor"`）。
  关键：**caller 侧粒度即文件级**——自研启发式不追踪「调用发生在哪个函数体内」。
  自研 Calls 解析逻辑（启发式，非类型系统）：
  1. 当前文件顶层 `function:{rel}:{callee}`；
  2. 经 import 映射解析到目标文件的顶层函数 / 命名空间调用 `ns.fn()` 剥前缀；
  3. 构造调用 `new Foo()` → `class` 节点；
  4. 全局同名兜底（命中多个则放弃）。
- **Extends 边**：`source = class:{rel}:{name}`，`target = interface|class|enum:{rel}:{name}`；
  `sub_kind="implements"` = implements，否则 extends。两端都是符号。
  目标 ID 先以 interface 前缀占位，`build.rs::fixup_extends_in_edges` 在全图完成后
  做跨文件唯一名解析（命中多个同名类型则放弃该边）。

`dump_symbol_graph.rs` 忠实导出，不做归一化；归一化全在 compare 脚本，两侧同口径。

## 文件级 vs 符号级口径

### 文件级聚合（先行，软门主指标）

跨系统**符号 ID 对齐**很难（见下），故先做文件级聚合，把对齐难度降到与已验证 import 图
同一量级（纯文件对）：
- Calls → `(caller_file, callee_file)`，忽略符号名 / 构造区分；
- Extends → `(child_file, parent_file)`；Implements 同理。

对三类各算 precision / recall / F1（self 为预测、oracle 为真值），快速量化
「自研启发式的文件级效果」。这是本 spike 的核心可交付指标。

### 符号级精确（stretch，不计入软门）

- 自研 caller 侧**无 enclosing 符号**（Calls 边 source 是文件节点），caller 符号对齐
  **天然不可能** → 只能比 callee 符号；
- callee 符号名空间也不一致：自研可能输出 `ns.foo`（命名空间前缀），ts-morph 输出解析
  后的定义名 `foo`；overload / 同名导出加剧歧义。

故符号级仅给「callee 符号名集合 Jaccard 重合度」作弱参考。真要做符号级精确对比，需先：
统一 caller 粒度（自研补 enclosing 符号，须改 build.rs 让 Calls 边 source 用符号节点）、
统一 callee 名（剥命名空间 + 跟随 re-export 到原始定义名）。属后续工作，非本 spike 范围。

## 软门 rationale

**软门（warn-only，退出码恒 0）而非硬门**，因为：
1. 自研是 tree-sitter **启发式**，精度**必然低于**类型系统，硬门会恒红、无意义；
2. spike 目标是「量化 + 暴露难点」，不是「卡 CI」；
3. import 图那套硬门（recall ≥ 0.98）成立是因为文件级 import 是确定性可达的；符号级
   调用的可达性依赖类型推断，启发式做不到同等召回。

警示阈值 `F1 < 0.7` 仅作「效果偏低」信号，提示人工判读，不阻断。

## 预期难点与精度预估

自研启发式相对 ts-morph 真值，**文件级** F1 预估区间：

| 关系 | 预估 F1 | 主要差距来源 |
|------|---------|-------------|
| **Calls** | 0.40 ~ 0.65（偏低） | 见下，召回是主瓶颈 |
| **Extends** | 0.75 ~ 0.92（较高） | heritage 是声明式语法，tree-sitter 可靠提取 |
| **Implements** | 0.75 ~ 0.92（较高） | 同 Extends |

### Calls 召回为何偏低（核心难点）

1. **方法调用 `obj.method()`**：自研只解析顶层 `function` 与构造 `new Class()`，
   **不解析实例 / 静态方法调用**。OOP 重的库里方法调用占比高 → 大量漏报。
2. **动态 / 高阶调用**：回调、`arr.map(fn)` 传递、`apply/call`、属性访问链——启发式无从
   静态解析，类型检查器能（部分）解析 → 漏报。
3. **re-export 链 / 深层 barrel**：自研 import 解析逐层，跨多层 re-export 易断；ts-morph
   `getAliasedSymbol` 一步跟到原始定义。
4. **重载 / 同名歧义**：自研「全局同名兜底命中多个则放弃」→ 漏报；命中错误文件 → 误报。

### Calls 精度（precision）相对较高的原因

自研在歧义时**保守放弃**（`find_unique_node` 命中多个返回 None），故误报少、precision 不低；
但召回低拖累 F1。即「自研 Calls 是 oracle 的稀疏子集」。

### 口径噪声（双向，影响 F1 但非自研 bug）

- ts-morph 把**类型位置**的引用也可能算进 callee（如泛型约束里的调用）；
- 自研构造调用计入 Calls，oracle 的 `NewExpression` 也计入，口径一致；
- 外部基类型（`extends Error`）两侧都剔除（oracle `canonRelToSrc` 返回 null）。

判读报告时须区分三类：**自研可改进**（如补方法调用解析）/ **口径差异**（归一化对齐）/
**启发式固有局限**（动态调用，类型系统才能解）。

## 运行方式（由主会话 background 跑，本 spike 不执行长命令）

```bash
# 1. 装 ts-morph oracle（钉版本）
cd tools/graph-validation/oracle
# 把 package.symbol.json 的 dependencies 并入 package.json 后：
npm install     # 或单独：npm install ts-morph@23.0.0

# 2. 编译自研 example
cd ../../../cli
cargo build -p rustmigrate-core --example dump_symbol_graph

# 3. 对单个仓库跑（建议先用 zod，~80 文件，最小）
SRC=tools/graph-validation/.work/zod
./target/debug/examples/dump_symbol_graph "$SRC/src" > /tmp/self-sym.json
node tools/graph-validation/oracle/symbol-graph-tsmorph.js "$SRC" src > /tmp/oracle-sym.json
node tools/graph-validation/compare/compare-symbol.js \
  --self /tmp/self-sym.json --oracle /tmp/oracle-sym.json \
  --name zod --sha ca42965 --src src \
  --out tools/graph-validation/reports/zod-symbol.md
```

后续可仿 import 图的 `run.sh` 加一个 `run-symbol.sh` 驱动 `repos.txt` 全量（zod / fp-ts / rxjs）。
