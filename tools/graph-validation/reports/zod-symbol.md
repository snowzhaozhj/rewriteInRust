# 符号级精度差分报告（文件级聚合）— zod

- 仓库 SHA：`ca42965df46b2f7e2747db29c40a26bcb32a51d5`　src 根：`src`
- oracle：ts-morph 类型检查器（真值）　预测：自研 tree-sitter 启发式
- 对比口径：**文件级聚合**（caller_file→callee_file，忽略符号名）
- 软门：F1 < 0.7 标注「⚠️ 启发式效果偏低」，**不阻断**（退出码恒 0）

## 文件级 precision / recall / F1（以 ts-morph 为真值）

| 关系 | 自研边数 | oracle 边数 | 命中(TP) | precision | recall | F1 |
|------|---------|------------|---------|-----------|--------|----|
| Calls（函数调用） | 41 | 106 | 41 | 1 | 0.3868 | **0.5578** ⚠️ |
| Extends（继承） | 0 | 0 | 0 | n.a. | n.a. | **n.a.** |
| Implements（接口实现） | 0 | 1 | 0 | 0 | 0 | **0** ⚠️ |

> precision = 自研边中被 oracle 认可的比例（误报越少越高）；
> recall = oracle 边中被自研覆盖的比例（漏报越少越高）。

## Calls（函数调用） 明细

- 自研 41 边 / oracle 106 边 / 命中 41
- 漏报（oracle 有、自研无）：65　误报（自研有、oracle 无）：0

漏报样本（最多 20，`from -> to`）：
```
__tests__/all-errors.test -> ZodError
__tests__/all-errors.test -> types
__tests__/anyunknown.test -> types
__tests__/array.test -> types
__tests__/async-parsing.test -> types
__tests__/async-refinements.test -> types
__tests__/base.test -> types
__tests__/bigint.test -> types
__tests__/branded.test -> types
__tests__/catch.test -> types
__tests__/coerce.test -> types
__tests__/complex.test -> types
__tests__/crazySchema -> types
__tests__/custom.test -> types
__tests__/date.test -> types
__tests__/default.test -> types
__tests__/description.test -> types
__tests__/discriminated-unions.test -> helpers/util
__tests__/discriminated-unions.test -> types
__tests__/enum.test -> types
```

## Implements（接口实现） 明细

- 自研 0 边 / oracle 1 边 / 命中 0
- 漏报（oracle 有、自研无）：1　误报（自研有、oracle 无）：0

漏报样本（最多 20，`from -> to`）：
```
types -> helpers/parseUtil
```

## 符号级 stretch（参考，不计入软门）

自研 caller 侧无 enclosing 符号（Calls 边 source 是文件节点），caller 符号无法对齐；
此处仅给 callee 符号名集合的 Jaccard 重合度作弱参考：

- 自研 callee 符号名：89　oracle callee 符号名：164
- 交集：28　Jaccard：0.1244

> 符号级精确对比的口径对齐难点见 `tools/graph-validation/SYMBOL-PRECISION.md`。

## 软门结论

⚠️ 存在 F1 < 阈值的关系类别（属预期：启发式精度必然低于类型系统）。
请按下列方向区分「自研可改进 / 口径差异 / 启发式固有局限」：
- Calls 漏报：跨文件方法调用 `obj.method()`（自研只解析顶层函数/构造）、
  re-export 链、命名空间深层调用、回调/高阶传递；
- Calls 误报：同名不同模块的兜底匹配命中错误文件；
- Extends/Implements 漏报：跨多层 barrel 的基类型、泛型基类、外部基类型（已剔）。

