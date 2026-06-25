# 项目状态快照

> 每次会话结束前更新。新会话读此文件 → 找到 PLAN.md 对应任务 → 继续执行。

## 当前位置

- **Milestone**: M1 ✅ → M2 ✅ → **M3 多语言支持（Python 优先）**
- **阶段**: M3 规划中，尚未开始实施
- **测试基线**: 407 测试 / clippy -D / deny / fmt / shellcheck 全绿
- **CI 覆盖率**: 91.96%
- **最新 PR**: [#32](https://github.com/snowzhaozhj/rewriteInRust/pull/32)（M2 Sprint F rxjs 验收）

### M2 遗留移交 M3

| 项目 | 说明 | 代码位置 |
|------|------|---------|
| **FFI 方向不匹配** | napi-rs 是 Node→Rust，降级需 Rust→TS。候选：rquickjs / deno_core / 子进程桥接 | `scaffold/ffi.rs` TODO(M3-FFI) |
| **TS 特有概念泛化** | `constructor_bindings` / `ImportKind::StaticType` / `SymbolKind::Default` 需下沉到 adapter 或用通用 metadata | `lang/mod.rs` TODO(M3) |
| **DEVIATION 4 项待 MDR** | fingerprint 提取范围、事务类型 DEFERRED、WAL pragma 未设置、exported_names 额外维度 | `docs/STATUS.md` 历史记录 |
| **F2-FFI 验收缺口** | mobx 51 文件 SCC 降级因 FFI 方向问题未跑通 | `docs/sprint-f-acceptance.md` |

### 下一步

**新会话从这里开始** → 读 `docs/PLAN-M3.md` → 按 Sprint 执行。

### M3 多语言扩展点（调研结论，2026-06-24）

**已就绪**：
- `LanguageAdapter` trait 6 方法已抽象（`language/can_handle/resolve_extensions/import_specifier_extensions/analyze_file/detect_tier`）
- `SourceLang` 枚举已预定义 TypeScript/Python/C/Go
- `profile/detect.rs` tokei 映射已含 Python/C
- Plugin 层 `SKILL.md` / `analyze.md` 已考虑多语言分发
- 设计文档 06 §11 有完整的语言扩展架构设计

**需泛化（TS 硬编码）**：
- `detect.rs`: 直接实例化 `TypeScriptAdapter`（需 adapter 工厂）
- `graph/build.rs`: `build_graph_ts()` 等 4 个便捷函数硬编码 TS adapter
- `stats/compare.rs`: `collect_ts_files()` / `ts_max_nesting()` / 独立创建 TS parser（绕过 adapter 抽象）
- `types/config.rs`: 默认 `source_language: TypeScript` / exclude 含 `node_modules`
- Plugin `translator.md`: 类型映射表以 TS 为基线
- Plugin `adapters/`: 仅有 `typescript/` 目录

## 历史归档

- **M1 详细记录**：[STATUS-M1-archive.md](STATUS-M1-archive.md)
- **M2 详细记录**：[STATUS-M2-archive.md](STATUS-M2-archive.md)（Sprint D/E/F 任务清单、PR 记录、审查修复、已知问题处理状态）
- **M2 计划**：[PLAN-M2.md](PLAN-M2.md)（55 项任务 + 5 项验收，Sprint A→F）
- **M2 Sprint F 验收**：[sprint-f-acceptance.md](sprint-f-acceptance.md)
