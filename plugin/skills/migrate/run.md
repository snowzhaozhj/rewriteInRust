# /migrate run — 执行模块迁移（Phase A/B 双阶段）

> **状态：M1 Phase 4 实现（M1-TRANS-04），当前为骨架占位。**
> 本命令依赖 `/migrate analyze` 的产出（`migration-state.json` state=sprint_loop、`porting/` 规则、`source-graph.db`）。在 analyze 流程（Phase 3）落地后、翻译循环（Phase 4）实现前，调用本命令应提示用户：翻译循环尚未实现，先运行 `/migrate analyze` 完成分析。

## 计划流程（权威骨架：09-appendix-schemas.md 附录 B § /migrate run）

复用 [SKILL.md](./SKILL.md) 的 Step 0 全局锁与共享约定。序列（06-plugin-structure.md §10.5）：

```
前置 blocked 检查点(Step 0.5/0.6)
  → translator(语义解构/意图摘要)
  → 人类确认门禁(Step 1.5)
  → translator(Phase A 忠实翻译)
  → verifier(对抗性审查)
  → translator(Phase B 惯用化优化)
  → verifier(测试验证)
  → 更新状态
```

关键产出物：`{module}-intent.md`（9 required 属性，L2 校验）、Rust 源码、`{module}-review.md`、测试、MDR。

## 前置条件
- `.rust-migration/migration-state.json` 存在且 state 为 `sprint_loop`
- `.rust-migration/porting/` 存在且含规则文件
- 目标模块已在 Sprint 计划中

> Phase 4 实现时，把上述骨架展开为完整分步指令，并对齐 `state transition` CLI（M1-STATE-04）的 substatus 落盘。
