# SubAgent / Skill 提示词编写规范（项目沉淀）

> 调研来源：Claude Code 官方文档（sub-agents / skills / plugins）+ skill-creator SKILL.md 最佳实践 + 本机 agent 注册表实证。
> 用途：本项目 `plugin/agents/*.md` 与 `plugin/skills/migrate/*.md` 的编写/优化基线，避免重复探索。
> 写作纪律：imperative 语气、解释 why、token 精炼但不漏关键。

## 1. 共通写作原则（agent body + skill body）

- **imperative 语气**：「读取 X → 校验 Y → 写 Z」式分步，不要段落式叙述。
- **解释 why，不堆 MUST**：说明某约束为何重要，让模型理解而非机械执行。满篇大写 ALWAYS/NEVER 是反模式信号——改为给出理由。
- **token 精炼**：删掉不"拉动重量"的内容；SubAgent 上下文隔离，**不要重复全局规则/CLAUDE.md**。
- **渐进式披露（progressive disclosure）**：细节放 body 或 references/，触发时才加载；摘要放 frontmatter description。SKILL.md body 理想 < 500 行，超了就加一层 references/ 并给清晰指引「何时去读哪个文件」。大参考文件（>300 行）加目录。
- **输出格式显式化**：用模板 + Input/Output 示例固定产出结构（下游靠文件/Schema 校验，不解析对话文本）。

## 2. SubAgent 定义文件（`agents/*.md`）

### Frontmatter
- **必填**：`name`（小写+连字符，Hooks 收到该值作 `agent_type`）、`description`。
- **可选**（按本项目相关度）：
  | 字段 | 含义 | 默认 |
  |------|------|------|
  | `tools` | 工具白名单。**省略=继承主会话全部**；显式列举=严格白名单 | 全部 |
  | `model` | `sonnet`/`opus`/`haiku`/`fable`/完整 ID/`inherit` | `inherit`（继承主会话） |
  | `disallowedTools` | 工具黑名单 | 无 |
  | `permissionMode` / `maxTurns` / `memory` / `skills` / `isolation` 等 | 见官方 sub-agents.md | — |
- **`description` 是自动委派的触发依据**：写清「何时用 + 做什么」，含「Use proactively」「Use when…」促进触发。泛泛的 `"A code reviewer"` 触发率低。
- **`tools` 最小权限**：只读型 agent 用 `Read, Grep, Glob`，不要默认给 `Write, Edit, Bash`。本项目 analyzer/verifier 偏分析，translator/scaffolder 需写文件。

### Body（system prompt）
- 仅写该 agent 的自定义指引，**不含** Claude Code 全局系统提示、主对话历史、已读文件（除非 `skills:` 预注入）。
- 组织：角色定位 → 输入/输出契约 → 分步工作流（1-2-3）→ 检查清单 → 输出格式。

### 交互契约（context 隔离）
- SubAgent 返回**摘要**给主代理；工具调用链/中间日志/读文件**不污染**主会话。
- SubAgent 启动上下文 = 自身 system prompt + 委派消息 + CLAUDE.md/memory + git 快照 +（可选）预注入 skills。**看不到主对话历史**（`fork` 型例外）。
- 本项目据此设计：SubAgent 间通过 `.rust-migration/` 文件 + JSON 通信，串行执行，主 Skill 只校验产出物文件（L1/L2），不解析返回文本。

## 3. Skill（`SKILL.md` + 子命令 `.md`）

- **必填 frontmatter**：`description`（自动触发依据，写「何时调用 + 做什么」，可略"pushy"提升触发）。
- 可选：`name`（覆盖文件夹名）、`disable-model-invocation`（仅 `/cmd` 手动）、`disable-user-invocation`（仅 Claude 自动）、`context: fork`。
- body 渐进式披露：主 SKILL.md 给工作流 + 选择逻辑，分支细节进 references/ 或子命令文件。

## 4. Plugin 内 SubAgent 命名（M1-PLG-05 定论）

- **插件 SubAgent 强制命名空间前缀**：`agents/analyzer.md`（插件 name=`rust-migrate`）→ agentType 为 **`rust-migrate:analyzer`**。子目录 `agents/review/x.md` → `rust-migrate:review:x`。
- **Agent 工具参数名是 `subagent_type`**（设计文档 §10.2.1 写作 `agentType` 是概念名；实际工具参数以 `subagent_type` 为准）。
- 实证：当前会话 agent 注册表中存在 `codex:codex-rescue`、`pr-review-toolkit:code-reviewer` 等插件命名空间 agentType，可直接经 `subagent_type` 调用 → 证实 `plugin:agent` 形式可用且必需。
- 优先级链（同名覆盖）：managed > `--agents` flag > `.claude/agents/` 项目级 > `~/.claude/agents/` 用户级 > **plugin agents（最低）**。

