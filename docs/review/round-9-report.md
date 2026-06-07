# 本轮审查报告

**轮次**: Round 1  
**发现总数**: 22 条  
**状态分布**: confirmed 16 | adjusted 3 | rejected 7

---

## 维度 1: 迁移质量与翻译方法论

### D1-01 | medium | 03-execution-model.md | confirmed
**问题**: SS7.7 universal proptest gate 与 SS7.1.1 module-type-scoped L2 matrix 交叉矛盾——SS7.1.1 仅对纯函数强制 L2，而 SS7.7 对所有函数无条件强制 proptest 生成，verifier 收到互斥指令。  
**方案**: 在 SS7.7 行动指南前新增关系说明段落，区分纯函数（FFI 等价断言=L2）与非纯函数（自正确性断言、无 FFI、不受 L2 非强制约束）。同步 09 Step 3.3 检查点改为条件性。

---

## 维度 2: 验证体系可靠性

### D2-01 | medium | 03-execution-model.md | rejected
**问题**: L3-simplified 对有状态库模块的 M1 强制要求与 04 SS5.3 M2 deferral 冲突。  
**Verifier 理由**: SS7.1 line 460 明确区分三种 L3-simplified 路径；有状态对象使用 shell scripts + operation sequences（进程级调用），不依赖 M2 的 FFI behavior recording framework。两节讨论不同机制，不构成矛盾。

### D2-02 | low | 03-execution-model.md | adjusted
**问题**: ops-recording.jsonl 无 schema 定义，而它是 L3-simplified 有状态测试和 Step 4 确定性生成的输入。  
**方案**: 在 09 附录 D 末尾新增一行说明 schema 延至 M1 定义（与设计全局 schema-deferral 模式一致）。严重度从 medium 下调为 low。

### D2-03 | medium | 03-execution-model.md | confirmed
**问题**: SS7.7 dimension proptest（边界测试）与 L2 proptest（等价测试）使用同一 proptest 名称但语义不同，文档未区分，verifier 可能将 L2 非强制误读为跳过 SS7.7 维度 proptest。  
**方案**: 与 D1-01 合并修复——在 SS7.7 行动指南前统一说明两种 proptest 的断言差异和适用范围。

---

## 维度 3: 工具架构与工程质量

### D3-01 | medium | 09-appendix-schemas.md | rejected
**问题**: 状态机缺少 testing->translating 回退路径处理残留 TODO(port)。  
**Verifier 理由**: 09 line 203-204 已定义 testing->paused->translating 间接回退路径（人类确认后重试），设计刻意在失败边界设置人类决策点，与全局安全哲学一致。

### D3-02 | medium | 09-appendix-schemas.md | confirmed
**问题**: 陈旧锁检测 branch(2)「PID 活 -> 真实并发」不区分同会话残留锁 vs 真正并发，导致同会话崩溃后重试被永久阻塞。  
**方案**: 细化 branch(2) 为 (2a) 同会话 PID -> 自动清除（串行语义保证前命令已结束）+ (2b) 不同会话 PID -> 报错退出+可选超时兜底。

### D3-03 | medium | 06-plugin-structure.md | rejected
**问题**: 单 core crate 捆绑不相关重依赖，违背参考架构 ast-grep/oxc 的 workspace 模式。  
**Verifier 理由**: 「参考」不等于「复刻」；编译时间已由 Spike 0 门控；C 构建产物有 cargo 缓存不随 Rust 源码变动重编；适配器贡献者仅写 shell 脚本不编译 core。

### D3-04 | low | 06-plugin-structure.md | confirmed
**问题**: Step 4 Phase B 存在两个独立重试计数器（max_retries_per_step=2 vs max_retry_rounds=3），SubAgent 超时时应减哪个未定义。  
**方案**: 在 SS10.2.2 追加一句：超时/校验失败计入 max_retries_per_step，编译失败计入 max_retry_rounds，两者独立，任一耗尽即 pause->degrade。

---

## 维度 4: 技术选型审查

### D4-01 | low | 04-toolchain.md | confirmed
**问题**: SS5.8 A 嵌入分类表遗漏 rusqlite，与同文件 SS5.7.3 及 06 SS10.0.1 不一致。  
**方案**: 在 SS5.8 A 表末追加 rusqlite 行。

