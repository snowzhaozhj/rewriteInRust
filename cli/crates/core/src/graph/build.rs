//! 源码图构建——语言无关。
//!
//! 遍历项目目录，通过 `LanguageAdapter` trait 分析每个文件，
//! 组装成完整的 `SourceGraph`。不依赖任何特定语言实现。

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::Serialize;

use crate::error::{MigrateError, Result};
use crate::lang::{FileAnalysis, LanguageAdapter, SymbolKind};
use crate::types::common::{NodeId, EXCLUDED_DIRS};
use crate::types::graph::{Dependency, EdgeSubKind, EdgeType, NodeType};

use super::fingerprint::{self, ChangeLevel, FileFingerprint};
use super::persist;
use super::SourceGraph;

/// `graph build --profile` 输出的性能画像。
///
/// 字段对齐设计文档 04-toolchain.md § 5.7.4.1（当前阶段仅含已实现的计时项；
/// 社区检测 batch_count/batch_sizes 和 memory_peak_mb 待 M2 补充）。
#[derive(Debug, Clone, Serialize)]
pub struct BuildProfile {
    /// 文件扫描 + AST 解析耗时（毫秒）。
    pub parse_ms: u64,
    /// 边构建耗时（extends 修正 + 跨文件 imports/calls 解析，毫秒）。
    pub edge_build_ms: u64,
    /// 总耗时（解析 + 边构建，不含持久化，毫秒）。
    pub total_ms: u64,
}

/// 增量构建统计。
#[derive(Debug, Clone, Serialize)]
pub struct IncrementalStats {
    /// 跳过的文件数（NONE 级别）。
    pub skipped: usize,
    /// 仅更新 hash 的文件数（COSMETIC 级别）。
    pub cosmetic: usize,
    /// 需重建的文件数（STRUCTURAL 级别）。
    pub structural: usize,
    /// 传递性更新波及的额外文件数。
    pub transitive: usize,
    /// 新增的文件数。
    pub new_files: usize,
    /// 已删除的文件数（磁盘不再存在）。
    pub deleted: usize,
    /// 是否发生了熔断截断。
    pub truncated: bool,
    /// 是否实际执行了增量（false 表示退化为全量——如 db 不存在）。
    pub incremental: bool,
}

/// 从项目根目录构建源码图（内部实现，可选计时插桩）。
///
/// `profile` 为 `true` 时在各阶段记录耗时，填充返回的 `BuildProfile`。
fn build_graph_inner(
    root: &Path,
    adapters: &mut [Box<dyn LanguageAdapter>],
    profile: bool,
) -> Result<(SourceGraph, BuildProfile, Vec<FileFingerprint>)> {
    let t_start = if profile { Some(Instant::now()) } else { None };

    let root = root
        .canonicalize()
        .map_err(|_| MigrateError::FileNotFound(root.to_path_buf()))?;

    let files = collect_source_files(&root, adapters)?;
    if files.is_empty() {
        let total_ms = t_start.map_or(0, |t| t.elapsed().as_millis() as u64);
        return Ok((
            SourceGraph::new(),
            BuildProfile {
                parse_ms: total_ms,
                edge_build_ms: 0,
                total_ms,
            },
            Vec::new(),
        ));
    }

    let mut graph = SourceGraph::new();
    let mut file_analyses: HashMap<String, FileAnalysis> = HashMap::new();
    let mut all_edges: Vec<Dependency> = Vec::new();
    let mut fingerprints: Vec<FileFingerprint> = Vec::new();

    // 第一遍：添加所有节点，收集所有边（解析阶段），同时计算指纹
    for (file_path, adapter_idx) in &files {
        let rel = make_relative(file_path, &root);
        let source = std::fs::read_to_string(file_path).map_err(MigrateError::Io)?;

        let analysis = match adapters[*adapter_idx].analyze_file(&source, &rel) {
            Ok(a) => a,
            Err(MigrateError::Parse { .. }) => {
                graph
                    .warnings
                    .push(format!("解析跳过 {rel}: tree-sitter 解析失败"));
                // 解析失败时 structure_hash = content_hash（保守：任何变更都视为 STRUCTURAL）
                let ch = fingerprint::content_hash(&source);
                fingerprints.push(FileFingerprint {
                    file_path: rel,
                    content_hash: ch.clone(),
                    structure_hash: ch,
                });
                continue;
            }
            Err(e) => return Err(e),
        };

        // 顺带计算指纹（source 和 analysis 已在手上，零额外 I/O）
        let ch = fingerprint::content_hash(&source);
        let sh = fingerprint::structure_hash(&analysis);
        fingerprints.push(FileFingerprint {
            file_path: rel.clone(),
            content_hash: ch,
            structure_hash: sh,
        });

        for node in &analysis.nodes {
            graph.add_node(node.clone());
        }
        all_edges.extend(analysis.edges.iter().cloned());

        file_analyses.insert(rel, analysis);
    }
    let parse_ms = t_start.map_or(0, |t| t.elapsed().as_millis() as u64);

    // 边构建阶段
    let t_edge = if profile { Some(Instant::now()) } else { None };

    // 修正 extends 边的目标 ID（跨文件查找），然后添加所有边
    let fixed_edges = fixup_extends_in_edges(&graph, all_edges);
    for edge in &fixed_edges {
        graph.add_edge(edge.clone());
    }

    // 收集所有 adapter 的解析扩展名（去重 + 排序保证确定性）
    let mut resolve_exts: Vec<&str> = adapters
        .iter()
        .flat_map(|a| a.resolve_extensions().iter().copied())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    resolve_exts.sort();

    // 构建跨文件边（Imports + Calls）
    let file_set: HashSet<String> = files.iter().map(|(p, _)| make_relative(p, &root)).collect();
    add_cross_file_edges(&mut graph, &file_analyses, &file_set, &resolve_exts);

    let edge_build_ms = t_edge.map_or(0, |t| t.elapsed().as_millis() as u64);
    let total_ms = t_start.map_or(0, |t| t.elapsed().as_millis() as u64);

    Ok((
        graph,
        BuildProfile {
            parse_ms,
            edge_build_ms,
            total_ms,
        },
        fingerprints,
    ))
}

