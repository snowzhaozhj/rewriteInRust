# MDR-009: 适配器 shell 脚本模式取消，adapter 目录契约 = analysis-tools.json + porting-template.md

- **状态**: 已决策
- **日期**: 2026-06-27
- **范围**: M3-PLG-01（含追认 M1 起的既有偏离）

## 问题

设计文档 06-plugin-structure §11.2 把语言适配器定义为"目录约定 + shell 脚本契约"：每个 `adapters/<lang>/` 目录应含 `adapter.json`（JSON Schema 元数据）、`detect.sh`（语言检测）、`extract-types.sh` / `extract-deps.sh`（AST/依赖提取）、`ffi-bridge.sh`、`analysis-tools.json`、`porting-template.md`，并把 `adapter.json` Schema 合规列为 CI 门禁。

但**该 shell 脚本模式从未落地**（`git log --all -- '**/adapter.json' '**/detect.sh'` 全仓库无任何提交）。M1 起实际架构改走：

- **语言检测**：`analyze.md` Step 2 由 Claude 读目录特征文件（`package.json`/`pyproject.toml`/`go.mod` 等）判定，非 `detect.sh`。
- **依赖/类型提取**：CLI `rustmigrate graph build` 走 tree-sitter（内置），非 `extract-*.sh`。
- **工具可用性检测**：CLI `rustmigrate profile --adapter-tools <analysis-tools.json>`，只消费 `analysis-tools.json`。
- **翻译规则**：translator Read `porting-template.md`。

TS 参照适配器目录因此实际只有 `analysis-tools.json` + `porting-template.md` 两个文件。M3 PLG-01 任务文字（PLAN-M3:109）和验收项（PLAN-M3:117）仍按设计文档字面要求建 `adapter.json` + `detect.sh`，与实现脱节。

## 决策

**取消适配器 shell 脚本模式**。adapter 目录契约固定为两个文件：

| 文件 | 用途 | 消费方 |
|------|------|--------|
| `analysis-tools.json` | 语言专用外部工具列表（profile 检测可用性） | CLI `profile --adapter-tools` |
| `porting-template.md` | 语言 → Rust 迁移规则模板 | translator（analyze 规则生成步） |

`adapter.json` / `detect.sh` / `extract-types.sh` / `extract-deps.sh` 作废——其职责已分别由 LLM 目录检测、CLI tree-sitter、CLI profile 承担。（`ffi-bridge.sh` 的取消已由 MDR-007 记录。）

Python adapter（`adapters/python/`）据此只建 `analysis-tools.json` + `porting-template.md`，与 TS 参照结构对齐。

## 理由

- 解析逻辑下沉到 CLI（tree-sitter）是 M3 架构核心（D-M3-04：import 解析是语言内部事务，图引擎保持语言无关），与"每语言一套 shell 脚本"模式互斥。
- 为对齐废弃契约而补建 `adapter.json`/`detect.sh` 会产出一组无人调用的死文件，反而与工作流不符。
- 单一权威：adapter 目录两文件契约简单、可被现有 CLI/skill 直接消费，无需额外 CI Schema 门禁。

## 影响

- `docs/design/06-plugin-structure.md` §11.2 需更新：删除 shell 脚本契约描述（adapter.json Schema / detect.sh / extract-*.sh / §11.2.1 脚本 I/O Schema / adapter.json CI 门禁），改为"adapter 目录 = analysis-tools.json + porting-template.md"。**本 MDR 记录决策，§11.2 正文同步在 Sprint C 收尾或 M3-VAL-07 设计文档同步任务中执行。**
- `docs/PLAN-M3.md`：PLG-01 任务描述与验收项 117 行的 `adapter.json` 要求标注作废（改为两文件契约）。
- `adapters/python/` 只含 `analysis-tools.json` + `porting-template.md`（PR-C1）。
- 既有 TS 适配器无需改动（早已是两文件结构）。
