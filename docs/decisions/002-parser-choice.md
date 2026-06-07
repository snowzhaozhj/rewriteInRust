# MDR-002: 解析器选型 — tree-sitter-typescript

## 背景

rustmigrate 需要从 TypeScript 源码中提取模块间依赖关系（imports、exports、calls），构建源码图。
解析器选型有两个候选：

1. **tree-sitter-typescript**（Rust 原生，增量解析）
2. **TypeScript Compiler API**（通过子进程调用 tsc 或 ts-morph，精度最高但引入 Node.js 依赖）

## 决策

**选择 tree-sitter-typescript** 作为 M1 阶段的解析器。

## 验证结果

### Spike S3 精度基准

对 20 个覆盖典型 TS 模式的代码片段进行精度测试：

| 维度 | Precision | Recall | F1 | TP/Pred/Truth |
|------|-----------|--------|------|---------------|
| Exports | 1.000 | 1.000 | 1.000 | 27/27/27 |
| Imports | 1.000 | 1.000 | 1.000 | 23/23/23 |
| Calls | 1.000 | 1.000 | 1.000 | 24/24/24 |

### 覆盖的模式

**Exports (10 种)**：
named function/class/const/var、interface/type/enum、
default function/class/expression、re-export named/star/star-as、type-only export

**Imports (7 种)**：
named、default、star (namespace)、mixed (default + named)、type-only、
side-effect、dynamic import()

**Calls (4 种)**：
simple function、method chain (a.b.c())、constructor (new)、nested calls

### 已有 fixture 验证

在 4 个 fixture 项目（linear-deps、diamond-deps、circular-deps、edge-cases）的
14 个 TS 文件上同样正确提取，包括：
- barrel re-export（6+ 个符号）
- 语法错误文件（部分提取）
- 空文件（零结果）
- 纯类型文件

### 版本兼容性

- tree-sitter = 0.22 + tree-sitter-typescript = 0.21 编译通过
- release 二进制 tree-sitter runtime 开销可忽略（pure Rust，无 C 依赖泄漏）

## 理由

1. **F1 = 1.0 远超 0.90 阈值**：在我们关注的三类关系上精度完美
2. **零外部依赖**：纯 Rust，无需 Node.js runtime
3. **增量解析**：M2 阶段增量图更新时可利用 tree-sitter 的增量能力
4. **容错解析**：语法错误文件仍可部分提取（`broken` 和 `valid` 都被识别）

## 已知局限

1. **多变量声明**：`export const x = 1, y = 2;` 单行多变量声明暂未完全支持（M1 实现时补全）
2. **模板字符串动态 import**：`import(\`./\${name}\`)` 的路径提取为模板字面量原文
3. **语义精度**：tree-sitter 只做语法分析，不做类型推断。
   如需类型级依赖（如 `implements` 关系），需补充语义分析层
4. **装饰器**：TS 5.0 装饰器语法需确认 tree-sitter-typescript 0.21 支持程度

## 替代方案

- **TypeScript Compiler API (tsc)**：精度更高（含类型信息），但引入 Node.js 依赖，
  性能较差（非增量），与「纯 Rust CLI」目标矛盾。保留为 M3+ 的可选增强层。
- **swc parser**：Rust 原生，但更偏向转换/编译场景，API 不如 tree-sitter 适合查询。

## 后果

- M1 Graph 模块使用 tree-sitter-typescript 构建源码图
- 提取逻辑位于 `cli/crates/core/src/ts_extract.rs`（Spike 验证产物，M1 重构到 `graph/build.rs`）
- M2 阶段如发现精度不足，可降级到 tsc subprocess 方案（PlanB 仍保留）
