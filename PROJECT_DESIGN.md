# Rust Migration Workflow — 项目设计文档

> **版本**: v0.1 | **日期**: 2026-06-06 | **基于**: 18 路深度调研 + 103 agent deep research

---

## 一、项目定位

### 1.1 我们要解决什么问题

任何人都能对 AI 说"把这个项目用 Rust 重写"，但结果一地鸡毛——语义丢失、边界遗漏、翻译腔代码、测试不过。数据显示：AI 生成代码的问题率是人类的 1.7 倍，逻辑错误多 75%，安全漏洞密度高 2.74 倍。

**核心洞察**：代码生成正在商品化，**验证才是价值所在**。

### 1.2 我们不做什么

- **不做端到端自动迁移工具**（与 Code Metal $2 亿融资正面竞争必输）
- **不做通用代码翻译平台**（太抽象，用户不知道干什么）
- **不替代 Claude Code**（我们是增强层，不是替代品）

### 1.3 我们做什么

一套 **Claude Code 插件生态**（Skills + Workflows + Hooks + SubAgents），将散落在各处的迁移最佳实践（Bun PORTING.md、SafeTrans 迭代修复、DepTrans 依赖图引导、UA 知识图谱）编码成**可重复执行的工作流**。

**核心价值主张**：
1. **方法论编码** — 把 Bun/Claw-Code 等成功案例的方法论打包进开箱即用的工作流
2. **验证基础设施** — 行为录制→差异测试→属性测试→模糊测试的自动化管线
3. **项目自适应** — 根据源语言、项目形态、迁移动机自动选择策略
4. **持久化产出物** — 每次执行产出 PORTING.md、PARITY.md、测试集、迁移决策记录

### 1.4 目标用户

- 想用 AI 做 Rust 迁移但质量不高的开发者/团队
- 有合规需求（内存安全）必须迁移到 Rust 的企业
- 想系统化学习迁移方法论的工程师

---

## 二、核心方法论

### 2.1 从 Understand Anything 学到的

UA 52,950 stars 的成功密码：**把 LLM 从"对话伙伴"变成"流水线中的处理节点"**。

我们的设计遵循同样的原则：
- 用户执行 `/migrate`，流水线自动跑完所有阶段
- 确定性工具（tree-sitter/AST）做结构分析，LLM 做语义翻译
- 所有中间产物持久化，支持断点续传
- 主 SKILL.md 要足够详细（UA 的主 SKILL.md 有 45KB）

### 2.2 三层范式：AI-工具-人类

| 层 | 角色 | 负责什么 |
|----|------|---------|
| AI | 高吞吐执行 | 语义理解、代码翻译、测试生成 |
| 工具 | 确定性约束 | 编译器、Lint、AST 分析、覆盖率、模糊测试 |
| 人类 | 判断与责任 | 架构决策、发布节奏、兼容性、unsafe 审计 |

### 2.3 意图驱动而非逐行直译

来自 CSDN 深度文章和 pi_agent_rust 项目的核心方法论：

1. **逻辑解构** — AI 阅读源码，总结核心职责、数据契约、副作用、异常流（不含任何源语言语法）
2. **环境约束** — 人类定义架构"宪法"（PORTING.md）
3. **原生重塑** — 用 idiomatic Rust 重新实现，而非翻译

### 2.4 学术前沿技术集成

| 技术 | 来源 | 效果 |
|------|------|------|
| 编译器反馈迭代修复 | SafeTrans (ACM 2025) | 成功率从 54% → 80% |
| 依赖图拓扑排序翻译 | DepTrans (FSE 2026) | 仅需修改 <15% 目标代码 |
| 多候选生成+最优选择 | LAC2R / MCTS | 避免陷入局部最优修复 |
| Few-shot 引导修复 | SafeTrans | 为每类错误准备匹配的修复示例库 |
| 等价性验证 | MatchFixAgent (ICML 2026) | 99.2% 等价性判定覆盖率 |

---

## 三、架构设计

### 3.1 整体架构

