# MDR-010: DEC-GATE 触发的拆解引擎修正——Python 分类器 + 跨非机械凸性合批 + 内聚门退化处理

- **状态**: 已决策
- **日期**: 2026-06-28
- **范围**: M3-DEC-01 拆解引擎（DEC-GATE 真实项目验收触发的必修）

## 问题

DEC-GATE 在真实 Python 项目（toolz / funcy）上跑 `graph decompose` dry-run 时暴露三处缺陷，使 §8 四维度验收无法通过：

1. **Python 机械/危险分类器缺失**。`classify_file` 只有 TypeScript 实现，`PythonAdapter` 回退默认 `FileClassification::conservative()`——把所有 Python 文件判为 `Normal`、非机械、零危险。后果：拆解引擎在 M3 的**整个目标语言**上零价值（永不合批、永不报危险信号）。`danger_files=0`、`batched_files=0` 是假阴性，非项目本身无机械文件。

2. **decompose 静默用 TS 适配器分类 Python 源码**。`cmd_graph_decompose` 在 `source_language` 未配置时 `unwrap_or(TypeScript)`，而非像 `graph build` 那样按 `--root` 探测语言。后果：TS tree-sitter 解析 `.py` 产出垃圾 `file_kind`/`danger`（toolz 误报 `barrel:2`）。

3. **严格"连续装箱"在真实项目上几乎永不合批**。原算法（decomposition-redesign §7）只累积**拓扑序连续**的机械组，遇任一非机械/超预算/循环组即封口。真实项目里机械文件（barrel `__init__.py`）被逻辑文件隔开、极少连续 → `batched_files=0`、残留机械单文件 >0（toolz 的 2 行 `sandbox/__init__.py` 等仍各自成独立模块），违反 §8「残留机械单文件≈0」「半小时翻 10 行那类文件必须从独立模块里消失」。

   附带：内聚门 MQ 的 ratio 测试（actual ≥ 1.5×随机基线）在**零耦合批次**（如两个空 `__init__.py`）上退化——internal=external=0，actual==baseline，ratio 恒为 1.0 永远 <1.5，把无害的空文件合批误判为内聚不达标。

## 决策

### D-1：实现 `PythonAdapter::classify_file`（对齐 TS 契约）

- **file_kind 四分类**（顶层结构判定）：
  - `Barrel`：仅 `import`/`from import`/`__all__`/docstring/空 → 纯转发壳（`__init__.py` 主力）。
  - `PureType`：仅类型别名（`X = list[int]` 下标泛型）/ `TypeVar`·`NewType`·`ParamSpec` / 仅注解无方法的 class（Enum/TypedDict/Protocol/纯 dataclass）。
  - `PureConstant`：仅不可变字面量绑定（int/float/str/bool/None/全不可变 tuple）。`__all__`/dunder 元数据不影响归类。
  - `Normal`：其余（函数 / 带方法的 class / 控制流 / 可变全局 / 副作用表达式）。
- **6 类 DangerCategory**（全树扫描，独立于 file_kind）：NumericPrecision（`math.`/float/Decimal/Fraction/complex/decimal·fractions import）、Concurrency（async/await/threading·asyncio·multiprocessing·concurrent import 与调用）、DynamicReflection（eval/exec/compile/getattr/setattr/__import__/metaclass=/`__getattr__` 族/importlib·inspect import）、IoSideEffect（open/input/os·sys·subprocess·socket… import）、Ffi（ctypes·cffi import/CDLL）、SharedMutableGlobal（模块级 list/dict/set 字面量绑定、`global`/`nonlocal`）。
- **锚点不变量**（§8）：`if TYPE_CHECKING:` 守卫不令文件变 Normal（保持机械资格）；纯 `if` 控制流→Normal 但不进 danger（"10 行带 if 不进重型"）；Python clamp 用内建 `min/max`（精确）不命中数值危险，而 `math.` 命中。
- 解析失败/含语法错误 → `conservative()`（绝不误判机械）。

### D-2：decompose 按 `--root` 探测语言

`cmd_graph_decompose` 改为：`source_language` 优先取配置；未配置则 `detect_language(&source_root)`（对齐 `graph build`）；探测失败显式 warning 回退 TS。消除"静默用 TS 适配器解析 Python"。

### D-3：放宽合批为"跨非机械凸性跳跃合并"

