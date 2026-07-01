//! 基础类型定义：项目级通用标识符、范围、语言枚举。

use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

use super::graph::NodeType;

/// 图节点的唯一标识符。
///
/// 格式：`{type}:{file_path}` 或 `{type}:{file_path}:{name}`。
/// 例如 `file:src/utils.ts`、`function:src/utils.ts:clamp`。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(String);

impl NodeId {
    /// 创建新的节点标识符（原始字符串）。
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// 构造文件节点 ID：`file:{rel_path}`。
    pub fn file(rel_path: &str) -> Self {
        Self(format!("{}:{rel_path}", NodeType::File))
    }

    /// 构造符号节点 ID：`{type}:{rel_path}:{name}`。
    pub fn symbol(node_type: NodeType, rel_path: &str, name: &str) -> Self {
        Self(format!("{node_type}:{rel_path}:{name}"))
    }

    /// 返回内部字符串引用。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 解析节点类型前缀。
    pub fn kind(&self) -> Option<NodeType> {
        let prefix = self.0.split(':').next()?;
        prefix.parse().ok()
    }

    /// 解析文件路径部分。
    pub fn file_path(&self) -> Option<&str> {
        let mut parts = self.0.splitn(3, ':');
        parts.next()?; // type prefix
        parts.next() // file_path (or file_path:name combined for 2-part IDs)
    }

    /// 解析符号名称（仅 3 段 ID 有值）。
    pub fn symbol_name(&self) -> Option<&str> {
        let mut parts = self.0.splitn(3, ':');
        parts.next()?; // type prefix
        parts.next()?; // file_path
        parts.next() // name
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for NodeId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for NodeId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// 图边的唯一标识符（源节点 + 目标节点 + 边类型的组合）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EdgeId(String);

impl EdgeId {
    /// 创建新的边标识符。
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// 返回内部字符串引用。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for EdgeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for EdgeId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for EdgeId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// 源码行范围（1-based，闭区间）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Span {
    /// 起始行号。
    pub start_line: u32,
    /// 结束行号。
    pub end_line: u32,
}

/// 源语言枚举。M3+ 扩展 Python/C/Go。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display, EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum SourceLang {
    /// TypeScript 源语言。
    #[serde(rename = "typescript")]
    #[strum(serialize = "typescript")]
    TypeScript,
    /// Python 源语言。
    Python,
    /// C 源语言。
    C,
    /// Go 源语言。
    Go,
}

/// 复杂度等级（由 profile 模块评估）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display, EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum Complexity {
    /// 简单模块。
    Simple,
    /// 中等复杂度模块。
    Moderate,
    /// 高复杂度模块。
    Complex,
}

/// 危险信号类别（M3-DEC-01 决策②(b)，M4-DEBT-02 上移 types 层）。
///
/// 命中任一 → 该文件即"非机械"（走重流程）+ 应注入对应 porting 规则 + 加定向测试。
/// 与"流程深浅"是两件事：重流程本身不抓陷阱，规则注入才是针对性的一刀。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DangerCategory {
    /// 数值精度（Math./浮点/Number/parseInt/NaN/Infinity）。
    NumericPrecision,
    /// 并发（async/await/Promise.all/定时器）。
    Concurrency,
    /// 动态/反射（eval/typeof/instanceof/as any/getattr/metaclass）。
    DynamicReflection,
    /// IO 副作用（fs/net/process 等 import 或顶层表达式语句）。
    IoSideEffect,
    /// FFI / 跨语言边界（如 napi、ctypes、cgo——主要供非 TS 语言）。
    Ffi,
    /// 跨文件共享可变全局（顶层 `let`/`var` 可变绑定）。
    SharedMutableGlobal,
    /// 未知类别（兼容兜底：无法识别的字符串反序列化为此变体，**不硬失败**）。
    ///
    /// **有损单向性**（M4-DEBT-02 知情取舍，PLAN-M4 DEBT-02 已授权 `#[serde(other)]` 方案）：
    /// 反序列化 `未知字符串 → Unknown`，但再序列化 `Unknown → "unknown"`，原始字符串不可恢复。
    /// **当前不可触发**：`danger` 值只由 `classify_file()` 分类器产出（恒为上方 6 类已知值），
    /// 封闭世界里 state 文件永不含未知 danger；唯一进入途径是手工编辑 JSON 或跨 CLI 版本读入
    /// （后者由 `migration-state.json` 的 `schema_version` 机制负责，见 06-plugin-structure §unknown
    /// 处理）。故 load→save 透传（含 Sprint F ROB-01a checkpoint）在正常流程中无损。
    /// 若未来真需保真未知值，改 `Unknown(String)` + 手写 serde（当前 ROI 不足，故取简单兜底）。
    #[serde(other)]
    Unknown,
}

