//! 源码 / Rust 结构复杂度对比，对应 `rustmigrate stats compare`。
//!
//! 设计（`06-plugin-structure.md` § CLI 表）：`stats compare` 复用 tokei + tree-sitter
//! 函数计数做结构对比，作为 Phase A 结构校验门禁（见 `03-execution-model.md` § 4.3 Step 4.5）。
//! 对比三个维度（与 03 § 4.3 / § 评分卡阈值表对齐）：
//!   1. 代码行数比（tokei，仅 code 行）
//!   2. 函数数量比（tree-sitter / 轻量扫描）
//!   3. 主控制流嵌套层级（循环/条件分支的最大嵌套深度）
//!
//! 比值定义统一为 `rust / source`（与设计「膨胀比」一致：> 1 表示 Rust 侧更多/更深）。
//!
//! **计数手段（实现层）**：源侧复用 [`crate::graph::build_graph_ts`] 的 tree-sitter 解析
//! （`Function` 节点含方法/箭头常量 + AST 精确嵌套）；Rust 侧用轻量词法扫描（剥离注释/字符串后
//! 计 `fn` 与控制流嵌套，无 `tree-sitter-rust` 依赖，已知近似）。JSON 以 `method` 字段标注每侧手段。
//!
//! **两侧口径差异、跨语言可比性限制、以及「函数数/嵌套比仅作粗粒度告警、行数比为门禁主依据」
//! 的决策，见 `03-execution-model.md` § 4.3 Step 4.5（设计权威，不在此重复）。**

use std::path::Path;

use serde::Serialize;

use crate::error::{MigrateError, Result};
use crate::graph::build::build_graph_for_lang;
use crate::lang::registry::create_adapter;
use crate::stats::loc::count_loc;
use crate::types::common::SourceLang;
use crate::types::config::{lang_vendor_dirs, COMMON_EXCLUDES};
use crate::types::graph::NodeType;

/// 函数/控制流计数手段（JSON 序列化为 kebab-case，与历史输出一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CountMethod {
    /// tree-sitter AST 精确解析（源码侧）。
    TreeSitter,
    /// 轻量词法扫描近似（Rust 侧，无 tree-sitter-rust 依赖）。
    LexicalScan,
}

/// 单侧结构度量。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StructureMetrics {
    /// 统计根目录（字符串形式，便于序列化）。
    pub root: String,
    /// 代码行数（tokei，不含注释/空行）。
    pub code: u64,
    /// 函数/方法数量。
    pub functions: usize,
    /// 主控制流（循环/条件分支）最大嵌套层级。
    pub max_nesting: usize,
    /// 函数计数手段。
    pub method: CountMethod,
}

/// 一个维度的对比比值（`rust / source`）。`None` 表示分母为 0（无法计算）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Ratio {
    /// 源码侧数值。
    pub source: f64,
    /// Rust 侧数值。
    pub rust: f64,
    /// `rust / source`；`source == 0` 时为 `None`。
    pub ratio: Option<f64>,
}

impl Ratio {
    fn new(source: f64, rust: f64) -> Self {
        let ratio = if source == 0.0 {
            None
        } else {
            Some(rust / source)
        };
        Self {
            source,
            rust,
            ratio,
        }
    }
}

/// 结构对比完整报告。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CompareReport {
    /// 源码侧度量。
    pub source: StructureMetrics,
    /// Rust 侧度量。
    pub rust: StructureMetrics,
    /// 代码行数比（rust/source）。
    pub loc_ratio: Ratio,
    /// 函数数量比（rust/source）。
    pub function_ratio: Ratio,
    /// 最大控制流嵌套层级比（rust/source）。
    pub nesting_ratio: Ratio,
}