```
┌─────────────────────────────────────────────────────────────┐
│                    用户入口 (Skills)                         │
│  /migrate-init  /migrate-plan  /migrate-run  /migrate-verify│
├─────────────────────────────────────────────────────────────┤
│                    编排层 (Workflows)                        │
│  项目画像 → 策略路由 → 阶段管理 → 进度跟踪                    │
├─────────────────────────────────────────────────────────────┤
│              分析层              │          转换层            │
│  tree-sitter (AST)              │  LLM 翻译引擎             │
│  OXC (JS/TS 深度解析)           │  PORTING.md 规则引擎       │
│  Mypy (Python 类型)             │  syn+quote (Rust 代码生成)  │
│  dependency-cruiser (JS 依赖)    │  ast-grep (模式匹配重写)    │
│  import-linter+grimp (Py 依赖)  │                            │
├─────────────────────────────────────────────────────────────┤
│                    验证层 (Harness)                          │
│  cargo check → clippy → cargo-llvm-cov → insta              │
│  proptest → cargo-fuzz → cargo-mutants → Miri               │
│  差异测试框架 → 行为录制 → 等价性检查                          │
├─────────────────────────────────────────────────────────────┤
│                    质量门禁 (Hooks)                          │
│  PostToolUse: 写入 .rs 后自动 cargo check                    │
│  TaskCompleted: 迁移模块完成后自动跑测试套件                   │
├─────────────────────────────────────────────────────────────┤
│                    FFI 桥接层 (增量迁移)                      │
│  napi-rs (Node.js) │ PyO3 (Python) │ cxx/bindgen (C/C++)    │
├─────────────────────────────────────────────────────────────┤
│                    产出物 (Artifacts)                        │
│  .rust-migration/                                           │
│  ├── PORTING.md          # 迁移规则宪法                      │
│  ├── PARITY.md           # 迁移进度跟踪                      │
│  ├── AGENTS.md           # AI 行为约束                       │
│  ├── migration-state.json # 状态机                           │
│  ├── intermediate/       # 中间产物                          │
│  ├── test-fixtures/      # 行为录制测试集                     │
│  └── decisions/          # 迁移决策记录                       │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 工作流阶段设计

#### Phase 0: 项目画像（Project Profiling）

**输入**：源码仓库路径
**产出**：`migration-state.json` 中的项目画像

自动分析并生成：
- 源语言/框架/构建系统识别
- 代码规模/模块结构/依赖图
- 测试现状（覆盖率、测试框架、测试风格）
- 关键难点识别（并发模型、FFI、动态特性、插件系统）
- 依赖可替代性分析（源语言包 → Rust crate 映射）
- 迁移动机确认（性能/安全/部署/并发/合规）

**策略路由引擎**：基于画像自动选择迁移策略模板。

#### Phase 1: 迁移规划

**输入**：项目画像
**产出**：PORTING.md、PARITY.md、AGENTS.md

自动生成初版 PORTING.md，包含 20 类规则：
1. 迁移阶段定义
2. 类型映射表
3. 错误处理模式（try/catch → Result<T,E>）
4. 内存管理/分配器策略
5. 指针/引用映射
6. 并发模式
7. 字符串处理（UTF-8/UTF-16 差异）
8. 命名约定转换
9. 模块/Crate 结构
10. 标准库函数映射
11. 禁止模式清单
12. unsafe 使用策略
13. 外部依赖映射
14. FFI 边界规则
15. 全局状态处理
16. 调度/热路径规则
17. 测试模式翻译
18. 构建系统规则
19. 惯用法映射表（Idiom Map）
20. 不确定性处理（留 TODO，禁止猜测）

**人工审查点**：生成后必须由人类审查确认。

#### Phase 2: 测试基础设施搭建

**核心原则：先迁移测试，再迁移代码。**

```
评估原项目测试质量
  → 有测试：翻译为 Rust 测试
  → 测试不足：
      ├── 行为录制（跑原版，录制 input/output）
      ├── AI 从源码逆向生成测试
      └── 覆盖率引导补充测试
  → 在原语言中验证补充测试通过
  → 建立黄金测试集
