# 本轮审查报告

**轮次**: Round 1  
**发现总数**: 16 条（confirmed 14 / adjusted 2 / rejected 0）  
**修复文件数**: 6 个设计文档

---

## 维度 1: 迁移质量与翻译方法论

**D1-01** | medium | 03-execution-model.md | confirmed  
bug_replica MDR 的 human_decision 字段无强制门禁，模块可在未决策情况下进入 done 状态。  
修复：在 09 附录 A done 前置条件增加 bug_replica MDR human_decision 非空约束；09 Step 5 增加 verifier 扫描检查点。

**D1-02** | medium | 03-execution-model.md | confirmed  
Proptest 回归基准初始化在 09 SKILL.md 骨架中无对应步骤，Phase B 对标机制不可实施。  
修复：在 09 Step 3 后新增 Step 3.3 proptest 回归基准初始化子步骤，含检查点验证文件存在。

**D1-03** | low | 03-execution-model.md | confirmed  
Phase B 针对性测试（loom/criterion）缺乏类似 dimension-coverage.yaml 的完整性自检机制。  
修复：在 03 Step 6 追加 {module}-phase-b-coverage.yaml 自检要求，MDR 数 > 0 但 yaml 条目不足时阻塞 done。

---

## 维度 2: 验证体系可靠性

**D2-01** | medium | 09-appendix-schemas.md | confirmed  
Step 5 列裸 cargo 命令而非 verify.sh，丢失 CLIPPY_CONF_DIR 和条件 loom/shuttle 逻辑。  
修复：Step 5 验证命令改为引用 hooks/scripts/verify.sh，裸命令保留为注释说明脚本内部逻辑。

**D2-02** | medium | 03-execution-model.md | confirmed  
coverage_threshold=80（06）与三级覆盖率规则 40% 绝对下限（03）关系未定义，判定逻辑存在冲突。  
修复：在 03 §7.5 段末明确两者取严格并集语义；同步更新 06 §11.1 coverage_threshold 注释。

---

## 维度 3: 工具架构与工程质量

**D3-01** | medium | 09-appendix-schemas.md | confirmed  
Step 0.3 将 reviewing 路由到 Step 5，触发非法 reviewing->testing 转换；失败恢复表复位值也有误。  
修复：路由表拆分 testing->Step 5 / reviewing->Step 6；Step 5 首行增加同状态跳过说明；恢复表复位值改为 testing。

**D3-02** | medium | 09-appendix-schemas.md | confirmed  
degrade_* + --force 恢复路径在 02/09 附录 A 有定义但 SKILL.md 骨架无执行步骤。  
修复：Step 0.3 新增 degrade_* + --force 路径，执行 state transition --to translating、清除降级字段、重置计数后跳至 Step 0.5。

---

## 维度 4: 技术选型审查

**D4-01** | medium | 04-toolchain.md | confirmed  
Leiden 算法无 Rust crate 识别且 TRIAL 表无风险条目，与其他核心算法选型深度不对等。  
修复：在 §5.5 TRIAL 表新增 Leiden 行，含三条回退选项（louvain-rs / 自实现 / 目录分组退化）。

---

## 维度 5: 编排可靠性与确定性

**D5-01** | medium | 09-appendix-schemas.md | confirmed  
Step 3.5 redo 路径保留旧审查报告作为 Step 4 输入，Phase B 获得过时修正指导。  
修复：redo 路径改为 重做 Phase A -> 删除旧 review.md -> 重新执行 Step 3 -> 通过结构重检后进入 Step 4。

---

## 维度 6: 规模化与性能

**D6-01** | medium | 04-toolchain.md | confirmed  
§5.7.3 SQLite DDL 缺 metadata 表定义，§5.7.5/§5.7.6 对 graph_integrity 字段的读写为悬空引用。  
修复：在 DDL 的 CREATE INDEX 后补齐 metadata 表定义及 MVP 预置行 INSERT。

**D6-02** | medium | 06-plugin-structure.md | confirmed  
graph build CLI 缺 --full 标志定义，增量/全量模式无说明，熔断恢复路径接口层面不可达。  
修复：在 §10.0.1 graph build 命令说明中补充增量默认行为及 --full 强制全量重建说明。

---

## 维度 7: 可维护性/可扩展性/社区贡献