impl DangerCategory {
    /// 稳定的 snake_case 标识符（与 `#[serde(rename_all = "snake_case")]` 序列化一致）。
    pub fn as_str(&self) -> &'static str {
        match self {
            DangerCategory::NumericPrecision => "numeric_precision",
            DangerCategory::Concurrency => "concurrency",
            DangerCategory::DynamicReflection => "dynamic_reflection",
            DangerCategory::IoSideEffect => "io_side_effect",
            DangerCategory::Ffi => "ffi",
            DangerCategory::SharedMutableGlobal => "shared_mutable_global",
            DangerCategory::Unknown => "unknown",
        }
    }

    /// 该陷阱的人读说明，供翻译上下文注入 / dry-run 报告展示。
    ///
    /// 仅在 concern 文案中点名已核实的 RULE-NN（RULE-6/10/12/15/20）——规则注入仍由
    /// translator 依完整规则目录决定，避免在核心层固化可能漂移的规则映射。
    pub fn concern(&self) -> &'static str {
        match self {
            DangerCategory::NumericPrecision => {
                "数值精度：源语言数值类型与 Rust 整型/浮点的取值范围、取整、溢出语义不同（如 JS number=f64、Go int=平台位宽、Python int 任意精度），需对边界值定向测试"
            }
            DangerCategory::Concurrency => {
                "并发：源语言并发原语需映射到 Rust（RULE-6 异步/并发）——async/Promise/goroutine/channel 等的执行顺序、取消、共享内存语义需显式建模"
            }
            DangerCategory::DynamicReflection => {
                "动态/反射：运行时类型操作（如 typeof/instanceof/any、getattr/metaclass、reflect）无法静态翻译，需显式建模（RULE-20）"
            }
            DangerCategory::IoSideEffect => {
                "IO 副作用（RULE-10 IO 子节）：文件/网络/进程调用，需隔离副作用并对错误路径定向测试"
            }
            DangerCategory::Ffi => "FFI/unsafe（RULE-12）：跨语言边界（napi/ctypes/cgo/unsafe.Pointer），按 degrade_skip 评估或选 Rust 替代 crate",
            DangerCategory::SharedMutableGlobal => {
                "跨文件共享可变全局（RULE-15）：模块级可变绑定（顶层 let/var、package 级 var）→ Rust 需 OnceLock/Mutex 等显式同步"
            }
            DangerCategory::Unknown => "未知危险类别（旧版数据兼容兜底）",
        }
    }
}

/// 迁移优先级（1 = 最高优先，无依赖的叶节点先迁移）。
pub type MigrationPriority = u32;

/// 时间戳（ISO 8601 / RFC 3339 字符串）。
///
/// 反序列化时即校验格式（见下方手写 [`Deserialize`] 实现），任何从 JSON/外部
/// 加载的 Timestamp 字段都会被自动校验——由类型层保证全覆盖，无需调用方手写遍历。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct Timestamp(String);

impl Timestamp {
    /// 创建新的时间戳。
    pub fn new(ts: impl Into<String>) -> Self {
        Self(ts.into())
    }

    /// 取当前 UTC 时间的 RFC 3339 时间戳。
    pub fn now() -> Self {
        Self(chrono::Utc::now().to_rfc3339())
    }

    /// 返回内部字符串引用。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 校验是否为合法 ISO 8601 / RFC 3339 时间戳。
    pub fn is_valid(&self) -> bool {
        chrono::DateTime::parse_from_rfc3339(&self.0).is_ok()
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    /// 反序列化时即校验格式：非法时间戳在此被拒（serde 错误，含非法值）。
    /// 这样所有含 Timestamp 的结构从外部加载时自动获得校验，无需各处手写遍历。
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let ts = Self(String::deserialize(deserializer)?);
        if !ts.is_valid() {
            return Err(serde::de::Error::custom(format!(
                "时间戳格式非法（期望 ISO 8601 / RFC 3339）: {ts}"
            )));
        }
        Ok(ts)
    }
}

impl std::fmt::Display for Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for Timestamp {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for Timestamp {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// Schema 版本号。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SchemaVersion(String);

impl SchemaVersion {
    /// 创建新的 Schema 版本号。
    pub fn new(v: impl Into<String>) -> Self {
        Self(v.into())
    }