/// 从项目根目录构建源码图。
///
/// `adapters` 是语言适配器列表，每个文件会尝试匹配第一个能处理它的适配器。
pub fn build_graph(root: &Path, adapters: &mut [Box<dyn LanguageAdapter>]) -> Result<SourceGraph> {
    build_graph_inner(root, adapters, false).map(|(graph, _, _)| graph)
}

/// 便捷函数：用默认 TypeScript adapter 构建图。
pub fn build_graph_ts(root: &Path) -> Result<SourceGraph> {
    let mut adapters: Vec<Box<dyn LanguageAdapter>> =
        vec![Box::new(crate::lang::typescript::TypeScriptAdapter::new()?)];
    build_graph(root, &mut adapters)
}

/// 带性能画像的图构建：返回 `(SourceGraph, BuildProfile)`。
///
/// 逻辑与 [`build_graph`] 共享 [`build_graph_inner`]，仅额外开启各阶段 `Instant` 计时。
/// 指纹由 `build_graph_inner` 内部顺带计算但此处丢弃（调用方不需要）。
pub fn build_graph_profiled(
    root: &Path,
    adapters: &mut [Box<dyn LanguageAdapter>],
) -> Result<(SourceGraph, BuildProfile)> {
    build_graph_inner(root, adapters, true).map(|(g, bp, _)| (g, bp))
}

/// 便捷函数：用默认 TypeScript adapter 构建图（带性能画像）。
pub fn build_graph_ts_profiled(root: &Path) -> Result<(SourceGraph, BuildProfile)> {
    let mut adapters: Vec<Box<dyn LanguageAdapter>> =
        vec![Box::new(crate::lang::typescript::TypeScriptAdapter::new()?)];
    build_graph_profiled(root, &mut adapters)
}

/// 便捷函数：全量构建 + 返回指纹（CLI 全量路径用，一次遍历同时产出图和指纹）。
pub fn build_graph_ts_full(
    root: &Path,
    profile: bool,
) -> Result<(SourceGraph, BuildProfile, Vec<FileFingerprint>)> {
    let mut adapters: Vec<Box<dyn LanguageAdapter>> =
        vec![Box::new(crate::lang::typescript::TypeScriptAdapter::new()?)];
    build_graph_inner(root, &mut adapters, profile)
}

// === 增量构建 ===

/// 反向 BFS 最大深度（设计文档 § 5.7.5）。
const MAX_REVERSE_BFS_DEPTH: usize = 3;
/// 反向 BFS 熔断阈值（设计文档 § 5.7.5）。
const FUSE_THRESHOLD: usize = 50;

