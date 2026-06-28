# M3-DEC-GATE 拆解验收记录

> 硬前置（decomposition-redesign §8）：翻译前只跑拆解 dry-run，对四维度判据判定**源码侧拆解是否合理**。不派翻译 subagent、不声称验证 Rust 侧/翻译质量。**不过门不许做 DEC-02。**
> 日期：2026-06-28。验收人：主会话 + 2 个独立勘察/评审 subagent。

## 真实项目（来源 = 自选，补小/中规模 + 小文件密度 + 耦合机械簇多样性）

| 项目 | 包目录 | 规模 | 选取理由 |
|------|--------|------|---------|
| **attrs** | `src/attrs`（6 文件） | 小 | **耦合机械簇**：`__init__.py` 用 `from . import …` 聚合 5 个同级薄壳 barrel → 内聚硬门非空样本 |
| **toolz** | `toolz/`（31 文件，含 tests） | 中 | 微型机械文件（`sandbox/__init__.py`=2 行）+ 超大文件（itertoolz 1057 行）+ 循环依赖（`_signatures`↔`functoolz`） |
| **funcy** | `funcy/`（16 文件） | 中 | 高小文件密度（types/primitives/funcolls=21~28 行）+ 数值(`calc`)/并发(`flow`)/动态(`_inspect`) 危险信号 |

> 选 attrs 是 toolz/funcy 勘察暴露"良 factored 函数式库机械文件多为叶子/孤立、无批内耦合"后，由 subagent 在 10 个候选 repo 中定位的**唯一**产生"批内有耦合边"的项目（shim/re-export 包模式）。

## 验收前发现的引擎缺陷（全部修复，见 MDR-010）

DEC-01 引擎只在 **TypeScript** 上实现 `classify_file`，在 **M3 目标语言 Python** 上完全缺失 → 零合批、零危险信号（假阴性）。另有 decompose 静默用 TS 适配器分类 Python、严格"连续装箱"在真实项目永不触发、内聚 ratio 测试在单批次/零耦合退化等问题。修复：
- D-1 实现 `PythonAdapter::classify_file`（file_kind 四分类 + 6 类 danger，19 单测，锚点全过）
- D-2 decompose 按 `--root` 探测语言
- D-3 合批放宽为"跨非机械凸性跳跃合并"（凸性保证不跨真实依赖、不成环）
- D-4 内聚门退化处理：零耦合真空满足 + 绝对内聚地板 `actual≥0.5`（修单批次 ratio 假阴性）

## 四维度判定

### 维度 1：目标达成（纯计数）

| 项目 | before | after | 缩减率 | batched | residual_mech | 评估 |
|------|:--:|:--:|:--:|:--:|:--:|------|
| attrs | 6 | 1 | **83.3%** | 6 | 0 | ✅ 5 薄壳 barrel + `__init__` 合 1 单元 |
| toolz | 31 | 29 | 6.5% | 2 | 1 | ✅ 2 个空 `__init__.py` 合批；残留 1（孤立 barrel，凸性不可并） |
| funcy | 16 | 16 | 0% | 0 | 0 | ✅ 无机械簇（皆逻辑文件），正确不强合 |

- "半小时翻 10 行那类文件必须从独立模块里消失"：attrs 5 个 3 行薄壳、toolz 2 个空 init **已消失**。✅
- 残留机械单文件 ≈0：attrs 0 / funcy 0 / toolz 1（孤立、凸性约束下不可并，属合理残留）。✅（≈0）

### 维度 2：正确性不变量（硬门 100%）

| 项目 | DAG 无环 | 每文件恰好一属 | 无批超预算 | 跑两次字节级一致 |
|------|:--:|:--:|:--:|:--:|
| attrs | ✅ | ✅ | ✅ | ✅ |
| toolz | ✅ | ✅ | ✅（6 超预算单文件已转人工） | ✅ |
| funcy | ✅ | ✅ | ✅（2 超预算转人工） | ✅ |

**100% 通过。** 确定性经双跑 `diff` 字节级校验（plan canonical hash 一致）。

### 维度 3：内聚质量（硬门）

| 项目 | batched | coupling_edges | actual MQ | baseline | 判定依据 | pass |
|------|:--:|:--:|:--:|:--:|------|:--:|
| **attrs** | 6 | 5 | **1.00** | 1.00 | actual≥0.5 绝对地板（单批次 baseline 退化） | ✅ |
| toolz | 2 | 0 | — | — | 零耦合真空满足（2 空文件） | ✅ |
| funcy | 0 | — | — | — | 无合批 N/A | ✅ |

**非空验证**：attrs 的 6-barrel 批 `__init__.py` 经 `from . import converters,exceptions,filters,setters,validators` 对 5 个同级 barrel 各产生 1 条 import 边，**5 条全落批内 → actual=1.0（完美内聚）**。这是内聚硬门的真实耦合样本。
> ratio 测试在单批次退化（重排成员是空操作 → baseline≡actual≡1.0 → ratio 恒 1.0 永达不到 1.5×），故 attrs 完美内聚批靠绝对地板判过（MDR-010 D-4）。反例守门：rich 误把互不 import 的 errors+region 合批 actual=0.0 → 正确 FAIL（勘察 subagent 实测）。

