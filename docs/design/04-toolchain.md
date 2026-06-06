> [返回主索引](./README.md)

# 五、工具链选型

## 5.1 三档分级

| Tier | 含义 | 触发方式 | 失败影响 |
|------|------|---------|---------|
| **Tier 0 硬性门禁** | 每次写入/提交必须通过 | Hook 自动触发 | 阻塞继续 |
| **Tier 1 推荐** | 画像自动启用，可按需关闭 | Sprint Review 触发 | 警告但不阻塞 |
| **Tier 2 高级** | 用户显式启用 | 手动触发 | 可选 |

## 5.2 Tier 0：硬性门禁

每次代码变更必须通过，无例外。

| 类别 | 工具 | 用途 | 生产验证 |
|------|------|------|---------|
| 编译 | **cargo check** | 编译通过 | Rust 标准工具 |
| Lint | **cargo clippy** | 惯用性检查 | Rust 标准工具 |
| 测试 | **cargo-nextest** | 测试执行 | cargo test 全面升级 |

## 5.3 Tier 1：推荐（画像自动启用）

| 类别 | 工具 | 用途 | 何时启用 |
|------|------|------|---------|
| 覆盖率 | **cargo-llvm-cov** | LLVM 原生覆盖率 | 始终 |
| 快照 | **insta** | 快照测试（锁定输出） | 有 CLI/API 输出时 |
| 属性 | **proptest** | 属性测试（等价性验证） | 有纯函数时 |
| 许可证 | **cargo-deny** | 许可证合规 + 依赖审计 | Sprint Review 触发 |
| CVE | **cargo-audit** | 已知漏洞扫描 | Sprint Review 触发 |
| 搜索/重写 | **ast-grep** | 模式匹配 + 代码重写 | 始终 |
| 统计 | **tokei + scc** | 代码复杂度对比 | 始终 |
| 多语言 AST | **tree-sitter** | 源码结构分析 | 始终 |
| Rust 代码生成 | **syn + quote** | 宏/过程宏 | 需要代码生成时 |
| 性能基准 | **criterion** | 性能回归检测 | 默认 Tier 2；当 `migration_motives` 含 `performance` 时自动提升为 Tier 1 |
| unsafe 审计 | **cargo-geiger** | unsafe 使用统计 | 始终 |
| 任务运行 | **just** | 任务自动化 | 始终 |
| 文件监控 | **bacon** | 持续编译反馈 | 本地开发 |

**语言专用工具**：

| 源语言 | 工具 | 用途 |
|--------|------|------|
| JS/TS | **dependency-cruiser** | 依赖图分析 |
| Python | **Mypy** | 类型提取 |
| Python | **import-linter + grimp** | 依赖图分析 |

**FFI 桥接**（按需启用）：

| 目标 | 工具 | 生产验证 |
|------|------|---------|
| Node.js | **napi-rs** | SWC/Next.js |
| Python | **PyO3 + maturin** | OpenAI, Hugging Face |
| C/C++ | **bindgen + cbindgen** | Rust 官方标准 |

## 5.4 Tier 2：高级（用户显式启用）

| 类别 | 工具 | 用途 | 风险/注意 |
|------|------|------|---------|
| 模糊测试 | **cargo-fuzz** | 随机输入差异对比 | 需要 corpus 管理 |
| 变异测试 | **cargo-mutants** | 验证测试质量 | 大项目耗时长 |
| UB 检测 | **Miri** | unsafe 代码 UB 检测 | 不支持所有 FFI |
| 形式化 | **Kani** | 关键路径验证 | 474 issues，有限制 |
| 并发 | **loom / shuttle** | 并发正确性验证 | 需要专门编写测试 |
| 安全扫描 | **Semgrep/OpenGrep** | 安全模式检测 | Rust 规则较少 |
| 精细编译 | **cargo-careful** | 额外 UB 检测 | 编译慢 |

## 5.5 谨慎使用（TRIAL）

| 工具 | 风险 | 缓解措施 |
|------|------|---------|
| **petgraph** | bus factor=1，279 issues | 轻量场景可自建 adjacency list |
| **cxx** | 作者称 MVP | 复杂 C++ 考虑 autocxx + bindgen |
| **OXC** (oxc_parser) | 0.x API 不稳定 | 备选 tree-sitter-typescript |
| **Semgrep** (Rust 规则) | Rust 规则太少 | 仅作为补充，不作为主要安全工具 |

## 5.6 明确不用（AVOID）

| 工具 | 原因 | 替代 |
|------|------|------|
| GoReplay | 停滞，仅 HTTP/1.1 | mitmproxy |
| madge | 2024 年 8 月后无更新 | dependency-cruiser |
| Pyright (作为管道工具) | 无 Python API | Mypy |
| pydeps | 无法处理动态导入 | import-linter + grimp |
| cargo-tarpaulin | 被 cargo-llvm-cov 超越 | cargo-llvm-cov |
| bolero | 维护不够 | cargo-fuzz |
| D2 | pre-1.0 | Mermaid |
| FalkorDB | 过度设计 | petgraph + SQLite |

## 5.7 图存储策略

- **内存图处理**：petgraph（DAG 依赖图、拓扑排序）
- **持久化存储**：SQLite（节点+边表，JSON 属性字段）
- **查询深度**：控制在 4-5 层以内（SQLite 递归 CTE 性能范围）
- **可视化**：Mermaid（文档内嵌）+ Graphviz DOT（自动生成）

## 5.8 工具集成方式分类

所有工具按集成方式分为三类，决定了安装方式、调用路径和用户体验。

### 类别 A：嵌入 CLI（`rustmigrate` Cargo 依赖）

