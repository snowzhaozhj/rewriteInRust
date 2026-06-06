# 常见陷阱与风险评估

> [返回主索引](./README.md)

---

## 九、常见陷阱与缓解

### 9.1 技术陷阱

| 陷阱 | 说明 | 缓解措施 |
|------|------|---------|
| 逐行直译 | 得到 `Arc<Mutex<>>` 满天飞的代码 | 意图驱动重构：先解构语义，再用 Rust 惯用法重新实现 |
| HashMap 迭代顺序 | Rust HashMap 随机化哈希 | 差异测试 + 需要顺序时用 BTreeMap 或 IndexMap |
| UTF-8 vs UTF-16 | JS string.length 返回 UTF-16 code units | 专门的字符串语义测试用例 |
| 整数溢出 | Debug panic, Release wrapping | PORTING.md 中明确溢出策略 |
| 全局状态 | 模块级变量 → OnceLock/lazy_static | 全局状态审计作为 PROFILE 阶段的一部分 |
| 错误处理范式 | 异常冒泡 ≠ Result 传播 | 重新设计错误处理策略（MDR 记录） |
| 浮点精度 | 不同编译器/平台结果不同 | 数值计算的 epsilon 对比测试 |
| 析构顺序 | Rust Drop 与 GC finalizer 不同 | 资源清理逻辑的专项测试 |

### 9.2 跨语言语义陷阱补充

| 陷阱 | 源语言 | Rust 差异 | 缓解 |
|------|--------|----------|------|
| 闭包语义 | JS/Python 引用捕获 | Rust 需明确 move/borrow | PORTING.md 规则 + clippy |
| null 三态 | JS: null/undefined/absent | Rust: Option<T> | 类型映射时统一为 Option |
| 隐式类型转换 | JS: "1" + 1 = "11" | Rust 无隐式转换 | PORTING.md 禁止模式 |
| 正则方言 | JS/Python/Rust regex 差异 | 语法/Unicode 支持不同 | 正则表达式专项测试 |
| 字符串切片 panic | — | Rust 非 char 边界切片 panic | 使用 .get() 安全访问 |
| 模块初始化顺序 | 语言定义顺序 | Rust 无保证 | OnceLock 显式初始化 |
| 迭代器惰性 | Python generator 惰性 | Rust iterator 也惰性但语义不同 | 注意 collect 时机 |
| 整数大小 | JS number = f64 | Rust 多种整数类型 | 类型映射表明确 |
| 相等性语义 | JS == vs === | Rust PartialEq/Eq | PORTING.md 统一规则 |
| **Promise eager vs Future lazy** | JS Promise 创建即执行 | Rust Future 需要 executor 驱动，不 `.await` 不执行 | 审查所有 async 调用点，确保 Future 被驱动；PORTING.md 规则 22 专项覆盖 |
| **Send/Sync 约束传染** | 源语言无编译期线程安全标记 | 跨 `.await` 持有的类型必须 Send，共享引用必须 Sync | 迁移后大量类型编译失败；需提前审计并发共享点，选择 Arc/Mutex 或重构 |
| **可变性传播与架构重组** | 源语言允许多处同时修改对象 | `&mut` 排他借用，同一时刻只能有一个可变引用 | 无法直译多处同时写入的模式；需重构为消息传递、Cell/RefCell 或拆分数据结构 |
| **异步取消安全性（Cancel Safety）** | JS Promise 创建后无法取消，始终运行到完成 | Rust Future 可在任意 `.await` 点被 drop（取消）；`tokio::select!` / `tokio::time::timeout` 会导致未选中的 Future 被 drop | 如果 Future 持有锁或处于半完成写操作状态，被 drop 会导致数据不一致。迁移时需逐一审查 `select!`/`timeout` 中的 Future 是否取消安全；PORTING.md 规则 22（异步运行时）中专项覆盖取消安全性审查 |

### 9.3 流程陷阱

| 陷阱 | 说明 | 缓解措施 |
|------|------|---------|
| 上下文污染 | 同一对话反复纠正，错误累积 | 方向错了果断 Reset：清空 Git + 清空对话 |
| 认知债务 | AI 代码通过测试但没人理解 | 代码认领制度：每个模块有人能不借助 AI 解释 |
| 审查瓶颈 | AI 翻译 10K 行/h，人类审阅 200-500 行/h | 控制并行度，不追求生成速度 |
| 最后 20% | uutils 在 90% 兼容性停滞 | 接受部分模块用 FFI 桥接（降级路径） |
| 迁移疲劳 | 5 年加一个递归功能（lychee 案例） | Sprint 里程碑，每个 Sprint 有可见产出 |
| 性能倒退 | Arc 开销可能超过 GC | 迁移后必须跑 criterion benchmark |
| 源项目变更 | 迁移期间源项目继续开发 | 选择并行策略（功能冻结/双轨/Strangler Fig） |
| 多 agent 冲突 | 多 agent 并行时共享文件合并冲突 | 每个 agent 在独立 worktree 中工作 |