/// 对源码目录与 Rust 目录做结构对比。
///
/// 任一侧目录不存在返回 [`MigrateError::FileNotFound`]（由 CLI 层转成 warning + 跳过，
/// 见 lib.rs；本函数只负责「目录都存在时」的对比，不静默吞缺失）。
pub fn compare_structure(
    source_root: &Path,
    rust_root: &Path,
    source_lang: SourceLang,
) -> Result<CompareReport> {
    let source = measure_source_for_lang(source_root, source_lang)?;
    let rust = measure_rust(rust_root)?;

    let loc_ratio = Ratio::new(source.code as f64, rust.code as f64);
    let function_ratio = Ratio::new(source.functions as f64, rust.functions as f64);
    let nesting_ratio = Ratio::new(source.max_nesting as f64, rust.max_nesting as f64);

    Ok(CompareReport {
        source,
        rust,
        loc_ratio,
        function_ratio,
        nesting_ratio,
    })
}

/// 按指定语言度量源码侧：tokei 取 code 行；tree-sitter 取 Function 节点数与控制流嵌套。
fn measure_source_for_lang(root: &Path, lang: SourceLang) -> Result<StructureMetrics> {
    let loc = count_loc(root)?;
    let graph = build_graph_for_lang(root, lang)?;
    let functions = graph.nodes_by_type(NodeType::Function).len();
    let max_nesting = source_max_nesting(root, lang)?;
    Ok(StructureMetrics {
        root: root.to_string_lossy().into_owned(),
        code: loc.code,
        functions,
        max_nesting,
        method: CountMethod::TreeSitter,
    })
}

/// 度量 Rust 侧：tokei 取 code 行；轻量词法扫描取 `fn` 数与控制流嵌套。
fn measure_rust(root: &Path) -> Result<StructureMetrics> {
    let loc = count_loc(root)?; // 目录不存在在此返回 FileNotFound
    let mut functions = 0usize;
    let mut max_nesting = 0usize;
    for path in collect_rust_files(root)? {
        let src = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => continue, // 非 UTF-8 / 读取失败的单文件跳过，不影响整体
        };
        let stripped = strip_comments_and_strings(&src);
        functions += count_rust_fns(&stripped);
        max_nesting = max_nesting.max(brace_keyword_nesting(&stripped));
    }
    Ok(StructureMetrics {
        root: root.to_string_lossy().into_owned(),
        code: loc.code,
        functions,
        max_nesting,
        method: CountMethod::LexicalScan,
    })
}

// === 源码侧（tree-sitter）控制流嵌套 ===

/// 计算源码目录下所有源文件的最大控制流嵌套层级（取全目录最大值）。
fn source_max_nesting(root: &Path, lang: SourceLang) -> Result<usize> {
    use tree_sitter::Parser;
    let mut parser = Parser::new();
    let (grammar, control_flow_fn): (_, fn(&str) -> bool) = match lang {
        SourceLang::TypeScript => (
            tree_sitter_typescript::language_typescript(),
            is_ts_control_flow,
        ),
        SourceLang::Python => (tree_sitter_python::language(), is_py_control_flow),
        _ => {
            return Err(MigrateError::NotImplemented(format!(
                "源码嵌套深度分析尚未支持: {lang}"
            )))
        }
    };
    parser
        .set_language(&grammar)
        .map_err(|e| MigrateError::Config(format!("tree-sitter 语法加载失败: {e}")))?;

    let mut max = 0usize;
    for path in collect_source_files(root, lang)? {
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Some(tree) = parser.parse(&src, None) else {
            continue;
        };
        max = max.max(node_nesting(tree.root_node(), 0, control_flow_fn));
    }
    Ok(max)
}

/// TS 控制流节点类型：循环 + 条件分支。`switch_statement` 计一层，case 不再加深。
fn is_ts_control_flow(kind: &str) -> bool {
    matches!(
        kind,
        "if_statement"
            | "for_statement"
            | "for_in_statement"
            | "while_statement"
            | "do_statement"
            | "switch_statement"
    )
}

/// Python 控制流节点类型：条件 + 循环 + 上下文/异常/匹配块（与 `lang/python.rs`
/// 机械性判定使用的嵌套节点集一致）。`try`/`with`/`match` 各计一层。
fn is_py_control_flow(kind: &str) -> bool {
    matches!(
        kind,
        "if_statement"
            | "for_statement"
            | "while_statement"
            | "with_statement"
            | "try_statement"
            | "match_statement"
    )
}

