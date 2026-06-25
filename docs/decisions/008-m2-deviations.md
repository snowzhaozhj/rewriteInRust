# MDR-008: M2 实现偏差记录（4 项 DEVIATION）

- **状态**: 已决策
- **日期**: 2026-06-25
- **范围**: M3-DEV-01（M2 遗留补录）

## DEVIATION 1: fingerprint 提取范围

**设计**：fingerprint structure_hash 应覆盖完整 AST 结构。
**实现**：structure_hash 基于 FileAnalysis 的节点/边/导入/调用摘要，不含 AST 原始结构。
**理由**：FileAnalysis 已涵盖图层关心的所有结构信息；原始 AST hash 变更噪声高（空行/注释变更触发 STRUCTURAL），实际运行中 content_hash 筛已足够区分。
**影响**：仅影响增量构建的变更检测精度，不影响正确性（宁多重建不漏）。

## DEVIATION 2: 事务类型 DEFERRED

**设计**：SQLite 持久化使用 IMMEDIATE 事务。
**实现**：使用默认 DEFERRED 事务。
**理由**：rustmigrate 为单进程单写者，不存在写写竞争。DEFERRED 启动更快，IMMEDIATE 的锁提前获取在此场景无收益。
**影响**：无功能影响。并发场景（M4 如有需要）再切换。

## DEVIATION 3: WAL pragma 未设置

**设计**：SQLite 应启用 WAL 模式（`PRAGMA journal_mode=WAL`）。
**实现**：未显式设置，使用默认 DELETE 模式。
**理由**：DB 文件为项目本地（`.rustmigrate/`），单进程访问，WAL 的并发读优势无用。DELETE 模式更简单、不产生额外 `-wal`/`-shm` 文件。
**影响**：无功能影响。大项目增量构建性能若成瓶颈，可在 M4 启用 WAL。

## DEVIATION 4: exported_names 额外维度

**设计**：`exported_names` 仅含符号名称集合。
**实现**：`exported_names` 包含通配标记（`*<-module_path` 格式）用于 re-export 透传。
**理由**：通配 re-export（`export * from 'm'`）需要在 re-export map 构建时区分本地导出和通配转发。通配标记是最低成本的区分方式，避免引入额外数据结构。
**影响**：`exported_names` 不再是纯符号名集合，但下游消费者（re-export map、降级报告）已适配。
