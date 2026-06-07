# MDR-001: Plugin 可行性验证

## 背景

rustmigrate 设计为 Claude Code Plugin，通过 skill/agent/hook 三件套实现 AI 辅助迁移。
需验证 Plugin 基础设施是否可用，以及 Rust CLI 二进制是否合理。

## 决策

**确认 Plugin 方案可行**，继续使用 skill/agent/hook 架构。

## 验证结果

### 结构验证（自动化）

| 检查项 | 结果 | 说明 |
|--------|------|------|
| plugin.json | ✓ | 与设计文档 §10.0 一致 |
| Skills 目录 | ✓ | SKILL.md + analyze/run/review 3 个子命令 |
| Agents 目录 | ✓ | analyzer/translator/verifier/scaffolder 4 个 |
| Hooks 配置 | ✓ | hooks.json 含 PostToolUse + PreToolUse |
| Hook 脚本 | ✓ | 3 个 shell 脚本均可执行（+x） |
| Release 二进制 | ✓ | 743K（LTO thin + strip），远低于 50MB |

### 二进制大小分析

```
rustmigrate (release, lto=thin, strip=true): 743K
```

当前二进制包含 petgraph、rusqlite(bundled)、tree-sitter、tokei 等全部依赖。
743K 说明 Rust 的链接时优化很有效。M1 增加更多功能后预计仍 < 5MB。

### 待 Live 验证项（需交互式 Claude Code 会话）

| 检查项 | 验证方法 |
|--------|---------|
| Skill 触发 | 在 Claude Code 中输入 `/migrate analyze`，观察是否路由到 analyze.md |
| Agent 调用 | 通过 skill 流程调用 analyzer agent，检查是否正常执行 |
| Hook fire | 编辑 .rs 文件后检查 `cargo fmt` 是否自动执行（PostToolUse） |

> **注意**：后台 Job 无法执行 live 验证。建议在下次交互式会话中补全这三项测试。
> 即使 live 测试失败，PlanB（纯 CLI + 手动协调）仍可行。

## 理由

1. **二进制极小**：743K 二进制说明依赖选型合理，分发无压力
2. **结构完整**：skill/agent/hook 三件套文件齐全，与设计文档一致
3. **增量可行**：当前 agent/skill 均为 TODO 骨架，可在 M1 Phase 3 逐步充实
4. **降级成本低**：即使 Plugin 加载失败，CLI 本身是独立的；
   skill/agent 可降级为 CLAUDE.md 中的指令 + 手动 workflow

## 替代方案

- **纯 CLI + 手动协调**：放弃 Plugin 架构，用户手动在 Claude Code 中执行 CLI 命令。
  优点：零 Plugin 依赖。缺点：无自动化编排，用户体验差。
  仅在 live 验证全部失败时考虑。

- **MCP Server 方案**：将 CLI 包装为 MCP Server，通过 tool use 协议与 Claude Code 通信。
  优点：更标准化。缺点：增加架构复杂度，M1 阶段不值得。
  保留为 M4 生态完善方向。

## 后果

- M1 Phase 3 在 Plugin 骨架上充实实现
- 首次交互式会话补全 live 验证
- 如 live 验证失败，在 `docs/decisions/` 追加变更记录