纯 Rust crate，编译进 `rustmigrate` 二进制。用户不需要单独安装，CLI 提供统一的 JSON 输出接口。

| 工具 | 输入 | 输出 | 集成理由 |
|------|------|------|---------|
| **tree-sitter** + 语言绑定（tree-sitter-typescript / tree-sitter-python 等） | 源文件路径 | AST 节点 JSON（类型、位置、子节点） | 多语言 AST 解析是核心能力，必须零依赖可用 |
| **ast-grep-core** | AST + 模式规则 | 匹配结果 JSON（位置、捕获） | 代码搜索/重写是高频操作，嵌入避免 CLI 调用开销 |
| **tokei** | 目录路径 | 语言统计 JSON（行数、文件数、复杂度） | 代码量对比是基础分析，嵌入保证跨平台一致性 |
| **syn + quote** | Rust 源码字符串 | Rust TokenStream / 格式化代码 | 代码生成和 AST 操作是翻译阶段核心依赖 |
| **petgraph** | 节点+边列表 | 拓扑排序、路径查询、子图提取 JSON | 依赖图是核心数据结构，内存操作性能敏感 |
| **jsonschema** | JSON 数据 + Schema 文件 | 校验结果（通过/失败+错误详情） | 检查点校验必须确定性执行，Schema 编译期内嵌 |

> **scc 与 tokei 的取舍**：5.3 节列出 `tokei + scc` 并用。v0.9.2 决定**仅嵌入 tokei**——tokei 是纯 Rust crate 可直接嵌入，覆盖核心 LOC 统计需求；scc 是 Go 编写的外部二进制，其额外的复杂度/COCOMO 估算能力可通过 tree-sitter AST 分析自行实现。如需 scc 的性能优势（大仓场景），可作为可选外部调用。

### 类别 B：外部调用（子进程 + JSON 解析）

独立工具链或非 Rust 语言编写，只能通过子进程调用。CLI 封装 `ToolRunner` trait 统一处理调用、超时、JSON 解析和错误上报。

| 工具 | 输入 | 输出 | 集成理由 |
|------|------|------|---------|
| **cargo check** | Cargo 项目路径 | 编译诊断 JSON（`--message-format=json`） | Rust 编译器本身是外部工具链 |
| **cargo clippy** | Cargo 项目路径 | Lint 诊断 JSON | 需要完整 rustc 工具链 |
| **cargo-nextest** | Cargo 项目路径 + 测试过滤 | 测试结果 JSON（JUnit XML 或 libtest JSON） | 独立二进制，替代 cargo test |
| **cargo-llvm-cov** | Cargo 项目路径 | 覆盖率 JSON（lcov 格式） | 依赖 LLVM 覆盖率工具链 |
| **cargo-deny** | Cargo.toml | 许可证/依赖审计 JSON | 独立工具，需单独安装 |
| **cargo-audit** | Cargo.lock | CVE 报告 JSON | 依赖 RustSec 数据库 |
| **cargo-geiger** | Cargo 项目路径 | unsafe 统计 JSON | 独立工具，需单独安装 |
| **cargo-fuzz** | 目标 + 语料目录 | 崩溃报告 | 依赖 libFuzzer |
| **cargo-mutants** | Cargo 项目路径 | 变异测试报告 JSON | 独立工具，耗时长 |
| **Miri** | Cargo 项目路径 | UB 检测报告 | rustup 组件，需单独安装 |
| **dependency-cruiser** | JS/TS 项目路径 | 依赖图 JSON | Node.js 工具，需 `npx` |
| **Mypy** | Python 项目路径 | 类型信息 JSON（`--output=json`） | Python 工具，需 `pip` |
| **import-linter + grimp** | Python 项目路径 | 依赖图 JSON | Python 工具，需 `pip` |
| **just** | Justfile | 任务执行结果 | 任务运行器，替代 Makefile |
| **bacon** | Cargo 项目路径 | 持续编译反馈 | 文件监控，本地开发辅助 |
| **Kani** | Cargo 项目路径 + 验证目标 | 形式化验证结果 | Tier 2，需 nightly + 额外安装 |
| **Semgrep/OpenGrep** | 源码 + 规则文件 | 安全扫描结果 JSON | 安全检测，Rust 规则较少 |
| **cargo-careful** | Cargo 项目路径 | UB 检测结果 | 编译慢，Tier 2 补充 |

### 类别 C：目标项目依赖（scaffold 注入）

被迁移项目的 `dev-dependencies` 或 `dependencies`，由 `rustmigrate scaffold` 命令注入到目标项目的 `Cargo.toml`。

| 工具 | 输入 | 输出 | 集成理由 |
|------|------|------|---------|
| **insta** | 测试函数中的值 | 快照文件（`.snap`） | 快照测试框架，需作为目标项目的 dev-dependency |
| **proptest** | 属性策略定义 | 测试结果 + 回归种子文件 | 属性测试框架，需编译进目标测试二进制 |
| **criterion** | 基准测试函数 | 性能报告 HTML/JSON | 基准测试框架，需作为目标项目的 dev-dependency |
| **loom / shuttle** | 并发测试代码 | 状态空间探索结果 | 并发测试框架，需替换标准库原语 |
| **napi-rs** | Rust 函数 + `#[napi]` 宏 | Node.js 可调用的 `.node` 二进制 | FFI 桥接，需作为目标项目的 dependency |
| **PyO3** | Rust 函数 + `#[pyfunction]` 宏 | Python 可调用的 `.so/.pyd` | FFI 桥接，需作为目标项目的 dependency |
| **bindgen / cbindgen** | C/C++ 头文件 / Rust 源码 | Rust FFI 绑定 / C 头文件 | FFI 桥接，需在目标项目的 build.rs 中配置 |
