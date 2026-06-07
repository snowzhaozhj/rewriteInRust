# 项目状态快照

> 每次会话结束前更新。新会话读此文件即可知道"在哪、做什么、下一步"。

## 当前阶段

**M0 假设验证** — 尚未开始（脚手架刚完成）

## 已完成

- [x] 设计文档 v0.9.4（9 轮对抗审查收敛，docs/design/）
- [x] 项目脚手架（CLI Cargo workspace + Plugin 骨架 + CI + Justfile）
- [x] GitHub 仓库初始化 (snowzhaozhj/rewriteInRust)

## 进行中

_无_

## 下一步（按优先级）

1. **M0 Spike 0**: 最小 Plugin 骨架验证（plugin/ 目录安装到 Claude Code,确认 skill/agent/hook 三件套 work）
2. **M0 Spike 1-4**: 并行验证（SubAgent 编排 / LSP 反馈 / tree-sitter 精度 / 指令跟随）
3. **CLI `graph build`**: 核心命令,依赖 tree-sitter + petgraph + rusqlite

## 阻塞项

_无_

## 关键决策待定

- Spike 0 失败时:走纯 CLAUDE.md + 项目级 agents/(不打包 Plugin) 还是等 Claude Code 更新?
- 集成验证用哪 3 个 <5K 行 TS 项目?(待选)

## 仓库结构

```
cli/          Rust CLI workspace (cargo check ✓)
plugin/       Claude Code Plugin 骨架
docs/design/  设计文档 v0.9.4
docs/review/  审查循环产物(9 轮)
docs/learnings/  开发知识沉淀(待填充)
docs/decisions/  项目自身 MDR(待填充)
fixtures/     验证用 TS 项目(待填充)
```
