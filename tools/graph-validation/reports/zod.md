# 源码图差分校验报告 — zod

- 仓库 SHA：`ca42965df46b2f7e2747db29c40a26bcb32a51d5`
- src 根：`src`
- 自研图：80 文件 / 129 import 边
- dependency-cruiser：134 边 · dpdm：134 边
- Oracle 交集（dc ∩ dpdm）：134 边

## 硬门结果

| 指标 | 值 | 门槛 | 结果 |
|------|----|------|------|
| 边召回率（自研 ∩ vs oracle 交集） | **0.9627** | ≥ 0.98 | ❌ 不达标 |
| 环节点一致 | missing 4 / extra 0 | 双向为 0 | ❌ 不一致 |

**综合硬门：❌ 不达标（见下方根因分析）**

## 边召回明细

- oracle 交集边数：134
- 自研图命中：129
- 缺失（oracle 交集有、自研图无）：5

缺失边样本（最多 30 条，`from -> to`）：

```
ZodError -> index
__tests__/catch.test -> index
__tests__/default.test -> index
__tests__/firstpartyschematypes.test -> index
__tests__/recursive.test -> index
```

- 自研图多出且双 oracle 都不认的边：0（可能是自研误报或 oracle 漏报）

## 环对比

- 自研图环上节点数：4
- oracle 交集环上节点数：8
- 自研缺失的环节点：4
- 自研多出（双 oracle 都不认）的环节点：0

缺失环节点样本：`ZodError`, `errors`, `helpers/parseUtil`, `locales/en`

- 自研图检测到的 SCC（环）数：1

## 根因分析（待人工核对填充）

> 自动判定未达标。请按以下方向核对，区分「自研 bug / 归一化口径差 / oracle 噪声」：
> - 缺失边：检查是否为 barrel re-export、动态 import、tsconfig paths 别名；
> - 多余边：检查是否为 type-only / 注释内 import 误提取；
> - 环差异：检查是否因个别边缺失导致 SCC 断裂。