### 9.4 遗漏清单（易忽略项）

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
- [ ] **Cargo workspace 结构设计**（MDR 记录）
- [ ] **异步运行时选择**（tokio/async-std，MDR 记录）
- [ ] **CI/CD 集成**（GitHub Actions/GitLab CI Rust 配置）

---

## 十二、风险评估

### 12.1 风险矩阵

| 风险 | 严重度 | 可能性 | 缓解措施 |
|------|--------|--------|---------|
| **MVP 编排依赖指令跟随** | 高 | 高 | MVP 编排通过 SKILL.md 指令实现，可靠性取决于 LLM 指令跟随能力（非确定性程序控制）。M0 Spike 1 验证后决定是否触发 Plan B（微 Skill 链 / 外部脚本编排）。**用户需知晓 MVP 阶段可能需手动干预编排流程** |
| 用户自建迁移工作流（AI IDE + 手动 prompt） | 高 | 高 | 最直接的替代方案。差异化在确定性门禁（独立脚本）和标准化产出物体系——纯 prompt 无法阻止 AI 跳过验证。**<2K 行项目建议直接手动迁移** |
| LLM 进步使工具过时 | 高 | 中 | 核心价值在验证层而非生成层——即使 LLM 翻译变完美，验证仍然需要 |
| Code Metal 下沉到中端 | 高 | 低 | 保持开源+轻量定位，做他们不做的验证层 |
| Dynamic Workflows 竞争 | 高 | 中 | 真正竞争来源——我们的差异化在**方法论编码**（PORTING.md + 验证管线），而非通用工作流 |
| MCP 生态 AST 工具从下方威胁 | 中 | 中 | 低层 AST 工具会商品化，我们的价值在方法论层而非工具层 |
| 用户基数小 | 中 | 中 | 产出物（PORTING.md、测试集、MDR）独立有价值 |
| 每种语言对需大量维护 | 中 | 高 | 优先支持 TS→Rust，LanguageAdapter 降低扩展成本 |
| UA 扩展到迁移场景 | 中 | 中 | 差异化在验证层，不在理解层 |
| petgraph 维护风险 | 低 | 中 | 轻量场景可自建 DAG |
| dependency-cruiser 单人风险 | 低 | 中 | 准备 fork 计划 |
| 过度设计 | 中 | 高 | **MVP 聚焦 TS 单模块，拒绝 scope creep** |

**竞品定位说明**：
- **RustLift**（C/C++→Rust 控制平面）：理念一致（Approval Token 等机制已借鉴），市场不重叠（他们做 C/C++，我们从 TS 起步）
- **Quarkus Migration Skills** 的 Gate Check 模式值得参考（已融入独立脚本门禁设计）
- ~~act101 / Holonic / ShiftCodex~~ — 经调研确认为 LLM 幻觉，不存在此类产品

### 12.2 Plan B 体系

每个关键技术假设有明确的 Plan B，在 M0 假设验证周中判定是否触发：

| 关键假设 | 验证方式（M0 Spike） | Plan B |
|---------|---------------------|--------|
| SubAgent 编排可靠 | Spike 1: 3+ 步调度序列测试 | 微 Skill 链（每个 Skill 只做 1 步）/ 外部脚本编排 |
| Hook 触发可靠 | Spike 2: PostToolUse 场景测试 | 改为 SKILL.md 显式指令 + 独立脚本 |
| tree-sitter 精度足够 | Spike 3: TS 项目 AST 精度测试 | TS Compiler API / LLM 直接读源码 |
| SKILL.md 长指令可跟随 | Spike 4: >2000 字指令跟随率测试 | 拆分为多个短 Skill |
| 用户愿意学配置 | 用户反馈收集 | 纯约定零配置模式（合理默认值，无 .rustmigrate.toml） |

**Plan B 具体方案**（M0 Spike 1 失败时触发）：

| 方案 | 实现方式 | 代价 | 用户体验退化 |
|------|---------|------|-------------|
| **Plan B1: 微 Skill 链** | 将 `/migrate run` 拆为 `/migrate-translate`、`/migrate-check`、`/migrate-test-gen` 等微命令，每个 Skill 只做 1 步（1 次 SubAgent 调用）。状态通过 `migration-state.json` 在微 Skill 间传递。用户手动或脚本串联。 | 额外 2-3 人天开发 | 用户需手动执行更多命令，但每步更可控 |
| **Plan B2: 外部脚本编排** | 用 bash/Python 脚本调用 Claude Code CLI（`claude -p "执行 /migrate run ..."`），脚本中做 if-else 分支、文件检查、重试逻辑。编排逻辑 100% 确定性。 | 额外 3-5 人天开发 | 依赖 Claude Code CLI API 的稳定性；需用户安装额外依赖 |
| **Plan B3: 混合方案** | 简单步骤（1-2 步）用 SKILL.md 指令，复杂编排（3+ 步循环/条件）用外部脚本。取两者优势。 | 额外 2-4 人天开发 | 最可能的实际落地方案 |

**行动指南**：M0 结束后更新 `DESIGN_ASSUMPTIONS.md`，标记每个假设的状态（verified / plan-b-triggered），后续里程碑据此调整实现方案。
