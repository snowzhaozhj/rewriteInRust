# 实施路线图与关键数据参考

> [返回主索引](./README.md)

---

## 十三、实施路线图

### M0: 假设验证周（1-2 周）

**目标**：验证 5 个关键技术假设，产出假设验证报告，而非项目骨架

**5 个 Spike（每个 1-2 天，Spike 3/5 可并行执行以缩短总时长）**：
- [ ] **Spike 1: SubAgent 编排可靠性** — 验证 Claude 能否可靠执行 3+ 步的 SubAgent 调度序列（Plan B：微 Skill 链 / 外部脚本编排）
- [ ] **Spike 2: rust-analyzer LSP 验证** — 验证 rust-analyzer LSP Plugin 在写入 .rs 文件后的诊断反馈延迟和可靠性（Plan B：回退到 PostToolUse Hook + cargo check）
- [ ] **Spike 3: tree-sitter 精度** — 验证 tree-sitter 对 TS 项目的 AST 解析精度是否满足模块拆分需求（Plan B：TS Compiler API / LLM 直接读源码）
- [ ] **Spike 4: SKILL.md 跟随边界** — 验证 SKILL.md 长指令（>2000 字）的指令跟随率和遗漏率（Plan B：拆分为多个短 Skill）
- [ ] **Spike 5: Beads/AgentMemory 集成评估** — 评估 Beads（任务状态持久化）和 AgentMemory（知识记忆）的集成可行性

**交付物**：
- [ ] `DESIGN_ASSUMPTIONS.md` — 假设验证报告（每个 Spike 的结论 + Plan B 是否触发）
- [ ] `migration-state.json` schema 定义（沿用）
- [ ] `.rustmigrate.toml` 配置 schema（沿用）

**验收指标**：5 个 Spike 全部完成，每个假设有明确的"验证通过"或"触发 Plan B"结论。

### M1: MVP（6-8 周）

**目标**：跑通 TypeScript → Rust 的**单模块纯函数/CLI 子模块**迁移

**范围限定（MVP 必须）**：
- [ ] `/migrate analyze` 完整版（合并原 init + plan + test：TS 项目画像 + 依赖图 + PORTING.md 生成 + 黄金文件测试搭建）
- [ ] `/migrate run` — 单模块迁移内循环（含 Phase A/B 双阶段翻译）
- [ ] `/migrate review` — 验证管线 + 进度仪表板（合并原 verify + status）
- [ ] Tier 0 门禁集成（cargo check + clippy + cargo test）
- [ ] 编译器反馈迭代修复（最多 3 轮）
- [ ] `migration-state.json` 状态管理 + 断点续传
- [ ] PARITY.md 自动更新
- [ ] 基础 MDR 模板

**MVP 不包含（后续迭代）**：
- 多候选生成
- 属性测试 / 模糊测试
- 多 agent 并行
- 行为录制框架
- `/migrate graduate`（含 unsafe 审计）

**验收指标**：
- 在 3 个真实 TS 小项目（<5K 行）中，每个项目至少完成 1 个纯函数模块的迁移（合计 ≥3 个模块）
- 迁移后代码通过 Tier 0 门禁
- 黄金文件测试 100% 通过
- 用户从 `/migrate analyze` 到 `/migrate review` 的完整流程可在 30 分钟内走通（单模块）
- 断点续传验证：中断会话后恢复，不丢失 migration-state.json 状态

**M1 工作量分解（粗估）**：

| 交付物 | 预估人天 | 说明 |
|--------|---------|------|
| 3 个 SKILL.md（analyze/run/review） | 4-6 | 每个 Skill 约 1.5-2 天（含调试）；analyze 最复杂 |
| 4 个 SubAgent agent.md | 3-4 | 系统提示编写 + 职责边界定义 |
| 2 个 Hook 脚本（fmt.sh + verify.sh） | 1 | check.sh 被 rust-analyzer 替代，减少 1 个 |
| 文件保护 Hook（file-guard.sh） | 0.5 | PreToolUse 文件保护 |
| 通用规则文件（.claude/rules/） | 2-3 | TS 通用规则拆分为独立文件 |
| TS 语言适配器 | 3-5 | detect.sh + extract-types.sh + extract-deps.sh + porting-template.md |
| Plugin 打包结构 | 1-2 | plugin.json + 目录组织 |
| migration-state.json 管理 | 2-3 | Schema 定义 + 状态流转逻辑 + 断点续传 |
| .rustmigrate.toml | 1 | Schema 定义 + 默认值生成 |
| 集成测试 + 调试 | 5-8 | 3 个真实项目上的端到端验证 |
| **合计** | **22-33** | 1 人约 6-8 周，2 人约 3-4 周 |

### M2: 质量提升（8-12 周）

**目标**：验证管线完整，翻译质量可靠

**交付物**：
- [ ] 多候选生成 + 最优选择
- [ ] 属性测试（proptest 等价性验证）
- [ ] 模糊测试（cargo-fuzz 差异对比）
- [ ] 变异测试（cargo-mutants 测试质量验证）
- [ ] 覆盖率门禁（cargo-llvm-cov）
- [ ] 行为录制框架（CLI + 库接口）
- [ ] KNOWN_DIFFERENCES.md 自动生成
- [ ] 降级路径实现（FFI 桥接）
- [ ] `/migrate review` 完整验证管线
- [ ] `/migrate graduate` 基础版（含 unsafe 审计）
- [ ] Sprint 循环外循环支持
- [ ] Workflow 定义文件（ultracode 格式）
- [ ] 多 agent worktree 隔离机制
- [ ] /goal 自主迁移循环支持

