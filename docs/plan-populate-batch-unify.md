# 修复：populate-modules 保留 decompose Batch 分组 + run 按成员复杂度分流

> 本文档已经过 grilling 拷问收敛（2026-06-28）。原始草案的「统一所有 batch 走一条无测试路径」被推翻——见下方「设计决策记录」。

## Context

在 jmespath.py（8 个文件的真实 Python 项目）上跑端到端验证时发现：decompose 引擎正确地将 7 个文件按耦合合成 1 个 batch（75% 缩减），但 `populate-modules` 因成员不是 mechanical 而将 batch 展开为 8 个独立模块。decompose 的分组决策被 populate 用另一套标准推翻，接口协议断裂。

根本原因：`populate` 第 1930-1955 行的 mechanical 门控（含非机械成员的 batch → 展开为独立单文件模块）是 PLG-06 引入的、与已冻结的 **MDR-011 §6**（「删除『机械 vs 重型』流程二分——agent 一次翻完一个簇」）相矛盾的回退。budget=12000（≈1000 行）已封顶 batch 的 self_size 之和，「整组一次翻完」在上下文窗口上有界安全。

## 设计决策记录（grilling 收敛）

| # | 决策点 | 结论 |
|---|------|------|
| 1 | 非机械 batch 的执行路径 | **按成员复杂度分流**。全机械成员 batch → 保留现有轻量路径（编译即门禁、无测试）；含任一非机械（Normal）成员的 batch → 走**整组完整路径**：整组翻译 → 结构门 → Phase B → 行为测试 → 审查。理由：MDR-011 §6 要求 batch 有「定向测试」，复杂逻辑簇只过编译+审查相对单文件翻译是质量回退。 |
| 2 | state 如何编码两种 batch | **新增 `CompositeKind::CoupledBatch` 枚举值**。`Batch` 收窄语义为「全机械合批」（轻量路径）；`CoupledBatch` = 含逻辑成员的耦合簇（完整组路径）。沿用现有 `Cycle`/`Batch` 显式落字段、run 不运行时重推导的设计原则。 |
| 3 | classify_file 是否从 populate 删除 | **保留**（原草案决策 #4 反转）。populate 靠 `classify_file` 判定成员机械性，决定写 `Batch` 还是 `CoupledBatch`。`graph decompose` 的 dry-run 报告仍用 classify_file（不动）。 |
| 4 | danger→RULE/定向测试注入 | **本计划不做，记 TODO**。这是 decomposition-redesign.md:144 列为 PR-1 交付、但从未接进 Plugin（`plugin/` 内 0 处 danger 引用）的跨路径既有缺口。logic batch 走完整路径+行为测试已能反应式抓住分歧；danger 主动规则是增强非正确性阻塞。与 MDR-011 §8 待校准项并轨。 |
| 5 | batch 内逆拓扑排序（run.md 6.2 TODO(CLI)） | **非阻塞，沿用现有兜底**。logic batch 是「一次调用翻完整组」，translator 单次调用看到全部成员，跨文件一致性由模型内部解决，逆拓扑仅作呈现顺序提示。编排器读 source-graph 的现有兜底够用，无需先补 CLI 参数。 |

## 改动清单

### 1. CLI 类型：`cli/crates/core/src/types/state.rs` — 新增 CompositeKind 变体

`CompositeKind` 枚举新增 `CoupledBatch`，更新三个变体的文档注释：

```rust
pub enum CompositeKind {
    /// 循环依赖组（互引文件折叠）——走现有契约+逐文件填空重路径。
    Cycle,
    /// 全机械合批组（成员全为 Barrel/PureType/PureConstant）——走轻量路径（整组一次翻完 + 编译即门禁，无行为测试）。
    Batch,
    /// 含逻辑成员的耦合凝聚簇（decompose 按耦合/目录分组）——走完整组路径（整组翻译 → 结构门 → Phase B → 行为测试 → 审查）。
    CoupledBatch,
}
```

`#[serde(rename_all="snake_case")]` 下序列化为 `coupled_batch`。检查所有 `match composite_kind` 处补全新分支（编译器会强制）。

同步更新 `ModuleState.composite_kind` 字段文档注释（state.rs:254-256），补 `Some(CoupledBatch)` 语义。

**派生与 JSON 输出（codex 审查确认，无需手写分支）**：`CompositeKind` 已派生 `Serialize/Deserialize/Display/EnumString`，`state get`（`serde_json::to_value(ModuleState)`）与 populate JSON 输出（`composite_kind.to_string()`）均自动产出 `coupled_batch`，新增变体无须改这两处。`retain_modules` 按 live key 删 orphan、不看 kind，亦无须改。

### 2. CLI：`cli/crates/cli/src/lib.rs` — `cmd_state_populate_modules()`

**2a. 保留 classify_file（不删 file_kinds 计算）** —— 原草案 1b 作废。`file_nodes` 循环内的 `adapter.classify_file(&src)` + `file_kinds.insert` 全部保留（第 1866-1914 行区域不动）。