非机械单文件**不再封口**当前合批——仅单独成 `Single`，合批保持开放，允许并入后续**真正独立**（无依赖路径相隔）的机械组。安全性由既有**凸性约束**保证：若被跳过的非机械组横亘在批内成员的依赖路径上，后续机械组的 `is_convex` 检查会自动拒绝（不跨真实依赖合批、不成环）。`Cycle`/`ManualOverBudget` 仍封口（SCC / 大文件是强拓扑边界，限制内聚稀释）。

> 这是 decomposition-redesign §7/§9 预设的"由验收门数据触发的优化"的**轻量先行版**：不做完整"内聚加权打包"（沿耦合重边合并），只解除"必须拓扑连续"这一过严约束。凸性 + 预算 + cycle/over-budget 封口共同界定批跨度。

### D-4：内聚门退化处理 → 零耦合真空满足 + 绝对内聚地板

`CohesionMq` 新增 `coupling_edges`（批内文件相关的耦合边总数 = 内部+外部）。内聚门判定改为：
`batched_files==0 || coupling_edges==0 || (baseline_退化 && actual≥0.5) || ratio≥1.5`，
其中 `baseline_退化 = |actual-baseline|<ε`（重排无法改变划分 → ratio 无判别力）。**绝对地板仅在 baseline 退化时兜底**——避免多批次场景下"actual≥0.5 但劣于随机基线(ratio<1.5)"被地板误放（design-checker nit）。

两类退化均被覆盖：
1. **零耦合批**（空/孤立文件，如两个空 `__init__.py`）：`coupling_edges==0`，无 ratio 可测、无泄漏风险 → 真空满足。
2. **单批次 baseline 退化**（DEC-GATE 在 attrs 上实测）：当全部合批文件构成**单个** batch（或批数太少），"保持批大小随机重排成员"是空操作——所有文件永远落入同一批 → `baseline≡actual`、`ratio` 恒为 1.0，**永远达不到 1.5×**。attrs 的 6 barrel 簇（`__init__.py` 用 `from . import …` 聚合 5 个同级薄壳）`actual=1.0`（5 条 import 边全在批内、完美内聚）却被 ratio 测试假阴性误杀。故加**绝对内聚地板** `actual≥0.5`（多数耦合留批内 = 客观高内聚）作单批次主判据。

**真泄漏**（coupling_edges>0 但 actual 低且 ratio 不显著）仍失败——如 rich 误把互不 import 的 `errors.py`+`region.py` 合批，`actual=0.0`（7 条耦合边全在批外）→ 正确 FAIL。区分"无害/高内聚批"与"机械凑数低内聚"。

> ratio 仍是**多批次**场景的相对判据（保留 §8 原设计）；绝对地板只在 baseline 退化时兜底。验收时判"批内有耦合"应以 `actual>0` 而非仅 `coupling_edges>0`（后者含批外入边）。

## 理由

- D-1/D-2 是纯缺陷修复：DEC-01 把"机械判定 predicate"的实现只落到 TS，而 DEC-GATE §8 明确要求 Python 锚点反例。TS-only 单测给了假信心（见 memory「验收≠工具自测」）。
- D-3 的凸性安全性是**已证明的**：凸性判定（集合内某组可达外部 x 且 x 可达集合内某组则禁止）天然阻断跨真实依赖的合并，放宽只新增"独立机械文件跨非机械间隙合并"，单测 `dependency_separated_mechanicals_stay_unbatched` 守住反例。
- D-4 修正度量缺陷而非放水：ratio 测试对零耦合本就无定义，强行套 1.5× 会把无害合批误杀；真正的低内聚耦合批仍被拦。

## 影响

- `cli/crates/core/src/lang/python.rs`：新增 `classify_file` + 分类/危险辅助函数 + 19 单测。
- `cli/crates/core/src/graph/decompose.rs`：`plan_decomposition` 放宽分支；`CohesionMq.coupling_edges` + `weighted_mq` 返回耦合边数；新增 3 单测。
- `cli/crates/cli/src/lib.rs`：decompose 语言探测；内聚门 `coupling_edges==0` 真空满足分支 + JSON 输出 `coupling_edges`。
- `docs/decomposition-redesign.md` §7 措辞"连续装箱"需脚注此放宽（保留原 first-fit 框架，解除连续约束）。
- TS 既有路径无回归（放宽只对"独立机械"生效，连续机械链行为不变；`convex_chain_batches_all`/`convexity_blocks_merge_across_external` 全绿）。