/// 增量图构建：利用 file_fingerprints 跳过未变更文件。
///
/// 工作流程：
/// 1. 从 DB 加载已有指纹和图
/// 2. 扫描磁盘文件，计算当前 content_hash
/// 3. 三级变更检测（NONE/COSMETIC/STRUCTURAL）
/// 4. STRUCTURAL 文件触发反向 BFS 传递性更新
/// 5. 仅重新解析变更文件，合并到已有图
/// 6. 增量保存到 DB
///
/// DB 不存在时退化为全量构建。
pub fn build_graph_incremental(
    root: &Path,
    db_path: &Path,
    adapters: &mut [Box<dyn LanguageAdapter>],
    profile: bool,
) -> Result<(SourceGraph, BuildProfile, IncrementalStats)> {
    let t_start = if profile { Some(Instant::now()) } else { None };

    let root = root
        .canonicalize()
        .map_err(|_| MigrateError::FileNotFound(root.to_path_buf()))?;

    // DB 不存在：退化为全量构建
    if !db_path.exists() {
        let (graph, bp, fps) = build_graph_inner(&root, adapters, profile)?;
        let file_count = graph.file_nodes().len();

        // 指纹由 build_graph_inner 一次遍历顺带计算，直接保存
        std::fs::create_dir_all(db_path.parent().unwrap_or(Path::new(".")))?;
        persist::save_to_db(&graph, db_path)?;
        persist::save_fingerprints(db_path, &fps)?;

        return Ok((
            graph,
            bp,
            IncrementalStats {
                skipped: 0,
                cosmetic: 0,
                structural: 0,
                transitive: 0,
                new_files: file_count,
                deleted: 0,
                truncated: false,
                incremental: false,
            },
        ));
    }

    // 加载已有指纹
    let old_fps: HashMap<String, FileFingerprint> = persist::load_fingerprints(db_path)?
        .into_iter()
        .map(|fp| (fp.file_path.clone(), fp))
        .collect();

    // 加载已有图（从 DB）
    let mut graph = persist::load_from_db(db_path)?;

    // 扫描磁盘文件
    let files = collect_source_files(&root, adapters)?;
    let current_rels: HashSet<String> =
        files.iter().map(|(p, _)| make_relative(p, &root)).collect();

    // 找出已删除文件（指纹中有但磁盘上不存在）
    let deleted_files: Vec<String> = old_fps
        .keys()
        .filter(|fp| !current_rels.contains(fp.as_str()))
        .cloned()
        .collect();

    // 删除已删除文件的节点
    for del in &deleted_files {
        graph.remove_nodes_by_file(del);
    }

    // 逐文件三级变更检测
    let mut none_files: Vec<String> = Vec::new();
    let mut cosmetic_files: Vec<String> = Vec::new();
    let mut structural_files: Vec<String> = Vec::new();
    let mut new_files: Vec<String> = Vec::new();
    // 记录每个待分析文件的 (rel_path, file_path, adapter_idx, content_hash, source)
    let mut to_analyze: Vec<(String, PathBuf, usize, String, String)> = Vec::new();

    for (file_path, adapter_idx) in &files {
        let rel = make_relative(file_path, &root);
        let source = std::fs::read_to_string(file_path).map_err(MigrateError::Io)?;
        let ch = fingerprint::content_hash(&source);

        if let Some(old_fp) = old_fps.get(&rel) {
            if old_fp.content_hash == ch {
                // NONE：完全跳过
                none_files.push(rel);
                continue;
            }
            // content_hash 不同——需要解析才能判断 COSMETIC vs STRUCTURAL
            to_analyze.push((rel, file_path.clone(), *adapter_idx, ch, source));
        } else {
            // 新文件
            new_files.push(rel.clone());
            to_analyze.push((rel, file_path.clone(), *adapter_idx, ch, source));
        }
    }

    // 解析变更文件，判断 COSMETIC vs STRUCTURAL
    let mut file_analyses: HashMap<String, FileAnalysis> = HashMap::new();
    let mut new_fingerprints: Vec<FileFingerprint> = Vec::new();

    for (rel, _file_path, adapter_idx, ch, source) in &to_analyze {
        let analysis = match adapters[*adapter_idx].analyze_file(source, rel) {
            Ok(a) => a,
            Err(MigrateError::Parse { .. }) => {
                graph
                    .warnings
                    .push(format!("解析跳过 {rel}: tree-sitter 解析失败"));
                continue;
            }
            Err(e) => return Err(e),
        };

        let sh = fingerprint::structure_hash(&analysis);

        if let Some(old_fp) = old_fps.get(rel.as_str()) {
            let change = fingerprint::detect_change(old_fp, ch, &sh);
            match change {
                ChangeLevel::None => {
                    // 不应到这里（已在上面过滤了），但保险起见
                    none_files.push(rel.clone());
                }
                ChangeLevel::Cosmetic => {
                    cosmetic_files.push(rel.clone());
                }
                ChangeLevel::Structural => {
                    structural_files.push(rel.clone());
                    file_analyses.insert(rel.clone(), analysis);
                }
            }
        } else {
            // 新文件总是 STRUCTURAL
            file_analyses.insert(rel.clone(), analysis);
        }

        new_fingerprints.push(FileFingerprint {
            file_path: rel.clone(),
            content_hash: ch.clone(),
            structure_hash: sh,
        });
    }

    // 传递性更新：STRUCTURAL 文件的导入者也需重分析
    let structural_set: HashSet<String> = structural_files
        .iter()
        .chain(new_files.iter())
        .cloned()
        .collect();
    let (transitive_files, truncated) = compute_transitive_updates(&graph, &structural_set);

    if truncated {
        graph.warnings.push(
            "传递性更新触发熔断截断（> 50 个文件），仅更新直接导入者。建议运行 `graph build --full` 做一次全量构建。"
                .to_string(),
        );
    }

    // 传递性波及的文件也需重分析（但排除已在变更列表中的）
    let mut transitive_to_analyze: Vec<String> = Vec::new();
    for trans_file in &transitive_files {
        if structural_set.contains(trans_file) || cosmetic_files.contains(trans_file) {
            continue;
        }
        transitive_to_analyze.push(trans_file.clone());
    }

    // 解析传递性波及文件
    for rel in &transitive_to_analyze {
        let file_path = root.join(rel);
        if !file_path.exists() {
            continue;
        }
        let source = std::fs::read_to_string(&file_path).map_err(MigrateError::Io)?;
        let adapter_idx = match adapters.iter().position(|a| a.can_handle(&file_path)) {
            Some(idx) => idx,
            None => continue,
        };
        let analysis = match adapters[adapter_idx].analyze_file(&source, rel) {
            Ok(a) => a,
            Err(MigrateError::Parse { .. }) => {
                graph
                    .warnings
                    .push(format!("解析跳过 {rel}: tree-sitter 解析失败"));
                continue;
            }
            Err(e) => return Err(e),
        };

        let ch = fingerprint::content_hash(&source);
        let sh = fingerprint::structure_hash(&analysis);

        file_analyses.insert(rel.clone(), analysis);
        new_fingerprints.push(FileFingerprint {
            file_path: rel.clone(),
            content_hash: ch,
            structure_hash: sh,
        });
    }

    // 删除需重建文件的旧节点
    let rebuild_files: HashSet<&str> = structural_files
        .iter()
        .chain(new_files.iter())
        .chain(transitive_to_analyze.iter())
        .map(String::as_str)
        .collect();
    for file in &rebuild_files {
        graph.remove_nodes_by_file(file);
    }

    let parse_ms = t_start.map_or(0, |t| t.elapsed().as_millis() as u64);

    // 边构建阶段
    let t_edge = if profile { Some(Instant::now()) } else { None };

    // 添加重建文件的新节点和内部边
    let mut all_new_edges: Vec<Dependency> = Vec::new();
    for (rel, analysis) in &file_analyses {
        let _ = rel;
        for node in &analysis.nodes {
            graph.add_node(node.clone());
        }
        all_new_edges.extend(analysis.edges.iter().cloned());
    }

    // 修正 extends 边并添加
    let fixed_edges = fixup_extends_in_edges(&graph, all_new_edges);
    for edge in &fixed_edges {
        graph.add_edge(edge.clone());
    }

    // 重建跨文件边——只针对有变更的文件
    let mut resolve_exts: Vec<&str> = adapters
        .iter()
        .flat_map(|a| a.resolve_extensions().iter().copied())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    resolve_exts.sort();

    // 需要为变更文件重建跨文件边。先删除旧的跨文件出边，再重建。
    // 但由于 graph 已经 remove_nodes_by_file 了，旧的跨文件边已被删除。
    // 现在用 file_analyses 来重建。
    let file_set: HashSet<String> = current_rels.clone();
    add_cross_file_edges(&mut graph, &file_analyses, &file_set, &resolve_exts);

    let edge_build_ms = t_edge.map_or(0, |t| t.elapsed().as_millis() as u64);
    let total_ms = t_start.map_or(0, |t| t.elapsed().as_millis() as u64);

    // 增量保存到 DB
    let changed_files: Vec<String> = rebuild_files.iter().map(|s| s.to_string()).collect();
    persist::save_incremental(
        &graph,
        db_path,
        &new_fingerprints,
        &changed_files,
        truncated,
    )?;

    // 清理已删除文件的指纹
    if !deleted_files.is_empty() {
        persist::remove_stale_fingerprints(db_path, &deleted_files)?;
    }

    // 为 COSMETIC 文件更新 content_hash（structure_hash 不变，不影响图）
    if !cosmetic_files.is_empty() {
        let cosmetic_fps: Vec<FileFingerprint> = cosmetic_files
            .iter()
            .filter_map(|rel| {
                new_fingerprints
                    .iter()
                    .find(|fp| &fp.file_path == rel)
                    .cloned()
            })
            .collect();
        if !cosmetic_fps.is_empty() {
            persist::save_fingerprints_update(db_path, &cosmetic_fps)?;
        }
    }

    let stats = IncrementalStats {
        skipped: none_files.len(),
        cosmetic: cosmetic_files.len(),
        structural: structural_files.len(),
        transitive: transitive_to_analyze.len(),
        new_files: new_files.len(),
        deleted: deleted_files.len(),
        truncated,
        incremental: true,
    };

    Ok((
        graph,
        BuildProfile {
            parse_ms,
            edge_build_ms,
            total_ms,
        },
        stats,
    ))
}

