# Rust 迁移验证工作台 — 项目设计文档

> **版本**: v0.9 | **日期**: 2026-06-06

> ⚠️ **此文件已拆分**。完整设计文档位于 [`docs/design/`](./docs/design/README.md)。

## 快速导航

| 文件 | 内容 |
|------|------|
| [README.md](./docs/design/README.md) | 主索引、TL;DR、导航 |
| [01 定位与方法论](./docs/design/01-positioning-and-methodology.md) | 项目定位、三层范式、意图驱动翻译 |
| [02 架构设计](./docs/design/02-architecture.md) | 整体架构、状态机、上下文管理 |
| [03 执行与测试](./docs/design/03-execution-model.md) | Sprint 循环、Phase A/B 翻译、测试分层 |
| [04 工具链选型](./docs/design/04-toolchain.md) | Tier 0/1/2 工具矩阵 |
| [05 文档体系](./docs/design/05-documentation-system.md) | 规则拆分、知识沉淀、产出物 |
| [06 插件结构](./docs/design/06-plugin-structure.md) | Plugin 设计、Skill/Agent/Hook、扩展 |
| [07 陷阱与风险](./docs/design/07-pitfalls-and-risks.md) | 常见陷阱、风险评估、Plan B |
| [08 路线图](./docs/design/08-roadmap-and-reference.md) | M0-M4 路线图、成本、参考案例 |
| [09 Schema 参考](./docs/design/09-appendix-schemas.md) | JSON Schema、SKILL.md 骨架 |

## HTML 可视化

```bash
cd docs/design && python3 -m http.server 8765
# 打开 http://localhost:8765/index.html
```

## 归档

v0.8.1 及之前的完整单文件版本见 [PROJECT_DESIGN.legacy.md](./PROJECT_DESIGN.legacy.md)。
