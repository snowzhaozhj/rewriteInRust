//! FFI binding 桩代码生成（已归档）。
//!
//! M3 决策（MDR-M3-FFI）：FFI 桥接取消，`degrade_skip` 为唯一降级路径。
//! 原因：模块级跨运行时桥接造成状态不同步、调试/部署复杂。
//! 翻不了的模块用 Rust crate 替代或标记 out-of-scope。
//!
//! `select_cycle_break_point` 和 `count_exports` 仍有价值，保留。

use std::fmt::Write as _;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{MigrateError, Result};

/// FFI 接口描述：一个导出函数的签名。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FfiInterface {
    /// 函数名。
    pub name: String,
    /// 参数列表：`(参数名, 类型名)`。
    pub params: Vec<(String, String)>,
    /// 返回类型（如 `"String"`、`"i32"`）。
    pub return_type: String,
}

#[deprecated(note = "M3 决策：FFI 桥接取消，degrade_skip 为唯一降级路径")]
/// 为降级 FFI 的模块生成 napi-rs binding 桩代码。
pub fn generate_ffi_binding(
    module_name: &str,
    interfaces: &[FfiInterface],
    target_dir: &Path,
) -> Result<String> {
    if module_name.is_empty() {
        return Err(MigrateError::Config("模块名不能为空".to_string()));
    }

    std::fs::create_dir_all(target_dir)?;

    let content = render_ffi_module(module_name, interfaces);
    let file_name = format!("{module_name}_ffi.rs");
    let file_path = target_dir.join(&file_name);
    std::fs::write(&file_path, &content)?;

    // 追加 napi 依赖到 Cargo.toml（如果存在且尚未包含）
    append_napi_deps(target_dir)?;

    Ok(file_name)
}

/// 渲染 FFI 模块的 Rust 源码内容。
fn render_ffi_module(module_name: &str, interfaces: &[FfiInterface]) -> String {
    let mut out = String::new();

    writeln!(out, "//! FFI binding 桩代码——模块 `{module_name}`。").unwrap();
    writeln!(out, "//!").unwrap();
    writeln!(out, "//! 由 rustmigrate 自动生成，降级 FFI 桥接层。").unwrap();
    writeln!(out, "//! 每个函数需手动接入源语言实现。").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "use napi_derive::napi;").unwrap();
    writeln!(out).unwrap();

    if interfaces.is_empty() {
        writeln!(out, "// 该模块无导出接口，无需 FFI 包装。").unwrap();
        return out;
    }

    for iface in interfaces {
        // 文档注释
        writeln!(
            out,
            "/// FFI 桥接：`{}`（来自模块 `{module_name}`）。",
            iface.name
        )
        .unwrap();
        writeln!(out, "#[napi]").unwrap();

        // 函数签名（参数名也做 sanitize + 关键字转义）
        let params_str = iface
            .params
            .iter()
            .map(|(name, ty)| {
                let safe_name = sanitize_ident(name);
                let safe_ty = if ty.is_empty() { "String" } else { ty.as_str() };
                format!("{safe_name}: {safe_ty}")
            })
            .collect::<Vec<_>>()
            .join(", ");

        let ret = if iface.return_type.is_empty() {
            "()"
        } else {
            &iface.return_type
        };

        writeln!(
            out,
            "pub fn {name}({params}) -> {ret} {{",
            name = sanitize_ident(&iface.name),
            params = params_str,
            ret = ret,
        )
        .unwrap();
        writeln!(out, "    // 来源: {module_name}::{name}", name = iface.name,).unwrap();
        writeln!(out, "    todo!(\"FFI: 调用源语言实现\")").unwrap();
        writeln!(out, "}}").unwrap();
        writeln!(out).unwrap();
    }

    out
}

/// Rust 严格关键字列表（需要 `r#` 前缀）。
const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while", "yield",
];

/// 将名称转换为合法的 Rust 标识符（snake_case），Rust 关键字用 `r#` 转义。
fn sanitize_ident(name: &str) -> String {
    let result: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();

    if result.is_empty() {
        return "_unnamed".to_string();
    }
    if result.starts_with(|c: char| c.is_ascii_digit()) {
        return format!("_{result}");
    }
    // Rust 关键字用 r# 前缀转义
    if RUST_KEYWORDS.contains(&result.as_str()) {
        return format!("r#{result}");
    }
    result
}