/// 便捷函数：增量构建（TypeScript adapter）。
pub fn build_graph_ts_incremental(
    root: &Path,
    db_path: &Path,
    profile: bool,
) -> Result<(SourceGraph, BuildProfile, IncrementalStats)> {
    let mut adapters: Vec<Box<dyn LanguageAdapter>> =
        vec![Box::new(crate::lang::typescript::TypeScriptAdapter::new()?)];
    build_graph_incremental(root, db_path, &mut adapters, profile)
}

/// 传递性更新：找出 STRUCTURAL 变更文件的所有（反向 BFS）导入者。
///
/// 返回 (需重分析的文件集合, 是否触发了熔断截断)。
fn compute_transitive_updates(
    graph: &SourceGraph,
    structural_files: &HashSet<String>,
) -> (HashSet<String>, bool) {
    let mut visited: HashSet<String> = HashSet::new();
    let mut truncated = false;

    for start_file in structural_files {
        let file_id = NodeId::file(start_file);
        if graph.node_index(&file_id).is_none() {
            continue;
        }

        // BFS 反向沿 imports 边
        let mut queue: VecDeque<(NodeId, usize)> = VecDeque::new();
        queue.push_back((file_id.clone(), 0));
        let mut local_visited: HashSet<String> = HashSet::new();
        local_visited.insert(start_file.clone());

        while let Some((current_id, depth)) = queue.pop_front() {
            if depth >= MAX_REVERSE_BFS_DEPTH {
                continue;
            }

            // 找到所有导入 current_id 的文件（反向 imports 边）
            let importers = graph.incoming_edges(&current_id);
            for (importer_node, edge_type) in importers {
                if edge_type != EdgeType::Imports {
                    continue;
                }
                let importer_file = &importer_node.file_path;
                if local_visited.contains(importer_file) {
                    continue;
                }
                local_visited.insert(importer_file.clone());
                visited.insert(importer_file.clone());

                // 熔断检查
                if visited.len() > FUSE_THRESHOLD {
                    truncated = true;
                    // 截断为仅直接导入者：清空队列，不再深入
                    queue.clear();
                    break;
                }

                queue.push_back((importer_node.id.clone(), depth + 1));
            }

            if truncated {
                break;
            }
        }

        if truncated {
            break;
        }
    }

    (visited, truncated)
}

/// 收集所有可被适配器处理的源文件，返回 (路径, 适配器索引)。
fn collect_source_files(
    root: &Path,
    adapters: &[Box<dyn LanguageAdapter>],
) -> Result<Vec<(PathBuf, usize)>> {
    let mut files = Vec::new();
    collect_recursive(root, adapters, &mut files)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

fn collect_recursive(
    dir: &Path,
    adapters: &[Box<dyn LanguageAdapter>],
    files: &mut Vec<(PathBuf, usize)>,
) -> Result<()> {
    let entries = std::fs::read_dir(dir).map_err(MigrateError::Io)?;
    for entry in entries {
        let entry = entry.map_err(MigrateError::Io)?;
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if EXCLUDED_DIRS.contains(&name.as_ref()) {
                continue;
            }
            collect_recursive(&path, adapters, files)?;
        } else if let Some(idx) = adapters.iter().position(|a| a.can_handle(&path)) {
            files.push((path, idx));
        }
    }
    Ok(())
}

fn make_relative(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// 修正 extends 边：适配器用当前文件路径作为目标前缀，实际目标可能在其他文件。
/// 在边添加到图之前调用（因为 add_edge 会丢弃目标不存在的边）。
fn fixup_extends_in_edges(graph: &SourceGraph, mut edges: Vec<Dependency>) -> Vec<Dependency> {
    // 预建「类型名 → 候选继承目标节点」索引，避免对每条未解析 extends 边都做 O(N)
    // 全图扫描（原 find_unique_node 逐边线性查找，整体 O(N·E)）。仅收录可作为
    // extends/implements 目标的类型（Interface/Class/Enum）。
    let mut heritage_index: HashMap<&str, Vec<&NodeId>> = HashMap::new();
    for node in graph.nodes() {
        if matches!(
            node.node_type,
            NodeType::Interface | NodeType::Class | NodeType::Enum
        ) {
            heritage_index
                .entry(node.name.as_str())
                .or_default()
                .push(&node.id);
        }
    }

    for edge in &mut edges {
        if edge.edge_type != EdgeType::Extends {
            continue;
        }
        if graph.node_index(&edge.target).is_some() {
            continue;
        }
        let Some(rel) = edge.target.file_path().map(|s| s.to_owned()) else {
            continue;
        };
        let Some(name) = edge.target.symbol_name().map(|s| s.to_owned()) else {
            continue;
        };

        // 1. 同文件内查找备选类型前缀
        let candidates = [
            NodeId::symbol(NodeType::Class, &rel, &name),
            NodeId::symbol(NodeType::Enum, &rel, &name),
        ];
        let mut resolved = false;
        for candidate in &candidates {
            if graph.node_index(candidate).is_some() {
                edge.target = candidate.clone();
                resolved = true;
                break;
            }
        }
        // 2. 跨文件按名称搜索：仅在全图唯一命中时绑定。
        //    命中多个同名类型则放弃（保持目标为占位 ID，add_edge 会丢弃该边），
        //    避免把 Extends 边连到同名但错误文件的类型。
        if !resolved {
            if let Some([target]) = heritage_index.get(name.as_str()).map(Vec::as_slice) {
                edge.target = (*target).clone();
            }
        }
    }
    edges
}

/// 剥离 callee 的导入基名前缀，得到目标模块内的符号名。
/// `ns.clamp`（base=`ns`）→ `clamp`；`fn`（base=`fn`）→ `fn`。
fn cross_symbol_name<'a>(callee: &'a str, base: &str) -> &'a str {
    callee
        .strip_prefix(base)
        .and_then(|s| s.strip_prefix('.'))
        .filter(|s| !s.is_empty())
        .unwrap_or(callee)
}