**2b. 改写 `UnitKind::Batch` 分支（第 1930-1955 行）** —— 不再展开非机械 batch，改为按机械性写不同 composite_kind：

```rust
UnitKind::Batch => {
    let all_mechanical = u.members.iter().all(|m| {
        matches!(
            file_kinds.get(m),
            Some(FileKind::Barrel | FileKind::PureType | FileKind::PureConstant)
        )
    });
    let kind = if all_mechanical {
        CompositeKind::Batch
    } else {
        CompositeKind::CoupledBatch
    };
    units.push(MigrationUnit {
        key: u.members.first().cloned().unwrap_or_default(),
        members: u.members.clone(),
        sprint: u.sprint,
        composite_kind: Some(kind),
    });
}
```

读失败分支中 `file_kinds.insert(id, FileKind::Normal)` 保留（保守按非机械处理，使含读失败成员的 batch 落 CoupledBatch 走完整路径——更安全）。

**2c. `cmd_graph_decompose`（第 1191 行）不动**——dry-run 报告的 classify_file 是分类统计的一部分。

**2d. `decomposition_frozen` / all-pending 重填语义（codex 审查补）** —— 现状 populate 每次都重跑 `plan_decomposition` 并给所有单元写同一 snapshot（frozen=true）；非 pending 模块拒绝重填，all-pending 允许幂等重填。CoupledBatch 沿用此行为：all-pending 重填时按当前 decompose 结果重新归组（与现有 Batch/Cycle 一致），`decomposition_frozen` 当前不阻止重算（这是既有语义，本计划不改）。新增 CoupledBatch 不引入新的冻结分支。

### 3. Plugin：`plugin/skills/migrate/run.md`

**3a. Step 1 路由表（第 51 行）** —— 拆成两行：

```
| pending/translating(null) + composite_kind=batch | 跑步骤 2-3，然后进入步骤 6「Batch 组轻量路径」（跳步骤 4/5）；content-hash 决定跳过翻译或整批重派 |
| pending/translating(null) + composite_kind=coupled_batch | 跑步骤 2-3，然后进入步骤 4 意图摘要 → 5 意图确认 → 步骤 6「CoupledBatch 组完整路径」 |
```

**3b. Step 6 形态判断（第 76-79 行）** —— 新增 `coupled_batch` 分支：

```
- composite_kind=batch（全机械合批）→「Batch 组轻量路径」。
- composite_kind=coupled_batch（含逻辑耦合簇）→「CoupledBatch 组完整路径」。
- member_files 为空 → 「单文件 Phase A」。
- composite_kind=cycle（或缺省循环组）→「SCC 组 Phase A」。
```

**3c. 「Batch 组轻量路径」（第 81-123 行）** —— 保留原样，仅把标题/前置条件里「成员全为可证明机械文件」描述明确限定为 `composite_kind=batch`（不再暗示所有 batch）。

**3d. 新增「CoupledBatch 组完整路径」小节** —— 在 Batch 轻量路径后插入。要点：
- 整组意图摘要（步骤 4，整组一次）+ 意图确认（步骤 5）。
- 整组翻译（translator，一次翻完整批，成员按逆拓扑序呈现；输入=全部成员源码 + 外部依赖 interfaces + porting rules + 裁剪依赖清单）。tier = 成员 max tier（已由 populate 算入 `modules[key].tier`），据此决定单/多候选。
- L1 校验（全成员 .rs 存在、非空、无 todo!()）→ 整组编译 → **步骤 8 结构门（整组）** → **步骤 9 Phase B（整组惯用化）** → **步骤 10 行为测试（verifier 对整组公共 API 生成测试并跑 verify.sh）** → **步骤 11 审查签批**。
- 状态机沿用现有 `translating→testing→reviewing→done` 转换矩阵（与单文件路径同，无需新 substatus）。
- **error/retry/skip 显式复用单文件路径（codex 审查补）**：CoupledBatch 完整复用单文件的失败恢复入口——断点路由的 `degrade_*`+`--force`、编译/测试失败 `→paused→degrade_*`、测试/审查不达标停 testing/reviewing 标 incomplete、`blocked` 恢复，均与单文件一致，不另立分支。
- content-hash 跳过检测可复用（整组原子）。

**3e. SCC 组路径（步骤 6 SCC）不动**——CoupledBatch 不复用 SCC 契约+stub 重路径（grilling 明确否决），走的是「一次翻完整组 + 完整门禁」。

### 4. Plugin：`plugin/agents/translator.md`

**4a.「Batch 组翻译」章节** —— 限定为 `composite_kind=batch`（全机械）：保持「机械文件、一次翻完」描述。

**4b. 新增「CoupledBatch 组翻译」章节** —— 含逻辑耦合簇：成员由 decompose 按耦合分组、可含任意复杂度文件；一次翻完整批，按实际复杂度决定翻译深度（与单文件 Phase A 同的 tier 多候选规则适用）；成员间引用规则、逆拓扑序呈现同 Batch。