### 维度 4：分类合理性（CLI 分布 + 自动锚点 + 人工抽检）

**自动锚点反例（19 单测，100% 通过）**：
- `if TYPE_CHECKING:` 守卫不令文件变 Normal（保持机械资格）✅
- 纯 `if` 控制流 → Normal 但**不**进 danger（"10 行带 if 不进重型"）✅
- `math.sqrt` 命中 NumericPrecision；Python 内建 `min/max` clamp **不**命中（精确）✅
- `async/await`→Concurrency、`eval`→DynamicReflection、`ctypes`→Ffi、模块级 `{}`→SharedMutableGlobal、`metaclass=`→DynamicReflection ✅

**CLI 分类分布**：attrs `{barrel:6}` danger=0；toolz `{barrel:3, normal:28}` danger=12；funcy `{pure_constant:1, normal:15}` danger=9。

**人工抽检（独立 ground-truth 对照）**：
- attrs 6/6（主会话逐文件读）：全为纯 re-export barrel、danger=0，分类器 **6/6 一致（100%）**。
  （注：`__init__.py` 末尾 `__getattr__ = _make_getattr(__name__)` 是 dunder 转发惯用法，按 Barrel/无危险处理——转发壳非翻译陷阱，且是其提供 5 条批内边的前提。）
- funcy（16）+ toolz（8 抽样）：独立评审 subagent 纯读源码产出 ground-truth，与引擎聚合对照：
  - **file_kind 一致率 29/30 = 96.7%**（attrs 6/6 + funcy 15/16 + toolz 8/8）。唯一分歧：funcy `__init__.py` 引擎判 PureConstant（有非 dunder 常量 `modules=('calc',…)` 字符串 tuple）、评审判 Barrel——两者均机械类，且该文件含 `import sys`→IoSideEffect danger 故 is_mechanical=False、不参与合批，分歧零决策影响。
  - **danger 一致率 30/30 = 100%**。funcy 9 danger / toolz 抽样 3 danger 与评审完全吻合；聚合数互证（funcy `pure_constant:1` 正是 `__init__`、`normal:15` 与评审 15 吻合）。
  - 评审主动提出的 3 个边界点经核对**引擎规则与评审判断一致**：`re.compile(`（属性调用 name≠builtin `compile`→不报 DynamicReflection）/ docstring 内 `float(`（在 string 节点非 call→不报 NumericPrecision）/ `curried/__init__.py` 的 `x=toolz.curry(…)` call 赋值+`del`→Normal。证明 AST 判定（非正则）正确规避假阳性。

> 抽检结论：语义一致率 96.7%（file_kind）/ 100%（danger），**远超 ≥80% 门槛**。

## 结论：**四维度全过，DEC-GATE 通过**

| 维度 | 判据 | 结果 |
|------|------|------|
| 1 目标达成 | 残留机械单文件≈0 / "10 行半小时"文件从独立模块消失 | ✅ attrs 6→1(83%)、toolz 残留 1(孤立合理)、funcy 无机械簇正确不强合 |
| 2 不变量+确定性（硬门 100%） | DAG 无环 / 凸 / 每文件恰一属 / 跑两次字节级一致 / 无批超预算 | ✅ 三项目 100% |
| 3 内聚（硬门 ≥1.5× 或绝对地板） | 批内耦合显著优于随机 | ✅ attrs actual=1.0 非空验证；rich 反例 actual=0 正确 FAIL |
| 4 分类合理性 | 锚点反例自动 100% + 人工抽检 ≥80% | ✅ 19 单测 100% + 抽检 96.7%/100% |

**过门 → 解锁 M3-DEC-02（轻量翻译路径）。**

### 验收附带的工程产出（本来不在 DEC-GATE 范围，但为过门必须修复）
DEC-GATE 作为硬前置，其真正价值在于**用真实项目数据驱动了 DEC-01 的 4 项必修**（MDR-010）——尤其揭穿了"TS 单测全绿"给出的假信心：拆解引擎在 M3 整个目标语言 Python 上原本零价值。这印证了"验收要跑真实目标场景、别用自测冒充里程碑"。

### 已知残留 / 推迟（不阻塞过门）
- toolz 残留机械单文件=1（孤立 barrel，凸性约束下不可并）——属合理残留，非缺陷。
- 合批放宽是"内聚加权打包"的轻量先行版；完整沿耦合重边加权合并仍按 §9 推迟，待更多真实数据。
- 大文件自动子文件拆分仍推迟（itertoolz 等超预算文件正确转人工）。
- funcy `__init__.py` 的 Barrel/PureConstant 子标签边界：不影响 is_mechanical 判定，记录备查。
