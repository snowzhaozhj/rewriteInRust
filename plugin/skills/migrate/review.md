# /migrate review — 验证管线 + 进度仪表板

> **状态：M1 Phase 4 实现（M1-TRANS-05），当前为骨架占位。**
> 合并原 verify + status：运行完整验证管线（F3 集成验证）+ 展示迁移进度仪表板。依赖已有迁移产出（`migration-state.json`、`PARITY.md`），在翻译循环（Phase 4）落地后才有可展示的数据。当前调用应提示用户：先运行 `/migrate analyze`，迁移进行后再 review。

## 计划流程（06-plugin-structure.md §10.5）

复用 [SKILL.md](./SKILL.md) 的共享约定。序列：

```
verifier(全量验证 + pattern 新鲜度扫描)
  → 生成 sprint-N-report.json
  → 更新 PARITY.md
  → 终端仪表板输出（按模块状态聚合）
```

- F3 验证通过 `hooks/scripts/full-verify.sh`（`cargo deny check` + `cargo audit` 等）。
- 仪表板从 `migration-state.json` 聚合各模块 status，对照 `PARITY.md` 的 Sprint 视图展示进度/通过率/覆盖率/阻塞项。

> Phase 4 实现时展开为完整分步指令，并接入 verifier SubAgent（见 `agents/verifier.md`，同属 Phase 4）。
