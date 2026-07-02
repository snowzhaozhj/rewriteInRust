//! Go 语言适配器。
//!
//! M4 Sprint A 接线：language/can_handle/resolve_extensions/detect_source_root。
//! M4 Sprint C PR-C1（GO-01）：detect_tier 复杂度分档（并发/反射/cgo/unsafe 危险信号）。
//! M4 Sprint C PR-C2：
//! - GO-02：import 解析（单/分组、别名/点/下划线）+ 文件过滤（`_test.go`/平台后缀在
//!   `can_handle`，`//go:build` 在 `analyze_file` 内容级）。
//! - GO-03：`resolve_import` Go 包 resolve（代表文件 baseline）+ `configure_project` 读 go.mod
//!   注入 module 前缀 + 扩 trait `list_dir` 目录列举。
//! - GO-04：符号提取（func/method/type struct→Class/interface→Interface/const/var→Variable）
//!   + Contains/Extends（struct 嵌入）边 + 首字母大写导出 + 后置 Exports 边。
//! - GO-05：调用分析（`pkg.Func`/`x.Method`/composite literal 构造）+ instance_type_bindings。
//! - GO-06：signature 提取（func/method 剥 body、interface 方法集、struct 字段骨架）。
//! - GO-07：interface 隐式实现——**不强连 Implements 边**（D-M4-02），方法集入 signature。
//!
//! **跨包 Calls 精度已知限制**：`resolve_import` 是 `&self` 只有 `exists`/`list_dir` 回调、
//! 拿不到其他文件符号表，跨包只能返回「包代表文件」（目录内字典序第一个非 `_test.go`）。
//! 若被调导出符号不在代表文件，build.rs 精确文件匹配会 miss → 该跨包 Calls 边**漏建**（非错
//! 建：精确匹配 miss 即 drop）。这不影响 decompose 拆解（靠 Imports 边 + 目录凝聚，不依赖
//! Calls），符号级精确解析需符号表（超 M4-GO-03 范围）。端到端死断言 owner=GO-09（PR-C3）。
//!
//! **跨文件 Contains 已知限制**：Go 允许方法与类型定义在同包不同文件；`extract_go_method`
//! 发的 `Contains(Class(recv_type,rel)→method)` 目标 Class 若在他文件则 `add_edge` 静默丢弃
//! （Contains 无 fixup，不同于 Extends）。PR-C2 单测限同文件，跨文件 fixup 记 TODO（PR-C3）。

use std::collections::{HashMap, HashSet};
use std::path::Path;

use tree_sitter::{Node, Parser};

use crate::error::{MigrateError, Result};
use crate::types::common::{NodeId, SourceLang, Span};
use crate::types::graph::{Dependency, EdgeType, NodeType, SourceNode};
use crate::types::state::ModuleTier;

use super::{
    CallInfo, FileAnalysis, ImportInfo, ImportKind, ImportedSymbol, LanguageAdapter, SymbolKind,
};

/// Go 语言适配器（基于 tree-sitter-go）。
pub struct GoAdapter {
    parser: Parser,
    /// go.mod 的 module 路径前缀（如 `example.com/proj`）。`None` = 无 go.mod/未 configure，
    /// 此时所有导入按外部依赖处理（跨包边不解析）。由 `configure_project` 读 go.mod 注入。
    module_path: Option<String>,
}

impl GoAdapter {
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_go::language())
            .map_err(|e| MigrateError::Config(format!("tree-sitter Go 语法加载失败: {e}")))?;
        Ok(Self {
            parser,
            module_path: None,
        })
    }
}

impl LanguageAdapter for GoAdapter {
    fn language(&self) -> SourceLang {
        SourceLang::Go
    }

    fn can_handle(&self, path: &Path) -> bool {
        // 仅 .go 源文件；排除 `_test.go`（同包测试污染符号集）与非默认平台后缀文件
        // （`_windows.go`/`_arm64.go` 等 GOOS/GOARCH 后缀，只迁默认构建集 linux/amd64）。
        // `//go:build` 内容级门控在 analyze_file 处理（can_handle 只有 Path 拿不到内容）。
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        name.ends_with(".go") && !go_is_test_file(&name) && !go_platform_suffix_excluded(&name)
    }

    fn resolve_extensions(&self) -> &[&str] {
        &["go"]
    }

    fn configure_project(&mut self, project_root: &Path) {
        // 读 go.mod 的 module 前缀，供 resolve_import 剥离本地包路径（M4-GO-03）。
        self.module_path = parse_go_module_path(project_root);
    }

    fn analyze_file(&mut self, source: &str, rel_path: &str) -> Result<FileAnalysis> {
        let tree = self
            .parser
            .parse(source, None)
            .ok_or_else(|| MigrateError::Parse {
                path: rel_path.into(),
            })?;

        let mut ctx = GoAnalysisContext {
            rel_path,
            source,
            nodes: Vec::new(),
            edges: Vec::new(),
            imports: Vec::new(),
            calls: Vec::new(),
            exported_names: HashSet::new(),
            instance_type_bindings: HashMap::new(),
        };

        // File 节点。
        ctx.nodes.push(SourceNode::new(
            NodeId::file(rel_path),
            NodeType::File,
            rel_path.to_string(),
            rel_path.to_string(),
        ));

        // `//go:build` 内容级门控（can_handle 只有 Path 拿不到内容）：排除默认目标的约束
        // → 仅产出 File 节点（图中孤立、由目录凝聚吸收、无害），不抽符号/边。
        if !go_build_constraint_excludes(source) {
            walk_go_toplevel(tree.root_node(), &mut ctx);
        }

        // 后置批量生成 Exports 边（复用 python.rs 模式）：导出符号 = File→symbol。
        // Go 导出=首字母大写（无 export 语句），Variable/TypeAlias 也可导出。
        let file_id = NodeId::file(rel_path);
        for node in &ctx.nodes {
            if node.is_exported && node.node_type != NodeType::File {
                ctx.edges.push(Dependency::new(
                    file_id.clone(),
                    node.id.clone(),
                    EdgeType::Exports,
                ));
            }
        }

        Ok(FileAnalysis {
            nodes: ctx.nodes,
            edges: ctx.edges,
            imports: ctx.imports,
            calls: ctx.calls,
            exported_names: ctx.exported_names,
            instance_type_bindings: ctx.instance_type_bindings,
        })
    }

    fn resolve_import(
        &self,
        specifier: &str,
        _current_file: &str,
        _exists: &dyn Fn(&str) -> bool,
        list_dir: &dyn Fn(&str) -> Vec<String>,
    ) -> Option<String> {
        // Go import 恒为绝对 module 路径（无相对导入），故 current_file/exists 均不用。
        // 无 module 前缀（未 configure/无 go.mod）→ 所有导入按外部，返回 None（安全）。
        let module = self.module_path.as_deref()?;

        // 剥离 module 前缀 → 包目录（项目相对）。先 strip module 再 strip '/' 可挡住
        // "example.com/foo" vs "example.com/foobar" 的部分段误匹配。
        let pkg_dir = if specifier == module {
            String::new() // import module 根包
        } else {
            match specifier
                .strip_prefix(module)
                .and_then(|s| s.strip_prefix('/'))
            {
                Some(sub) => sub.to_string(),
                None => return None, // 标准库/第三方 → 外部依赖，无边
            }
        };

        pick_representative_go_file(list_dir(&pkg_dir))
    }

    fn detect_tier(&mut self, source: &str) -> ModuleTier {
        // 对齐 python.rs：parse 失败/含语法错误 → 保守 Full；否则按危险信号 + 内容分档。
        let tree = match self.parser.parse(source, None) {
            Some(t) => t,
            None => return ModuleTier::Full,
        };
        let root = tree.root_node();
        if root.has_error() {
            return ModuleTier::Full;
        }
        let signals = scan_go_tier_signals(root, source);
        if signals.has_danger {
            ModuleTier::Full
        } else if signals.has_non_trivial_content {
            ModuleTier::Standard
        } else {
            ModuleTier::Trivial
        }
    }