**D7-01** | medium | 05-documentation-system.md | confirmed  
_porting_manifest.json 有消费者（verifier + review）但无定义的生产者，SubAgent 接口表遗漏。  
修复：在 06 §10.2 接口表 translator 行追加 manifest 为输出；09 Step 2 检查点补充生成责任。

**D7-02** | medium | 06-plugin-structure.md | confirmed  
适配器 porting-template 验收无规则类覆盖检查，贡献可仅含类型映射即通过。  
修复：§11.2 验收表新增覆盖行（基线 RULE-2/3/8 + CI grep）；§11.2.1 契约补充惯用法差异规则类要求。

**D7-03** | low | 06-plugin-structure.md | confirmed  
多语言项目下适配器枚举与路由机制未定义，source_language 未设置时无执行路径。  
修复：§11.2 调用链路段首追加适配器枚举前置步骤说明，明确 MVP 必填 / M2 自动枚举。

---

## 维度 8: 范围控制/过度设计/路线图

**D8-01** | medium | 08-roadmap-and-reference.md | confirmed  
M1 验收 '30 分钟' 与性能门禁 '30-40 分钟' 自相矛盾，且后者虚假宣称与前者一致。  
修复：验收指标改为 '30-40 分钟内'，删除性能门禁中冗余括注。

---

## 维度 9: 实操盲点

**BS1-01** | low | 03-execution-model.md | adjusted  
源项目构建环境未在 Sprint 间保持，FFI 验证可能因环境退化失败。  
修复：在 §4.2 Sprint Planning 增加源项目 smoke-build 检查 bullet。  
调整理由：原 finding 过度放大风险——FFI 桥接失败非静默（子进程返回非零）、lockfile 已保证依赖可复现、npm 反删除政策和 Node LTS 周期覆盖典型迁移时长。仅 Sprint 启动冒烟检查为有效增量。

**BS1-02** | medium | 03-execution-model.md | confirmed  
L2 proptest 对所有纯函数强制要求，但 FFI 子进程桥接仅支持 JSON-safe 类型子集，Map/Set 纯函数无路径。  
修复：06 §11.2 新增反转测试运行器条件步骤（structured-clone 兼容类型走 napi-rs 进程内对比）；03 §7.6.1 补充说明。

---

## 维度 10: OSS 工程基线

**BS2-01** | medium | 03-execution-model.md | confirmed  
verify-reproducibility.sh 路径在 03 定义但 06 §10.6 权威目录树中不存在，跨文件不一致。  
修复：06 §10.6 目录树增加 ci/ 子目录条目。

**BS2-02** | low | 08-roadmap-and-reference.md | adjusted  
可复现性检查无 workflow 归属，项目 CI 架构散落多文件缺统一清单。  
修复：03 §4.11.3 末尾追加一句交叉引用明确 ci.yml PR workflow 归属。  
调整理由：verify-reproducibility.sh 并非无执行载体（08 有专门工作项含 Actions 集成指引），仅差 .yml 文件名归属——属实现细节非设计缺陷。散落描述符合各自上下文归属逻辑。

---

## 维度 11: 内部矛盾/跨文件一致性

**BS3-01** | medium | 08-roadmap-and-reference.md | confirmed  
M2 CLI 命令数：06 权威定义 5 个 vs 08 声称 16 个（含虚构命令名 search/analyze/report）。  
修复：08 line 165 替换为与 06 §10.0.1 一致的 5 命令清单，保留 M2 候选命令待评估前向指针。

---

## 本轮修复文件清单

| 文件 | 修复条目 |
|------|----------|
| 03-execution-model.md | D1-01, D1-02, D1-03, D2-02, BS1-01, BS1-02, BS2-01 |
| 09-appendix-schemas.md | D2-01, D3-01, D3-02, D5-01, D7-01(部分) |
| 04-toolchain.md | D4-01, D6-01 |
| 06-plugin-structure.md | D6-02, D7-01(部分), D7-02, D7-03, BS2-01 |
| 08-roadmap-and-reference.md | D8-01, BS3-01 |
| 05-documentation-system.md | 无需改动（消费侧定义完整，生产者归属于 06/09） |

---

## 转 M0 spike 清单

本轮无实证类诉求被排除至 M0 spike。所有 16 条发现均为设计文档层面的定义缺失、跨文件不一致或流程遗漏，已在文档内就地修复。
