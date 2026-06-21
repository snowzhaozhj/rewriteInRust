# SCC 组「stub-first 契约」机制验证样例（Level 2）

> Phase 2「SCC 逐成员文件翻译 + 整组编译门禁」的**机制自洽验证**（无 LLM，纯人工）。
> 背景与设计见 [`docs/phase2-scc-per-file-handoff.md`](../../phase2-scc-per-file-handoff.md) 三/五.Level 2。

## 验证什么

把图论 SCC 当「编译门禁单元」而非「原子翻译单元」：翻译粒度=单文件，跨文件一致性
**由编译器强制**（stub 骨架先 `cargo check` 过），不靠 LLM 自觉遵守 `.md` 契约。

手写都跑不通则提示词无意义——本样例先证机制成立，再做 PR-C 提示词改造。

素材：`fixtures/circular-deps/`，3 文件真环 `{emitter, event-bus, handler}`（`graph cycles` 实测），
对象引用环 `Emitter→EventBus→Handler→Emitter`，需一条 `Weak` 回边破环。

## 三件产物

| 文件/目录 | 角色 | 对应机制步骤 |
|-----------|------|-------------|
| `contract.md` | 组级契约（6 字段） | 步骤1：组级一次产出 |
| `stub/` | Rust 签名骨架，body 全 `todo!()` | 步骤1.5：**契约门** |
| `impl/` | 由 stub 逐文件填 `todo!()` 得到 | 步骤2 逐文件翻译 + **实现门** |

`stub/` 与 `impl/` 各源文件**签名逐字节一致**，差异仅函数体（`diff` 可验）——
对应「逐文件 agent 填空、签名锁定不许改、禁碰 mod.rs/Cargo.toml/Error enum」。

## 两道门

1. **契约门**（`stub/`）：`cargo check` 通过 ⇒ 跨文件签名一致、所有权类型（`Rc`/`Weak`/`RefCell`）可解析 ⇒ 契约 valid。一致性由编译器保证。
2. **实现门**（`impl/`）：`cargo check && cargo test` 通过 ⇒ 整组真编译过；`break_cycle::weak_back_edge_breaks_strong_cycle` 断言 `Rc::strong_count(&emitter)==1` ⇒ Weak 回边正确破除强引用环（若误用 `Rc` 则为 2 且泄漏）。

## 复现

```bash
# 契约门：stub 骨架编译过
( cd docs/examples/scc-stub-first-contract/stub && cargo check )

# 实现门：整组 check/test + 破环断言 + clippy
( cd docs/examples/scc-stub-first-contract/impl && cargo test && cargo clippy --all-targets -- -D warnings )

# 证明逐文件仅填 body、签名锁定（仅 todo!()→实体）
diff docs/examples/scc-stub-first-contract/{stub,impl}/src/emitter.rs
```

两 crate 独立于 `cli/` workspace（`publish = false`），不进 `just ci`，作机制参考样例人工验证。

## 结论

stub-first 契约门 + 逐文件填空机制**自洽可行**：手写契约→stub 编译过→按契约填实现→整组编译/测试过 + 破环断言成立。Phase 2 可据此做 PR-C 提示词改造（translator/run/workflow）。

### 已知 scope 边界（非本样例缺陷）

- `graph interfaces --members` 对 class 仅给 `class X` 签名，方法签名由契约手写补全（已知提取边界，见 handoff 八.5）。
- 类型别名 `EventName = string` 当前未由 CLI 入图（ground-truth 节点表亦无该节点），契约 §2 手写补全。
- `EventPayload` 的 `unknown` 忠实阶段占位为 `String`（**实质收窄**：TS 可放 number/object/null）；惯用化（Phase B）可换 `serde_json::Value` 等。