    // classify_file 不 override——用 trait 默认 conservative()（Normal + 无危险），
    // 保证 Go 文件绝不会被误判为机械合批，也不 panic。Go 机械分类（barrel/纯常量等）
    // 当前不做（MDR-011 拆解已不用机械门分流），保守默认足够安全。

    fn detect_source_root(&self, project_root: &Path) -> Option<String> {
        // Go 项目：含 go.mod 的目录为 module 根，源码即在根目录。返回 Some(".")（探测成功）
        // 而非 None——None 语义是「未探测到，调用方回退 . 并告警」，对正确识别的 Go 项目
        // 吐 fallback warning 会误导（专项审查 nit）。
        if project_root.join("go.mod").exists() {
            return Some(".".to_string());
        }
        // 无 go.mod：回退默认 src/ 检查。
        let src_dir = project_root.join("src");
        if src_dir.is_dir() && super::dir_has_source_files(&src_dir, self.resolve_extensions(), 5) {
            return Some("src".to_string());
        }
        None
    }
}

/// Go 复杂度分档信号（仿 python.rs `PyTierSignals`）。
#[derive(Default)]
struct GoTierSignals {
    /// 含并发/反射/cgo/unsafe 等语义无法机械翻译的危险信号 → Full。
    has_danger: bool,
    /// 含函数/方法/类型定义等实质内容 → 至少 Standard（否则纯 const/var/import → Trivial）。
    has_non_trivial_content: bool,
}

/// 扫描顶层节点分档：import 单独查危险包，函数/方法/类型体递归查并发危险。
fn scan_go_tier_signals(root: Node, source: &str) -> GoTierSignals {
    let mut s = GoTierSignals::default();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        // Go 把 `\n` 等终结符作为 source_file 的匿名子节点吐出（Python grammar 无此行为），
        // 若不跳过，`_ =>` 兜底会把纯换行误判为实质内容。
        if !child.is_named() {
            continue;
        }
        match child.kind() {
            "package_clause" | "comment" => {}
            // import 危险包（reflect/unsafe/C）在顶层 import 声明，不在函数子树。
            "import_declaration" => check_import_danger(child, source, &mut s),
            "function_declaration" | "method_declaration" | "type_declaration" => {
                s.has_non_trivial_content = true;
                check_danger_in_subtree(child, &mut s);
            }
            // 纯 const/var 声明本身算 trivial，但初始化表达式可能含 make(chan)/go 等危险。
            "const_declaration" | "var_declaration" => check_danger_in_subtree(child, &mut s),
            _ => s.has_non_trivial_content = true,
        }
    }
    s
}

/// import 声明里若引入 reflect（反射）/unsafe（unsafe.Pointer）/C（cgo），标危险。
fn check_import_danger(node: Node, source: &str, signals: &mut GoTierSignals) {
    let mut cursor = node.walk();
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        if current.kind() == "import_spec" {
            if let Some(path) = current.child_by_field_name("path") {
                // interpreted_string_literal 文本含引号，如 "reflect"。
                let raw = &source[path.byte_range()];
                let pkg = raw.trim_matches(|c| c == '"' || c == '`');
                if matches!(pkg, "reflect" | "unsafe" | "C") {
                    signals.has_danger = true;
                    return;
                }
            }
        }
        cursor.reset(current);
        for child in current.children(&mut cursor) {
            stack.push(child);
        }
    }
}

/// 递归子树找并发危险：goroutine（go_statement）/select/channel/send/接收。
fn check_danger_in_subtree(node: Node, signals: &mut GoTierSignals) {
    let mut cursor = node.walk();
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        match current.kind() {
            "go_statement" | "select_statement" | "channel_type" | "send_statement" => {
                signals.has_danger = true;
                return;
            }
            // 接收操作 `<-ch`：tree-sitter-go 解析为带 `<-` 操作符的 unary_expression。
            // 仅接收 channel（如 `v := <-getCh()`）的代码不含上面任何节点，需单独识别，
            // 否则漏判并发。send_statement/channel_type 已在上分支先命中并返回，不会误入此处。
            "unary_expression" => {
                let mut op = current.walk();
                if current.children(&mut op).any(|c| c.kind() == "<-") {
                    signals.has_danger = true;
                    return;
                }
            }
            _ => {}
        }
        cursor.reset(current);
        for child in current.children(&mut cursor) {
            stack.push(child);
        }
    }
}

// ==================== analyze_file 支撑（GO-02/04/05/06/07）====================

/// 单文件分析累积器（仿 python.rs `PyAnalysisContext`）。
struct GoAnalysisContext<'a> {
    rel_path: &'a str,
    source: &'a str,
    nodes: Vec<SourceNode>,
    edges: Vec<Dependency>,
    imports: Vec<ImportInfo>,
    calls: Vec<CallInfo>,
    /// Go 无显式 export 表；此字段仅为对齐 `FileAnalysis` 形状（导出由首字母大写逐名判定）。
    exported_names: HashSet<String>,
    instance_type_bindings: HashMap<String, String>,
}

/// 取节点源码文本（零 panic，对齐 python.rs `py_node_text`）。
fn go_node_text<'a>(node: Node, source: &'a str) -> &'a str {
    source.get(node.byte_range()).unwrap_or("")
}

/// 节点行号跨度（tree-sitter 0-based → 1-based，对齐 python.rs `py_node_span`）。
fn go_node_span(node: Node) -> Span {
    Span {
        start_line: node.start_position().row as u32 + 1,
        end_line: node.end_position().row as u32 + 1,
    }
}

/// Go 导出规则：标识符首**字符**为 Unicode 大写字母即导出（`unicode.IsUpper`）。
/// 必须用 `is_uppercase()` 而非 `is_ascii_uppercase()`（否则 `Édgar` 误判非导出）；
/// `_`/空标识符非导出。
fn go_is_exported(name: &str) -> bool {
    name.chars().next().is_some_and(char::is_uppercase)
}

/// 顶层派发：遍历 source_file 的 **named** 子节点（Go grammar 把 `\n` 作匿名子节点吐出，
/// 不过滤会污染；detect_tier 已踩过此坑）。
fn walk_go_toplevel(root: Node, ctx: &mut GoAnalysisContext) {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if !child.is_named() {
            continue;
        }
        match child.kind() {
            "package_clause" | "comment" => {}
            "import_declaration" => extract_go_imports(child, ctx),
            "const_declaration" | "var_declaration" => extract_go_var_const(child, ctx),
            "type_declaration" => extract_go_type(child, ctx),
            "function_declaration" => extract_go_function(child, ctx),
            "method_declaration" => extract_go_method(child, ctx),
            _ => {}
        }
    }
}

// ==================== GO-03 module_path / 代表文件 ====================

/// 解析 go.mod 首个 `module <path>` 声明（剥行内注释与引号）。
fn parse_go_module_path(root: &Path) -> Option<String> {
    let content = std::fs::read_to_string(root.join("go.mod")).ok()?;
    for line in content.lines() {
        if let Some(rest) = line.trim().strip_prefix("module ") {
            let m = rest
                .split("//")
                .next()
                .unwrap_or("")
                .trim()
                .trim_matches('"');
            if !m.is_empty() {
                return Some(m.to_string());
            }
        }
    }
    None
}

/// 包目录内选「代表文件」：排除 `_test.go`，取字典序第一个 `.go`。字典序保证确定性
/// （所有 importer 汇聚到同一 gateway；跨包 Calls 精度限制见模块头）。
fn pick_representative_go_file(mut files: Vec<String>) -> Option<String> {
    files.retain(|p| p.ends_with(".go") && !p.ends_with("_test.go"));
    files.sort();
    files.into_iter().next()
}