/// 递归计算 AST 子树的控制流最大嵌套深度。
///
/// 遇到控制流节点深度 +1；非控制流节点透传当前深度。返回子树内出现过的最大深度。
fn node_nesting(node: tree_sitter::Node, depth: usize, is_control_flow: fn(&str) -> bool) -> usize {
    let here = if is_control_flow(node.kind()) {
        depth + 1
    } else {
        depth
    };
    let mut max = here;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        max = max.max(node_nesting(child, here, is_control_flow));
    }
    max
}

// === 文件收集 ===

/// 按 `excludes` 跳过目录、按 `accept` 判定收集文件。
fn collect_files(
    root: &Path,
    excludes: &[&str],
    accept: impl Fn(&Path) -> bool,
) -> Result<Vec<std::path::PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(MigrateError::Io)?;
        for entry in entries {
            let entry = entry.map_err(MigrateError::Io)?;
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if excludes.contains(&name.as_ref()) {
                    continue;
                }
                stack.push(path);
            } else if accept(&path) {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// 收集 Rust 源文件（`.rs`）——Rust 产物目录走通用排除（`.git`/`target`）。
fn collect_rust_files(root: &Path) -> Result<Vec<std::path::PathBuf>> {
    collect_files(root, COMMON_EXCLUDES, |p| {
        p.extension().and_then(|e| e.to_str()) == Some("rs")
    })
}

/// 按语言收集源文件——目录排除用该语言精确排除（非全语言全集，避免误伤同名业务目录）；
/// 后缀与归属判定复用 adapter 的 `can_handle`，与 graph 构建路径口径一致。
fn collect_source_files(root: &Path, lang: SourceLang) -> Result<Vec<std::path::PathBuf>> {
    let adapter = create_adapter(lang)?;
    let excludes: Vec<&str> = lang_vendor_dirs(lang)
        .iter()
        .copied()
        .chain(COMMON_EXCLUDES.iter().copied())
        .collect();
    collect_files(root, &excludes, |p| adapter.can_handle(p))
}

// === Rust 侧轻量词法扫描 ===

/// 剥离行注释 `//…`、块注释 `/*…*/` 与字符串/字符字面量，避免它们里的
/// `fn`/`if` 等关键字与 `{}` 被误计。是近似实现（不处理 raw string `r#"…"#`、
/// 字节串、嵌套块注释的精确边界），但对常规 Rust 代码足够稳健，且只会**低估**
/// 噪声、不会把代码本身的关键字吃掉。
fn strip_comments_and_strings(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        // 行注释
        if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        // 块注释（支持嵌套，Rust 块注释可嵌套）
        if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            let mut depth = 1usize;
            i += 2;
            while i < bytes.len() && depth > 0 {
                if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                    depth += 1;
                    i += 2;
                } else if bytes[i] == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            out.push(' ');
            continue;
        }
        // 字符串字面量（含转义）
        if b == b'"' {
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if bytes[i] == b'"' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            out.push_str("\"\"");
            continue;
        }
        // 字符字面量：区分 char 字面量与生命周期标注。char 字面量（含内含引号的 `'"'`）
        // 必须整体跳过，否则其中的 `"` 会被上面的字符串分支误判为字符串起点，吞掉后续真实代码。
        // 生命周期/标签（`'a` / `'static`，后不紧跟闭合 `'`）原样保留，不进入吞噬。
        if b == b'\'' {
            let is_char_lit = (i + 1 < bytes.len() && bytes[i + 1] == b'\\')
                || (i + 2 < bytes.len() && bytes[i + 2] == b'\'');
            if is_char_lit {
                i += 1; // 跳过开引号
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i += 2; // 跳过转义序列起始（\n / \' / \\ 等）
                        continue;
                    }
                    if bytes[i] == b'\'' {
                        i += 1; // 跳过闭引号
                        break;
                    }
                    i += 1;
                }
                out.push_str("''");
                continue;
            }
        }
        out.push(b as char);
        i += 1;
    }
    out
}