**验收指标**：
- 在 3 个真实 TS 中型项目（5K-20K 行）中完成多模块迁移
- 属性测试覆盖核心函数
- 翻译膨胀率 < 3.0x
- 降级路径（FFI 桥接）在至少 1 个复杂模块上成功

### M3: 多语言支持（8-16 周）

**目标**：支持 Python → Rust

**交付物**：
- [ ] Python LanguageAdapter（Mypy 类型提取 + PyO3 桥接）
- [ ] Python 专用 PORTING.md 模板
- [ ] 统一差异测试框架
- [ ] `/migrate graduate` 毕业评估
- [ ] 性能基准对比自动化（criterion 集成）
- [ ] 并发测试（loom/shuttle 集成）
- [ ] 依赖图可视化（Mermaid 自动生成）

**验收指标**：
- 在 2 个真实 Python 项目中完成至少 1 个模块迁移
- Python FFI 桥接（PyO3）在迁移项目中可用
- 毕业评估能正确识别"已完成"vs"未完成"状态

### M4: 完善（持续）

- C/C++ LanguageAdapter（bindgen + cbindgen）
- Go LanguageAdapter
- Kani 集成（关键路径形式化验证）
- 社区反馈驱动的规则库积累
- 多 agent 并行编排优化
- Strangler Fig 模式工具支持

---

## 十四、关键数据参考

### 成本估算

> **注意**：以下"预估成本"栏**仅指 LLM API 调用成本**，不含人力、基础设施、CI/CD 等其他费用。

| 规模 | 预估时间 | 预估成本（仅 LLM API） | 备注 |
|------|---------|----------------------|------|
| 1K 行 | 1-3 天 | $3-$30 | 纯函数模块 |
| 10K 行 | 2-4 周（1-2 人） | $30-$300 | 含测试搭建 |
| 50K 行 | 2-4 个月（2-4 人） | $150-$1500 | 含 FFI 桥接 |
| 100K+ 行 | 6-12 个月（团队） | 视项目而定 | 必须用 Strangler Fig |

### 行业参考案例

> **注意**：以下案例仅作为行业参考，不作为时间或质量基准。不同项目的复杂度、团队经验、工具成熟度差异巨大，直接对标可能产生误导。

| 项目 | 规模 | 耗时 | 结果 | 证据等级 | 可参考维度 |
|------|------|------|------|---------|-----------|
| Bun (Zig→Rust) | 100 万行 | 11 天 | 99.8% 测试通过 | **商业案例**（Bun 团队博客） | 测试驱动验证流程、大规模迁移节奏 |
| Claw-Code (TS→Rust) | 48K 行 | 4 天 | 功能完整 | **社区传闻**（GitHub 项目） | AI 辅助翻译工作流、PORTING.md 实践 |
| Pokemon Showdown (JS→Rust) | 10 万行 | 7 天 | 功能完整 | **社区传闻**（GitHub 项目） | 大型 JS 项目迁移模式、模块拆分策略 |
| Cloudflare Pingora (C→Rust) | 从零构建 | N/A | CPU-70%, 内存-67% | **商业案例**（Cloudflare 博客） | 性能动机验收标准、FFI 桥接方案 |
| Discord (Go→Rust) | 单服务 | N/A | 消除 GC 延迟尖刺 | **商业案例**（Discord 博客） | 并发安全动机、GC→所有权模型迁移 |

> **注意**：Bun 和 Claw-Code 的极端速度可能包含未公开的前期准备工作，不应作为时间估算基准。

**Bun PORTING.md 引用说明**：本项目设计中多处引用 Bun 的 576 行 PORTING.md 作为方法论参考（[见文档体系 > 迁移规则体系](./05-documentation-system.md#62-迁移规则体系通用--项目专有)）。该引用**待验证**——需确认 Bun 的 PORTING.md 当前是否仍然公开可访问、内容是否与引用描述一致。M0 阶段应安排验证此引用，如不可验证则调整为其他公开案例或移除具体行数引用。

### 关键论文

| 论文 | 会议 | 核心贡献 | 与本项目关系 | 验证状态 |
|------|------|---------|-------------|---------|
| SafeTrans | ACM CCS 2025 | 迭代修复 54%→80% | 反馈循环设计基础 | 已验证 |
| DepTrans | ACM FSE 2026 | 7B 模型超 32B，依赖图引导 | 拓扑排序翻译策略 | 待验证 |
| Environment-in-the-Loop | ACM ReCode 2026 | 编译环境作为反馈参与者 | F1 反馈循环理论依据 | 待验证 |
| MatchFixAgent | ICML 2026 | 99.2% 等价性判定 | 验证层方法参考 | 待验证 |
| Hayroll | PLDI 2026 | C 宏翻译 | C→Rust 适配器参考 | 待验证 |
| LLMigrate | arXiv 2025 | 调用图引导，<15% 修改 | 依赖图分析策略 | 已验证 |
