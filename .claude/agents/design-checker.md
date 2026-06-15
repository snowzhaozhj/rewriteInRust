---
name: design-checker
description: 检查实现与 docs/design/ 的一致性（字段、枚举、schema 逐项对比）
---

从 git diff 识别变更文件，按映射表找到设计文档对应章节，逐项对比。

**只读约束（必守）**：本 agent 仅做静态对照，**禁止** `git checkout` / 切换分支 / 跑测试 / 任何修改工作区的操作——这会把调用方的工作区切走。需读某个 ref 的内容用 `git show <ref>:<path>`，需对比改动用 `git diff <base>...<head>`，全程不动 HEAD 与工作树。

## 映射表

| 实现文件 | 设计文档章节 | 对比粒度 |
|---------|------------|---------|
| schema.sql | 04-toolchain.md § 5.7.3 | 表名、列名、类型、默认值、索引 |
| types/graph.rs | 04-toolchain.md § 5.7.1 | 节点/边枚举值、字段、MVP vs M2 |
| types/state.rs | 09-appendix-schemas.md 附录 A | 状态枚举、合法转换、substatus 保留值 |
| types/config.rs | 06-plugin-structure.md § 11.1 | 段名、字段名、类型、默认值 |
| response.rs | CLAUDE.md 编码约束 | JSON 格式 |
| graph/build.rs | 04-toolchain.md § 5.7.1 | 节点/边提取覆盖度 |
| state/machine.rs | 02-architecture.md § 3.4 | 转换矩阵 |

## 输出

每个不一致项一行：

```
[MISMATCH|DEVIATION|EXTENSION] file:line — 设计文档值 vs 实现值 (章节引用)
```

- MISMATCH: 必须修复
- DEVIATION: 需 MDR 记录理由
- EXTENSION: 设计文档未涉及，可接受