/// 统计 `fn` 关键字声明数：要求 `fn` 前为非标识符边界、后跟空白。
/// 覆盖 `pub fn` / `async fn` / `unsafe fn` / `fn`；不区分自由函数与方法（与源码侧
/// Function 节点含方法的口径一致）。
fn count_rust_fns(src: &str) -> usize {
    let bytes = src.as_bytes();
    let mut count = 0usize;
    let mut i = 0;
    while i + 2 <= bytes.len() {
        if bytes[i] == b'f' && bytes[i + 1] == b'n' {
            let before_ok = i == 0 || !is_ident_byte(bytes[i - 1]);
            let after_ok = i + 2 >= bytes.len() || bytes[i + 2].is_ascii_whitespace();
            if before_ok && after_ok {
                count += 1;
                i += 2;
                continue;
            }
        }
        i += 1;
    }
    count
}

fn is_ident_byte(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphanumeric()
}

/// 估算 Rust 控制流最大嵌套层级：按 `{}` 配平的块深度计，仅当块由控制流关键字
/// （`if`/`else`/`for`/`while`/`loop`/`match`）引导时才计入深度，普通块（函数体、
/// 结构体、模块）不计。返回出现过的最大控制流块深度。
///
/// 近似策略：扫描到控制流关键字后，下一个 `{` 视为开启一层控制流块；其余 `{`
/// 视为普通块（深度不变但需配平 `}`）。用栈记录每个 `{` 是否为控制流块。
fn brace_keyword_nesting(src: &str) -> usize {
    let bytes = src.as_bytes();
    let mut stack: Vec<bool> = Vec::new(); // true = 控制流块
    let mut cur_ctrl_depth = 0usize;
    let mut max = 0usize;
    let mut pending_ctrl = false; // 最近遇到控制流关键字，等待其 `{`
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if is_ident_byte(b) {
            // 读取一个标识符 token，判断是否控制流关键字
            let start = i;
            while i < bytes.len() && is_ident_byte(bytes[i]) {
                i += 1;
            }
            let word = &src[start..i];
            if matches!(word, "if" | "for" | "while" | "loop" | "match" | "else") {
                pending_ctrl = true;
            }
            continue;
        }
        match b {
            b'{' => {
                let is_ctrl = pending_ctrl;
                pending_ctrl = false;
                stack.push(is_ctrl);
                if is_ctrl {
                    cur_ctrl_depth += 1;
                    max = max.max(cur_ctrl_depth);
                }
            }
            b'}' => {
                if let Some(was_ctrl) = stack.pop() {
                    if was_ctrl {
                        cur_ctrl_depth = cur_ctrl_depth.saturating_sub(1);
                    }
                }
            }
            // `;` 会中断「关键字→`{`」的待定（如 `if cond { }` 之外的误判收敛）
            b';' => pending_ctrl = false,
            _ => {}
        }
        i += 1;
    }
    max
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn ratio_handles_zero_denominator() {
        let r = Ratio::new(0.0, 5.0);
        assert_eq!(r.ratio, None);
        let r2 = Ratio::new(2.0, 4.0);
        assert_eq!(r2.ratio, Some(2.0));
    }

    #[test]
    fn count_rust_fns_basic() {
        let src = "pub fn a() {}\nasync fn b() {}\nfn c() {}\nlet effn = 1;\n";
        // 三个 fn，effn 不算
        assert_eq!(count_rust_fns(src), 3);
    }

    #[test]
    fn strip_removes_fn_in_comments_and_strings() {
        let src = r#"
// fn commented() {}
fn real() {
    let s = "fn not_a_fn() {}";
    /* fn block_commented() {} */
}
"#;
        let stripped = strip_comments_and_strings(src);
        // 注释 / 字符串里的 fn 不应被计入，只剩 real
        assert_eq!(count_rust_fns(&stripped), 1);
    }

    #[test]
    fn strip_handles_char_literal_with_quote() {
        // char 字面量内含双引号：不应被误判为字符串起点而吞掉后续 fn。
        let src = r#"fn a() { let c = '"'; } fn b() {}"#;
        let stripped = strip_comments_and_strings(src);
        assert_eq!(count_rust_fns(&stripped), 2, "两个 fn 都应保留");
    }

    #[test]
    fn strip_handles_escaped_char_literal() {
        // 转义 char `'\''`（被转义的引号）不应破坏后续扫描。
        let src = r#"fn a() { let q = '\''; let s = "x"; } fn b() {}"#;
        let stripped = strip_comments_and_strings(src);
        assert_eq!(count_rust_fns(&stripped), 2);
    }

    #[test]
    fn strip_keeps_lifetimes() {
        // 生命周期 `'a`（非 char 字面量）应原样保留，不进入吞噬。
        let src = "fn f<'a>(x: &'a str) -> &'a str { x }";
        let stripped = strip_comments_and_strings(src);
        assert_eq!(count_rust_fns(&stripped), 1);
    }

    #[test]
    fn nested_block_comment_stripped() {
        let src = "fn a() { /* outer /* inner fn x() */ still */ let y = 1; }";
        let stripped = strip_comments_and_strings(src);
        assert_eq!(count_rust_fns(&stripped), 1);
    }

    #[test]
    fn brace_nesting_counts_only_control_flow() {
        // 函数体（普通块）不计；if 内嵌 for 计 2 层。
        let src = "fn f() { if a { for b { let x = 1; } } }";
        assert_eq!(brace_keyword_nesting(src), 2);
    }

    #[test]
    fn brace_nesting_plain_blocks_are_zero() {
        let src = "struct S { a: u32 } fn f() { let x = S { a: 1 }; }";
        assert_eq!(brace_keyword_nesting(src), 0);
    }

    #[test]
    fn brace_nesting_match_and_if() {
        let src = "fn f() { match x { _ => { if y { z(); } } } }";
        // match 1 层 + 其 arm 块（普通块不计）+ if 1 层 = 最大 2
        assert_eq!(brace_keyword_nesting(src), 2);
    }

    #[test]
    fn compare_missing_source_dir() {
        let err = compare_structure(
            Path::new("/tmp/不存在/src"),
            Path::new("/tmp/不存在/rust"),
            SourceLang::TypeScript,
        )
        .unwrap_err();
        assert!(matches!(err, MigrateError::FileNotFound(_)));
    }

    #[test]
    fn compare_ts_vs_rust_end_to_end() {
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        let rust_dir = dir.path().join("rust");
        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&rust_dir).unwrap();

        // 源码：2 个函数，最大嵌套 2（if 内 for）
        fs::write(
            src_dir.join("a.ts"),
            "export function f(x: number) {\n  if (x > 0) {\n    for (let i = 0; i < x; i++) {\n      console.log(i);\n    }\n  }\n}\nexport const g = () => {};\n",
        )
        .unwrap();
        // Rust：1 个函数，最大嵌套 1（if）
        fs::write(
            rust_dir.join("a.rs"),
            "pub fn f(x: i64) {\n    if x > 0 {\n        println!(\"{}\", x);\n    }\n}\n",
        )
        .unwrap();

        let report = compare_structure(&src_dir, &rust_dir, SourceLang::TypeScript).unwrap();
        assert_eq!(report.source.functions, 2, "源码应有 f + g 两个函数");
        assert_eq!(report.rust.functions, 1, "Rust 应有 1 个函数");
        assert_eq!(report.source.method, CountMethod::TreeSitter);
        assert_eq!(report.rust.method, CountMethod::LexicalScan);
        assert_eq!(report.source.max_nesting, 2, "if>for 嵌套 2 层");
        assert_eq!(report.rust.max_nesting, 1, "Rust 仅 if 一层");
        // 比值 = rust/source
        assert_eq!(report.function_ratio.ratio, Some(0.5));
        assert!(report.loc_ratio.source > 0.0 && report.loc_ratio.rust > 0.0);

        // JSON 形状契约
        let json = serde_json::to_value(&report).unwrap();
        assert!(json["source"]["functions"].is_number());
        assert!(json["function_ratio"]["ratio"].is_number());
        assert_eq!(json["rust"]["method"], "lexical-scan");
    }

    #[test]
    fn compare_structure_supports_python_source() {
        // M3-VAL-02 验收暴露的缺口修复：Python 源结构门（此前 stats compare 硬编码 TS、
        // 非 TS 直接报错 NotImplemented）。Python 控制流走 is_py_control_flow。
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        let rust_dir = dir.path().join("rust");
        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&rust_dir).unwrap();

        // Python 源：2 个函数，最大嵌套 2（for 内 if）。
        fs::write(
            src_dir.join("a.py"),
            "def f(x):\n    for i in range(x):\n        if i > 0:\n            print(i)\n\ndef g():\n    pass\n",
        )
        .unwrap();
        // Rust：1 个函数，最大嵌套 1。
        fs::write(
            rust_dir.join("a.rs"),
            "pub fn f(x: i64) {\n    for i in 0..x {\n        let _ = i;\n    }\n}\n",
        )
        .unwrap();

        let report = compare_structure(&src_dir, &rust_dir, SourceLang::Python).unwrap();
        assert_eq!(report.source.functions, 2, "Python 源应有 f + g 两个函数");
        assert_eq!(report.source.method, CountMethod::TreeSitter);
        assert_eq!(report.source.max_nesting, 2, "for>if 嵌套 2 层");
    }

    #[test]
    fn python_nesting_elif_try_dont_overcount() {
        // 审查关注点：elif/else 是 if_statement 的子句（非嵌套 if），不应加深度；
        // try/except 计一层（except_clause 非控制流节点）；空文件 → 0。
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("a.py"),
            "def h(x):\n    if x == 1:\n        pass\n    elif x == 2:\n        pass\n    else:\n        pass\n    try:\n        pass\n    except Exception:\n        pass\n    for i in x:\n        with open(i) as fp:\n            pass\n",
        )
        .unwrap();
        assert_eq!(
            source_max_nesting(dir.path(), SourceLang::Python).unwrap(),
            2,
            "for>with=2；elif/else 不应把 if 计成多层、try/except 仅一层"
        );

        // 空 Python 文件：无控制流，嵌套 0（解析成功但无控制流节点）。
        let empty = tempfile::tempdir().unwrap();
        fs::write(empty.path().join("e.py"), "").unwrap();
        assert_eq!(
            source_max_nesting(empty.path(), SourceLang::Python).unwrap(),
            0,
            "空 Python 文件嵌套 0"
        );
    }

    #[test]
    fn compare_excludes_test_files_consistently() {
        // 回归：函数计数（走 build_graph→adapter.can_handle）与嵌套深度
        // （走 collect_source_files）必须用同一份文件集——测试/声明文件两侧都排除。
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        let rust_dir = dir.path().join("rust");
        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&rust_dir).unwrap();

        // 真实源：1 函数，嵌套 1。
        fs::write(
            src_dir.join("a.ts"),
            "export function f(x: number) {\n  if (x > 0) { return x; }\n}\n",
        )
        .unwrap();
        // 测试文件：深嵌套（if>for>while），不应进入任何统计。
        fs::write(
            src_dir.join("a.test.ts"),
            "test('x', () => {\n  if (1) { for (;;) { while (1) { break; } } }\n});\n",
        )
        .unwrap();
        // 类型声明：也不应进入。
        fs::write(src_dir.join("a.d.ts"), "export declare const z: number;\n").unwrap();
        fs::write(rust_dir.join("a.rs"), "pub fn f(x: i64) -> i64 { x }\n").unwrap();

        let report = compare_structure(&src_dir, &rust_dir, SourceLang::TypeScript).unwrap();
        assert_eq!(
            report.source.functions, 1,
            "仅 a.ts 的 f，测试/声明文件排除"
        );
        assert_eq!(
            report.source.max_nesting, 1,
            "嵌套深度仅看 a.ts，测试文件的深嵌套不计入（口径与函数计数一致）"
        );
    }
}