/// 构建跨文件的 Imports 和 Calls 边。
fn add_cross_file_edges(
    graph: &mut SourceGraph,
    analyses: &HashMap<String, FileAnalysis>,
    file_set: &HashSet<String>,
    resolve_exts: &[&str],
) {
    // 名称索引：用于 O(1) 唯一名称查找，替代逐边 O(N) 的 find_unique_node。
    let mut class_index: HashMap<String, Vec<NodeId>> = HashMap::new();
    let mut fn_index: HashMap<String, Vec<NodeId>> = HashMap::new();
    for node in graph.nodes() {
        match node.node_type {
            NodeType::Class => class_index
                .entry(node.name.clone())
                .or_default()
                .push(node.id.clone()),
            NodeType::Function => fn_index
                .entry(node.name.clone())
                .or_default()
                .push(node.id.clone()),
            _ => {}
        }
    }

    // 按文件相对路径排序遍历，保证跨文件边的插入顺序确定（analyses 是 HashMap）
    let mut rels: Vec<&String> = analyses.keys().collect();
    rels.sort();
    for rel in rels {
        let analysis = &analyses[rel];
        let file_id = NodeId::file(rel);

        // Imports 边 + 构建导入符号 → 源文件的映射
        let mut import_map: HashMap<String, String> = HashMap::new();
        // 同一本地别名被绑定到不同模块时视为歧义，移除并禁用，避免把调用连到错误模块
        let mut ambiguous: HashSet<String> = HashSet::new();
        // 别名 → 原名映射（仅 Named import；namespace import 跳过以免污染）
        let mut alias_to_original: HashMap<String, String> = HashMap::new();
        for import in &analysis.imports {
            if let Some(target_rel) =
                resolve_import(&import.module_path, rel, file_set, resolve_exts)
            {
                graph.add_edge(Dependency::new(
                    file_id.clone(),
                    NodeId::file(&target_rel),
                    EdgeType::Imports,
                ));
                for sym in &import.symbols {
                    let local_name = sym.alias.as_deref().unwrap_or(&sym.name);
                    if ambiguous.contains(local_name) {
                        continue;
                    }
                    match import_map.get(local_name) {
                        Some(existing) if existing != &target_rel => {
                            import_map.remove(local_name);
                            ambiguous.insert(local_name.to_string());
                        }
                        _ => {
                            import_map.insert(local_name.to_string(), target_rel.clone());
                        }
                    }
                    if sym.kind == SymbolKind::Named {
                        if let Some(alias) = &sym.alias {
                            match alias_to_original.get(alias.as_str()) {
                                Some(existing) if existing != &sym.name => {
                                    alias_to_original.remove(alias.as_str());
                                }
                                _ => {
                                    alias_to_original.insert(alias.clone(), sym.name.clone());
                                }
                            }
                        }
                    }
                }
            }
        }

        // Calls 边（跨文件：通过 imports 解析调用目标）
        for call in &analysis.calls {
            let callee_base = call.callee.split('.').next().unwrap_or(&call.callee);
            if call.is_constructor {
                let target_id = NodeId::symbol(NodeType::Class, rel, &call.callee);
                let resolved = if graph.node_index(&target_id).is_some() {
                    Some(target_id)
                } else if let Some(src_file) = import_map.get(callee_base) {
                    let sym = cross_symbol_name(&call.callee, callee_base);
                    let original = alias_to_original
                        .get(sym)
                        .map(String::as_str)
                        .unwrap_or(sym);
                    let cross_id = NodeId::symbol(NodeType::Class, src_file, original);
                    graph.node_index(&cross_id).is_some().then_some(cross_id)
                } else {
                    // 全局唯一同名兜底（命中多个则放弃，避免连到错误文件）
                    class_index
                        .get(call.callee.as_str())
                        .and_then(|ids| match ids.as_slice() {
                            [single] => Some(single.clone()),
                            _ => None,
                        })
                };
                if let Some(target) = resolved {
                    graph.add_edge(
                        Dependency::new(file_id.clone(), target, EdgeType::Calls)
                            .with_sub_kind(EdgeSubKind::Constructor),
                    );
                }
            } else {
                // 1. 当前文件的函数
                let target_id = NodeId::symbol(NodeType::Function, rel, &call.callee);
                if graph.node_index(&target_id).is_some() {
                    graph.add_edge(Dependency::new(file_id.clone(), target_id, EdgeType::Calls));
                } else if let Some(src_file) = import_map.get(callee_base) {
                    // 2. 通过 import 解析到其他文件的函数。
                    let sym = cross_symbol_name(&call.callee, callee_base);
                    let cross_id = NodeId::symbol(NodeType::Function, src_file, sym);
                    if graph.node_index(&cross_id).is_some() {
                        graph.add_edge(Dependency::new(file_id.clone(), cross_id, EdgeType::Calls));
                    }
                } else if let Some(dot_pos) = call.callee.find('.') {
                    // 3. 方法调用解析（REFAC-10 档1）：
                    //    `obj.method()` — 若 obj 是本地构造绑定（`const obj = new Foo()`），
                    //    用 Foo + import_map 找到源文件，查 `function:{file}:Foo.method`。
                    //    若 obj 直接是导入类名，也尝试在同文件或导入源查方法节点。
                    let receiver = &call.callee[..dot_pos];
                    let method = &call.callee[dot_pos + 1..];
                    let class_name = analysis
                        .constructor_bindings
                        .get(receiver)
                        .map(String::as_str)
                        .unwrap_or(receiver);
                    let qualified = format!("{class_name}.{method}");
                    let local_id = NodeId::symbol(NodeType::Function, rel, &qualified);
                    if graph.node_index(&local_id).is_some() {
                        graph.add_edge(Dependency::new(file_id.clone(), local_id, EdgeType::Calls));
                    } else if let Some(src_file) = import_map.get(class_name) {
                        let original = alias_to_original
                            .get(class_name)
                            .map(String::as_str)
                            .unwrap_or(class_name);
                        let cross_qualified = format!("{original}.{method}");
                        let cross_id =
                            NodeId::symbol(NodeType::Function, src_file, &cross_qualified);
                        if graph.node_index(&cross_id).is_some() {
                            graph.add_edge(Dependency::new(
                                file_id.clone(),
                                cross_id,
                                EdgeType::Calls,
                            ));
                        }
                    } else {
                        // 全图唯一方法名兜底
                        if let Some(target) =
                            fn_index
                                .get(qualified.as_str())
                                .and_then(|ids| match ids.as_slice() {
                                    [single] => Some(single.clone()),
                                    _ => None,
                                })
                        {
                            graph.add_edge(Dependency::new(
                                file_id.clone(),
                                target,
                                EdgeType::Calls,
                            ));
                        }
                    }
                }
            }
        }
    }
}