/// 追加 napi / napi-derive 依赖到 Cargo.toml（幂等）。
fn append_napi_deps(target_dir: &Path) -> Result<()> {
    let cargo_path = target_dir.join("Cargo.toml");
    if !cargo_path.exists() {
        // 没有 Cargo.toml 则跳过——scaffold 阶段会另行生成
        return Ok(());
    }

    let content = std::fs::read_to_string(&cargo_path)?;
    if content.contains("napi") {
        // 已包含 napi 依赖，跳过
        return Ok(());
    }

    let napi_deps = r#"
napi = { version = "2", features = ["napi4"] }
napi-derive = "2"
"#;

    // 在 [dependencies] 段末尾追加
    let new_content = if let Some(dep_pos) = content.find("[dependencies]") {
        // 找到 [dependencies] 段后的下一个段头或文件末尾
        let after_dep = &content[dep_pos..];
        let next_section = after_dep[1..]
            .find("\n[")
            .map(|p| dep_pos + 1 + p)
            .unwrap_or(content.len());
        let mut result = String::with_capacity(content.len() + napi_deps.len());
        result.push_str(&content[..next_section]);
        result.push_str(napi_deps);
        result.push_str(&content[next_section..]);
        result
    } else {
        // 没有 [dependencies] 段——追加到末尾
        format!("{content}\n[dependencies]{napi_deps}")
    };

    std::fs::write(&cargo_path, new_content)?;
    Ok(())
}

/// 在环依赖中选择最佳 FFI 降级候选。
///
/// 策略：选导出接口最少的模块（FFI binding 代价最低）。
/// 导出接口数 = 该文件节点 `Exports` 出边的数量。
///
/// 若环为空则 panic（调用方应保证环至少 2 个节点）。
pub fn select_cycle_break_point(
    cycle: &[crate::types::common::NodeId],
    graph: &crate::graph::SourceGraph,
) -> crate::types::common::NodeId {
    use crate::types::graph::EdgeType;

    assert!(!cycle.is_empty(), "环不能为空");

    cycle
        .iter()
        .min_by_key(|node_id| {
            // 计算该节点的 Exports 出边数量
            graph
                .outgoing_edges(node_id)
                .iter()
                .filter(|(_, et)| *et == EdgeType::Exports)
                .count()
        })
        .expect("环不能为空")
        .clone()
}