### D4-02 | low | 04-toolchain.md | confirmed
**问题**: SS5.8 B 表混入 just/bacon（开发辅助工具），与 ToolRunner 定义矛盾。  
**方案**: 移除 just/bacon 出 B 表，在 SS5.3 追加说明其为推荐本地开发工具、不经 ToolRunner 调用。

---

## 维度 5: 编排可靠性与确定性

### D5-01 | medium | 09-appendix-schemas.md | adjusted
**问题**: 同会话陈旧锁致误判并发（与 D3-02 为同一底层问题）。  
**方案**: 与 D3-02 合并修复。严重度从原始定位确认为 medium（有 workaround 但属设计不一致）。

### D5-02 | medium | 09-appendix-schemas.md | confirmed
**问题**: 断点续传路由依赖两个 substatus 值（phase_a_complete_awaiting_review / phase_b_optimization_in_progress），但 SKILL.md 骨架无写入指令——路由分支为死代码。  
**方案**: 在 Step 2 检查点后和 Step 4 开头各插入一条 `rustmigrate state transition --substatus ...` 指令。

### D5-03 | low | 09-appendix-schemas.md | confirmed
**问题**: Line 512 注释称 reviewing 状态路由到 Step 5，实际路由表映射到 Step 6——交叉引用错误。  
**方案**: 将「Step 5」改为「Step 6」。

---

## 维度 6: 规模化与性能

### D6-01 | medium | 09-appendix-schemas.md | confirmed
**问题**: topo-sort Step 2.8 仅处理循环依赖错误，忽略 graph_truncated 失败模式（04 SS5.7.6 定义的两种非零退出之一）。  
**方案**: 替换单一非零处理为 JSON 判别分支：error=graph_truncated 提示 graph build --full；cycle_path 走已有降级。

### D6-02 | low | 03-execution-model.md | confirmed
**问题**: SS4.10(4) 升级指标含「磁盘吞吐 < 50 MB/s」，与 SS4.10(3) 声明的瓶颈层级（LLM 等待 >> 编译 >> Skill 调用）矛盾——迁移工具永远不会达到磁盘瓶颈。  
**方案**: 替换为「LLM 等待时间占 Sprint 总耗时 > 60%」，与声明的主瓶颈一致。

---

## 维度 7: 可维护性/可扩展性/社区贡献

### D7-01 | medium | 06-plugin-structure.md | confirmed
**问题**: SS10.6 权威产出物目录树缺 _porting_manifest.json 及 module-learnings 内部子目录结构。  
**方案**: 展开 module-learnings/ 显示 per-module 子目录（含 learnings.md + _porting_manifest.json），与 SS10.2 SubAgent IO 表一致。

### D7-02 | medium | 05-documentation-system.md | rejected
**问题**: L1 module-learnings 缺 YAML frontmatter 规范，与 patterns 格式体系不一致。  
**Verifier 理由**: Line 575 已明确 `.rust-migration/context/` 下所有文件统一采用「YAML frontmatter + Markdown」格式；SS6.12 scope 声明仅覆盖 L0-L3 长期产出物 patterns/anti-patterns，L1 的不同生命周期处理是刻意设计。

---

## 维度 8: 范围控制/过度设计/路线图

### D8-01 | medium | 08-roadmap-and-reference.md | confirmed
**问题**: M0 per-spike 估算 6x(1-2)=6-12 人天 vs 聚合声明 5-10 人天，算术不一致。  
**方案**: 改 per-spike 为「每个 0.5-2 天，均值约 1 天」，新范围 3-12 涵盖聚合 5-10。

### D8-02 | low | 08-roadmap-and-reference.md | rejected
**问题**: Section 13.1.2 含实现级 JSON 字段名和框架选型，超出设计范围。  
**Verifier 理由**: 该节实为 11 行（非 30 行），criterion 引用已出现在 6+ 设计文件中为既定选型，JSON 字段为接口/schema 契约而非实现代码。五个编号点回答了合理的设计问题。

---

## 实操盲点 (BS1)

