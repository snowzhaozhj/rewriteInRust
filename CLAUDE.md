# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概述

这是 **Rust 迁移验证工作台** 的设计文档仓库（当前阶段：纯设计，尚未进入实现）。目标产出物是一个 Claude Code Plugin + `rustmigrate` CLI，帮助开发者将 TS/Python/C 项目迁移到 Rust。

## 仓库结构

- `docs/design/` — 设计文档主体（v0.9.4，10 个 Markdown 子文件 + schemas/ + index.html）
- `docs/design/README.md` — 主索引和 TL;DR，**从这里开始阅读**
- `docs/design/schemas/` — JSON Schema 示例文件（migration-state、source-graph、type-map、call-graph）
- `PROJECT_DESIGN.md` — 重定向到 docs/design/README.md
- `PROJECT_DESIGN.legacy.md` — v0.8.1 单文件归档（106KB，勿修改）

## 文档查看

```bash
cd docs/design && python3 -m http.server 8765
# 浏览器打开 http://localhost:8765/index.html
```

## 核心术语

- **State**：编排器状态机节点（INIT/PROFILE/PLAN/SCAFFOLD/SPRINT_LOOP/GRADUATE）
- **Milestone (M)**：实施路线图阶段（M0 验证/M1 MVP/M2 质量/M3 多语言/M4 完善）
- **Sprint**：SPRINT_LOOP 内的迭代循环
- **Phase A/B**：模块级翻译阶段（忠实翻译 vs 惯用化优化），与路线图阶段无关
- **迁移规则**：存储在 `.rust-migration/porting/` 目录下的规则文件（不再使用 "PORTING.md" 单文件称谓）
- **source-graph.db**：源码图的主存储（SQLite），JSON 为导出格式

## 编辑设计文档的注意事项

- 跨文件引用使用相对路径链接（如 `[见 04 § 5.7](./04-toolchain.md#57-图存储与查询架构)`）
- 术语必须与 README.md 的命名约定一致（State/Milestone/Sprint/Phase A-B）
- 修改某个文件后检查是否有其他文件引用了相同概念（`grep -rn "关键词" docs/design/`）
- 版本号在 README.md 第 3 行和 index.html 中需同步更新
- MVP CLI 为 11 个命令（v0.9.4 裁剪后），M2 扩展 5 个，以 06-plugin-structure.md 为准
- 工作量估算以 08-roadmap-and-reference.md 为唯一权威来源
- source-graph 主存储是 SQLite（.db），JSON 是导出格式；文档中不应出现 `source-graph.json` 作为主存储路径
- 配置文件 `.rustmigrate.toml` 位于项目根目录（不在 `.rust-migration/` 内）

## Commit 规范

```
docs: vX.Y.Z 简要描述
```
- 前缀固定为 `docs:`（纯设计文档仓库）
- 包含版本号变化时标注 vX.Y.Z
- commit message 用中文，Co-Authored-By 行自动添加

## 文件权威来源

同一信息在多个文件中出现时，以下文件为唯一权威：
- CLI 命令列表 → `06-plugin-structure.md`
- 工作量估算 → `08-roadmap-and-reference.md`
- 图数据模型 → `04-toolchain.md § 5.7.1`
- 迁移案例参考 → `08-roadmap-and-reference.md § 14`
- 产出物目录结构 → `06-plugin-structure.md § 10.6`
- 状态机定义 → `02-architecture.md § 3.4` + `09-appendix-schemas.md`
- 迁移规则体系（26 类规则列表）→ `05-documentation-system.md § 6.2`
- 规则分层策略（核心/参考/项目专有）→ `06-plugin-structure.md § 10.1.1`
- Plugin 目录结构 → `06-plugin-structure.md § 10.0`

## 调研与审查方法论

- 开源项目 clone 到 `~/workspace/explore/` 统一管理
- LLM 调研结果必须交叉验证（已确认的幻觉：Pokemon Showdown JS→Rust、act101/Holonic/ShiftCodex、Bun 576 行 PORTING.md）
- 多维度审查用并行 subAgent（可行性/架构合理性/架构明确性/文档质量），每个维度 10 分制打分
- 修复后必须重新审查确认分数提升，不能自评
- subAgent 写大文件容易卡住，用轻量方案替代（分段写入或直接编辑）

## 设计文档一致性检查清单

修改设计文档后，运行以下检查避免跨文件矛盾（本次会话的四轮审查中反复出现这些问题）：

```bash
# 检查 source-graph.json 残留（主存储应为 .db）
grep -rn "source-graph\.json" docs/design/ --include="*.md"
# 检查 PORTING.md 单文件残留（应为 porting/ 目录）
grep -rn "PORTING\.md" docs/design/ --include="*.md"
# 检查 .claude/rules/ 残留（Plugin 不支持 rules/ 分发）
grep -rn "\.claude/rules" docs/design/ --include="*.md"
# 检查 "4 步" 残留（应为"3 次 SubAgent 调用"或"7 步序列"）
grep -rn "4 步\|4步" docs/design/ --include="*.md"
```
