//! 文件指纹计算——三级变更检测。
//!
//! 参照 docs/design/04-toolchain.md § 5.7.5 增量更新策略：
//! - `content_hash`：文件内容 SHA256
//! - `structure_hash`：函数签名 + 类签名 + import 列表的 SHA256
//!
//! 三级变更：NONE（跳过）/ COSMETIC（仅更新 hash）/ STRUCTURAL（删旧重建）

use sha2::{Digest, Sha256};

use crate::lang::FileAnalysis;

/// 文件指纹记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFingerprint {
    /// 相对于项目根的文件路径。
    pub file_path: String,
    /// 文件内容 SHA256（hex）。
    pub content_hash: String,
    /// AST 结构签名 SHA256（hex）——函数签名 + 类签名 + import 列表。
    pub structure_hash: String,
}

/// 三级变更检测结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeLevel {
    /// 内容 hash 完全相同，跳过。
    None,
    /// 内容变了但结构 hash 不变——仅函数体内部修改。更新 hash，不重建图。
    Cosmetic,
    /// 结构 hash 变了——新增/删除函数、参数变化、导出状态变化。删旧重建。
    Structural,
}

/// 计算文件内容的 SHA256 哈希。
pub fn content_hash(source: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// 从 `FileAnalysis` 提取结构指纹并计算 SHA256。
///
/// 提取的签名信息包括：
/// 1. 节点签名：类型 + 名称 + 导出状态 + async
/// 2. import 列表：模块路径 + 符号名 + kind
/// 3. 导出名称集合
/// 4. 调用摘要：调用目标名（检测函数体内调用变更，避免 Calls 边过期）
///
/// 排序保证确定性（HashMap 遍历无序）。
pub fn structure_hash(analysis: &FileAnalysis) -> String {
    let mut hasher = Sha256::new();

    // 1. 节点签名（排序后写入保证确定性）
    let mut signatures: Vec<String> = Vec::new();
    for node in &analysis.nodes {
        // 跳过 File 节点，只提取符号级签名
        if node.node_type == crate::types::graph::NodeType::File {
            continue;
        }
        signatures.push(format!(
            "{}:{}:{}:{}",
            node.node_type, node.name, node.is_exported, node.is_async
        ));
    }
    signatures.sort();
    for sig in &signatures {
        hasher.update(sig.as_bytes());
        hasher.update(b"\n");
    }

    // 2. import 列表（排序后写入）
    let mut import_sigs: Vec<String> = Vec::new();
    for import in &analysis.imports {
        let mut sym_names: Vec<String> = import
            .symbols
            .iter()
            .map(|s| {
                if let Some(alias) = &s.alias {
                    format!("{} as {}", s.name, alias)
                } else {
                    s.name.clone()
                }
            })
            .collect();
        sym_names.sort();
        import_sigs.push(format!(
            "import:{}:{}:{:?}:re={}",
            import.module_path,
            sym_names.join(","),
            import.kind,
            import.reexport
        ));
    }
    import_sigs.sort();
    for sig in &import_sigs {
        hasher.update(sig.as_bytes());
        hasher.update(b"\n");
    }

    // 3. 导出名称集合
    let mut exported: Vec<&String> = analysis.exported_names.iter().collect();
    exported.sort();
    for name in &exported {
        hasher.update(b"export:");
        hasher.update(name.as_bytes());
        hasher.update(b"\n");
    }

    // 4. 调用摘要（函数体内调用目标变更 → STRUCTURAL，避免 Calls 边过期）
    let mut call_sigs: Vec<String> = analysis
        .calls
        .iter()
        .map(|c| format!("call:{}:{}", c.callee, c.is_constructor))
        .collect();
    call_sigs.sort();
    for sig in &call_sigs {
        hasher.update(sig.as_bytes());
        hasher.update(b"\n");
    }

    format!("{:x}", hasher.finalize())
}

/// 比较旧指纹和新指纹，返回变更级别。
pub fn detect_change(
    old: &FileFingerprint,
    new_content_hash: &str,
    new_structure_hash: &str,
) -> ChangeLevel {
    if old.content_hash == new_content_hash {
        ChangeLevel::None
    } else if old.structure_hash == new_structure_hash {
        ChangeLevel::Cosmetic
    } else {
        ChangeLevel::Structural
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::{FileAnalysis, ImportInfo, ImportKind, ImportedSymbol, SymbolKind};
    use crate::types::common::NodeId;
    use crate::types::graph::{NodeType, SourceNode};
    use std::collections::{HashMap, HashSet};

    fn make_analysis(
        nodes: Vec<SourceNode>,
        imports: Vec<ImportInfo>,
        exported: HashSet<String>,
    ) -> FileAnalysis {
        FileAnalysis {
            nodes,
            edges: Vec::new(),
            imports,
            calls: Vec::new(),
            exported_names: exported,
            constructor_bindings: HashMap::new(),
        }
    }

    fn make_fn_node(name: &str, exported: bool) -> SourceNode {
        let mut n = SourceNode::new(
            NodeId::new(format!("function:test.ts:{name}")),
            NodeType::Function,
            name.to_string(),
            "test.ts".to_string(),
        );
        n.is_exported = exported;
        n
    }

    #[test]
    fn content_hash_deterministic() {
        let h1 = content_hash("hello world");
        let h2 = content_hash("hello world");
        assert_eq!(h1, h2);
    }

    #[test]
    fn content_hash_differs_for_different_content() {
        let h1 = content_hash("hello");
        let h2 = content_hash("world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn structure_hash_deterministic() {
        let analysis = make_analysis(
            vec![make_fn_node("foo", true), make_fn_node("bar", false)],
            Vec::new(),
            HashSet::new(),
        );
        let h1 = structure_hash(&analysis);
        let h2 = structure_hash(&analysis);
        assert_eq!(h1, h2);
    }

    #[test]
    fn structure_hash_ignores_body_changes() {
        // 相同的节点签名 = 相同的 structure_hash
        let a1 = make_analysis(vec![make_fn_node("foo", true)], Vec::new(), HashSet::new());
        let a2 = make_analysis(vec![make_fn_node("foo", true)], Vec::new(), HashSet::new());
        assert_eq!(structure_hash(&a1), structure_hash(&a2));
    }

    #[test]
    fn structure_hash_changes_with_new_function() {
        let a1 = make_analysis(vec![make_fn_node("foo", true)], Vec::new(), HashSet::new());
        let a2 = make_analysis(
            vec![make_fn_node("foo", true), make_fn_node("bar", false)],
            Vec::new(),
            HashSet::new(),
        );
        assert_ne!(structure_hash(&a1), structure_hash(&a2));
    }

    #[test]
    fn structure_hash_changes_with_export_change() {
        let a1 = make_analysis(vec![make_fn_node("foo", true)], Vec::new(), HashSet::new());
        let a2 = make_analysis(vec![make_fn_node("foo", false)], Vec::new(), HashSet::new());
        assert_ne!(structure_hash(&a1), structure_hash(&a2));
    }

    #[test]
    fn structure_hash_changes_with_import_change() {
        let a1 = make_analysis(
            vec![],
            vec![ImportInfo {
                module_path: "./utils".to_string(),
                symbols: vec![ImportedSymbol {
                    name: "clamp".to_string(),
                    alias: None,
                    kind: SymbolKind::Named,
                }],
                kind: ImportKind::StaticValue,
                reexport: false,
            }],
            HashSet::new(),
        );
        let a2 = make_analysis(
            vec![],
            vec![ImportInfo {
                module_path: "./utils".to_string(),
                symbols: vec![
                    ImportedSymbol {
                        name: "clamp".to_string(),
                        alias: None,
                        kind: SymbolKind::Named,
                    },
                    ImportedSymbol {
                        name: "lerp".to_string(),
                        alias: None,
                        kind: SymbolKind::Named,
                    },
                ],
                kind: ImportKind::StaticValue,
                reexport: false,
            }],
            HashSet::new(),
        );
        assert_ne!(structure_hash(&a1), structure_hash(&a2));
    }

    #[test]
    fn detect_change_none() {
        let old = FileFingerprint {
            file_path: "test.ts".to_string(),
            content_hash: "abc".to_string(),
            structure_hash: "def".to_string(),
        };
        assert_eq!(detect_change(&old, "abc", "def"), ChangeLevel::None);
    }

    #[test]
    fn detect_change_cosmetic() {
        let old = FileFingerprint {
            file_path: "test.ts".to_string(),
            content_hash: "abc".to_string(),
            structure_hash: "def".to_string(),
        };
        assert_eq!(
            detect_change(&old, "changed_content", "def"),
            ChangeLevel::Cosmetic
        );
    }

    #[test]
    fn detect_change_structural() {
        let old = FileFingerprint {
            file_path: "test.ts".to_string(),
            content_hash: "abc".to_string(),
            structure_hash: "def".to_string(),
        };
        assert_eq!(
            detect_change(&old, "changed_content", "changed_structure"),
            ChangeLevel::Structural
        );
    }
}