// ==================== GO-02 文件过滤 ====================

/// 默认构建目标（确定性优先，不用宿主相关 `std::env::consts` 以保 CI/fixture 可复现）。
const GO_DEFAULT_GOOS: &str = "linux";
const GO_DEFAULT_GOARCH: &str = "amd64";

/// Go GOOS 令牌（`go/build/syslist.go`）。
const GOOS_TOKENS: &[&str] = &[
    "aix",
    "android",
    "darwin",
    "dragonfly",
    "freebsd",
    "hurd",
    "illumos",
    "ios",
    "js",
    "linux",
    "nacl",
    "netbsd",
    "openbsd",
    "plan9",
    "solaris",
    "wasip1",
    "windows",
    "zos",
];

/// Go GOARCH 令牌（`go/build/syslist.go`）。
const GOARCH_TOKENS: &[&str] = &[
    "386",
    "amd64",
    "amd64p32",
    "arm",
    "arm64",
    "arm64be",
    "armbe",
    "loong64",
    "mips",
    "mips64",
    "mips64le",
    "mips64p32",
    "mips64p32le",
    "mipsle",
    "ppc",
    "ppc64",
    "ppc64le",
    "riscv",
    "riscv64",
    "s390",
    "s390x",
    "sparc",
    "sparc64",
    "wasm",
];

/// `*_test.go` 测试文件。
fn go_is_test_file(name: &str) -> bool {
    name.ends_with("_test.go")
}

/// 平台后缀文件（`name_GOOS.go`/`name_GOARCH.go`/`name_GOOS_GOARCH.go`）且不匹配默认目标
/// → 排除。规则同 Go：GOOS/GOARCH 前须有非空前缀（`linux.go`/`amd64.go` 本身不受约束）。
fn go_platform_suffix_excluded(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".go") else {
        return false;
    };
    let parts: Vec<&str> = stem.split('_').collect();
    let is_goos = |t: &str| GOOS_TOKENS.contains(&t);
    let is_goarch = |t: &str| GOARCH_TOKENS.contains(&t);

    // name_GOOS_GOARCH.go：末两段分别是 GOOS/GOARCH，且前缀非空（parts.len() >= 3）。
    if parts.len() >= 3 {
        let (os, arch) = (parts[parts.len() - 2], parts[parts.len() - 1]);
        if is_goos(os) && is_goarch(arch) {
            return os != GO_DEFAULT_GOOS || arch != GO_DEFAULT_GOARCH;
        }
    }
    // name_GOOS.go 或 name_GOARCH.go：末段是 GOOS 或 GOARCH，且前缀非空（parts.len() >= 2）。
    if parts.len() >= 2 {
        let last = parts[parts.len() - 1];
        if is_goos(last) {
            return last != GO_DEFAULT_GOOS;
        }
        if is_goarch(last) {
            return last != GO_DEFAULT_GOARCH;
        }
    }
    false
}

// ==================== GO-02 import 解析 ====================