## 5. Plugin 仓库结构与安装（monorepo）

本仓库是 monorepo：插件本体在 `plugin/`（`plugin/.claude-plugin/plugin.json`），CLI 在 `cli/`。

- **plugin.json**：`name`+`version` 必填；`author` 必须是**对象** `{name}`（非字符串，校验器强制）；**不要**声明 `skills`/`agents`/`hooks` 路径——目录约定自动发现（`skills/`、`agents/`、`hooks/hooks.json`），且 `agents`/`commands` 等是"替换默认"语义，误声明会绕过默认扫描。
- **可安装性**：子目录插件须在**仓库根**放 `.claude-plugin/marketplace.json`，`plugins[].source` 指向 `./plugin`（相对 marketplace root=仓库根，不能含 `..`）；marketplace 需 `name`+`owner.name`，建议加 `description`。否则无法 `/plugin marketplace add`。
- **安装/开发**：用户 `/plugin marketplace add snowzhaozhj/rewriteInRust` → `/plugin install rust-migrate@rust-migrate-catalog`；本地开发 `claude --plugin-dir ./plugin`。
- **提交前校验**（已验证通过）：`claude plugin validate .`（marketplace）+ `claude plugin validate ./plugin`（插件本体，含 skill/agent/hook frontmatter 解析）。

## 6. Plugin Live 验证经验（实跑验证，复用降本）

静态校验（`claude plugin validate`）只查 frontmatter/schema；**真正的端到端行为必须 Live 跑**——2026-06-14 的 Live 验证正是靠实跑才发现 analyze→run 的状态填充缺口（静态审查全没发现）。方法论：

- **不安装、不污染本会话**：用独立 headless 进程驱动 `claude --plugin-dir <绝对路径>/plugin -p "<prompt>"`。本会话默认**不加载**开发中的插件（可用 agent/skill 列表里看不到 `rust-migrate:*`），所以不能在本会话内调 `/migrate`，必须 `--plugin-dir`。
- **命名空间确认**：headless 跑 `-p "列出可用 skills 和 agents 名称"` 即可实测插件注册形态——本插件实测 `/migrate`→`rust-migrate:migrate`、SubAgent=`rust-migrate:{analyzer,translator,scaffolder,verifier}`，确认 `<plugin-name>:` 前缀。这是最便宜、最高价值的第一步，先做。
- **`-p` headless 限制**：① 人类确认门禁（如 run 的 Step 1.5 `auto_confirm_intent=false`）无法交互，会卡住——验证 run 类需临时设 `auto_confirm_intent=true` 或只验 analyze；② stdout 末尾常为空，**结论靠核查产出物文件**（`.rust-migration/` 下的 state/db/规则），不靠 stdout；③ 写操作需 `--dangerously-skip-permissions`。
- **隔离**：把 fixture 复制到 `/tmp` 再跑，绝不动仓库；CLI 先 `cargo build` 并 `export PATH=.../target/debug:$PATH`（skill 裸调 `rustmigrate` 假设在 PATH，见 M1-BOOT-01）。
- **分阶段、带超时**：analyze 串行 spawn 3 个 SubAgent，实测 ~9 分钟。每条命令 `timeout`，先廉价验证（加载+命名空间）再跑重的端到端，避免一个大命令静默卡死（呼应 [[feedback_watchdog_stall]]）。
- **核查清单**（analyze）：`migration-state.json` 的 `state` 值、`source-graph.db` 节点/边/calls 计数、`porting/` 规则含关键标题、`PARITY.md`/golden fixtures 真实非占位、SubAgent 是否实际产出（专属产出物存在＝触发证据）。注意 schema 权威字段（modules/sprint/subagent_calls）是否真被填——**SKILL 指令说"填"≠ 真能填**（需对应 CLI 写命令存在）。

## 7. 官方出处

- SubAgent：https://code.claude.com/docs/en/sub-agents.md
- Skill：https://code.claude.com/docs/en/skills.md
- Plugin：https://code.claude.com/docs/en/plugins.md
- skill-creator 最佳实践：本机 `~/.claude/plugins/marketplaces/anthropic-agent-skills/skills/skill-creator/SKILL.md`
