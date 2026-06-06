# 设计文档修订摘要

本文件汇总所有审查反馈和补充调研的修改要求，供重写 agent 使用。

## A. 4 路审查的共识修改点

### A1. 定位调整（产品审查 + Codex）
- 从"迁移生态平台"调整为"Rust 迁移验证工作台"
- 验证层是核心价值，自动翻译降级为辅助能力
- 不承诺"等价性覆盖"，改为"不等价证据收集系统"

### A2. 架构修改（架构审查）
- 砍掉"通用类型 IR"（G1），改为语言专用类型提取 + LLM 映射
- G2 保留拓扑排序，砍掉跨语言代码图"对比验证"
- Phase 0/1 边界要清晰：Phase 0 只做客观事实采集，Phase 1 做主观决策
- Phase 2 修正循环依赖：Rust 代码不存在时不能写 Rust 测试，改为行为录制 + 接口契约
- Phase 3 加降级路径：3 轮重试后降级为 FFI 桥接或人工介入或功能裁剪
- 补充编排器的状态机设计、错误恢复、断点续传
- 7 个 SubAgent 合并为 4 个：analyzer, translator, verifier, scaffolder

### A3. 工具链调整（技术审查 + 企业验证）
- Semgrep: ADOPT → TRIAL（Rust 规则太少）
- cargo-mutants: TRIAL → ADOPT（测试策略核心依赖它就必须 ADOPT）
- 新增 ADOPT：criterion（性能基准）、cargo-deny（许可证审计）、cargo-audit（CVE 扫描）
- 新增 TRIAL：loom/shuttle（并发测试）、cargo-careful
- 工具链分三档：Tier 0 硬性门禁 / Tier 1 推荐（画像自动启用）/ Tier 2 高级（用户显式启用）

### A4. 测试策略补充（技术审查）
- 新增 L0 层：E2E 差异测试
- 新增 L6 层：criterion 性能回归
- 新增 L7 层：loom/shuttle 并发正确性
- 测试执行确定性保障（proptest seed、corpus 持久化）

### A5. PORTING.md 补充（技术审查 + Codex）
- 新增规则 #21：生命周期与所有权模式映射
- 新增规则 #22：异步运行时与并发原语映射
- 新增规则 #23：序列化/反序列化兼容性
- 新增规则 #24：日志/可观测性映射
- 新增规则 #25：平台特定行为映射
- PORTING.md 渐进式生成：初版只生成必须的规则，迁移失败后追加

### A6. 遗漏项补充（架构 + 技术 + Codex）
- LLM 上下文窗口管理策略
- 源项目在迁移期间的变更处理（版本锁定）
- Cargo workspace 结构设计
- 异步运行时选择（tokio/async-std）
- CI/CD 集成
- 多 agent 并行时的共享文件合并冲突
- 增加 Phase 0.5：原项目可复现基线
- unsafe 分类管理制度（不只是"加注释"）
- 案例引用分级标注（论文结果 vs 商业案例 vs 社区传闻）
- 路线图加验收指标
- 跨语言语义陷阱补充（闭包语义、null 三态、隐式类型转换、正则方言、字符串切片 panic、模块初始化顺序、迭代器惰性、整数大小、相等性语义）

### A7. 路线图现实性（架构 + Codex）
- MVP 4-6 周范围过大，缩减到 TS 单模块纯函数/CLI 子模块
- 质量阶段 4-6 周偏乐观，改为 8-12 周
- 多语言 4-6 周不现实，改为 8-16 周
- 路线图加验收指标（如：MVP 验收 = 在 3 个真实 TS 小项目中完成 1 个模块迁移）

## B. 补充调研新增内容

### B1. 知识沉淀体系（新增章节）
- MDR（迁移决策记录）格式和生成机制
- 三级代码注释策略（模块级溯源 / 函数级决策 / 行内保留）
- KNOWN_DIFFERENCES.md 产出物
- 迁移产物生命周期（长期保留 / 归档 / 安全丢弃）
- Phase 6: 知识固化
- /migrate-graduate skill：评估从迁移模式过渡到原生开发
- /migrate-unsafe-audit skill
- 从"迁移模式"到"原生开发模式"的 Graduation Criteria
- unsafe 清理优先级矩阵（P0-P4）

### B2. 执行模式重构（新增章节）
- 线性 Phase 改为 Sprint 循环模型
  - 外循环：Sprint 级（跨会话/天/周）
  - 内循环：模块级（单会话内）
- Work Unit 概念：每个会话是一个完整的工作单元
- 三层反馈循环：L1 编译（秒级）/ L2 测试（分钟级）/ L3 集成（Sprint 级）
- 问题前移矩阵（哪些能在开发阶段发现 vs 必须测试发现）
- 三种并行开发策略：功能冻结 / 双轨开发 / Strangler Fig
- PORTING.md 版本化演化（每 Sprint Review 可修改）
- migration-state.json 完整 schema（含模块 substatus、尝试历史、Sprint 元数据）
- PARITY.md 增强（Sprint 级聚合、管理层视图、风险登记）

### B3. 工作流灵活性（重构已有章节）
- 组件可选性：Tier 0/1/2 三层分级 + 自动触发条件
- 验证管线改为 DAG 而非线性序列
- 多语言项目处理：语言热图 + FFI 边界检测 + 迁移策略决策树
- 语言扩展架构：LanguageAdapter trait + 适配器模式
- .rustmigrate.toml 配置文件设计
- 4 级渐进式用户旅程
- 智能项目类型检测信号矩阵

## C. 修订原则
- 用中文
- 结构清晰，可执行
- 区分 MVP 必须 vs 后续迭代
- 案例引用要标注证据等级
- 不过度承诺