/// 计算指定节点的导出接口数（`Exports` 出边数量）。
///
/// 辅助函数，供外部（如降级报告）获取导出数。
pub fn count_exports(
    node_id: &crate::types::common::NodeId,
    graph: &crate::graph::SourceGraph,
) -> usize {
    use crate::types::graph::EdgeType;

    graph
        .outgoing_edges(node_id)
        .iter()
        .filter(|(_, et)| *et == EdgeType::Exports)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_interfaces() -> Vec<FfiInterface> {
        vec![
            FfiInterface {
                name: "greet".to_string(),
                params: vec![("name".to_string(), "String".to_string())],
                return_type: "String".to_string(),
            },
            FfiInterface {
                name: "add".to_string(),
                params: vec![
                    ("a".to_string(), "i32".to_string()),
                    ("b".to_string(), "i32".to_string()),
                ],
                return_type: "i32".to_string(),
            },
        ]
    }

    #[test]
    fn test_generate_ffi_binding_basic() {
        let tmp = TempDir::new().unwrap();
        let interfaces = sample_interfaces();

        let file_name = generate_ffi_binding("utils", &interfaces, tmp.path()).unwrap();

        assert_eq!(file_name, "utils_ffi.rs");
        let content = std::fs::read_to_string(tmp.path().join("utils_ffi.rs")).unwrap();

        // 检查 napi 导入
        assert!(content.contains("use napi_derive::napi;"));
        // 检查 #[napi] 标注
        assert!(content.contains("#[napi]"));
        // 检查函数签名
        assert!(content.contains("pub fn greet(name: String) -> String"));
        assert!(content.contains("pub fn add(a: i32, b: i32) -> i32"));
        // 检查 todo! 占位
        assert!(content.contains("todo!(\"FFI: 调用源语言实现\")"));
        // 检查来源注释
        assert!(content.contains("// 来源: utils::greet"));
    }

    #[test]
    fn test_generate_ffi_binding_empty_interfaces() {
        let tmp = TempDir::new().unwrap();

        let file_name = generate_ffi_binding("empty_mod", &[], tmp.path()).unwrap();

        assert_eq!(file_name, "empty_mod_ffi.rs");
        let content = std::fs::read_to_string(tmp.path().join("empty_mod_ffi.rs")).unwrap();
        assert!(content.contains("无导出接口"));
    }

    #[test]
    fn test_generate_ffi_binding_empty_name() {
        let tmp = TempDir::new().unwrap();
        let result = generate_ffi_binding("", &[], tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_sanitize_ident() {
        assert_eq!(sanitize_ident("hello"), "hello");
        assert_eq!(sanitize_ident("my-func"), "my_func");
        assert_eq!(sanitize_ident("123abc"), "_123abc");
        assert_eq!(sanitize_ident(""), "_unnamed");
        assert_eq!(sanitize_ident("a.b.c"), "a_b_c");
    }

    #[test]
    fn test_append_napi_deps_no_cargo_toml() {
        let tmp = TempDir::new().unwrap();
        // 没有 Cargo.toml 时应该静默跳过
        append_napi_deps(tmp.path()).unwrap();
        assert!(!tmp.path().join("Cargo.toml").exists());
    }

    #[test]
    fn test_append_napi_deps_idempotent() {
        let tmp = TempDir::new().unwrap();
        let cargo_path = tmp.path().join("Cargo.toml");
        std::fs::write(
            &cargo_path,
            "[package]\nname = \"test\"\n\n[dependencies]\nserde = \"1\"\n",
        )
        .unwrap();

        // 第一次追加
        append_napi_deps(tmp.path()).unwrap();
        let content = std::fs::read_to_string(&cargo_path).unwrap();
        assert!(content.contains("napi"));
        assert!(content.contains("napi-derive"));

        // 第二次应该跳过（幂等）
        let before = content.clone();
        append_napi_deps(tmp.path()).unwrap();
        let after = std::fs::read_to_string(&cargo_path).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn test_render_ffi_module_content() {
        let content = render_ffi_module("mymod", &sample_interfaces());

        // 模块文档
        assert!(content.contains("模块 `mymod`"));
        // 函数数量
        assert_eq!(content.matches("#[napi]").count(), 2);
        // 第一个函数
        assert!(content.contains("pub fn greet(name: String) -> String"));
        // 第二个函数
        assert!(content.contains("pub fn add(a: i32, b: i32) -> i32"));
    }

    // --- 环断点选择测试 ---

    use crate::graph::SourceGraph;
    use crate::types::common::NodeId;
    use crate::types::graph::{Dependency, EdgeType, NodeType, SourceNode};

    fn make_file_node(path: &str) -> SourceNode {
        SourceNode::new(
            NodeId::file(path),
            NodeType::File,
            path.to_string(),
            path.to_string(),
        )
    }

    fn make_func_node(file: &str, name: &str) -> SourceNode {
        let mut node = SourceNode::new(
            NodeId::symbol(NodeType::Function, file, name),
            NodeType::Function,
            name.to_string(),
            file.to_string(),
        );
        node.is_exported = true;
        node
    }

    #[test]
    fn test_select_cycle_break_point_fewest_exports() {
        let mut graph = SourceGraph::new();

        // 文件 A：3 个导出
        let a_id = NodeId::file("a.ts");
        graph.add_node(make_file_node("a.ts"));
        for name in &["foo", "bar", "baz"] {
            graph.add_node(make_func_node("a.ts", name));
            graph.add_edge(Dependency::new(
                a_id.clone(),
                NodeId::symbol(NodeType::Function, "a.ts", name),
                EdgeType::Exports,
            ));
        }

        // 文件 B：1 个导出（应被选中）
        let b_id = NodeId::file("b.ts");
        graph.add_node(make_file_node("b.ts"));
        graph.add_node(make_func_node("b.ts", "only"));
        graph.add_edge(Dependency::new(
            b_id.clone(),
            NodeId::symbol(NodeType::Function, "b.ts", "only"),
            EdgeType::Exports,
        ));

        // 文件 C：2 个导出
        let c_id = NodeId::file("c.ts");
        graph.add_node(make_file_node("c.ts"));
        for name in &["x", "y"] {
            graph.add_node(make_func_node("c.ts", name));
            graph.add_edge(Dependency::new(
                c_id.clone(),
                NodeId::symbol(NodeType::Function, "c.ts", name),
                EdgeType::Exports,
            ));
        }

        let cycle = vec![a_id, b_id.clone(), c_id];
        let selected = select_cycle_break_point(&cycle, &graph);
        assert_eq!(selected, b_id, "应选导出最少的模块 b.ts");
    }

    #[test]
    fn test_select_cycle_break_point_no_exports() {
        let mut graph = SourceGraph::new();

        // 两个文件都没有 Exports 边
        graph.add_node(make_file_node("a.ts"));
        graph.add_node(make_file_node("b.ts"));

        let a_id = NodeId::file("a.ts");
        let b_id = NodeId::file("b.ts");
        let cycle = vec![a_id.clone(), b_id];
        let selected = select_cycle_break_point(&cycle, &graph);
        // 两者导出数都是 0，应选第一个（min_by_key 稳定）
        assert_eq!(selected, a_id);
    }

    #[test]
    fn test_count_exports() {
        let mut graph = SourceGraph::new();
        let file_id = NodeId::file("mod.ts");
        graph.add_node(make_file_node("mod.ts"));
        graph.add_node(make_func_node("mod.ts", "alpha"));
        graph.add_node(make_func_node("mod.ts", "beta"));
        graph.add_edge(Dependency::new(
            file_id.clone(),
            NodeId::symbol(NodeType::Function, "mod.ts", "alpha"),
            EdgeType::Exports,
        ));
        graph.add_edge(Dependency::new(
            file_id.clone(),
            NodeId::symbol(NodeType::Function, "mod.ts", "beta"),
            EdgeType::Exports,
        ));

        assert_eq!(count_exports(&file_id, &graph), 2);

        // 不存在的节点返回 0
        assert_eq!(count_exports(&NodeId::file("nonexistent.ts"), &graph), 0);
    }
}
