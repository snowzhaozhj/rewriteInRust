# v0.7 修订摘要

基于 Codex 审查 + 8 路补充调研的核心改进。

## 1. M0 改为"假设验证周"
位置：路线图 M0
改为 5 个 spike（每个 1-2 天）：SubAgent 编排可靠性、Hook 验证、tree-sitter 精度、SKILL.md 跟随边界、Beads/AgentMemory 集成评估。产出物是假设验证报告，不是项目骨架。新增 DESIGN_ASSUMPTIONS.md 产出物。

## 2. 确定性门禁改用独立脚本（借鉴 DAE）
位置：验证层设计
关键原则："门禁用独立脚本，agent 无法说服自己跳过"。所有 Tier 0 门禁改为 .claude/scripts/ 中的独立脚本，通过 Hook 调用。不依赖 SKILL.md 提示词的指令跟随。

## 3. 降级决策改为人类确认
位置：状态机降级路径
3 轮失败后不自动降级，改为：暂停 + 生成降级分析报告 + 人类通过 `/migrate-run --degrade=ffi` 显式确认。

## 4. 增量知识沉淀架构
位置：新增或增强知识沉淀章节
- 4 层知识存储（L0 会话/L1 模块/L2 Sprint/L3 项目）
- 新增 patterns/（翻译模式库）和 anti-patterns/（失败经验库）
- PORTING.md 底部增加 changelog 节
- MDR 写入时机改为"决策发生时立即记录"
- KNOWN_DIFFERENCES.md 即时写入（verifier 发现差异时立即追加）
- 新增 SPRINT_LEARNINGS.md
- 建议评估集成 Beads（任务状态）+ AgentMemory（知识记忆）

## 5. 借鉴 RustLift 的设计
位置：验证管线或编排设计
- Approval Token：批量执行前需要预览令牌
- Preview-before-spend：AI 调用前预估 token 成本
- "不自动宣布成功"：成功停在 needs_review

## 6. 借鉴 Compound Engineering 的知识复利
位置：Sprint Review 或知识沉淀
- 每次迁移后执行知识沉淀步骤
- 核心理念："每一次工程活动都应让下一次更容易"

## 7. 反合理化表（借鉴 Agent Skills）
位置：AGENTS.md 或 SubAgent 系统提示
- 列出 agent 可能跳过验证的借口及反驳
- "verification-is-non-negotiable"

## 8. 8 处确定性 vs AI 边界修正
- PORTING.md ~60% 规则确定性模板生成
- 编译修复先跑 cargo fix，剩余给 AI
- 质量评估建立分层评分卡
- 差异分类预定义"已知差异类型表"
- 测试骨架从录制数据确定性生成
- 多候选选择用确定性指标排序
- 降级决策改为人类确认
- 编排检查点用确定性文件存在性检查

## 9. 竞品定位更新
- RustLift（C/C++→Rust 控制平面）：理念一致，市场不重叠
- Dynamic Workflows 是真正竞争来源，我们的差异化在方法论编码
- MCP 生态的 AST 工具是从下方的威胁
- Quarkus Migration Skills 的 Gate Check 模式值得参考
- 之前提到的 act101/Holonic/ShiftCodex 等是 LLM 幻觉，不存在

## 10. Plan B 体系
每个关键假设有明确的 Plan B：
- SubAgent 编排不可靠 → 微 Skill 链 / 外部脚本编排
- Hook 不可靠 → 改为 SKILL.md 显式指令
- tree-sitter 精度不够 → TS Compiler API / LLM 直接读源码
- 用户不愿学配置 → 纯约定零配置

## 修订原则
- 用中文
- 只改需要改的部分，不全面重写
- 标注哪些是 MVP 必须，哪些是后续迭代
