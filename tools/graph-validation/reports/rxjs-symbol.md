# 符号级精度差分报告（文件级聚合）— rxjs

- 仓库 SHA：`x`　src 根：`src/internal`
- oracle：ts-morph 类型检查器（真值）　预测：自研 tree-sitter 启发式
- 对比口径：**文件级聚合**（caller_file→callee_file，忽略符号名）
- 软门：F1 < 0.7 标注「⚠️ 启发式效果偏低」，**不阻断**（退出码恒 0）

## 文件级 precision / recall / F1（以 ts-morph 为真值）

| 关系 | 自研边数 | oracle 边数 | 命中(TP) | precision | recall | F1 |
|------|---------|------------|---------|-----------|--------|----|
| Calls（函数调用） | 480 | 734 | 480 | 1 | 0.654 | **0.7908** |
| Extends（继承） | 28 | 28 | 28 | 1 | 1 | **1** |
| Implements（接口实现） | 7 | 7 | 7 | 1 | 1 | **1** |

> precision = 自研边中被 oracle 认可的比例（误报越少越高）；
> recall = oracle 边中被自研覆盖的比例（漏报越少越高）。

## Calls（函数调用） 明细

- 自研 480 边 / oracle 734 边 / 命中 480
- 漏报（oracle 有、自研无）：254　误报（自研有、oracle 无）：0

漏报样本（最多 20，`from -> to`）：
```
AsyncSubject -> Subject
AsyncSubject -> Subscriber
BehaviorSubject -> Subject
BehaviorSubject -> Subscriber
Notification -> types
Observable -> Operator
ReplaySubject -> Subject
ReplaySubject -> Subscriber
ReplaySubject -> types
Scheduler -> scheduler/Action
Subject -> Subscriber
Subject -> types
Subject -> util/ObjectUnsubscribedError
Subscriber -> scheduler/timeoutProvider
Subscriber -> types
Subscription -> types
Subscription -> util/UnsubscriptionError
ajax/ajax -> Subscriber
ajax/ajax -> ajax/errors
ajax/ajax -> ajax/types
```

## Extends（继承） 明细

- 自研 28 边 / oracle 28 边 / 命中 28
- 漏报（oracle 有、自研无）：0　误报（自研有、oracle 无）：0

## Implements（接口实现） 明细

- 自研 7 边 / oracle 7 边 / 命中 7
- 漏报（oracle 有、自研无）：0　误报（自研有、oracle 无）：0

## 符号级 stretch（参考，不计入软门）

自研 caller 侧无 enclosing 符号（Calls 边 source 是文件节点），caller 符号无法对齐；
此处仅给 callee 符号名集合的 Jaccard 重合度作弱参考：

- 自研 callee 符号名：190　oracle callee 符号名：161
- 交集：108　Jaccard：0.4444

> 符号级精确对比的口径对齐难点见 `tools/graph-validation/SYMBOL-PRECISION.md`。

## 软门结论

✅ 各关系类别 F1 均达警示阈值以上。