### BS1-01 | medium | 03-execution-model.md | rejected
**问题**: Translator context 缺已翻译依赖模块的 Rust 接口，首次 cargo check 必定因类型不匹配失败。  
**Verifier 理由**: 设计通过 F1 反馈机制显式处理此场景——03 line 239 Problem Matrix 标记「类型不匹配 | 能在 F1 发现」；F1 为秒级诊断，在 SubAgent 调用内解决，不消耗重试预算；translator 可读 rust_root/ 文件。

### BS1-02 | low | 06-plugin-structure.md | adjusted
**问题**: SubAgent IO 表未明确 translator 负责 Cargo.toml [dependencies] 更新。  
**方案**: 在 translator 输出列追加「Cargo.toml [dependencies] 更新（按 dependency-mapping.md 映射）」。严重度从 medium 下调为 low（F1 已覆盖遗漏场景）。

### BS1-03 | medium | 06-plugin-structure.md | rejected
**问题**: 适配器验证标准遗漏 Node.js Runtime API 映射。  
**Verifier 理由**: Node.js 内置模块即 TS 标准库，RULE-10 自然覆盖；验证标准的自评覆盖条款 + CI heading count 已保证实际收录；pre-check 与 porting-template 为两个独立机制分别处理 npm 包和语言标准库。

---

## OSS 工程基线 (BS2)

### BS2-01 | medium | 08-roadmap-and-reference.md | confirmed
**问题**: Spike 0 建立的二进制体积/编译时间基线在 M0 后无 CI 持续执行——M1 开发期无自动门禁防回归。  
**方案**: 在 06 SS10.0.1 release.yml 中加一句：post-build 记录各平台二进制体积并与 Spike 0 基线比对，超 2x 则 fail release。

### BS2-02 | low | 06-plugin-structure.md | confirmed
**问题**: CI 和发版检查清单缺 cargo doc --no-deps -D warnings（crates.io 发布的文档质量门禁）。  
**方案**: 在 SS10.0.2 发版检查清单新增一条。

---

## 内部矛盾/跨文件一致性 (BS3)

### BS3-01 | medium | 03-execution-model.md / 06-plugin-structure.md | confirmed
**问题**: .rustmigrate.toml schema 权威（06 SS11.1）缺 async_strategy 字段，而 03 SS4.7 明确要求 PLAN 阶段将该字段写入 TOML。  
**方案**: 在 06 SS11.1 [strategy] 段补入 `async_strategy = "boundary_async"` 行（含三选一值域注释）。

### BS3-02 | medium | 06-plugin-structure.md | confirmed
**问题**: verify.sh 引用不存在的 .rustmigrate.toml migration_root 字段。  
**方案**: 删除 migration_root 引用，改为确定性约定 `$(git rev-parse --show-toplevel)/.rust-migration`，保留 $MIGRATION_ROOT 环境变量作为 override。

### BS3-03 | medium | 04-toolchain.md | confirmed
**问题**: SS5.7.4 图构建 Stage 1 标注 `rustmigrate profile` 为执行工具，但 09 SKILL.md 直接调 graph build 且 06 定义两者为独立命令——三处互相矛盾。  
**方案**: 在阶段表下追加 blockquote 澄清 Stage 1 文件扫描由 graph build 内部完成，profile 是独立诊断命令，无数据依赖。

---

## 本轮修复文件清单

| 文件 | 修复 ID |
|------|---------|
| docs/design/03-execution-model.md | D1-01, D2-02, D2-03, D6-02, BS3-01(部分) |
| docs/design/09-appendix-schemas.md | D3-02, D5-01, D5-02, D5-03, D6-01 |
| docs/design/06-plugin-structure.md | D3-04, D7-01, BS1-02, BS2-01, BS2-02, BS3-01(部分), BS3-02 |
| docs/design/04-toolchain.md | D4-01, D4-02, BS3-03 |
| docs/design/08-roadmap-and-reference.md | D8-01 |

---

## 转 M0 Spike 清单

本轮无明确的实证类诉求被排除至 M0 Spike。所有 rejected findings 均因事实性错误或设计意图误读被驳回，而非因需要实证数据。