```

**测试分层**：
- L1: 黄金文件测试（录制的 input/output 对）
- L2: 属性测试（`proptest`: `for all x: old(x) == new(x)`）
- L3: 快照测试（`insta`: 锁定每个函数的输出）
- L4: 模糊测试（`cargo-fuzz`: 随机输入差异对比）
- L5: 变异测试（`cargo-mutants`: 验证测试真正保护了行为）

#### Phase 3: 增量迁移执行

**迁移顺序**：依赖图拓扑排序，叶子模块优先。

**单模块迁移循环**：
```
分析模块语义（LLM 解构）
  → 生成 3-5 个翻译候选（多候选策略）
    → 编译过滤（cargo check）
      → 测试过滤（运行该模块的测试）
        → 选择最优候选
          → clippy + 安全检查
            → 对抗性审查（独立 agent 审查）
              → 通过 → 标记完成，更新 PARITY.md
              → 不通过 → 编译器错误反馈 → LLM 重试（最多 3 轮）
```

**并行策略**：
- 无依赖关系的叶子模块可并行迁移
- 每个 agent 在独立 worktree 中工作
- 最多 5 个并发 agent（参考 UA 的限制）

#### Phase 4: 集成验证

- 差异测试：同样输入跑原版和 Rust 版，对比输出
- 性能基准：benchmark 对比，确保不退化
- unsafe 审计：cargo-geiger 扫描，Miri 检测 UB
- 覆盖率验证：cargo-llvm-cov ≥ 原项目覆盖率
- 复杂度对比：tokei + scc 检测"翻译膨胀"

#### Phase 5: 渐进部署（可选）

- Strangler Fig 模式：通过路由层逐步切换
- FFI 桥接：napi-rs / PyO3 / cxx
- Kill switch：可随时回滚到旧实现
- 双运行（Dual-Run）：并行跑新旧系统对比

---

## 四、工具链选型

### 4.1 最终选型矩阵

基于企业级成熟度深度验证后的最终决策。

#### 确认采用（ADOPT）

| 类别 | 工具 | 生产验证 |
|------|------|---------|
| 多语言 AST | **tree-sitter** | GitHub, Neovim, Zed |
| Rust 代码生成 | **syn + quote** | 几乎整个 Rust 生态 |
| 快照测试 | **insta** | Armin Ronacher (Flask/Sentry 作者) 维护 |
| 属性测试 | **proptest** | 功能稳定，"passive maintenance" |
| 模糊测试 | **cargo-fuzz** | libFuzzer 集成，OSS-Fuzz 标准 |
| UB 检测 | **Miri** | Rust 官方工具 |
| 覆盖率 | **cargo-llvm-cov** | LLVM 原生机制，比 tarpaulin 更准确 |
| 代码搜索/重写 | **ast-grep** | 14K stars，Rust 原生，极活跃 |
| 安全扫描 | **Semgrep/OpenGrep** | Dropbox, Figma 生产使用（但 Rust 规则少） |
| JS/TS 依赖 | **dependency-cruiser** | JS/TS 唯一可选 |
| Python 类型 | **Mypy** | Dropbox, Google, Meta, Bloomberg |
| Python 依赖 | **import-linter + grimp** | 可程序化、可执行约束 |
| 代码统计 | **tokei + scc** | 互补使用 |
| Node.js FFI | **napi-rs** | SWC/Next.js 生产验证 |
| Python FFI | **PyO3 + maturin** | OpenAI, Hugging Face 生产使用 |
| C FFI | **bindgen + cbindgen** | Rust 官方标准 |
| 任务运行 | **just** | 34K stars，简单可靠 |
| 测试运行 | **cargo-nextest** | cargo test 全面升级 |
| 文件监控 | **bacon** | 简单有效 |
| 文档生成 | **mdBook** | Rust 官方工具 |
| 图表 | **Mermaid** | GitHub 原生渲染 |
| unsafe 审计 | **cargo-geiger** | 快速扫描 |

#### 谨慎使用（TRIAL）

| 工具 | 风险 | 缓解措施 |
|------|------|---------|
| **petgraph** | bus factor=1，279 issues | 轻量场景可自建 adjacency list |
| **Kani** | 474 issues，对真实代码有限制 | 仅用于关键核心函数 |
| **cargo-mutants** | 个人业余项目 | 需实测大规模表现 |
| **cxx** | 作者称 MVP | 复杂 C++ 考虑 autocxx + bindgen |
| **OXC** (oxc_parser) | 0.x API 不稳定 | 备选 tree-sitter-typescript |

#### 明确不用（AVOID）

| 工具 | 原因 | 替代 |
|------|------|------|
| GoReplay | 停滞，290 issues，仅 HTTP/1.1 | mitmproxy |
| madge | 2024 年 8 月后无更新 | dependency-cruiser |
| Pyright (作为管道工具) | 单人项目，无 Python API | Mypy |
| pydeps | 无法处理动态导入 | import-linter + grimp |
| cargo-tarpaulin | 被 cargo-llvm-cov 超越 | cargo-llvm-cov |
| bolero | 246 stars，维护不够 | cargo-fuzz |
| rust-code-analysis | v0.0.x，Mozilla 不投入 | tree-sitter 自建 |
| D2 | pre-1.0，无 GitHub 原生支持 | Mermaid |
| FalkorDB (代码图) | 过度设计 | petgraph + SQLite |
| SCIP (独立使用) | 脱离 Sourcegraph 价值有限 | LSP 直接提取 |
| Graphify/UA 作为核心依赖 | 太新，语法级深度不够 | tree-sitter 直接使用 |

### 4.2 必须自建的组件（核心差异化）

| # | 组件 | 复杂度 | 说明 |
|---|------|--------|------|
| G1 | **跨语言类型 IR + Rust 类型生成器** | 高 | 从 TS/Python/Go 提取类型 → 统一中间表示 → idiomatic Rust 类型 |
| G2 | **跨语言代码图映射层** | 高 | 源/目标代码结构图对比，调用图拓扑验证 |
| G3 | **统一差异测试框架** | 中 | 跨 HTTP/CLI/库接口的录制回放 + 对比引擎 |
| G4 | **Rust Scientist 库** | 低 | 并行执行新旧代码路径，~300 行 |
| G5 | **统一依赖图格式转换器** | 低 | 各工具输出 → 统一格式 |
| G6 | **AI 迁移编排器** | 高 | 协调 LLM + 工具 + 测试的"大脑"（即 Workflow 脚本） |

### 4.3 图存储策略

- **内存图处理**：petgraph（DAG 依赖图、拓扑排序）
- **持久化存储**：SQLite（节点+边表，JSON 属性字段）
- **查询深度**：控制在 4-5 层以内（SQLite 递归 CTE 性能范围）
- **可视化**：Mermaid（文档内嵌）+ Graphviz DOT（自动生成）

---

## 五、文档体系

### 5.1 三份核心文档

#### PORTING.md — 迁移规则宪法

参考 Bun 的 576 行 PORTING.md，定义所有翻译规则：
- 20 类规则（见 Phase 1）
- 每条规则有：Zig/TS/Python 模式 → Rust 等价物
- 禁止模式清单
- 不确定性处理策略

**自动生成 + 人工审查**：Phase 1 由 AI 生成初版，人类审查确认。

#### PARITY.md — 迁移进度跟踪

参考 Claw-Code 的 9-lane checkpoint 系统：
- 每个子系统/模块一行
- 状态：pending / translating / testing / verified / done
- 关联 commit hash
- 测试通过率

#### AGENTS.md — AI 行为约束

参考 pi_agent_rust 项目：
- 禁止删除文件
- Git 安全规则
- 禁止引入未经批准的依赖
- 不确定时必须留 TODO
- 每个 unsafe 块必须有 SAFETY 注释

### 5.2 CLAUDE.md 迁移配置

```markdown
# 迁移项目配置