### 5. Plugin：`plugin/skills/migrate/workflow.md`

**关键修正（codex 审查标为真风险）**：workflow.md 第 63-71 行当前把「`member_files` 多文件」一律描述为 **SCC 模块组**。CoupledBatch 也是 `member_files` 多文件——若不改，并行编排会把 CoupledBatch 当 SCC 走契约+stub 路径，**直接违反 grilling 决策**（CoupledBatch 不走 SCC 契约路径）。必须把分派条件从「多文件=SCC」改为按 `composite_kind` 分流：

- `composite_kind=cycle` → SCC 契约+逐文件填空路径。
- `composite_kind=coupled_batch` → 整组完整路径（在 worktree 内按整组单元跑「翻译→结构门→Phase B→测试→审查」完整循环）。
- `composite_kind=batch` → 整组轻量路径。
- `member_files` 为空 → 单文件路径。

并同步搜 batch 引用，区分 `batch`（机械轻量）与 `coupled_batch`（完整路径）描述。

### 5.5. Plugin：`plugin/skills/migrate/analyze.md`（codex 审查补）

第 48 行附近仍只描述 populate 写「SCC 缩点后的 member_files」并只提 `state deps`。补一句：populate 经 decompose 后产出 `cycle`/`batch`/`coupled_batch` 三类 composite（均带 `member_files`），下游 run/workflow 按 `composite_kind` 分流；依赖门禁统一走 `state deps`（见下）。

### 5.6. `state deps` 一致性（codex 审查补，确认项不改代码）

`state deps`（lib.rs:2134-2154）对所有 composite 一视同仁、按 `member_files` 建「文件→组代表」映射，**不按 `composite_kind` 分流**。故 `cycle`/`batch`/`coupled_batch` 在依赖就绪门禁上行为一致，差异只在 run/workflow 的**执行路径**。计划在文档（run.md 步骤 3 / analyze.md）明确这一点，CLI 代码无须改。

### 6. 测试更新

**6a. `cli_e2e.rs`：`e2e_populate_modules_linear_unblocks_run`** —— linear-deps 3 文件被 decompose 合成 1 个簇。先确认成员机械性：
- 若全机械 → 断言 1 个 `composite_kind=batch`；若含逻辑 → 断言 1 个 `composite_kind=coupled_batch`。
- `module_count` 从 3 改为 1；新增 member_files 含 3 文件断言；用 batch key 做 `state deps` 验证组感知依赖。

**6b. `cli_e2e.rs`：`e2e_populate_cleans_orphan_pending`** —— 首轮 module_count 3→1，删文件重填断言相应调整。

**6c. PLG-06 相关测试** —— py-pkg-deps 的 mixed batch（含 normal）：改为断言保留为 1 个 `composite_kind=coupled_batch`（不展开）。原全机械 batch 测试断言 `composite_kind=batch` 不变。

**6d. 新增测试**
- 全机械 batch → `composite_kind=batch`（py 机械 fixture）。
- 含逻辑 batch → `composite_kind=coupled_batch`，不展开（py-linear/py-pkg fixture）。
- `state deps` 对 `coupled_batch` 组的组感知依赖断言（codex 审查补）：用 coupled_batch 组代表 key 查 `state deps`，验证成员文件级依赖正确映射回组代表、剔除组内自依赖——与 cycle/batch 行为一致。
- `--no-decompose` 旧路径不受影响（regression）。

### 7. TODO 落账（不在本计划实现）

- `TODO(M3-DEC)`：danger 信号（`ClassifyResult.danger`，6 类）→ translator RULE + verifier 定向测试注入。跨所有翻译路径的既有缺口，与 MDR-011 §8 / decomposition-redesign.md:144 并轨，独立 PR。
- `TODO(CLI)`：`graph topo-sort --members --reverse` 供 batch 内逆拓扑直接取（现兜底为编排器读 source-graph）。

## 不改的部分

- `decompose.rs`：引擎逻辑正确（UnitKind::Batch 涵盖所有 ≥2 簇，机械性区分在 populate 做）。
- `cmd_graph_decompose`：dry-run 报告的 classify_file 保留。
- `CompositeKind::Cycle` / SCC 契约路径：不动。
- `--no-decompose` 旧路径：不动。

## 验证

1. `just ci` 全过（含新增 CompositeKind 分支编译完整性）。
2. `cargo run -- state populate-modules --root <py 机械 fixture>` → 1 个 `composite_kind=batch`。
3. `cargo run -- state populate-modules --root fixtures/py-linear-deps/src`（含逻辑）→ 1 个 `composite_kind=coupled_batch`，不展开。
4. 对 /tmp/val-d/jmespath 重跑 populate-modules → 产出 2 模块（1 coupled_batch + 1 single）而非 8。
5. 对 /tmp/val-d/jmespath 重跑 `/migrate analyze` → 端到端跑通，模块数 ≤ 2。
