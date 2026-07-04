# M4 Sprint D 验收记录（Plugin Go 适配，PLG-01~06）

> Go 线 Sprint D。PLG-01/02 由 PR #62 交付；PLG-03~06 由 PR #63 交付。本文记录 PLG-05/06 的端到端验证证据。

## 验收标准对照（PLAN-M4 §Sprint D）

| 验收项 | 状态 | 证据 |
|--------|------|------|
| `adapters/go/` 两文件契约（frontmatter 含 `language_id`/`target_languages:[go]`/`rule_version`） | ✅ | PR #62（PLG-01/02） |
| `/migrate analyze` 对 Go 项目正确填充 `migration-state.json` | ✅ | 本文 §1 |
| `/migrate run` 对 Go fixture headless 跑通，≥1 模块推进到 `translating` | ✅ | 本文 §2 |
| translator 可按 `source_language=go` 切换类型映射表 | ✅ | PLG-03（translator.md Go 分支 + 语言基线机制） |
| goroutine/channel/unsafe.Pointer 等触发 degrade_skip 并输出推荐替代 | ✅ | 本文 §3（classify_file danger + porting-template crate 推荐） |

## §1 analyze 确定性链路（init → graph build → populate）

`fixtures/go-linear-deps`（3 目录：main/service/utils）：

```
init → ok
graph build --root . → node=11 edge=13（status=warning：Go 全量降级，符合 CLI 契约）
populate-modules --root . → 1 个 coupled_batch 组（3 文件同 module 凝聚），tier/danger 落 state
```

MDR-011 目录优先凝聚对 Go 生效（跨目录同 module 的文件凝聚为迁移单位）。

## §2 run 推进到 translating（单文件纯计算 Go 模块）

单文件 Go 项目（`mathx.go`：`Fib`/`GCD` 纯计算，无危险信号）headless 全链路：

```
init → graph build --root . → populate-modules --root .
  → modules[file:mathx.go]{status:pending, tier:standard, danger:[]}
state transition --to profile → plan → scaffold → sprint_loop   （项目级推进）
state transition --module file:mathx.go --to translating         （模块级，run 首步）
  → state get file:mathx.go: status=translating ✓
```

**真实 Phase A 翻译**（按 translator.md Go 分支：大写导出 `Fib`/`GCD`→`pub fn`、`int`→`i64`、
snake_case、1:1 结构对应、Go `a,b = b,a%b`→Rust 元组同步赋值）：

```rust
pub fn fib(n: i64) -> i64 { /* 迭代 */ }
pub fn gcd(mut a: i64, mut b: i64) -> i64 { while b != 0 { (a, b) = (b, a % b); } a }
```

- `cargo check` 绿；行为等价测试通过：`fib(10)==55`、`gcd(48,18)==6`（与 Go 语义一致）。

## §3 PLG-05 degrade_skip Go 边界（danger 端到端）

**Go `classify_file` danger 分类**（`go.rs`）：goroutine/select/channel/`<-` → `Concurrency`；
`reflect` → `DynamicReflection`；cgo(`import "C"`)/`unsafe.Pointer` → `Ffi`（口径与 GO-01/PLG-04
及 `adapters/go/porting-template.md` 统一）。

端到端（含并发 + 反射的两文件同包项目）：

```
worker.go（goroutine + channel）+ meta.go（import "reflect"）
  → populate 默认 decompose 凝聚为 1 coupled_batch
  → 组 danger = ["concurrency", "dynamic_reflection"]（成员并集，字典序）落 migration-state.json
```

`ModuleState.danger` 驱动 headless run 的 degrade_skip 边界与 verifier 定向探测；`recommended_alternatives`
的并发/FFI crate 取自 `porting-template.md`（`go f()`→tokio/rayon、`chan`/`select`→mpsc/crossbeam、
cgo→bindgen）。`blocked_by_skip` 传播复用 M3 机制（run.md 裁剪依赖注入，语言无关）。

## §4 修复的 pre-existing bug（PLG-06 暴露）

**populate tier 分档硬编码 TS adapter**：`detect::detect_tier` 硬编码 `SourceLang::TypeScript`
→ 非 TS 文件 `can_handle=false` → 一律保守判 `Full`（Go/Python tier 全失真）。修复：populate
复用已解析的 `lang`（config 优先→探测→回退 TS），改调 `detect_tier_for_lang`。修复后 Go 纯计算
单文件正确判 `standard`（此前恒 `full`）。回归测试 `e2e_populate_go_tier_detected_by_language`。

## 测试基线

- `go.rs` 单测 51（+10 `classify_file_danger_categories`）；`cli_e2e` 81（+2：Go danger 端到端 +
  tier-by-language 回归）。
- `just ci` 全绿（fmt + clippy -D + test + deny + shellcheck）。