## 核心规则
- 本项目正在从 [源语言] 迁移到 Rust
- 所有迁移规则见 .rust-migration/PORTING.md，必须严格遵守
- 迁移进度见 .rust-migration/PARITY.md
- AI 行为约束见 .rust-migration/AGENTS.md

## 当前阶段
- Phase [0-5]，当前聚焦模块：[模块名]

## 验证要求
- 每个模块完成后必须：cargo check + clippy + 测试通过 + 覆盖率 ≥ 原版
```

---

## 六、测试与验证策略

### 6.1 验证管线（4 阶段循环）

```
阶段 1: 源码分析（确定性）
  tree-sitter → 函数签名/类型提取
  Mypy/TS API → 完整类型信息
  dependency-cruiser/import-linter → 依赖图
  ast-grep → 问题模式检测
  tokei + scc → 基线复杂度指标

阶段 2: AI 翻译（非确定性）
  LLM + 阶段 1 的上下文 → Rust 代码
  多候选生成（3-5 个）→ 编译+测试过滤 → 选最优
  编译器反馈迭代修复（2-3 轮）

阶段 3: 确定性验证
  cargo check → 编译通过
  cargo clippy --pedantic → 惯用性
  cargo-geiger → unsafe 审计
  Semgrep (p/rust) → 安全模式检测
  cargo test + nextest → 单元/集成测试
  cargo-llvm-cov → 覆盖率
  cargo-mutants → 变异测试（测试质量验证）
  Miri → unsafe UB 检测
  Kani → 关键路径形式化验证（可选）
  tokei + scc → 复杂度对比（检测翻译膨胀）
  依赖图对比 → 调用关系拓扑验证