fn resolve_import(
    module_path: &str,
    current_rel: &str,
    file_set: &HashSet<String>,
    extensions: &[&str],
) -> Option<String> {
    if !module_path.starts_with('.') {
        return None;
    }

    let current_dir = Path::new(current_rel).parent().unwrap_or(Path::new(""));
    let resolved = current_dir.join(module_path);
    let normalized = normalize_path(&resolved)?;

    // 精确匹配（已带扩展名的路径）
    if file_set.contains(&normalized) {
        return Some(normalized);
    }

    // TS ESM（NodeNext/Node16）规范：相对 import 须带 `.js`/`.mjs`/`.cjs`/`.jsx`
    // 扩展名，但实际指向同名 `.ts`/`.tsx` 源文件。strip JS 扩展名后按源扩展名重试，
    // 否则现代 ESM TypeScript 项目（如 microsoft/node-jsonc-parser）的 import 边会全部
    // 漏掉——依赖图断裂、误判无环、sprint 排序与门禁连带失效。
    let js_stripped = normalized
        .strip_suffix(".js")
        .or_else(|| normalized.strip_suffix(".mjs"))
        .or_else(|| normalized.strip_suffix(".cjs"))
        .or_else(|| normalized.strip_suffix(".jsx"));
    if let Some(base) = js_stripped {
        for ext in extensions {
            let with_ext = format!("{base}.{ext}");
            if file_set.contains(&with_ext) {
                return Some(with_ext);
            }
        }
    }

    // 按 adapter 提供的扩展名生成候选：{path}.ext, {path}/index.ext
    for ext in extensions {
        // normalized 为空 = import 解析到 src 根目录（如 `__tests__/x.ts` 的 `from ".."`）。
        // 根目录本身不是文件，跳过 `{path}.ext`；barrel 候选不能带前导斜杠，否则
        // `/index.ext` 永不匹配根下的 `index.ext`，导致 `from ".."` 这类 barrel 导入漏边
        // （进而 SCC 断裂、漏报循环依赖）。
        if !normalized.is_empty() {
            let with_ext = format!("{normalized}.{ext}");
            if file_set.contains(&with_ext) {
                return Some(with_ext);
            }
        }
        let barrel = if normalized.is_empty() {
            format!("index.{ext}")
        } else {
            format!("{normalized}/index.{ext}")
        };
        if file_set.contains(&barrel) {
            return Some(barrel);
        }
    }

    None
}

