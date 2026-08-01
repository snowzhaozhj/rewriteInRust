# MDR-020: scaffold 外层 workspace 牵连检测——判据取「问状态」而非「比变化」

- **状态**: 已落地（2026-08-02，PR #87）
- **日期**: 2026-08-01（判据经两轮审查各自推翻后重写）
- **范围**: `cli/crates/core/src/scaffold/template.rs`（检测实现）+ CLI `cmd_scaffold_workspace`（warnings 接线）+ `docs/design/06-plugin-structure.md` 命令表行 + `plugin/agents/scaffolder.md` R1 护栏。收口 [#86](https://github.com/snowzhaozhj/rewriteInRust/pull/86) 审查记账 TODO ②。含**源码破坏性变更**（见文末登记）。

## 背景

`rustmigrate scaffold workspace` 委托 `cargo init` 生成迁移目标骨架。**用户的典型场景恰是「已有 Rust workspace 的仓库里迁模块进来」**，而此时目标 crate 会成为该 workspace 的成员——此后该仓库的 `cargo build --workspace` / `cargo test --workspace` 会连带编译迁移产物，而迁移中的 crate 常处于 `unimplemented!()` / `TODO(port)` 的不可编译中间态，足以让用户原本通过的构建开始失败。

CLI 此前对此**返回 `status:ok` 零 warning**——静默牵连了用户仓库的构建配置。#86 审查期间编排器实测确认：`/tmp` 造含 `[workspace] members=["crates/existing"]` 的父仓，在其中执行 scaffold → CLI 报 `{"status":"ok"}`，而父 `members` 已被静默改成 `["crates/existing", "crates/migrated"]`。

该路径此前**零测试覆盖**：全部 scaffold 测试都在裸 tempdir 里跑。

## 决策

**检测判据 = 问「当前状态」，不比「变化量」，不匹配文案。** 具体：调 `cargo metadata --format-version 1 --no-deps`（在目标目录下执行），判断目标是否出现在 `workspace_members` 中。

三个候选判据里，前两个均经审查实证推翻——**理由须留存，否则下个改动者会退回朴素方案**：

### ⒜ 匹配 `cargo init` 的 stderr 文案 —— 排除

`cargo init` 追加成员时打 `Adding ... as member of workspace`。但该文案随 cargo 版本变动、且可能被本地化。判据不该对工具的人类可读输出做假设。

### ⒝ 比对父 manifest 的改动前后内容 —— 初版采用，被主审实证推翻（阻断级）

初版实现 `warn_if_parent_workspace_mutated(path, before)`：`cargo init` 前后各读一次父 manifest，内容不等则告警。

**结构性盲区**：父 workspace 写 `members = ["crates/*"]`（glob）时，cargo **根本不改 manifest**，新 crate 却自动成为成员——比对判据在这类仓库里**永远不可能触发**，而危害分毫不减。

编排器独立复现确认：glob 父仓下 CLI 报 `ok` 零 warning、父 manifest 逐字节未变；往新 crate 塞 `compile_error!` 后父仓 `cargo build --workspace` 立即变红。**glob 不是罕见写法**——`~/workspace/explore/oxc/Cargo.toml` 就是 `members = ["apps/*", "crates/*", "napi/*", "tasks/*"]`。

附带缺陷（同源）：词法匹配 `[workspace]` 段头会漏掉 Cargo 接受的多种等价写法。异构交叉审查（codex）实测这 5 种下父 manifest 被改而 CLI 仍报 `ok`，编排器逐一复现：`[workspace.package]` / `[workspace.dependencies]` / `[ workspace ]`（带空格）/ `workspace.members = []`（dotted）/ `workspace = { members = [] }`（inline table）。

### ⒞ 问 `cargo metadata` 的成员关系 —— 采用

`cargo metadata` 自己解析 `members` / glob / `exclude` / `default-members`，是成员关系的**权威真值源**。一个判据同时覆盖：显式 members 追加、glob 覆盖、以及「上次运行已把它加进去、用户没看到告警」的重跑场景。

`exclude` 语义尤其值得交给它——手写判据很难做对，而实测 excluded crate 的 `workspace_root` 指向自身，天然走「目标即自己的 workspace 根」短路、正确不告警。

实现要点（每条都有实测依据）：

- **`--no-deps`**：实测它不解析依赖图——`[dependencies]` 里写一个不存在的 crate，metadata 仍 exit 0。故检测不受网络 / 私有 registry / 依赖不可解析影响。
- **路径先绝对化再比对**：`Path::ancestors()` 对相对路径只走到 `""`（当前目录），到不了 `..` 及更上层（实测 `"crates/rel"` 的 ancestors 为 `["crates", ""]`）。而 `--target` 默认值就是相对的 `rust`，故「用户在 workspace 子目录里执行」是默认路径而非边缘情况。
- **比对两侧过 `canonicalize`**：metadata 返回符号链接解析后的真实路径（macOS 上 `/tmp/x` → `/private/tmp/x`），而绝对化只做词法消解。最初的实现正是在此静默失效（测试用的 `TempDir` 就在 `/var` → `/private/var` 下）。展示给用户的仍是未 canonicalize 的形态——那是用户输入的样子，更容易认。
- **metadata 失败不静默**：实测「裸目录」与「workspace 已有语法坏成员」**都是 exit 101**，故不能靠 stderr 文案区分（同 ⒜ 的理由）。改按「目标的祖先目录里是否存在任何 `Cargo.toml`」：无 → 裸目录 scaffold，正常，不告警；有 → 检测确实没能进行，如实报「无法判定」+ 提示手工确认，不让调用方以为已确认无事。

### 处置 = 告警不报错

成为 member 本身没破坏任何东西，且用户可能**确实想要**这个结果（把迁移产物纳入 workspace 是合理意图）。故汇入 `warnings` + 降级 `status=warning`，能否接受由用户判断；CLI 的职责是不让它静默发生。

**告警文案两处经审查订正**：

1. **必须同时提 `exclude`**——主审实测：照旧文案「从 `members` 移除该条目」操作后，cargo 报 `current package believes it's in a workspace when it's not`，用户得到一个编译不了的 crate。而 `scaffolder.md` 又禁止 agent 自行加 `exclude`，旧文案把用户领进死路。
2. **危害范围限定到 `--workspace`**——设计契约审查实测：配了 `default-members` 时裸 `cargo build` / `cargo test` **不**编译迁移产物，只有 `--workspace` 才会。原文案「该仓库的 `cargo build`/`cargo test` 会连带编译」是过度承诺。

### 分工：CLI 检测并如实报，不代用户决定

`plugin/agents/scaffolder.md` R1 护栏：**agent 不得自行编辑用户的 workspace 根 `Cargo.toml`**（无论移除条目还是加 `exclude`）——那是用户仓库的构建配置，改法取决于其意图。照原样转达告警；用户明确要求处理时，须按 members + exclude 双改（或换仓库外的 `--target`）。

## 破坏性变更登记

两个 `pub fn` 的返回类型变更（经 `scaffold/mod.rs` re-export）：

```rust
// 变更前
pub fn scaffold_project(name: &str, target_dir: &Path) -> Result<()>
pub fn scaffold_project_with_bin(name: &str, target_dir: &Path) -> Result<()>
// 变更后
pub fn scaffold_project(name: &str, target_dir: &Path) -> Result<Vec<String>>
pub fn scaffold_project_with_bin(name: &str, target_dir: &Path) -> Result<Vec<String>>
```

属**源码破坏性变更**：外部 `fn wrapper(...) -> Result<()> { scaffold_project(a, b) }` 形式的调用会编译失败。

- **仓内调用点已全部更新**：非测试调用点仅 `cli/crates/cli/src/lib.rs` 的 `cmd_scaffold_workspace` 一处；`scaffold_project_with_bin` 除测试外零调用点。
- **不走 deprecation 期**：0.x 阶段 + 无 `cargo publish` 流程（`.github/workflows/` 仅 `ci.yml`）。同 [MDR-019](019-post-translation-review-gate.md) 与 #86 先例：MDR + STATUS 双处记「破坏性变更」即可。
- **不保留 `Result<()>` 旧签名**：告警是本次修复的**全部价值**，留一个丢弃告警的旧入口等于留一个静默失败的口子。

返回 `Vec<String>` 而非结构化告警类型，与既有惯例一致（`LanguageAdapter::configure_project` 同样返 `Vec<String>` 汇入图 warnings，见 [MDR-013](013-danger-signal-to-state.md) 时期的 Go adapter 实现）。

## 验证

`just ci` 全绿（844 测试 + fmt + clippy `--all-targets -D warnings` + deny + shellcheck）。测试 828 → 844（+16）：`template.rs` 的测试函数 9 → 23（+14，含 `absolutize` 与 metadata 失败分支的单测）+ CLI e2e 2 + core 侧 `with_cwd` helper（改 cwd 的测试须串行化，仿 `cli_e2e` 同名先例；非测试函数，不计数）。

多数用例带**前置假设断言**——先证 cargo 的实际行为符合前提（如「glob 下 manifest 确实未变」），cargo 行为若变化会让测试报红，而不是让告警断言静默失去意义。

### 负向实证（独立 worktree，非推断）

| 变异 | 预期 | 实际 |
|------|------|------|
| 检测恒返回空 | 相关测试红 | 3 个测试红 |
| 只摘 `scaffold_project_with_bin` 一处接线 | 仅对应测试红（证两函数各有守卫、不互相掩盖） | `test_scaffold_with_bin_also_warns_on_workspace_membership` 独立红 |
| 摘掉路径绝对化（`absolutize`） | 相对路径用例红 | 2 个测试红，且报错信息复现原始症状（告警里出现 `../Cargo.toml`） |

> **证据留痕说明**（设计契约审查指出）：上表的变异实证在临时 worktree 中进行、worktree 已销毁，仓库内无对应 commit 或产物。可复核的部分是「存在能被这些实证钉住的测试」（表中测试名均可在 `template.rs` 找到且断言方向正确）；实证过程本身需重跑才能再次确认。

### 端到端回归矩阵

| 场景 | 期望 | 实际 |
|------|------|------|
| 显式 `members` | warning | ✅ |
| `[workspace.package]`（无独立 `[workspace]` 段） | warning | ✅ |
| `[ workspace ]`（带空格） | warning | ✅ |
| `workspace = { members = [...] }`（inline table） | warning | ✅ |
| 子目录 + 短相对 `--target` | warning | ✅ |
| 裸目录（无 Cargo 项目） | ok | ✅ |
| 父为普通 `[package]`（非 workspace） | ok | ✅ |
| 目标在 workspace 的 `exclude` 里 | ok | ✅ |

> 5 种等价 TOML 写法中，`[workspace.package]` 与显式 `members` 有 core 层回归守卫；`[ workspace ]` / dotted / inline table 仅由上表的端到端实证覆盖。判据是 TOML 解析器本身（`cargo metadata`），语法变体在原理上被覆盖，故未逐一加守卫。

## 关联

- 收口 [#86](https://github.com/snowzhaozhj/rewriteInRust/pull/86) 记账 TODO ②。
- `docs/design/06-plugin-structure.md` 的 `scaffold workspace` 表行记录判据与处置口径，细节以本文件为准。
- 单 crate 输出（命令名中的 `workspace` 是历史沿称）是既定设计，见 06 § M2 写隔离约束；与本文件的检测正交。