阶段 4: 反馈循环
  验证失败 → 错误信息 + 上下文 → LLM 重新生成
  复杂度膨胀告警 → 提示 LLM 简化
  unsafe 使用 → 提示 LLM 寻找 safe 替代
```

### 6.2 "翻译膨胀"检测标准

| 指标 | 健康范围 | 告警阈值 |
|------|---------|---------|
| 代码行数比 | 1.2x - 2.0x | > 3.0x |
| 圈复杂度比 | 0.8x - 1.2x | > 1.5x |
| 函数数量比 | 0.9x - 1.3x | > 2.0x |

### 6.3 行为等价性验证

- **CLI 工具**：录制 args→stdout/stderr/exitcode，黄金文件对比
- **HTTP 服务**：mitmproxy 录制请求/响应，差异对比
- **库/SDK**：通过 FFI（PyO3/napi-rs）调用原实现，proptest 生成输入对比输出
- **有状态服务**：共享数据库 schema，对比操作后状态

---

## 七、迁移动机驱动的策略路由

不同动机决定不同的优先级和验收标准：

| 动机 | 迁移顺序 | 验收标准 | 允许"不等价"？ |
|------|---------|---------|---------------|
| 性能 | profiling 驱动，热路径优先 | benchmark 对比 | 是（更快的算法） |
| 内存安全 | unsafe 密集区优先 | CVE 消除、cargo-geiger 通过 | 否 |
| 部署简化 | 整体迁移 | 交叉编译通过、单二进制 | 否 |
| 并发安全 | 并发热点优先 | 编译通过即证明无数据竞争 | 否 |
| 合规 | 外部要求驱动 | 审计报告通过 | 否 |

---

## 八、常见陷阱与缓解

### 8.1 技术陷阱

| 陷阱 | 说明 | 缓解措施 |
|------|------|---------|
| 逐行直译 | 得到 `Arc<Mutex<>>` 满天飞的代码 | 意图驱动重构：先解构语义，再用 Rust 惯用法重新实现 |
| HashMap 迭代顺序 | Rust HashMap 随机化哈希，迭代顺序不确定 | 差异测试 + 需要顺序时用 BTreeMap 或 IndexMap |
| UTF-8 vs UTF-16 | JS string.length 返回 UTF-16 code units | 专门的字符串语义测试用例 |
| 整数溢出 | Debug panic, Release wrapping | PORTING.md 中明确溢出策略 |
| 全局状态 | 模块级变量 → OnceLock/lazy_static | 全局状态审计作为 Phase 0 的一部分 |
| 错误处理范式 | 异常冒泡 ≠ Result 传播 | 重新设计错误处理策略（anyhow vs thiserror） |
| 浮点精度 | 不同编译器/平台结果不同 | 数值计算的 epsilon 对比测试 |
| 析构顺序 | Rust Drop 与 GC finalizer 不同 | 资源清理逻辑的专项测试 |

### 8.2 流程陷阱

| 陷阱 | 说明 | 缓解措施 |
|------|------|---------|
| 上下文污染 | 同一对话反复纠正，错误累积 | 方向错了果断 Reset：清空 Git + 清空对话 |
| 认知债务 | AI 代码通过测试但没人理解 | 代码认领制度：每个模块有人能不借助 AI 解释 |
| 审查瓶颈 | AI 翻译 10K 行/h，人类审阅 200-500 行/h | 控制并行度，不追求生成速度 |
| 最后 20% | uutils 在 90% 兼容性停滞 | 接受部分模块用 FFI 桥接 |
| 迁移疲劳 | 5 年加一个递归功能（lychee 案例） | 设置里程碑，每个阶段有可见产出 |
| 性能倒退 | Arc 开销可能超过 GC | 迁移后必须跑 benchmark |

### 8.3 遗漏清单（易忽略项）

- [ ] 构建系统迁移（package.json → Cargo.toml，CI/CD 重配）
- [ ] 用户面向字符串提取和回归测试
- [ ] 许可证兼容性审计（`cargo-deny`）
- [ ] 全局状态和初始化顺序审计
- [ ] 插件系统可行性评估
- [ ] 监控/日志格式兼容性（tracing vs 传统日志）
- [ ] 回滚计划和双运行架构
- [ ] 序列化格式字节级兼容性测试
- [ ] 团队 Rust 学习曲线预算（3-6 个月达到生产力）
- [ ] 配置/环境变量处理
- [ ] 平台特定代码的 `cfg` 映射

---

## 九、Claude Code 插件结构

### 9.1 Skills（用户入口）

| Skill | 触发 | 功能 |
|-------|------|------|
| `/migrate-init` | 手动 | 初始化迁移项目，分析源码仓库，生成项目画像 |
| `/migrate-plan` | 手动 | 生成 PORTING.md + PARITY.md + AGENTS.md |
| `/migrate-test` | 手动 | 搭建测试基础设施，录制行为，生成测试套件 |
| `/migrate-run` | 手动 | 执行指定模块的迁移 |
| `/migrate-verify` | 手动 | 运行完整验证管线 |
| `/migrate-status` | 手动 | 查看迁移进度仪表板 |

### 9.2 SubAgents（专职角色）

| Agent | 职责 |
|-------|------|
| `project-profiler` | 分析源项目特征，生成画像 |
| `porting-guide-writer` | 生成 PORTING.md |
| `test-scaffolder` | 搭建测试基础设施 |
| `code-translator` | 执行代码翻译（意图驱动） |
| `equivalence-checker` | 验证行为等价性 |
| `adversarial-reviewer` | 对抗性审查翻译质量 |
| `rust-idiom-checker` | 检查 idiomatic Rust |

### 9.3 Hooks（自动化门禁）

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "event": "Write",
        "pattern": "**/*.rs",
        "command": "cargo check --message-format=json 2>&1 | head -50"
      }
    ],
    "TaskCompleted": [
      {
        "command": "cargo test --lib 2>&1 | tail -20"
      }
    ]
  }
}
```