/// 归一化相对路径。路径逃逸项目根时返回 None。
fn normalize_path(path: &Path) -> Option<String> {
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
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixtures_dir() -> PathBuf {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let repo_root = manifest.ancestors().nth(3).unwrap();
        repo_root.join("fixtures")
    }

    #[test]
    fn build_linear_deps() {
        let root = fixtures_dir().join("linear-deps/src");
        let graph = build_graph_ts(&root).unwrap();

        assert!(
            graph.node_index(&NodeId::new("file:utils.ts")).is_some(),
            "should have utils.ts, nodes: {:?}",
            graph.nodes().map(|n| n.id.as_str()).collect::<Vec<_>>()
        );

        let stats = graph.stats();
        assert!(stats.total_nodes >= 3, "at least 3 file nodes");
        assert!(stats.total_edges > 0, "should have edges");
    }

    #[test]
    fn build_empty_dir() {
        let dir = std::env::temp_dir().join("rustmigrate_empty_test");
        let _ = std::fs::create_dir_all(&dir);
        let graph = build_graph_ts(&dir).unwrap();
        assert_eq!(graph.node_count(), 0);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn build_nonexistent_dir() {
        let result = build_graph_ts(Path::new("/nonexistent/path"));
        assert!(result.is_err());
    }

    #[test]
    fn build_with_no_adapters() {
        let root = fixtures_dir().join("linear-deps/src");
        let mut adapters: Vec<Box<dyn LanguageAdapter>> = vec![];
        let graph = build_graph(&root, &mut adapters).unwrap();
        assert_eq!(graph.node_count(), 0);
    }

    const TS_EXTS: &[&str] = &["ts", "tsx"];

    fn file_set(files: &[&str]) -> HashSet<String> {
        files.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn cross_symbol_name_strips_import_base() {
        assert_eq!(cross_symbol_name("ns.clamp", "ns"), "clamp");
        assert_eq!(cross_symbol_name("clamp", "clamp"), "clamp"); // 非点号：返回完整名
        assert_eq!(cross_symbol_name("a.b.c", "a"), "b.c");
    }

    #[test]
    fn namespace_call_resolves_cross_file() {
        let dir = std::env::temp_dir().join("rustmigrate_ns_call_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("utils.ts"),
            "export function clamp(x: number) { return x; }\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("app.ts"),
            "import * as utils from './utils';\nutils.clamp(1);\n",
        )
        .unwrap();

        let graph = build_graph_ts(&dir).unwrap();
        let has_call = graph.edges().any(|e| {
            e.edge_type == EdgeType::Calls
                && e.source.as_str() == "file:app.ts"
                && e.target.as_str() == "function:utils.ts:clamp"
        });
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            has_call,
            "命名空间调用 utils.clamp() 应解析为跨文件 Calls 边"
        );
    }

    #[test]
    fn resolve_import_parent_dir() {
        let files = file_set(&["utils.ts", "sub/service.ts"]);
        assert_eq!(
            resolve_import("../utils", "sub/service.ts", &files, TS_EXTS),
            Some("utils.ts".to_string())
        );
    }

    #[test]
    fn resolve_import_sibling() {
        let files = file_set(&["a/foo.ts", "a/bar.ts"]);
        assert_eq!(
            resolve_import("./bar", "a/foo.ts", &files, TS_EXTS),
            Some("a/bar.ts".to_string())
        );
    }

    #[test]
    fn resolve_import_index_barrel() {
        let files = file_set(&["shared/index.ts"]);
        assert_eq!(
            resolve_import("./shared", "app.ts", &files, TS_EXTS),
            Some("shared/index.ts".to_string())
        );
    }

    #[test]
    fn resolve_import_parent_to_root_barrel() {
        // 深度 1 目录的文件 `from ".."` 应解析到 src 根的 barrel index.ts
        // （zod 测试文件 `import { z } from ".."` 模式，曾因 `/index.ts` 前导斜杠漏边）。
        let files = file_set(&["index.ts", "__tests__/catch.test.ts"]);
        assert_eq!(
            resolve_import("..", "__tests__/catch.test.ts", &files, TS_EXTS),
            Some("index.ts".to_string())
        );
    }

    #[test]
    fn resolve_import_non_relative_returns_none() {
        let files = file_set(&["express.ts"]);
        assert_eq!(resolve_import("express", "app.ts", &files, TS_EXTS), None);
    }

    #[test]
    fn resolve_import_above_root_no_match() {
        let files = file_set(&["utils.ts"]);
        assert_eq!(
            resolve_import("../../escape", "utils.ts", &files, TS_EXTS),
            None
        );
    }

    #[test]
    fn resolve_import_exact_match_with_extension() {
        let files = file_set(&["lib/helper.ts"]);
        assert_eq!(
            resolve_import("./helper.ts", "lib/app.ts", &files, TS_EXTS),
            Some("lib/helper.ts".to_string())
        );
    }

    #[test]
    fn resolve_import_tsx_extension() {
        let files = file_set(&["components/Button.tsx"]);
        assert_eq!(
            resolve_import("./Button", "components/App.tsx", &files, TS_EXTS),
            Some("components/Button.tsx".to_string())
        );
    }

    #[test]
    fn resolve_import_js_extension_maps_to_ts() {
        // TS ESM（NodeNext）：`./scanner.js` 实际指向 `scanner.ts` 源文件。
        let files = file_set(&["impl/scanner.ts"]);
        assert_eq!(
            resolve_import("./scanner.js", "impl/parser.ts", &files, TS_EXTS),
            Some("impl/scanner.ts".to_string())
        );
    }

    #[test]
    fn resolve_import_mjs_cjs_jsx_extensions_map_to_source() {
        let files = file_set(&["a.ts", "b.ts", "C.tsx"]);
        assert_eq!(
            resolve_import("./a.mjs", "root.ts", &files, TS_EXTS),
            Some("a.ts".to_string())
        );
        assert_eq!(
            resolve_import("./b.cjs", "root.ts", &files, TS_EXTS),
            Some("b.ts".to_string())
        );
        assert_eq!(
            resolve_import("./C.jsx", "root.ts", &files, TS_EXTS),
            Some("C.tsx".to_string())
        );
    }

    #[test]
    fn cross_file_method_call_via_constructor_binding() {
        let dir = std::env::temp_dir().join("rustmigrate_method_call_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("service.ts"),
            "export class Greeter {\n  greet() { return 'hi'; }\n}\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("app.ts"),
            "import { Greeter } from './service';\nconst g = new Greeter();\ng.greet();\n",
        )
        .unwrap();

        let graph = build_graph_ts(&dir).unwrap();
        let has_method_call = graph.edges().any(|e| {
            e.edge_type == EdgeType::Calls
                && e.source.as_str() == "file:app.ts"
                && e.target.as_str() == "function:service.ts:Greeter.greet"
        });
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            has_method_call,
            "跨文件方法调用 g.greet() 应通过构造绑定解析为 Greeter.greet Calls 边"
        );
    }

    #[test]
    fn cross_file_method_call_unique_fallback() {
        let dir = std::env::temp_dir().join("rustmigrate_method_unique_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("worker.ts"),
            "export class Worker {\n  run() {}\n}\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("main.ts"),
            "import { Worker } from './worker';\nconst w = new Worker();\nw.run();\n",
        )
        .unwrap();

        let graph = build_graph_ts(&dir).unwrap();
        let has_call = graph.edges().any(|e| {
            e.edge_type == EdgeType::Calls
                && e.source.as_str() == "file:main.ts"
                && e.target.as_str() == "function:worker.ts:Worker.run"
        });
        let _ = std::fs::remove_dir_all(&dir);
        assert!(has_call, "方法调用应解析到 Worker.run");
    }

    #[test]
    fn diamond_deps_method_call_resolves() {
        let root = fixtures_dir().join("diamond-deps/src");
        let graph = build_graph_ts(&root).unwrap();
        let has_authenticate = graph.edges().any(|e| {
            e.edge_type == EdgeType::Calls
                && e.source.as_str() == "file:index.ts"
                && e.target.as_str() == "function:auth.ts:AuthService.authenticate"
        });
        assert!(
            has_authenticate,
            "diamond-deps: service.authenticate() 应解析为跨文件方法调用 Calls 边"
        );
    }

    #[test]
    fn cross_file_method_call_with_import_alias() {
        let dir = std::env::temp_dir().join("rustmigrate_alias_method_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("service.ts"),
            "export class Greeter {\n  hello() { return 'hi'; }\n}\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("app.ts"),
            "import { Greeter as G } from './service';\nconst g = new G();\ng.hello();\n",
        )
        .unwrap();

        let graph = build_graph_ts(&dir).unwrap();
        let has_call = graph.edges().any(|e| {
            e.edge_type == EdgeType::Calls
                && e.source.as_str() == "file:app.ts"
                && e.target.as_str() == "function:service.ts:Greeter.hello"
        });
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            has_call,
            "import 别名场景下方法调用应解析到原类名 Greeter.hello"
        );
    }

    // === 增量构建测试 ===

    fn temp_db(name: &str) -> PathBuf {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("rustmigrate_incr_{name}_{ts}.db"))
    }

    fn make_ts_dir(name: &str) -> PathBuf {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("rustmigrate_incr_{name}_{ts}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn incremental_skips_unchanged_files() {
        let dir = make_ts_dir("skip_unchanged");
        std::fs::write(dir.join("a.ts"), "export function foo() { return 1; }\n").unwrap();
        std::fs::write(
            dir.join("b.ts"),
            "import { foo } from './a';\nexport function bar() { return foo(); }\n",
        )
        .unwrap();

        let db = temp_db("skip_unchanged");

        // 第一次构建（退化为全量）
        let (g1, _bp1, stats1) = build_graph_ts_incremental(&dir, &db, false).unwrap();
        assert!(!stats1.incremental, "首次构建应退化为全量");
        assert!(g1.node_count() > 0);

        // 第二次构建（无变更——全部 NONE 跳过）
        let (_g2, _bp2, stats2) = build_graph_ts_incremental(&dir, &db, false).unwrap();
        assert!(stats2.incremental);
        assert_eq!(stats2.skipped, 2, "两个文件都应被跳过");
        assert_eq!(stats2.cosmetic, 0);
        assert_eq!(stats2.structural, 0);
        assert_eq!(stats2.new_files, 0);

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn incremental_cosmetic_change() {
        let dir = make_ts_dir("cosmetic");
        std::fs::write(dir.join("a.ts"), "export function foo() { return 1; }\n").unwrap();

        let db = temp_db("cosmetic");

        // 全量构建
        let _ = build_graph_ts_incremental(&dir, &db, false).unwrap();

        // 修改函数体（不改签名）= COSMETIC
        std::fs::write(dir.join("a.ts"), "export function foo() { return 42; }\n").unwrap();

        let (_g, _bp, stats) = build_graph_ts_incremental(&dir, &db, false).unwrap();
        assert!(stats.incremental);
        assert_eq!(stats.cosmetic, 1, "函数体修改应为 COSMETIC");
        assert_eq!(stats.structural, 0);

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn incremental_structural_change() {
        let dir = make_ts_dir("structural");
        std::fs::write(dir.join("a.ts"), "export function foo() { return 1; }\n").unwrap();

        let db = temp_db("structural");

        // 全量构建
        let _ = build_graph_ts_incremental(&dir, &db, false).unwrap();

        // 新增函数 = STRUCTURAL
        std::fs::write(
            dir.join("a.ts"),
            "export function foo() { return 1; }\nexport function bar() { return 2; }\n",
        )
        .unwrap();

        let (g, _bp, stats) = build_graph_ts_incremental(&dir, &db, false).unwrap();
        assert!(stats.incremental);
        assert_eq!(stats.structural, 1, "新增函数应为 STRUCTURAL");
        // 验证新函数存在
        let has_bar = g.nodes().any(|n| n.name == "bar");
        assert!(has_bar, "新增的 bar 函数应出现在图中");

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn incremental_new_file() {
        let dir = make_ts_dir("new_file");
        std::fs::write(dir.join("a.ts"), "export function foo() { return 1; }\n").unwrap();

        let db = temp_db("new_file");

        // 全量构建
        let _ = build_graph_ts_incremental(&dir, &db, false).unwrap();

        // 新增文件
        std::fs::write(dir.join("b.ts"), "export function bar() { return 2; }\n").unwrap();

        let (g, _bp, stats) = build_graph_ts_incremental(&dir, &db, false).unwrap();
        assert!(stats.incremental);
        assert_eq!(stats.new_files, 1);
        assert_eq!(stats.skipped, 1, "a.ts 应跳过");

        let has_b = g.node_index(&NodeId::file("b.ts")).is_some();
        assert!(has_b, "新文件 b.ts 应存在");

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn incremental_deleted_file() {
        let dir = make_ts_dir("deleted");
        std::fs::write(dir.join("a.ts"), "export function foo() { return 1; }\n").unwrap();
        std::fs::write(dir.join("b.ts"), "export function bar() { return 2; }\n").unwrap();

        let db = temp_db("deleted");

        // 全量构建
        let _ = build_graph_ts_incremental(&dir, &db, false).unwrap();

        // 删除 b.ts
        std::fs::remove_file(dir.join("b.ts")).unwrap();

        let (g, _bp, stats) = build_graph_ts_incremental(&dir, &db, false).unwrap();
        assert!(stats.incremental);
        assert_eq!(stats.deleted, 1);

        let has_b = g.node_index(&NodeId::file("b.ts")).is_some();
        assert!(!has_b, "删除的文件节点不应存在");

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn incremental_transitive_update() {
        let dir = make_ts_dir("transitive");
        // b 依赖 a
        std::fs::write(dir.join("a.ts"), "export function foo() { return 1; }\n").unwrap();
        std::fs::write(
            dir.join("b.ts"),
            "import { foo } from './a';\nexport function bar() { return foo(); }\n",
        )
        .unwrap();

        let db = temp_db("transitive");

        // 全量构建
        let _ = build_graph_ts_incremental(&dir, &db, false).unwrap();

        // STRUCTURAL 修改 a.ts（新增导出函数）
        std::fs::write(
            dir.join("a.ts"),
            "export function foo() { return 1; }\nexport function baz() {}\n",
        )
        .unwrap();

        let (_g, _bp, stats) = build_graph_ts_incremental(&dir, &db, false).unwrap();
        assert!(stats.incremental);
        assert_eq!(stats.structural, 1, "a.ts 应为 STRUCTURAL");
        assert!(
            stats.transitive >= 1,
            "b.ts 应因传递性更新被重分析，transitive={}",
            stats.transitive
        );

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn incremental_preserves_graph_correctness() {
        // 验证增量构建与全量构建结果一致
        let dir = make_ts_dir("correctness");
        std::fs::write(
            dir.join("utils.ts"),
            "export function clamp(x: number) { return x; }\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("app.ts"),
            "import { clamp } from './utils';\nclamp(1);\n",
        )
        .unwrap();

        let db_incr = temp_db("correctness_incr");
        let db_full = temp_db("correctness_full");

        // 增量首次 = 全量
        let _ = build_graph_ts_incremental(&dir, &db_incr, false).unwrap();

        // 修改文件
        std::fs::write(
            dir.join("utils.ts"),
            "export function clamp(x: number) { return x; }\nexport function lerp(a: number, b: number, t: number) { return a + (b - a) * t; }\n",
        )
        .unwrap();

        // 增量构建
        let (g_incr, _, _) = build_graph_ts_incremental(&dir, &db_incr, false).unwrap();

        // 全量构建
        let g_full = build_graph_ts(&dir).unwrap();

        assert_eq!(
            g_incr.node_count(),
            g_full.node_count(),
            "增量与全量节点数应一致: 增量={}, 全量={}",
            g_incr.node_count(),
            g_full.node_count()
        );

        // 验证所有全量节点在增量图中都存在
        for node in g_full.nodes() {
            assert!(
                g_incr.node_index(&node.id).is_some(),
                "增量图缺少节点: {}",
                node.id
            );
        }

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&db_incr);
        let _ = std::fs::remove_file(&db_full);
    }
}