/// 提取 import 声明。单 import（`import "fmt"`）直挂 `import_spec`；分组（`import (...)`）
/// 经 `import_spec_list` 容器。
fn extract_go_imports(node: Node, ctx: &mut GoAnalysisContext) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "import_spec" => {
                if let Some(info) = go_import_spec_to_info(child, ctx.source) {
                    ctx.imports.push(info);
                }
            }
            "import_spec_list" => {
                let mut inner = child.walk();
                for spec in child.children(&mut inner) {
                    if spec.kind() == "import_spec" {
                        if let Some(info) = go_import_spec_to_info(spec, ctx.source) {
                            ctx.imports.push(info);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// 单个 `import_spec` → `ImportInfo`。Go 导入是**包粒度**（`pkg.Symbol` 访问），产出单个
/// `Namespace` 符号，本地名 = 别名 or 导入路径末段——这是 build.rs `import_map[pkg]` 命中、
/// 解析跨包 `pkg.Func()` 的接线点。
fn go_import_spec_to_info(spec: Node, source: &str) -> Option<ImportInfo> {
    let path_node = spec.child_by_field_name("path")?;
    let module_path = go_node_text(path_node, source)
        .trim_matches(|c| c == '"' || c == '`')
        .to_string();
    if module_path.is_empty() {
        return None;
    }

    // 可选 name field：package_identifier（别名）/ blank_identifier（`_`）/ dot（`.`）。
    let name_node = spec.child_by_field_name("name");
    let name_kind = name_node.map(|n| n.kind());

    match name_kind {
        // `_ "x"`：副作用导入，无符号。
        Some("blank_identifier") => Some(ImportInfo {
            module_path,
            symbols: Vec::new(),
            kind: ImportKind::SideEffect,
            reexport: false,
        }),
        // `. "x"`：符号扁平进当前作用域，静态无法枚举归属 → 空符号（build.rs 走 use-all）。
        // 已知边界：点导入的具名符号解析当前不支持。
        Some("dot") => Some(ImportInfo {
            module_path,
            symbols: Vec::new(),
            kind: ImportKind::StaticValue,
            reexport: false,
        }),
        // 别名 `f "fmt"` 或普通 `import "fmt"`：本地名 = 别名 or 路径末段。
        _ => {
            let alias = name_node.map(|n| go_node_text(n, source).to_string());
            let local = alias
                .clone()
                .unwrap_or_else(|| go_package_base_name(&module_path).to_string());
            Some(ImportInfo {
                module_path,
                symbols: vec![ImportedSymbol {
                    name: local,
                    alias: None,
                    kind: SymbolKind::Namespace,
                }],
                kind: ImportKind::StaticValue,
                reexport: false,
            })
        }
    }
}

/// 导入路径末段作包本地名（`net/http` → `http`）。已知缺陷：`gopkg.in/yaml.v2` 末段
/// `yaml.v2`（真实包名 `yaml`），别名 import 可规避，记 TODO。
fn go_package_base_name(module_path: &str) -> &str {
    module_path.rsplit('/').next().unwrap_or(module_path)
}

/// 判断首个 `//go:build` 约束是否排除默认目标 linux/amd64。仅支持 MVP 子集：`ignore`
/// 约束、单 term、`&&`/`||`/`!` 直式；复杂嵌套（带括号）保守放行（不排除）记 TODO。
/// 约束须在首个非空/非注释代码行之前的注释块内（Go 规范），出现代码即停止扫描。
fn go_build_constraint_excludes(source: &str) -> bool {
    for line in source.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with("//") {
            if let Some(expr) = t.strip_prefix("//go:build ") {
                return !go_build_expr_matches(expr.trim());
            }
            continue;
        }
        // 遇到 package 子句/任何代码 → 约束区结束。
        break;
    }
    false
}

/// 对固定目标 linux/amd64 求值 `//go:build` 表达式（MVP：`||` 顶层、`&&` 次级、`!` 一元、
/// 括号不支持则保守判为匹配）。未知 term（非 GOOS/GOARCH/常见 tag）保守判 false（不满足）。
fn go_build_expr_matches(expr: &str) -> bool {
    if expr.contains('(') || expr.contains(')') {
        return true; // 括号嵌套超 MVP，保守放行（不排除文件）
    }
    // `||` 任一子句成立即成立。
    expr.split("||").any(|clause| {
        // `&&` 全部子句成立才成立。
        clause.split("&&").all(|term| {
            let term = term.trim();
            if let Some(neg) = term.strip_prefix('!') {
                !go_build_term_matches(neg.trim())
            } else {
                go_build_term_matches(term)
            }
        })
    })
}

/// 单个 build term 对 linux/amd64 是否成立。`ignore` 恒 false（排除）；GOOS/GOARCH 精确比对；
/// 其余未知 tag 保守判 false（不满足 → 可能排除，宁缺勿滥迁）。
fn go_build_term_matches(term: &str) -> bool {
    if term.is_empty() || term == "ignore" {
        return false;
    }
    if GOOS_TOKENS.contains(&term) {
        return term == GO_DEFAULT_GOOS;
    }
    if GOARCH_TOKENS.contains(&term) {
        return term == GO_DEFAULT_GOARCH;
    }
    // 常见恒真 tag（构建约束里的 Go 版本/自定义 tag 无法静态判定）→ 保守放行。
    matches!(term, "unix" | "gc" | "cgo") || term.starts_with("go1")
}

// ==================== GO-04 符号提取 ====================

/// 剥指针（`*T`）与泛型实参（`Stack[T]` → `Stack`），取基类型名。用于 receiver 归属、
/// struct 嵌入目标、composite literal 构造类型。
fn go_base_type_name(node: Node, source: &str) -> Option<String> {
    match node.kind() {
        "type_identifier" => Some(go_node_text(node, source).to_string()),
        // `*T`：pointer_type 子节点为被指向类型。
        "pointer_type" => node
            .named_child(0)
            .and_then(|c| go_base_type_name(c, source)),
        // `Stack[T]`：generic_type 的 type field 为基类型。
        "generic_type" => node
            .child_by_field_name("type")
            .and_then(|c| go_base_type_name(c, source)),
        // `pkg.Type`：qualified_type 取名字段（丢包前缀，跨包构造绑定已知边界见模块头）。
        "qualified_type" => node
            .child_by_field_name("name")
            .map(|n| go_node_text(n, source).to_string()),
        _ => None,
    }
}

/// const/var 声明 → Variable 节点（激活 M2 预留变体）。遍历 spec 的多名 `name`（`var a, b int`），
/// 过滤 `_` 空标识符与逗号 token。
fn extract_go_var_const(node: Node, ctx: &mut GoAnalysisContext) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let spec_kind = child.kind();
        if spec_kind != "const_spec" && spec_kind != "var_spec" {
            // 分组 const(...)/var(...) 的 spec 直挂 declaration，无独立 list 容器。
            continue;
        }
        let sig = go_node_text(child, ctx.source).trim().to_string();
        let mut nc = child.walk();
        for name_node in child.children_by_field_name("name", &mut nc) {
            if name_node.kind() != "identifier" {
                continue; // 过滤 const_spec.name 里混入的 `,` token
            }
            let name = go_node_text(name_node, ctx.source);
            if name.is_empty() || name == "_" {
                continue;
            }
            let exported = go_is_exported(name);
            if exported {
                ctx.exported_names.insert(name.to_string());
            }
            let mut n = SourceNode::new(
                NodeId::symbol(NodeType::Variable, ctx.rel_path, name),
                NodeType::Variable,
                name.to_string(),
                ctx.rel_path.to_string(),
            );
            n.line_range = Some(go_node_span(child));
            n.signature = Some(sig.clone());
            n.is_exported = exported;
            ctx.nodes.push(n);
        }
    }
}

/// type 声明 → Class（struct）/Interface（interface）/TypeAlias（其余定义或 `type X = Y`）。
/// struct/interface 嵌入 → Extends 边（**不**发 struct→interface Implements，D-M4-02）。
fn extract_go_type(node: Node, ctx: &mut GoAnalysisContext) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            // `type X = Y` 别名（独立节点，非 type_spec）。
            "type_alias" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    push_go_type_node(child, name_node, NodeType::TypeAlias, ctx);
                }
            }
            "type_spec" => {
                let Some(name_node) = child.child_by_field_name("name") else {
                    continue;
                };
                let type_node = child.child_by_field_name("type");
                let node_type = match type_node.map(|t| t.kind()) {
                    Some("struct_type") => NodeType::Class,
                    Some("interface_type") => NodeType::Interface,
                    _ => NodeType::TypeAlias, // defined type（type MyInt int）等
                };
                let self_id = push_go_type_node(child, name_node, node_type, ctx);
                // 嵌入 → Extends。
                if let Some(t) = type_node {
                    match t.kind() {
                        "struct_type" => extract_struct_embeds(t, &self_id, ctx),
                        "interface_type" => extract_interface_embeds(t, &self_id, ctx),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

/// 建一个 type 节点（含 signature = 声明文本，导出判定），返回其 NodeId。
fn push_go_type_node(
    decl_child: Node,
    name_node: Node,
    node_type: NodeType,
    ctx: &mut GoAnalysisContext,
) -> NodeId {
    let name = go_node_text(name_node, ctx.source).to_string();
    let id = NodeId::symbol(node_type, ctx.rel_path, &name);
    let exported = go_is_exported(&name);
    if exported {
        ctx.exported_names.insert(name.clone());
    }
    let mut n = SourceNode::new(id.clone(), node_type, name, ctx.rel_path.to_string());
    n.line_range = Some(go_node_span(decl_child));
    n.signature = Some(go_node_text(decl_child, ctx.source).trim().to_string());
    n.is_exported = exported;
    ctx.nodes.push(n);
    id
}

/// struct 嵌入字段（`field_declaration` 缺 `name` field）→ Extends 边到被嵌入类型。
fn extract_struct_embeds(struct_type: Node, self_id: &NodeId, ctx: &mut GoAnalysisContext) {
    let Some(list) = struct_type.named_child(0) else {
        return; // field_declaration_list
    };
    if list.kind() != "field_declaration_list" {
        return;
    }
    let mut cursor = list.walk();
    for field in list.children(&mut cursor) {
        if field.kind() != "field_declaration" {
            continue;
        }
        // 嵌入字段：无 name field，只有 type。
        if field.child_by_field_name("name").is_some() {
            continue;
        }
        if let Some(type_node) = field.child_by_field_name("type") {
            if let Some(base) = go_base_type_name(type_node, ctx.source) {
                let target = NodeId::symbol(NodeType::Class, ctx.rel_path, &base);
                ctx.edges
                    .push(Dependency::new(self_id.clone(), target, EdgeType::Extends));
            }
        }
    }
}

/// interface 嵌入（`type_elem` 引用另一 interface）→ Extends 边。
fn extract_interface_embeds(iface: Node, self_id: &NodeId, ctx: &mut GoAnalysisContext) {
    let mut cursor = iface.walk();
    for elem in iface.children(&mut cursor) {
        if elem.kind() != "type_elem" {
            continue;
        }
        if let Some(base) = go_base_type_name(elem.named_child(0).unwrap_or(elem), ctx.source) {
            let target = NodeId::symbol(NodeType::Interface, ctx.rel_path, &base);
            ctx.edges
                .push(Dependency::new(self_id.clone(), target, EdgeType::Extends));
        }
    }
}

/// 顶层函数 → Function 节点 + 递归体提调用/绑定。
fn extract_go_function(node: Node, ctx: &mut GoAnalysisContext) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = go_node_text(name_node, ctx.source).to_string();
    let id = NodeId::symbol(NodeType::Function, ctx.rel_path, &name);
    let exported = go_is_exported(&name);
    if exported {
        ctx.exported_names.insert(name.clone());
    }
    let mut n = SourceNode::new(id, NodeType::Function, name, ctx.rel_path.to_string());
    n.line_range = Some(go_node_span(node));
    n.signature = Some(go_signature(node, ctx.source));
    n.is_exported = exported;
    ctx.nodes.push(n);

    if let Some(body) = node.child_by_field_name("body") {
        extract_go_calls(body, ctx);
    }
}

/// 方法 → Function 节点，name = 限定名 `Type.Method`（对齐 build.rs 档3 `Class.method`），
/// Contains 边 `Class(recv_type) → method`，receiver 变量 → instance_type_bindings。
fn extract_go_method(node: Node, ctx: &mut GoAnalysisContext) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let method_name = go_node_text(name_node, ctx.source).to_string();
    let recv = node.child_by_field_name("receiver");
    let (recv_type, recv_var) = recv
        .map(|r| go_receiver_type_and_var(r, ctx.source))
        .unwrap_or((None, None));
    let Some(recv_type) = recv_type else {
        return; // 无法定 receiver 类型（异常），跳过
    };

    let qualified = format!("{recv_type}.{method_name}");
    let id = NodeId::symbol(NodeType::Function, ctx.rel_path, &qualified);
    // 导出用方法名判定（不用限定名）。
    let exported = go_is_exported(&method_name);
    if exported {
        ctx.exported_names.insert(qualified.clone());
    }
    let mut n = SourceNode::new(
        id.clone(),
        NodeType::Function,
        qualified,
        ctx.rel_path.to_string(),
    );
    n.line_range = Some(go_node_span(node));
    n.signature = Some(go_signature(node, ctx.source));
    n.is_exported = exported;
    ctx.nodes.push(n);

    // Contains 边：Class(recv_type) → method（同文件；跨文件丢弃见模块头限制）。
    let class_id = NodeId::symbol(NodeType::Class, ctx.rel_path, &recv_type);
    ctx.edges
        .push(Dependency::new(class_id, id, EdgeType::Contains));

    // receiver 变量绑定：`r.Other()` 可解析（对齐 python self→class）。
    if let Some(var) = recv_var {
        if var != "_" {
            ctx.instance_type_bindings.insert(var, recv_type);
        }
    }

    if let Some(body) = node.child_by_field_name("body") {
        extract_go_calls(body, ctx);
    }
}

/// 从 receiver（parameter_list）取 (类型名, 变量名)。剥指针/泛型实参。
fn go_receiver_type_and_var(receiver: Node, source: &str) -> (Option<String>, Option<String>) {
    let mut cursor = receiver.walk();
    for pd in receiver.children(&mut cursor) {
        if pd.kind() != "parameter_declaration" {
            continue;
        }
        let ty = pd
            .child_by_field_name("type")
            .and_then(|t| go_base_type_name(t, source));
        let var = pd
            .child_by_field_name("name")
            .map(|n| go_node_text(n, source).to_string());
        return (ty, var);
    }
    (None, None)
}

// ==================== GO-06 签名 ====================

/// func/method 签名 = 声明起始 → body 起始的源码切片（含 receiver/参数/多返回值/可变参
/// `...T`/泛型 `[T any]`），剥函数体。无 body（异常）则取整节点文本。interface 方法集/struct
/// 字段骨架已由类型节点整体 signature（声明文本）承载（见 push_go_type_node）。
fn go_signature(node: Node, source: &str) -> String {
    let end = node
        .child_by_field_name("body")
        .map(|b| b.start_byte())
        .unwrap_or_else(|| node.end_byte());
    source
        .get(node.start_byte()..end)
        .unwrap_or("")
        .trim()
        .to_string()
}

// ==================== GO-05 调用 + instance_type_bindings ====================

/// 递归提取函数/方法体内的调用与构造绑定（手写 stack DFS，对齐 python.rs）。
fn extract_go_calls(root: Node, ctx: &mut GoAnalysisContext) {
    let mut cursor = root.walk();
    let mut stack = vec![root];
    while let Some(current) = stack.pop() {
        match current.kind() {
            "call_expression" => {
                if let Some(func) = current.child_by_field_name("function") {
                    if let Some(callee) = go_callee_name(func, ctx.source) {
                        ctx.calls.push(CallInfo {
                            callee,
                            is_constructor: false,
                        });
                    }
                }
            }
            // Go 构造类比：具名类型 `Foo{}`/`&Foo{}`（& 由父 unary 包裹，DFS 仍访问此节点）。
            "composite_literal" => {
                if let Some(t) = current.child_by_field_name("type") {
                    if let Some(name) = go_base_type_name(t, ctx.source) {
                        ctx.calls.push(CallInfo {
                            callee: name,
                            is_constructor: true,
                        });
                    }
                    // 匿名结构体/`[]T{}`/`map[K]V{}`：go_base_type_name 返回 None → 跳过。
                }
            }
            "short_var_declaration" | "assignment_statement" => go_bind_assign(current, ctx),
            "var_declaration" => go_bind_local_var(current, ctx),
            _ => {}
        }
        cursor.reset(current);
        for child in current.children(&mut cursor) {
            stack.push(child);
        }
    }
}

/// 调用目标名：`Foo()` → `Foo`；`pkg.Func()`/`x.Method()` → `pkg.Func`/`x.Method`（selector
/// 全文本，build.rs 三级解析按首段拆包名/receiver）。其他复杂形态（链式/括号）→ None。
fn go_callee_name(func: Node, source: &str) -> Option<String> {
    match func.kind() {
        "identifier" | "selector_expression" => Some(go_node_text(func, source).to_string()),
        _ => None,
    }
}

/// `v := Foo{}` / `v = &Foo{}` 短变量/赋值绑定：左标识符 → 右 composite 类型（剥指针）。
/// 工厂 `v := NewFoo()`（call）不绑定（静态无法定型，保守——build.rs 落 fn_index 唯一兜底）。
fn go_bind_assign(node: Node, ctx: &mut GoAnalysisContext) {
    let (Some(left), Some(right)) = (
        node.child_by_field_name("left"),
        node.child_by_field_name("right"),
    ) else {
        return;
    };
    let lefts = go_named_children(left);
    let rights = go_named_children(right);
    for (l, r) in lefts.iter().zip(rights.iter()) {
        if l.kind() != "identifier" {
            continue;
        }
        let var = go_node_text(*l, ctx.source);
        if var == "_" {
            continue;
        }
        if let Some(ty) = go_composite_binding_type(*r, ctx.source) {
            ctx.instance_type_bindings.insert(var.to_string(), ty);
        }
    }
}

/// 局部 `var v Foo` / `var v = Foo{}` 绑定（top-level var 走 extract_go_var_const 建 Variable
/// 节点，不来此；本函数仅处理函数体内的 var_declaration）。
fn go_bind_local_var(node: Node, ctx: &mut GoAnalysisContext) {
    let mut cursor = node.walk();
    for spec in node.children(&mut cursor) {
        if spec.kind() != "var_spec" {
            continue;
        }
        let names = go_field_identifiers(spec, "name", ctx.source);
        // 优先显式类型 `var v Foo`；否则看初始化器 `var v = Foo{}`。
        if let Some(ty) = spec
            .child_by_field_name("type")
            .and_then(|t| go_base_type_name(t, ctx.source))
        {
            for name in names {
                if name != "_" {
                    ctx.instance_type_bindings.insert(name, ty.clone());
                }
            }
        } else if let Some(value) = spec.child_by_field_name("value") {
            let vals = go_named_children(value);
            for (name, v) in names.iter().zip(vals.iter()) {
                if name == "_" {
                    continue;
                }
                if let Some(ty) = go_composite_binding_type(*v, ctx.source) {
                    ctx.instance_type_bindings.insert(name.clone(), ty);
                }
            }
        }
    }
}

/// composite 绑定类型：`Foo{}` → `Foo`；`&Foo{}`（unary_expression `&`）→ 剥指针取 `Foo`；
/// 其余（call/字面量）→ None（不绑定）。
fn go_composite_binding_type(node: Node, source: &str) -> Option<String> {
    match node.kind() {
        "composite_literal" => node
            .child_by_field_name("type")
            .and_then(|t| go_base_type_name(t, source)),
        "unary_expression" => node
            .child_by_field_name("operand")
            .and_then(|op| go_composite_binding_type(op, source)),
        _ => None,
    }
}

/// 收集节点的 named 子节点（避免借用冲突，先 clone 到 Vec）。
fn go_named_children(node: Node) -> Vec<Node> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

/// 收集某 field 下的 identifier 文本（过滤逗号 token 等）。
fn go_field_identifiers(node: Node, field: &str, source: &str) -> Vec<String> {
    let mut cursor = node.walk();
    node.children_by_field_name(field, &mut cursor)
        .filter(|n| n.kind() == "identifier")
        .map(|n| go_node_text(n, source).to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// 核心方法不 panic（回归：防 todo!() 让 Go 项目 graph build 崩进程）。
    #[test]
    fn core_methods_do_not_panic() {
        let mut adapter = GoAdapter::new().unwrap();
        // analyze_file 正常产出（纯 package → 仅 File 节点），不 panic、不报错。
        let a = adapter.analyze_file("package main\n", "main.go").unwrap();
        assert_eq!(a.nodes.len(), 1); // 仅 File 节点
        assert_eq!(a.nodes[0].node_type, NodeType::File);
        // classify_file（trait 默认）返回保守分类，不 panic、不判机械。
        let cls = adapter.classify_file("package main\n");
        assert!(cls.danger.is_empty());
        assert!(!cls.is_mechanical());
        // resolve_import：未 configure（module_path=None）→ None，不 panic。
        assert_eq!(
            adapter.resolve_import("fmt", "main.go", &|_| false, &|_| Vec::new()),
            None
        );
    }

    /// 纯 package 声明 / 纯常量 → Trivial（无实质内容、无危险）。
    #[test]
    fn detect_tier_trivial() {
        let mut adapter = GoAdapter::new().unwrap();
        assert_eq!(adapter.detect_tier("package main\n"), ModuleTier::Trivial);
        assert_eq!(
            adapter.detect_tier("package config\n\nconst Version = \"1.0\"\nvar Debug = false\n"),
            ModuleTier::Trivial
        );
    }

    /// 普通函数/类型定义、无危险信号 → Standard。
    #[test]
    fn detect_tier_standard() {
        let mut adapter = GoAdapter::new().unwrap();
        let src = "package m\n\nfunc Add(a, b int) int {\n\treturn a + b\n}\n\ntype Point struct {\n\tX int\n\tY int\n}\n";
        assert_eq!(adapter.detect_tier(src), ModuleTier::Standard);
    }

    /// goroutine → Full。
    #[test]
    fn detect_tier_goroutine_is_full() {
        let mut adapter = GoAdapter::new().unwrap();
        let src = "package m\n\nfunc run() {\n\tgo work()\n}\n";
        assert_eq!(adapter.detect_tier(src), ModuleTier::Full);
    }

    /// channel（send + chan 类型）→ Full。
    #[test]
    fn detect_tier_channel_is_full() {
        let mut adapter = GoAdapter::new().unwrap();
        let src = "package m\n\nfunc pipe(ch chan int) {\n\tch <- 1\n}\n";
        assert_eq!(adapter.detect_tier(src), ModuleTier::Full);
    }

    /// select → Full。
    #[test]
    fn detect_tier_select_is_full() {
        let mut adapter = GoAdapter::new().unwrap();
        let src = "package m\n\nfunc pick(a, b chan int) {\n\tselect {\n\tcase <-a:\n\tcase <-b:\n\t}\n}\n";
        assert_eq!(adapter.detect_tier(src), ModuleTier::Full);
    }

    /// 仅从 channel 接收（`v := <-getCh()`，无 chan 类型/send/go/select 语法节点）→ Full。
    /// 回归：codex+主审审查发现的接收表达式漏判。
    #[test]
    fn detect_tier_receive_is_full() {
        let mut adapter = GoAdapter::new().unwrap();
        // 从函数返回的 channel 接收：函数签名不含 channel_type，仅有 unary_expression `<-`。
        let assign = "package m\n\nfunc consume() int {\n\treturn <-getCh()\n}\n";
        assert_eq!(adapter.detect_tier(assign), ModuleTier::Full);
        // 语句式接收 `<-done`。
        let stmt = "package m\n\nfunc wait() {\n\t<-getDone()\n}\n";
        assert_eq!(adapter.detect_tier(stmt), ModuleTier::Full);
    }

    /// import "reflect" / "unsafe" / "C"（cgo）→ Full。
    #[test]
    fn detect_tier_danger_imports_are_full() {
        let mut adapter = GoAdapter::new().unwrap();
        for pkg in ["reflect", "unsafe", "C"] {
            let src = format!("package m\n\nimport \"{pkg}\"\n\nfunc f() {{}}\n");
            assert_eq!(
                adapter.detect_tier(&src),
                ModuleTier::Full,
                "import {pkg} 应判 Full"
            );
        }
        // 分组 import 形式也应命中。
        let grouped =
            "package m\n\nimport (\n\t\"fmt\"\n\t\"reflect\"\n)\n\nfunc f() { fmt.Println() }\n";
        assert_eq!(adapter.detect_tier(grouped), ModuleTier::Full);
    }

    /// 无害 import（fmt/strings）不触发危险 → Standard。
    #[test]
    fn detect_tier_safe_imports_not_full() {
        let mut adapter = GoAdapter::new().unwrap();
        let src = "package m\n\nimport \"fmt\"\n\nfunc greet() {\n\tfmt.Println(\"hi\")\n}\n";
        assert_eq!(adapter.detect_tier(src), ModuleTier::Standard);
    }

    /// 语法错误 → 保守 Full。
    #[test]
    fn detect_tier_syntax_error_is_full() {
        let mut adapter = GoAdapter::new().unwrap();
        assert_eq!(
            adapter.detect_tier("package m\n\nfunc broken( {\n"),
            ModuleTier::Full
        );
    }

    /// 含 go.mod 的目录探测为源码根 Some(".")（探测成功，不触发 fallback warning）。
    #[test]
    fn detect_source_root_with_go_mod_returns_dot() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("go.mod"), "module example.com/x\n").unwrap();
        let adapter = GoAdapter::new().unwrap();
        assert_eq!(
            adapter.detect_source_root(tmp.path()),
            Some(".".to_string())
        );
    }

    /// 无 go.mod 且无 src/ 时返回 None（回退由调用方处理）。
    #[test]
    fn detect_source_root_without_go_mod_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let adapter = GoAdapter::new().unwrap();
        assert_eq!(adapter.detect_source_root(Path::new(tmp.path())), None);
    }

    // ==================== GO-02/04/05/06 analyze_file 测试 ====================

    fn analyze(src: &str) -> FileAnalysis {
        GoAdapter::new()
            .unwrap()
            .analyze_file(src, "pkg/m.go")
            .unwrap()
    }

    fn find_node<'a>(a: &'a FileAnalysis, name: &str) -> Option<&'a SourceNode> {
        a.nodes.iter().find(|n| n.name == name)
    }

    fn has_edge(a: &FileAnalysis, et: EdgeType, src_sub: &str, tgt_sub: &str) -> bool {
        a.edges.iter().any(|e| {
            e.edge_type == et
                && e.source.as_str().contains(src_sub)
                && e.target.as_str().contains(tgt_sub)
        })
    }

    /// GO-04：func 节点 + 首字母大写导出判定 + signature 剥 body + File 节点。
    #[test]
    fn analyze_go_function_basic() {
        let a = analyze(
            "package m\n\nfunc Add(a, b int) int {\n\treturn a + b\n}\n\nfunc helper() {}\n",
        );
        let add = find_node(&a, "Add").expect("Add 节点");
        assert_eq!(add.node_type, NodeType::Function);
        assert!(add.is_exported, "Add 首字母大写应导出");
        let sig = add.signature.as_deref().unwrap_or("");
        assert!(
            sig.contains("func Add(a, b int) int"),
            "signature 应含声明: {sig}"
        );
        assert!(!sig.contains('{'), "signature 应剥 body: {sig}");
        let helper = find_node(&a, "helper").expect("helper 节点");
        assert!(!helper.is_exported, "helper 小写非导出");
        assert!(find_node(&a, "pkg/m.go").is_some(), "File 节点存在");
        // Exports 边仅大写符号。
        assert!(has_edge(&a, EdgeType::Exports, "pkg/m.go", "Add"));
        assert!(!has_edge(&a, EdgeType::Exports, "pkg/m.go", "helper"));
    }

    /// GO-04：struct → Class 节点，signature 含字段骨架。
    #[test]
    fn analyze_go_struct_is_class() {
        let a = analyze("package m\n\ntype Rect struct {\n\tW int\n\tH int\n}\n");
        let rect = find_node(&a, "Rect").expect("Rect 节点");
        assert_eq!(rect.node_type, NodeType::Class);
        assert!(rect.is_exported);
        assert!(rect.signature.as_deref().unwrap_or("").contains("W int"));
    }

    /// GO-04/07：interface → Interface 节点，无 struct→interface Implements 边（D-M4-02）。
    #[test]
    fn analyze_go_interface_is_interface_no_implements() {
        let a = analyze("package m\n\ntype Shape interface {\n\tArea() float64\n}\n\ntype Rect struct{}\n\nfunc (r Rect) Area() float64 { return 0 }\n");
        assert_eq!(
            find_node(&a, "Shape").unwrap().node_type,
            NodeType::Interface
        );
        // 不发 Implements（EdgeType 无 Implements；即便隐式满足也不连 Extends 到 interface）。
        assert!(!has_edge(&a, EdgeType::Extends, "Rect", "Shape"));
    }

    /// GO-04：struct 嵌入 → Extends 边。
    #[test]
    fn analyze_go_struct_embedding_extends() {
        let a = analyze(
            "package m\n\ntype Base struct{}\n\ntype Derived struct {\n\tBase\n\tX int\n}\n",
        );
        assert!(
            has_edge(&a, EdgeType::Extends, "Derived", "Base"),
            "嵌入应发 Extends"
        );
    }

    /// GO-04：interface 嵌入 → Extends 边。
    #[test]
    fn analyze_go_interface_embedding_extends() {
        let a = analyze("package m\n\ntype Reader interface{ Read() }\n\ntype Writer interface{ Write() }\n\ntype RW interface {\n\tReader\n\tWriter\n}\n");
        assert!(has_edge(&a, EdgeType::Extends, "RW", "Reader"));
        assert!(has_edge(&a, EdgeType::Extends, "RW", "Writer"));
    }

    /// GO-04：type alias（`type X = Y`）与 defined type（`type MyInt int`）→ TypeAlias 节点。
    #[test]
    fn analyze_go_type_alias_and_defined() {
        let a = analyze("package m\n\ntype MyInt int\n\ntype Alias = Rect\n");
        assert_eq!(
            find_node(&a, "MyInt").unwrap().node_type,
            NodeType::TypeAlias
        );
        assert_eq!(
            find_node(&a, "Alias").unwrap().node_type,
            NodeType::TypeAlias
        );
    }

    /// GO-04：method 限定名 `Type.Method` + Contains 边；值 receiver。
    #[test]
    fn analyze_go_method_qualified_and_contains() {
        let a = analyze(
            "package m\n\ntype Rect struct{}\n\nfunc (r Rect) Area() float64 {\n\treturn 0\n}\n",
        );
        let m = find_node(&a, "Rect.Area").expect("限定名 Rect.Area");
        assert_eq!(m.node_type, NodeType::Function);
        assert!(m.is_exported, "Area 大写导出");
        assert!(has_edge(&a, EdgeType::Contains, "Rect", "Rect.Area"));
    }

    /// GO-04：指针 receiver `*Rect` 归属到 Rect（剥指针）。
    #[test]
    fn analyze_go_pointer_receiver() {
        let a = analyze("package m\n\ntype Rect struct{}\n\nfunc (r *Rect) Scale() {}\n");
        assert!(
            find_node(&a, "Rect.Scale").is_some(),
            "指针 receiver 应归 Rect"
        );
    }

    /// GO-04：泛型 receiver `Stack[T]` 归属到 Stack（剥泛型实参）。
    #[test]
    fn analyze_go_generic_receiver() {
        let a =
            analyze("package m\n\ntype Stack[T any] struct{}\n\nfunc (s Stack[T]) Push(v T) {}\n");
        assert!(
            find_node(&a, "Stack.Push").is_some(),
            "泛型 receiver 应归 Stack"
        );
    }

    /// GO-04：同名方法跨不同 receiver → 双唯一节点。
    #[test]
    fn analyze_go_same_method_diff_receiver() {
        let a = analyze("package m\n\ntype A struct{}\ntype B struct{}\n\nfunc (a A) Do() {}\nfunc (b B) Do() {}\n");
        assert!(find_node(&a, "A.Do").is_some());
        assert!(find_node(&a, "B.Do").is_some());
    }

    /// GO-04：激活 Variable——顶层 const/var → Variable 节点 + 导出判定；`_` 不建节点。
    #[test]
    fn analyze_go_activates_variable_nodes() {
        let a = analyze(
            "package m\n\nconst Version = \"1.0\"\n\nvar debug = false\n\nvar _ = ignored()\n",
        );
        let v = find_node(&a, "Version").expect("Version Variable");
        assert_eq!(v.node_type, NodeType::Variable);
        assert!(v.is_exported);
        let d = find_node(&a, "debug").expect("debug Variable");
        assert_eq!(d.node_type, NodeType::Variable);
        assert!(!d.is_exported);
        assert!(find_node(&a, "_").is_none(), "空标识符不建节点");
        // 导出 Variable 进 Exports 边。
        assert!(has_edge(&a, EdgeType::Exports, "pkg/m.go", "Version"));
    }

    /// GO-04：多名声明 `var a, b int` → 各建 Variable 节点。
    #[test]
    fn analyze_go_multi_name_var() {
        let a = analyze("package m\n\nvar X, Y int\n");
        assert!(find_node(&a, "X").is_some());
        assert!(find_node(&a, "Y").is_some());
    }

    /// GO-05：本地调用 / `pkg.Func` / `x.Method` callee 串。
    #[test]
    fn analyze_go_calls() {
        let a = analyze("package m\n\nimport \"fmt\"\n\nfunc run(r Rect) {\n\thelper()\n\tfmt.Println(\"x\")\n\tr.Area()\n}\n");
        let callees: Vec<&str> = a.calls.iter().map(|c| c.callee.as_str()).collect();
        assert!(callees.contains(&"helper"), "本地调用: {callees:?}");
        assert!(callees.contains(&"fmt.Println"), "跨包调用: {callees:?}");
        assert!(callees.contains(&"r.Area"), "方法调用: {callees:?}");
    }

    /// GO-05：composite literal `Foo{}`/`&Foo{}` → 构造 + 绑定；`[]int{}` 不产。
    #[test]
    fn analyze_go_composite_constructor_and_binding() {
        let a = analyze(
            "package m\n\nfunc build() {\n\ta := Rect{}\n\tb := &Point{}\n\t_ = []int{1, 2}\n}\n",
        );
        let ctors: Vec<&str> = a
            .calls
            .iter()
            .filter(|c| c.is_constructor)
            .map(|c| c.callee.as_str())
            .collect();
        assert!(ctors.contains(&"Rect"), "Rect{{}} 构造: {ctors:?}");
        assert!(ctors.contains(&"Point"), "&Point{{}} 构造: {ctors:?}");
        assert!(!ctors.contains(&"int"), "[]int{{}} 非具名构造");
        assert_eq!(a.instance_type_bindings.get("a"), Some(&"Rect".to_string()));
        assert_eq!(
            a.instance_type_bindings.get("b"),
            Some(&"Point".to_string())
        );
    }

    /// GO-05：工厂函数 `v := NewFoo()` 不绑定（保守）。
    #[test]
    fn analyze_go_factory_no_binding() {
        let a = analyze("package m\n\nfunc build() {\n\tv := NewFoo()\n\t_ = v\n}\n");
        assert!(
            !a.instance_type_bindings.contains_key("v"),
            "工厂调用不绑定类型"
        );
    }

    /// GO-05：receiver 变量绑定（`r.other()` 可解析）。
    #[test]
    fn analyze_go_receiver_var_binding() {
        let a = analyze("package m\n\ntype Rect struct{}\n\nfunc (r Rect) Area() {}\n");
        assert_eq!(a.instance_type_bindings.get("r"), Some(&"Rect".to_string()));
    }

    /// GO-06：signature 含多返回值/可变参/泛型。
    #[test]
    fn analyze_go_signature_variants() {
        let a = analyze("package m\n\nfunc Multi() (int, error) { return 0, nil }\n\nfunc Variadic(xs ...int) {}\n\nfunc Generic[T any](v T) T { return v }\n");
        assert!(find_node(&a, "Multi")
            .unwrap()
            .signature
            .as_deref()
            .unwrap()
            .contains("(int, error)"));
        assert!(find_node(&a, "Variadic")
            .unwrap()
            .signature
            .as_deref()
            .unwrap()
            .contains("...int"));
        assert!(find_node(&a, "Generic")
            .unwrap()
            .signature
            .as_deref()
            .unwrap()
            .contains("[T any]"));
    }

    /// GO-02：import 三形态（普通末段本地名 / 别名 / `_` 副作用 / `.` 点导入）。
    #[test]
    fn analyze_go_imports_shapes() {
        let a = analyze("package m\n\nimport (\n\t\"net/http\"\n\tf \"fmt\"\n\t_ \"lib/pq\"\n\t. \"strings\"\n)\n");
        let http = a
            .imports
            .iter()
            .find(|i| i.module_path == "net/http")
            .unwrap();
        assert_eq!(http.symbols[0].name, "http", "本地名=末段");
        assert_eq!(http.symbols[0].kind, SymbolKind::Namespace);
        let fmt_i = a.imports.iter().find(|i| i.module_path == "fmt").unwrap();
        assert_eq!(fmt_i.symbols[0].name, "f", "别名本地名");
        let pq = a
            .imports
            .iter()
            .find(|i| i.module_path == "lib/pq")
            .unwrap();
        assert_eq!(pq.kind, ImportKind::SideEffect);
        assert!(pq.symbols.is_empty());
        let strings_i = a
            .imports
            .iter()
            .find(|i| i.module_path == "strings")
            .unwrap();
        assert!(strings_i.symbols.is_empty(), "点导入空符号");
    }

    /// GO-02：单行 import 与分组 import 均解析。
    #[test]
    fn analyze_go_single_import() {
        let a = analyze("package m\n\nimport \"fmt\"\n");
        assert_eq!(a.imports.len(), 1);
        assert_eq!(a.imports[0].module_path, "fmt");
    }

    /// GO-02：`can_handle` 排除 `_test.go` 与非默认平台后缀。
    #[test]
    fn can_handle_filters_test_and_platform() {
        let ad = GoAdapter::new().unwrap();
        assert!(ad.can_handle(Path::new("pkg/svc.go")));
        assert!(
            !ad.can_handle(Path::new("pkg/svc_test.go")),
            "_test.go 排除"
        );
        assert!(
            !ad.can_handle(Path::new("pkg/svc_windows.go")),
            "_windows.go 排除"
        );
        assert!(
            !ad.can_handle(Path::new("pkg/svc_arm64.go")),
            "_arm64.go 排除"
        );
        assert!(
            !ad.can_handle(Path::new("pkg/svc_windows_amd64.go")),
            "非默认 OS_ARCH 排除"
        );
        assert!(
            ad.can_handle(Path::new("pkg/svc_linux.go")),
            "_linux.go 默认目标保留"
        );
        assert!(
            ad.can_handle(Path::new("pkg/svc_linux_amd64.go")),
            "默认 OS_ARCH 保留"
        );
        assert!(
            ad.can_handle(Path::new("pkg/linux.go")),
            "无前缀 linux.go 不受约束"
        );
    }

    /// GO-02：`//go:build` 排除非默认平台 → 仅 File 节点。
    #[test]
    fn analyze_go_build_constraint_excludes() {
        let a = analyze("//go:build windows\n\npackage m\n\nfunc OnlyWin() {}\n");
        assert!(find_node(&a, "OnlyWin").is_none(), "windows-only 应被排除");
        assert_eq!(a.nodes.len(), 1, "仅 File 节点");
        // linux 约束保留。
        let b = analyze("//go:build linux\n\npackage m\n\nfunc OnLinux() {}\n");
        assert!(find_node(&b, "OnLinux").is_some(), "linux 约束保留");
    }

    /// GO-02：`ignore` 约束排除。
    #[test]
    fn analyze_go_build_ignore() {
        let a = analyze("//go:build ignore\n\npackage m\n\nfunc F() {}\n");
        assert!(find_node(&a, "F").is_none());
    }

    /// GO-03：configure_project 读 go.mod → resolve_import 剥前缀取代表文件。
    #[test]
    fn resolve_import_representative_file() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("go.mod"), "module example.com/x\n").unwrap();
        let mut ad = GoAdapter::new().unwrap();
        ad.configure_project(tmp.path());
        let list_dir = |dir: &str| {
            if dir == "internal/foo" {
                vec![
                    "internal/foo/b.go".to_string(),
                    "internal/foo/a.go".to_string(),
                    "internal/foo/a_test.go".to_string(),
                ]
            } else {
                Vec::new()
            }
        };
        let r = ad.resolve_import(
            "example.com/x/internal/foo",
            "main.go",
            &|_| false,
            &list_dir,
        );
        assert_eq!(
            r,
            Some("internal/foo/a.go".to_string()),
            "字典序第一非 _test.go"
        );
    }

    /// GO-03：stdlib/第三方/部分段误匹配 → None；无 module_path → None。
    #[test]
    fn resolve_import_external_and_none() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("go.mod"), "module example.com/foo\n").unwrap();
        let mut ad = GoAdapter::new().unwrap();
        ad.configure_project(tmp.path());
        let any = |_: &str| vec!["x.go".to_string()];
        assert_eq!(
            ad.resolve_import("fmt", "m.go", &|_| false, &any),
            None,
            "stdlib"
        );
        assert_eq!(
            ad.resolve_import("github.com/a/b", "m.go", &|_| false, &any),
            None,
            "第三方"
        );
        assert_eq!(
            ad.resolve_import("example.com/foobar/x", "m.go", &|_| false, &any),
            None,
            "部分段误匹配防护"
        );
        // 无 module_path。
        let ad2 = GoAdapter::new().unwrap();
        assert_eq!(
            ad2.resolve_import("example.com/foo/x", "m.go", &|_| false, &any),
            None
        );
    }

    /// GO-03：parse_go_module_path 处理普通/行内注释/引号/缺失。
    #[test]
    fn parse_go_module_path_variants() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("go.mod"),
            "module example.com/plain\n\ngo 1.21\n",
        )
        .unwrap();
        assert_eq!(
            parse_go_module_path(tmp.path()),
            Some("example.com/plain".into())
        );
        std::fs::write(
            tmp.path().join("go.mod"),
            "module example.com/c // comment\n",
        )
        .unwrap();
        assert_eq!(
            parse_go_module_path(tmp.path()),
            Some("example.com/c".into())
        );
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(parse_go_module_path(empty.path()), None, "无 go.mod → None");
    }

    /// 顶层匿名换行节点不产生伪符号（is_named 过滤回归）。
    #[test]
    fn analyze_go_no_phantom_from_newlines() {
        let a = analyze("package m\n\n\n\nfunc F() {}\n\n\n");
        // 仅 File + F 两个节点，无换行产生的额外节点。
        assert_eq!(
            a.nodes.len(),
            2,
            "节点: {:?}",
            a.nodes.iter().map(|n| &n.name).collect::<Vec<_>>()
        );
    }
}