### 9.4 产出物目录结构

```
.rust-migration/
├── PORTING.md              # 迁移规则宪法
├── PARITY.md               # 迁移进度跟踪
├── AGENTS.md               # AI 行为约束
├── migration-state.json    # 项目画像 + 状态机
├── intermediate/           # 中间分析产物
│   ├── source-graph.json   # 源码依赖图
│   ├── type-map.json       # 类型映射中间表示
│   └── call-graph.json     # 调用图
├── test-fixtures/          # 行为录制测试集
│   ├── golden/             # 黄金文件 (input/output 对)
│   └── recordings/         # HTTP/CLI 录制
├── decisions/              # 迁移决策记录
│   └── 001-error-strategy.md
└── reports/                # 验证报告
    ├── coverage.json
    ├── complexity-comparison.json
    └── unsafe-audit.json
```

---

## 十、风险评估

| 风险 | 严重度 | 可能性 | 缓解措施 |
|------|--------|--------|---------|
| LLM 进步使工具过时 | 高 | 中 | 核心价值在验证层而非生成层 |
| Code Metal 下沉到中端 | 高 | 低 | 保持开源+轻量定位 |
| 用户基数小 | 中 | 中 | 产出物（PORTING.md、测试集）独立有价值 |
| 每种语言对需大量维护 | 中 | 高 | 优先支持 TS→Rust 和 Python→Rust |
| UA 扩展到迁移场景 | 中 | 中 | 差异化在验证层，不在理解层 |
| petgraph 维护风险 | 低 | 中 | 轻量场景可自建 DAG |
| dependency-cruiser 单人风险 | 低 | 中 | 准备 fork 计划 |

