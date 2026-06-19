# /migrate workflow — Sprint 级批量翻译编排

按 sprint 分组批量翻译模块：同拓扑层并行派发（worktree 隔离），逐层合并 + 整组验证。底层单模块翻译复用 [run.md](./run.md) 的 Phase A/B 循环，并行隔离复用 run.md §「并行编排（M2-SCALE-02）」的 7 步通信协议。共享约定（全局锁、CLI 解析、SubAgent 校验、失败恢复）见 [SKILL.md](./SKILL.md)。

## 调用形式

```
/migrate workflow [--sprint <N>] [--max-concurrent <N>]
```

- `--sprint`：目标 sprint 编号。缺省从 `migration-state.json` 的 `sprint.current` 读取。
- `--max-concurrent`：同层并行上限，默认 3（对应 `.rustmigrate.toml` 的 `max_concurrent_agents`）。

## 前置条件

- `migration-state.json` 存在，项目 state 为 `sprint_loop`。
- `.rust-migration/porting/` 存在且含规则文件。
- `modules` 非空，目标 sprint 的模块已被 `populate-modules` 分配。
- Rust workspace 已搭建（`scaffold workspace` 已执行）。

不满足说明 `/migrate analyze` 未完成，应提示用户先跑 analyze。

## 流程

开始时取全局锁（见 SKILL.md「全局锁」），编排器持锁全程，SubAgent 不各自取锁。结束或异常退出时释放。

### 1. 读取 sprint 模块列表 + 分层

```bash
# 获取当前 sprint
rustmigrate state get sprint

# 获取所有模块状态
rustmigrate state get modules
```

从 `modules` 中筛选 `sprint == N` 的模块。读 `migration-state.json` 的 `migration_sequence.parallel_groups`（拓扑层分组）——同组内模块无相互依赖，可安全并行。

**过滤已完成模块**：`status` 为 `done` / `degrade_*` 的模块跳过（幂等支持断点续跑）。

**空 sprint 检测**：若筛选后无待处理模块，报告 sprint 已完成，尝试 `try_advance_sprint`（见步骤 6）。

### 2. 逐层处理

按 `parallel_groups` 索引从小到大（拓扑序）遍历每一层。每层处理流程：

#### 2a. 同层并行派发

同层内独立模块并行派发，最多 `max_concurrent` 路。每路执行：

1. **创建 worktree**：
   ```bash
   git worktree add .wt/{module} -b wt/{module} HEAD
   ```
   设置 `CARGO_TARGET_DIR=.wt/{module}/target` 避免编译锁争用。

2. **准备派发数据**（`TranslationDispatch`）：
   - `module_key`：模块标识
   - `worktree_path`：`.wt/{module}`
   - `dependency_interfaces`：`rustmigrate graph interfaces <module> --deps-of <dep>` 收集已完成依赖的接口
   - `porting_rules`：`.rust-migration/porting/` 下适用规则

3. **派发 SubAgent**（使用 Agent tool，`isolation: "worktree"`）：
   每个 SubAgent 在独立 worktree 内执行完整翻译循环（即 run.md 步骤 2-11 的单模块流程：translate → cargo check → compile_fix → test）。SubAgent 完成后在 worktree 内 `git add -A && git commit`。

   **派发前记台账**：
   ```bash
   rustmigrate state record-subagent-call --step-index 2 --subagent-name translator --status started
   ```

   **回传后记台账**：按结果记 `--status ok` 或 `--status error --error-message "<原因>"`。

4. **回传**（`TranslationResult`）：SubAgent 只回传 `touched-list`（own_files + shared_touched + self_check + test），代码留盘。回传后标记：
   ```bash
   rustmigrate state transition --module <M> --substatus agent_done
   ```

**并发控制**：同层模块数 > `max_concurrent` 时，按模块序分批，前一批全部回传后再派下一批。

#### 2b. 等待本层全部完成

所有 SubAgent 回传 `agent_done`（或失败进入 `paused`）后进入合并阶段。

**失败不阻塞**：某模块翻译失败（重试耗尽）→ 按 run.md 失败恢复标 `paused`（headless 下自动 `degrade_skip`，见下文），继续处理同层其他模块。

#### 2c. 合并（git merge）

编排器在主分支上逐个合并 worktree 分支：

```bash
git checkout main
git merge wt/{module_1}
git merge wt/{module_2}
# ...依次合并
```

**冲突处理**（reconcile）：
- 合并冲突时 `git merge --abort`，标记冲突模块需重译。
- 冲突模块在各自 worktree 内 rebase 到已合并主线后重译。
- **轮次上限**：默认 3（`max_reconcile_rounds`）。
- 超限 → 降级串行处理或转人工：
  ```bash
  rustmigrate state transition --module <M> --to paused --reason "reconcile 3轮冲突未解"
  ```
- 冲突文件列表用 `git diff --name-only --diff-filter=U` 获取。

#### 2d. 整组验证（真门）

