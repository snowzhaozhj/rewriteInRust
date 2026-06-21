# 项目状态快照

> 每次会话结束前更新。新会话读此文件 → 找到 PLAN.md 对应任务 → 继续执行。

## 当前位置

- **Milestone**: M1 MVP ✅ → **M2 质量提升**
- **Phase**: M2 Sprint D/E + SCC 破环（PR #22/#23/#24/#25 **均已合并**）→ **Sprint F 验收进行中**
- **测试基线**: 407 测试 / clippy -D / deny / fmt / shellcheck 全绿（含本轮新增 ESM 接线测试）
- **🔜 下一步大改（进行中）**：Phase 2「SCC 逐成员文件翻译 + 整组编译门禁」——详见 **[docs/phase2-scc-per-file-handoff.md](phase2-scc-per-file-handoff.md)**（含三根因链、stub-first 契约设计、改动清单、分级验证、续接指引）。

### Phase 2 Level 0 ✅ 天花板假设已证（最大盲点闭合）

- **量了再信**：`graph interfaces --members`（整组 SCC 导出签名一次输出）+ `signature_text`（按 line_range 读源、body-bearing 剥离函数体）已实现，透传 single/`--deps-of`/`--members` 三模式。
- **mobx 实测（SCC=51 文件 / 187 导出，比 41 真环保守）**：签名总计 **~4,627 token**（深度感知提取）；含函数体上界 ~37K；全源码绝对硬上界 ~65K。**三档全远低于 200K 窗口 → 「契约 agent 装得下」成立，>40x 余量，无需 SCC 子簇分契约**。
- **审查闭环（PR #27，4 视角全跑）**：design-checker 零 MISMATCH；主审+pr-review+codex 收敛出 2 项 important（签名首 `{` 截断命中泛型/对象参数 → 改括号深度感知扫描；源读不到静默低估 → 加 missing_source warning）+ 效率/文档 nit，**已全修**；export* 歧义/路径排序记为 pre-existing/edge。
- **Level 1**：`--members` happy/缺源告警 e2e + `signature_tests` 深度感知单测入 nextest，全过 + clippy -D + fmt。
- **下一步**：Phase 2 PR-A 收尾（signature_text 已落地，可补 cmd_graph_interfaces 文档串） → PR-B（Level 2 手写契约+stub 机制自洽）→ PR-C（提示词改造 + Level 3 LLM 端到端）。详见交接文档六、实施顺序。

### Sprint F 进行中：破环（M2-SCALE-SCC）✅

- **设计变更**：源码循环依赖不再拒绝填充，改为 **SCC 缩点折叠为翻译单元**——每个强连通分量成为一个 composite 模块组（`ModuleState.member_files`），在缩点 DAG 上排 sprint 层级，translator 整组翻译为一组互引 Rust mod（同 crate 内 mod 间循环 `use` 合法，无需破环/shared-types/FFI）。
- **实现**：`topo.rs` 新增 `scc_groups`/`SccGroup` + 缩点层级；`lib.rs` populate 删拒绝改折叠；`state.rs` ModuleState 加 `member_files`；提示词 translator/run/workflow/verifier/SKILL 同步优化。
- **真实项目验证**：zod（82 文件）→ 75 模块，8 文件核心环（ZodError/types/errors/index...）折叠为 1 个 full-tier composite 组，4 sprint 层级。分支 `feat/m2-scale-scc-break-cycle`（commit 4c9e4da）。
- **门禁衔接**（commit 17f81a4）：新增 `state deps` 组感知依赖门禁——composite 组成员依赖映射回组代表，修复 zod 65 处缺口（审查 Important #1）。
- **设计文档 + MDR**（commit 29b89e3）：MDR-004 + 02/03/04/06/09 修订 + analyze/SKILL 对齐（审查 5 项 important 闭环）。
- **LLM 端到端跑通**（circular-deps fixture，瘦编排 + subagent 翻译）✅：
  - 三向引用环 {emitter,event-bus,handler} 折叠为 composite → translator 整组翻译为三互引 Rust mod，Handler 用 `Weak` 破强引用环，`Rc::strong_count==1` 断言成立。
  - cargo check/test(2 passed)/clippy 全过；状态机推进 2 迁移单位→done + sprint 推进→all_completed；member_files 全程持久化；validate state ok。