---

## 十一、实施路线图

### Phase 1: MVP（4-6 周）

**目标**：跑通 TypeScript → Rust 的单模块迁移

- 实现 `/migrate-init`（项目画像）
- 实现核心 PORTING.md 生成（TS→Rust 模板）
- 实现单模块迁移循环（翻译→编译→测试→修复）
- 基础验证管线（cargo check + clippy + 测试）
- 黄金文件测试框架

### Phase 2: 质量提升（4-6 周）

- 多候选生成+最优选择
- 编译器反馈迭代修复（2-3 轮）
- 属性测试（proptest 等价性验证）
- 模糊测试（cargo-fuzz 差异对比）
- PARITY.md 自动更新

### Phase 3: 多语言支持（4-6 周）

- Python → Rust 支持（Mypy 类型提取 + PyO3 桥接）
- 统一差异测试框架
- 行为录制框架
- 依赖图可视化

### Phase 4: 完善（持续）

- C/C++ → Rust 支持
- Go → Rust 支持
- Kani 集成（关键路径验证）
- 社区反馈驱动的规则库积累
- 性能基准对比自动化

---

## 十二、关键数据参考

### 成本估算

- 每 1000 行源代码迁移：$3 - $30（含多轮迭代），中位数 ~$10/千行
- 10K 行项目：2-4 周（1-2 人）
- 50K 行项目：2-4 个月（2-4 人）
- 100K+ 行项目：6-12 个月（团队）

### 成功案例基准

| 项目 | 规模 | 耗时 | 测试通过率 |
|------|------|------|-----------|
| Bun (Zig→Rust) | 100 万行 | 11 天 | 99.8% |
| Claw-Code (TS→Rust) | 48K 行 | 4 天 | N/A |
| Pokemon Showdown (JS→Rust) | 10 万行 | 7 天 | N/A |
| Cloudflare Pingora (C→Rust) | 从零构建 | N/A | CPU-70%, 内存-67% |
| Discord (Go→Rust) | 单服务 | N/A | 消除 GC 延迟尖刺 |

### 关键论文

| 论文 | 会议 | 核心贡献 |
|------|------|---------|
| SafeTrans | ACM CCS 2025 | 迭代修复 54%→80% |
| DepTrans | ACM FSE 2026 | 7B 模型超 32B，依赖图引导 |
| Environment-in-the-Loop | ACM ReCode 2026 | 编译环境作为反馈参与者 |
| MatchFixAgent | ICML 2026 | 99.2% 等价性判定 |
| Hayroll | PLDI 2026 | C 宏翻译 |
| LLMigrate | arXiv 2025 | 调用图引导，<15% 修改 |