    /// 返回内部字符串引用。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for SchemaVersion {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for SchemaVersion {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// 文件遍历时排除的目录名——**跨语言统计专用全集**。
///
/// 仅用于"目的就是跨语言扫描"的两处：`profile::detect`（在确定语言前探测主语言，
/// 鸡生蛋）和 `stats::loc`（统计所有语言行数）。**图构建与结构对比按已知语言精确
/// 排除**（`graph::build` / `stats::compare` 用 [`crate::types::config::lang_vendor_dirs`]），
/// 不用本全集——否则别语言的 vendor 名会误伤本语言项目的同名业务目录。
///
/// 全集是**有意的近似**：它也会忽略命中别语言 vendor 名的业务目录（如 TS 项目的
/// `build/`），但这两处可容忍——detect 是探测语言前的鸡生蛋两害相权（不排则被
/// `node_modules` 淹没主语言判定）；loc 比值两侧对称排除以保可比性，优先于单侧绝对
/// 精度。要正确性的"哪些文件进依赖图"另走精确排除（见上）。
///
/// 内容是 [`crate::types::config::default_excludes_for_lang`] 各语言的并集，一致性由
/// config 模块的 `excluded_dirs_is_union_of_all_langs` 测试保证（防止漂移）。
pub const EXCLUDED_DIRS: &[&str] = &[
    // 通用（COMMON_EXCLUDES）
    ".git",
    "target",
    // TypeScript
    "node_modules",
    "dist",
    // Python
    "__pycache__",
    ".venv",
    "venv",
    ".mypy_cache",
    // C
    "build",
    ".obj",
    // Go
    "vendor",
];

/// 归一化相对路径（消除 `.` 和 `..`）。路径逃逸项目根时返回 None。
pub fn normalize_path(path: &std::path::Path) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                parts.pop()?;
            }
            std::path::Component::Normal(s) => {
                parts.push(s.to_str().unwrap_or(""));
            }
            _ => {}
        }
    }
    Some(parts.join("/"))
}

#[cfg(test)]
mod danger_category_serde_tests {
    use super::DangerCategory;

    /// `as_str()` 与 `#[serde(rename_all = "snake_case")]` 序列化一致（防变体新增/重命名漂移）。
    #[test]
    fn as_str_matches_serde_serialize() {
        for cat in [
            DangerCategory::NumericPrecision,
            DangerCategory::Concurrency,
            DangerCategory::DynamicReflection,
            DangerCategory::IoSideEffect,
            DangerCategory::Ffi,
            DangerCategory::SharedMutableGlobal,
            DangerCategory::Unknown,
        ] {
            assert_eq!(
                serde_json::to_value(cat).unwrap(),
                serde_json::json!(cat.as_str()),
                "as_str() 与 serde 序列化应一致: {cat:?}"
            );
        }
    }

    /// 双向 round-trip：序列化后再反序列化得回同一变体（M4-DEBT-02，旧 state 文件可正确加载）。
    #[test]
    fn serde_round_trip() {
        for cat in [
            DangerCategory::NumericPrecision,
            DangerCategory::Concurrency,
            DangerCategory::DynamicReflection,
            DangerCategory::IoSideEffect,
            DangerCategory::Ffi,
            DangerCategory::SharedMutableGlobal,
        ] {
            let json = serde_json::to_value(cat).unwrap();
            let back: DangerCategory = serde_json::from_value(json).unwrap();
            assert_eq!(cat, back, "round-trip 应保持变体: {cat:?}");
        }
    }

    /// 未知字符串经 `#[serde(other)]` 兜底为 `Unknown`，不硬失败（旧/新版兼容韧性）。
    #[test]
    fn unknown_string_falls_back_to_unknown() {
        let parsed: DangerCategory =
            serde_json::from_value(serde_json::json!("some_future_category"))
                .expect("未知类别应兜底为 Unknown，不应反序列化失败");
        assert_eq!(parsed, DangerCategory::Unknown);
    }

    /// 旧版 state 中的 `Vec<String>` danger 数组可整体反序列化为 `Vec<DangerCategory>`。
    #[test]
    fn legacy_string_array_deserializes() {
        let legacy = serde_json::json!(["numeric_precision", "concurrency", "io_side_effect"]);
        let parsed: Vec<DangerCategory> = serde_json::from_value(legacy).unwrap();
        assert_eq!(
            parsed,
            vec![
                DangerCategory::NumericPrecision,
                DangerCategory::Concurrency,
                DangerCategory::IoSideEffect,
            ]
        );
    }
}