- **审查闭环**（CLAUDE.md 4 视角 + 修复后复审）：design-checker（5 项文档）+ pr-review/codex（门禁缺口）+ **主审 /code-review**（absent 死锁 + 注释 rot + cleanup）全部修复；修复后复审逐项验证 6 项修复正确无回归。
- **PR [#25](https://github.com/snowzhaozhj/rewriteInRust/pull/25)**（7 commit）：**mergeStateStatus=CLEAN**，GitHub CI 5 项全绿（check/deny/shellcheck/test/coverage），本地 `just ci` 全过（404 测试），**可直接验收合并**。
- **已知 TODO**（代码已标注）：`TODO(perf)` 多源 BFS、`TODO(refactor)` 层级计算抽共用、`TODO(M3-FFI)` 单 SCC 超预算兜底（zod composite 8 文件 full-tier 会触发，留 Sprint F 后续）。
- **Sprint F 后续**：zod/真实项目全量 LLM 翻译（需 FFI 兜底实现）。

### Sprint F 验收进行中（无环路径优先）

详细记录见 `docs/sprint-f-acceptance.md`。摘要：

- **F1 选型**：dogfood 筛选 io-ts(16f)/zustand(15f) 为无环主项目，mobx(57f,51 文件 SCC) 留 F2-FFI 降级验收。
- **F2 full 档端到端 PoC ✅**：io-ts FreeSemigroup/DecodeError 两模块 scaffold→翻译→check/test/clippy→状态机推进全打通（DecodeError 递归 ADT 触发 E0072→Box 修复，典型 Phase A compile_fixing）。
- **F2 降级 ✅**：io-ts Schemable（HKT 密集，Rust 无 HKT）经 translating→compile_fixing→paused→degrade_skip 正确降级。
- **阻塞结论**：io-ts(HKT 库)/zustand(前端框架库) 超出"模块化忠实翻译"边界 → F1≥3模块、F3 并行吞吐缺干净可翻译素材。**选型教训**：需预筛"可翻译性"（HKT/typeclass 密度、框架耦合、动态类型），待用户定重选项目 or 接受当前结论。

### 🔴 重大 bug 修复 + 重构：graph 漏解析 ESM `.js` 扩展名 import（PR [#26](https://github.com/snowzhaozhj/rewriteInRust/pull/26)，OPEN）

- **现象/根因**：现代 NodeNext TS 项目（jsonc-parser）相对 import 带 `.js`/`.mjs`/`.cjs`/`.jsx` 但指向 `.ts`/`.tsx` 源，`resolve_import` 无此映射 → imports 边全丢 → 误判无环、sprint 排序/门禁/SCC 折叠/并行编排连带失效（R3 实锤）。
- **修复（commit 8aa6659）**：strip JS 扩展名后按源扩展名重试 + `can_handle` 排除 `.test`/`.spec` 测试文件。
- **重构（commit a675a99）**：扩展名清单下沉到 `LanguageAdapter::import_specifier_extensions()`（TS 返回 `.js/.jsx/.mjs/.cjs`），build.rs 全量+增量两条路径收集为 `strip_exts` 传入 resolve_import——graph 层不再内嵌语言字面量（与 `resolve_extensions` 同构）。
- **验证**：jsonc-parser `--full` 重建检出 `scanner→format→main→parser→edit` 5 文件环；新增 build_graph 端到端接线测试；core 346 + 全量 407 测试、clippy -D、fmt 全过。
- **审查闭环（4 视角全跑）**：design-checker 无 MISMATCH；pr-review/codex 无 important、无回归；主审找出唯一 actionable nit（缺端到端接线测试）→已补；finder 报"精确匹配优先反转 ESM 重映射"经核实 REFUTED（`.js` 文件不被 `can_handle` 收集，永不进 file_set）。

### Sprint D/E 完成总结（3 个 PR，3 波并行执行）

| 波次 | PR | 测试 | 任务 |
|------|-----|------|------|
| 波次 1 | [#22](https://github.com/snowzhaozhj/rewriteInRust/pull/22) ✅ | 269→291 | VER-01/02, CICD-01+COV-01, PARITY-01, ADV-04/05/09/10 |
| 波次 2 | [#23](https://github.com/snowzhaozhj/rewriteInRust/pull/23) ✅ | 291→353 | CLI-01~06, ERR-01, SCALE-02(全部子任务) |
| 波次 3 | [#24](https://github.com/snowzhaozhj/rewriteInRust/pull/24) 待合并 | 353→399 | SCALE-03, SCALE-01, SCALE-LOCK, PETGRAPH-01, ADV-01/02/08 + 审查修复 |

### Sprint D 任务清单

| 任务 ID | 状态 | 内容 |
|---------|------|------|
| M2-SCALE-02 | ✅ 波次2 | 写隔离：types/parallel.rs + run.md 通信协议（删 1460 行过度设计） |
| M2-SCALE-01 | ✅ 波次3 | Workflow 批量翻译：新建 workflow.md（sprint 级并行编排） |
| M2-SCALE-LOCK | ✅ 波次3 | 全局锁改造：编排器持锁，SubAgent 不取锁 |
| M2-PETGRAPH-01 | ✅ 波次3 | petgraph 副本隔离验证 + WAL 回归（7 测试） |
| M2-ADV-01 | ✅ 波次3 | 多候选生成：translator 多策略 + verifier 选优 |
| M2-ADV-02 | ✅ 波次3 | 降级 FFI：binding 桩 + 降级报告 + 环断点选择（**TODO(M3-FFI): napi-rs 方向不匹配，Sprint F 再定**） |
| M2-ADV-04 | ✅ 波次1 | graph build --profile 性能画像 |
| M2-ADV-05 | ✅ 波次1 | graph interfaces --deps-of 批量输出 |
| M2-ADV-08 | ✅ 波次3 | profile 自动定位 analysis-tools.json |
| M2-ADV-09 | ✅ 波次1 | 子进程超时（wait-timeout） |
| M2-ADV-10 | ✅ 波次1 | persistence 配置段（backup_on_write/retention_days） |

### Sprint E 任务清单

| 任务 ID | 状态 | 内容 |
|---------|------|------|
| M2-VER-01 | ✅ 波次1 | proptest 图操作不变量（7 个属性测试） |
| M2-VER-02 | ✅ 波次1 | cargo-fuzz 解析器健壮性（2 fuzz target） |
| M2-COV-01 | ✅ 波次1 | 覆盖率门禁（cargo-llvm-cov CI 集成） |
| M2-CICD-01 | ✅ 波次1 | GitHub Actions CI（5 并行 job） |
| M2-PARITY-01 | ✅ 波次1 | PARITY.md 等价深度扩展 |
| M2-SCALE-03 | ✅ 波次3 | 增量图更新：三级变更检测 + 反向 BFS + 熔断 |
| M2-CLI-01 | ✅ 波次2 | graph rdeps 反向依赖 |
| M2-CLI-02 | ✅ 波次2 | graph cycles SCC 环检测 |
| M2-CLI-03 | ✅ 波次2 | graph export JSON/DOT/Mermaid |
| M2-CLI-04 | ✅ 波次2 | validate config |
| M2-CLI-05 | ✅ 波次2 | state update --cas-version CAS 乐观锁 |
| M2-CLI-06 | ✅ 波次2 | validate state --check-blocked --auto-unblock |
| M2-ERR-01 | ✅ 波次2 | 错误码枚举化（E001-E015） |

### 审查修复要点（波次 3）

- profile 参数透传（增量模式下 --profile 全零 → 修复）
- remove_stale_fingerprints 事务保护
- structure_hash 纳入 calls 摘要（防 Calls 边过期）
- FFI 桩参数名 sanitize + Rust 关键字 r# 转义
- cmd_graph_build 全量路径指纹代码消重（-26 行）
- skip.effort 按 downstream_count 分档
- 全量构建一次遍历同时产出图和指纹（消除双遍历）

### 已知问题 / TODO

- **TODO(M3-FFI)**: `scaffold/ffi.rs` 生成 napi-rs `#[napi]` 桩方向不匹配（napi-rs 是 Node.js→Rust，降级需 Rust→TS）。M2 无触发路径（headless 走 degrade_skip）。Sprint F 实测时选定方案（rquickjs/deno_core/子进程桥接）
- 设计文档 DEVIATION 4 项待 MDR：fingerprint 提取范围、事务类型 DEFERRED、WAL pragma 未设置、exported_names 额外维度

### 下一步

**新会话从这里开始** → **Sprint F 验收**（PLAN-M2 §9，7-10 天）：
- PR #22/#23/#24/#25 均已合并；**PR #26（ESM 修复）待合并** → 合并后基线 407 测试
- 待用户定：Sprint F 重选"可翻译性"达标的中型无环 TS 项目（解析器/算法库/CLI 工具）补 F1≥3模块 + F3 并行吞吐，或接受当前 HKT 阻塞结论
- F1: 真实项目端到端（3 个 5K-20K 行 TS 项目）
- F2: 降级验收（circular-deps FFI）
- F3: 并行吞吐（≥1.5 模块/小时）
- F4: 性能无退化（±10%）
- F5: 测试质量（proptest 1000 次 + fuzz 24h）
- F6: 覆盖率 ≥70%

## M1 完成总结

| Phase | 内容 | PR | 测试 |
|-------|------|-----|------|
| M0 Sprint 0 | Spike S0/S3 假设验证 | — | — |
| Phase 0 | 冻结合约（types/error/response/schema） | — | cargo check |
| Phase 1 | 四路并行实现（graph/state/profile/hooks） | PR #5 | 121→202 |
| Phase 2 | 集成验证（14 命令路由 + E2E） | PR #3 | +25 e2e |
| Phase 3 | Plugin 实现（4 agent + SKILL + hooks） | PR #8/#9 | Live 验证 |
| Phase 4 | 翻译循环 + MVP 验收 | PR #9 | 4 fixture Live |
| §9.5 | analyze→run 衔接 + 审查加固 | PR #10 | +3 e2e, 202 总 |

**M1 验收（§9 + §9.5）**：
- linear(3 模块) + diamond(5 模块) 完整迁移到 done，nextest 33/33 + 12/12、clippy 零
- circular 环暂停正确；edge 含 M2 特性不 done（验证鲁棒性）
- review 仪表板、断点续传均验证通过
- 质量门：202 测试 | clippy -D warnings 零 | fmt | shellcheck | design-checker 零 MISMATCH

**M1 已知限制（沉淀到 M2）**：
- diamond 靠决策注入跑通，headless 无人值守撞 TODO(port) 必卡 → M2「默认 TODO 决策策略」
- 单文件 module + 完整 11 步循环 + 串行对真实项目不实用 → M2-TIER-01 + M2-SCALE
- populate 孤儿清理 + 契约加固 → M2-VER-04

## M2 起点

### M2 计划概览（详见 `docs/PLAN-M2.md`）

```
Sprint A (基础加固)  → Sprint B (类型+图精度) → Sprint C (核心功能双线)
  → Sprint D (并行+高级) ‖ Sprint E (验证+CLI) → Sprint F (验收)
```

- **55 项任务 + 5 项验收活动**，预计 25-33 天纯开发（日历 5-7 周）
- 5 个设计决策已定稿（D1 done 终态 / D2 blocked 规则 / **D3 写隔离=worktree+约束包** / D4 tier 分档 / D5 SQLite 集中 writer）
- M1 deferred TODO 已分配到对应 M2 任务（ADV-08/09, REFAC-13）
- 部分设计文档 M2 交付物推迟到 M2.5/M3（状态机程序化、行为录制框架等）

### M1 历史归档

M1 各 Phase 的详细审查修复记录、提交历史、Live 验证产物见 [STATUS-M1-archive.md](STATUS-M1-archive.md)。