> 这是**唯一 done 真门**：agent 级 worktree 自检通过只升 `agent_done`（substatus），orphan rule / E0119 coherence / feature 冲突 / 宏展开 / 命名空间撞名等**跨并发兄弟冲突只能整组编译才暴露**，故必须整组验证后才升最终 `done`。

全部分支合并后，在主 worktree 上执行整组 cargo check/test：

```bash
cargo check 2>&1
cargo test 2>&1
```

**判定逻辑**：

- **全部通过** → `batch_transition_done`：对每个 `agent_done` 模块执行 `reviewing → done` 转换。
  ```bash
  # 对每个 agent_done 模块：
  rustmigrate state transition --module <M> --to done
  ```

- **存在失败** → 进入 compile_fixing 子流程：编排器解析 rustc 错误，按下表归因后修复。

  | 错误类型 | 判定标准 | 处理 |
  |---------|---------|------|
  | **单模块本地错误** | 错误源文件可归因到某一模块的 own 文件 | 该模块回 `compile_fixing` |
  | **跨模块冲突** | E0119 coherence / feature 冲突 / 类型签名不一致，涉及多个模块 | 相关模块整组回 `compile_fixing` |
  | **图缺陷** | 同层模块间出现不应存在的依赖引用（同 sprint 兄弟本不该互相依赖） | 相关模块回退串行 + 记 `metadata.last_error` |

  - 失败模块 `state transition --module <M> --to compile_fixing --substatus "<错误摘要>"`，在其 worktree 内重启 SubAgent 修复 → 重新标 `agent_done` → 编排器重新 merge + 整组 check。
  - 最多 3 轮（`max_compile_retries`，复用 M1 的 `max_retry_rounds`）；3 轮仍失败 → 丢弃该 worktree、`paused`、继续其他模块。
  - **headless 下**：进入 `paused` 后自动 `degrade_skip`（同下「Headless 模式」），不挂起。

#### 2e. 清理 worktree

本层所有模块处理完毕后清理：

```bash
git worktree remove .wt/{module} && git branch -D wt/{module}
```

失败模块的 worktree 保留（供诊断），记入日志。

### 3. 下一层

本层完成后进入下一拓扑层，重复步骤 2。

**层间依赖门禁**：下一层模块的依赖（上层模块）必须全部 `done` / `degrade_*`。若有依赖未就绪（上层模块 paused），下层依赖该模块的模块自动标 `blocked`（填 `blocked_by`），跳过处理。

### 4. Sprint 完成

全部层处理完毕后：

1. **检查 sprint 完成度**：统计目标 sprint 内模块状态分布。
2. **全部 done/degrade_\*** → `try_advance_sprint`：
   ```bash
   # sprint.current += 1，记入 sprint.history
   rustmigrate state transition --to sprint_loop --reason "sprint N 完成，推进到 sprint N+1"
   ```
3. **有 paused/blocked 模块** → 报告未完成模块列表，建议用户处理后重跑 `/migrate workflow`。
4. **输出 sprint 摘要**：
   - 完成模块数 / 总数
   - 各模块最终状态
   - 耗时统计
   - 降级 / 失败模块及原因

## Headless 模式

`.rustmigrate.toml` 设 `headless=true` 时，本命令自动启用以下行为（详见 run.md「Headless 模式」）：

- **意图确认门禁跳过**：`auto_confirm_intent=true` 自动生效。
- **自动 degrade**：模块进入 `paused` 后不等人工确认，自动执行：
  ```bash
  rustmigrate state transition --module <M> --to degrade_skip \
    --substatus "headless_auto_degrade" \
    --reason "headless: 翻译/编译失败自动降级"
  ```
- **继续处理**：降级后继续同 sprint 其余模块，不中断 workflow。

## 断点续跑

workflow 支持中断后重入：

- 重新调用 `/migrate workflow [--sprint N]` 时，步骤 1 会过滤已完成模块。
- 已 `done` / `degrade_*` 的模块跳过，只处理 `pending` / `translating` / `paused`（需 `--force`）/ `blocked`（依赖就绪后自动解除）的模块。
- 每个模块的断点续传由 run.md 步骤 1（断点续传路由）处理。

## 与 `/migrate run` 的关系

- `/migrate run <module>` 是**单模块**入口，串行执行完整翻译循环。
- `/migrate workflow` 是**sprint 级批量**入口，按拓扑层并行编排多模块翻译。
- workflow 内部对每个模块的翻译逻辑复用 run.md 的步骤 2-11。
- run.md「并行编排（M2-SCALE-02）」节定义单模块 worktree 隔离的 7 步通信协议，是 workflow 的底层基础设施；真门整组验证与 compile_fixing 子流程定义在本文件（步骤 2d）。

## 错误汇总

workflow 结束时输出所有异常模块的汇总表：

| 模块 | 最终状态 | 原因 | 建议操作 |
|------|---------|------|---------|
| `<module>` | `paused` | 编译 3 轮失败 | `/migrate run <m> --degrade=ffi\|skip` |
| `<module>` | `blocked` | 依赖 X 未完成 | 先处理 X |
| `<module>` | `degrade_skip` | headless 自动降级 | 人工审查决定是否改为 FFI |
