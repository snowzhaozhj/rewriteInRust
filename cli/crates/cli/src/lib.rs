//! `rustmigrate` CLI 入口与命令路由。
//!
//! 所有命令统一输出 JSON：`{"status":"ok|error|warning","data":{...},"warnings":[...]}`。
//! 命令权威定义见 `docs/design/06-plugin-structure.md` 的「CLI 命令概览」。
//!
//! 数据流约定（见 `docs/design/04-toolchain.md § 5.7.3`）：
//! - `graph build` 解析源码写入 `.rust-migration/source-graph.db`（SQLite）；
//! - `graph topo-sort` / `deps` / `interfaces` / `stats` 从该 db 读取；
//! - 迁移状态文件位于 `.rust-migration/migration-state.json`。

use std::io::Write;
use std::path::{Path, PathBuf};

use clap::Parser;
use serde::Serialize;
use serde_json::json;

use rustmigrate_core::error::MigrateError;
use rustmigrate_core::graph::build::build_graph_ts;
use rustmigrate_core::graph::persist::{load_from_db, save_to_db};
use rustmigrate_core::graph::topo::{detect_cycles, topological_sort};
use rustmigrate_core::graph::SourceGraph;
use rustmigrate_core::profile::profile_project;
use rustmigrate_core::response::{ErrorData, Response, Status};
use rustmigrate_core::scaffold::scaffold_project;
use rustmigrate_core::state::MigrationStateMachine;
use rustmigrate_core::stats::compute_stats;
use rustmigrate_core::types::common::NodeId;
use rustmigrate_core::types::graph::{EdgeType, NodeType};
use rustmigrate_core::validate::validate_state;

/// `.rust-migration/` 工作目录名（见 `docs/design/04-toolchain.md § 5.7.3`）。
const WORK_DIR: &str = ".rust-migration";
/// 源码图数据库文件名。
const DB_FILE: &str = "source-graph.db";
/// 迁移状态文件名。
const STATE_FILE: &str = "migration-state.json";

/// CLI 顶层入口。
#[derive(Parser)]
#[command(name = "rustmigrate", version, about = "Rust 迁移验证工作台 CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// 顶层子命令。
#[derive(clap::Subcommand)]
pub enum Commands {
    /// 初始化 `.rust-migration/` 目录与迁移状态文件。
    Init,
    /// 项目画像分析（语言检测 + 复杂度）。
    Profile {
        /// 项目根目录（默认当前目录）。
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// 源码图相关命令。
    Graph {
        #[command(subcommand)]
        action: GraphCommands,
    },
    /// 校验相关命令。
    Validate {
        #[command(subcommand)]
        action: ValidateCommands,
    },
    /// 状态机相关命令。
    State {
        #[command(subcommand)]
        action: StateCommands,
    },
    /// 统计相关命令。
    Stats {
        #[command(subcommand)]
        action: StatsCommands,
    },
    /// 脚手架相关命令。
    Scaffold {
        #[command(subcommand)]
        action: ScaffoldCommands,
    },
}

/// Graph 子命令。
#[derive(clap::Subcommand)]
pub enum GraphCommands {
    /// 解析源码构建图并写入 `source-graph.db`。
    Build {
        /// 源码根目录（设计示例 `--root ./src`）。
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// 强制全量重建（默认增量；增量逻辑见 M2，当前等价全量）。
        #[arg(long)]
        full: bool,
        /// 输出性能画像 JSON（M2 落地）。
        #[arg(long)]
        profile: bool,
    },
    /// 对依赖图执行拓扑排序，输出迁移顺序；检测到环非零退出。
    TopoSort,
    /// 查询模块的正向依赖（imports 边的传递闭包）。
    Deps { module: String },
    /// 输出模块的导出接口签名。
    Interfaces {
        module: String,
        /// 批量输出 target 的直接依赖模块接口（M2 落地）。
        #[arg(long)]
        deps_of: Option<String>,
    },
    /// 图统计信息（节点/边计数、分类计数）。
    Stats,
}

/// Validate 子命令。
#[derive(clap::Subcommand)]
pub enum ValidateCommands {
    /// 校验 `migration-state.json` 合法性。
    State,
}

/// State 子命令。
#[derive(clap::Subcommand)]
pub enum StateCommands {
    /// 查询指定模块的当前迁移状态。
    Get { module: String },
    /// 执行项目状态机转换（带合法性前置检查）。
    Transition {
        #[arg(long)]
        module: String,
        #[arg(long)]
        to: String,
    },
}

/// Stats 子命令。
#[derive(clap::Subcommand)]
pub enum StatsCommands {
    /// 迁移进度统计（按模块状态分类）。
    Loc,
    /// 源码与 Rust 结构复杂度对比（M2 落地）。
    Compare,
}

/// Scaffold 子命令。
#[derive(clap::Subcommand)]
pub enum ScaffoldCommands {
    /// 生成 Cargo workspace 骨架。
    Workspace {
        /// 目标目录（默认 `rust/`）。
        #[arg(long, default_value = "rust")]
        target: PathBuf,
        /// crate 名称（默认 `migrated`）。
        #[arg(long, default_value = "migrated")]
        name: String,
    },
}

/// CLI 入口：解析参数并执行，结果写入 writer。
///
/// 返回进程退出码：0 成功；1 一般错误；2 拓扑排序检测到环（与设计「非零退出」对齐）。
/// 测试中传 `Vec<u8>` 捕获输出；生产中传 stdout。
pub fn run_with_args<I, S, W>(args: I, writer: &mut W) -> i32
where
    I: IntoIterator<Item = S>,
    S: Into<std::ffi::OsString> + Clone,
    W: Write,
{
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(e) => {
            let _ = write!(writer, "{e}");
            return if e.use_stderr() { 1 } else { 0 };
        }
    };

    execute(&cli.command, writer)
}

/// 执行命令，返回进程退出码。
fn execute<W: Write>(command: &Commands, writer: &mut W) -> i32 {
    match command {
        Commands::Init => emit(writer, cmd_init()),
        Commands::Profile { root } => emit(writer, cmd_profile(root)),
        Commands::Graph { action } => match action {
            GraphCommands::Build {
                root,
                full,
                profile,
            } => emit(writer, cmd_graph_build(root, *full, *profile)),
            // topo-sort 有环时需非零退出，单独处理退出码。
            GraphCommands::TopoSort => cmd_graph_topo_sort(writer),
            GraphCommands::Deps { module } => emit(writer, cmd_graph_deps(module)),
            GraphCommands::Interfaces { module, deps_of } => {
                emit(writer, cmd_graph_interfaces(module, deps_of.as_deref()))
            }
            GraphCommands::Stats => emit(writer, cmd_graph_stats()),
        },
        Commands::Validate { action } => match action {
            ValidateCommands::State => emit(writer, cmd_validate_state()),
        },
        Commands::State { action } => match action {
            StateCommands::Get { module } => emit(writer, cmd_state_get(module)),
            StateCommands::Transition { module, to } => {
                emit(writer, cmd_state_transition(module, to))
            }
        },
        Commands::Stats { action } => match action {
            StatsCommands::Loc => emit(writer, cmd_stats_loc()),
            StatsCommands::Compare => emit(writer, cmd_stats_compare()),
        },
        Commands::Scaffold { action } => match action {
            ScaffoldCommands::Workspace { target, name } => {
                emit(writer, cmd_scaffold_workspace(name, target))
            }
        },
    }
}

/// 命令结果：成功数据（JSON value + 警告）或错误。
type CmdResult = Result<(serde_json::Value, Vec<String>), MigrateError>;

/// 将命令结果序列化为统一 JSON 响应并写入 writer，返回退出码（0 成功 / 1 错误）。
fn emit<W: Write>(writer: &mut W, result: CmdResult) -> i32 {
    match result {
        Ok((data, warnings)) => {
            let resp = Response::ok_with_warnings(data, warnings);
            write_json(writer, &resp);
            0
        }
        Err(err) => {
            let resp: Response<ErrorData> = err.into();
            write_json(writer, &resp);
            1
        }
    }
}

/// 序列化响应为单行 JSON 并写入（附换行）。
fn write_json<W: Write, T: Serialize>(writer: &mut W, resp: &Response<T>) {
    match serde_json::to_string(resp) {
        Ok(s) => {
            let _ = writeln!(writer, "{s}");
        }
        Err(e) => {
            // 序列化失败兜底：手写最小错误 JSON，避免静默吞掉。
            let _ = writeln!(
                writer,
                r#"{{"status":"error","data":{{"kind":"json","message":"{}"}}}}"#,
                e.to_string().replace('"', "\\\"")
            );
        }
    }
}

// === 路径辅助 ===

/// `.rust-migration/` 目录。
fn work_dir() -> PathBuf {
    PathBuf::from(WORK_DIR)
}

/// `source-graph.db` 路径。
fn db_path() -> PathBuf {
    work_dir().join(DB_FILE)
}

/// `migration-state.json` 路径。
fn state_path() -> PathBuf {
    work_dir().join(STATE_FILE)
}

// === 命令实现 ===

/// `init`：创建 `.rust-migration/` 目录与初始 `migration-state.json`。
///
/// 已存在状态文件时不覆盖（幂等），仅返回已初始化标记。
fn cmd_init() -> CmdResult {
    let dir = work_dir();
    std::fs::create_dir_all(&dir)?;

    let state = state_path();
    let already = state.exists();
    if !already {
        // 主语言在 init 阶段未知，先用 profile 探测；探测失败回退 TypeScript。
        let lang = rustmigrate_core::profile::detect_language(Path::new("."))
            .unwrap_or(rustmigrate_core::types::common::SourceLang::TypeScript);
        let machine = MigrationStateMachine::init_new(&project_name(), lang);
        machine.save(&state)?;
    }

    Ok((
        json!({
            "message": "initialized",
            "work_dir": dir.to_string_lossy(),
            "state_file": state.to_string_lossy(),
            "already_initialized": already,
        }),
        Vec::new(),
    ))
}

/// 取当前目录名作为项目名，无法解析时回退 `project`。
fn project_name() -> String {
    std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "project".to_owned())
}

/// `profile --root <path>`：项目画像分析。
fn cmd_profile(root: &Path) -> CmdResult {
    let profile = profile_project(root)?;
    Ok((serde_json::to_value(&profile)?, Vec::new()))
}

/// `graph build --root <path> [--full] [--profile]`：构建图并写入 db。
///
/// 当前以 TypeScript adapter 全量构建。`--full` 在 M1 等价默认（增量推 M2），
/// `--profile` 性能画像推 M2。
fn cmd_graph_build(root: &Path, full: bool, profile: bool) -> CmdResult {
    let mut warnings: Vec<String> = Vec::new();

    // TODO(M2): 增量构建（file_fingerprints 跳过未变更文件）；当前 --full 无差异。
    // TODO(M2): --profile 性能画像 JSON（见 04-toolchain.md § 5.7.4.1）。
    if profile {
        warnings.push("--profile 性能画像尚未实现（推迟 M2），本次按普通构建处理".to_owned());
    }

    let graph = build_graph_ts(root)?;
    warnings.extend(graph.warnings().iter().cloned());

    // 确保 `.rust-migration/` 存在后再写 db。
    std::fs::create_dir_all(work_dir())?;
    let db = db_path();
    save_to_db(&graph, &db)?;

    // 标记 graph 构建完成（若状态文件存在）。
    mark_graph_built(&mut warnings);

    Ok((
        json!({
            "db_path": db.to_string_lossy(),
            "node_count": graph.node_count(),
            "edge_count": graph.edge_count(),
            "full": full,
        }),
        warnings,
    ))
}

/// 若状态文件存在，标记 `metadata.graph_build_completed = true`。
///
/// 状态文件不存在（未 init）属正常用法，仅记一条提示而非报错。
fn mark_graph_built(warnings: &mut Vec<String>) {
    let state = state_path();
    if !state.exists() {
        warnings.push(
            "未找到 migration-state.json，跳过 graph_build_completed 标记（建议先 init）"
                .to_owned(),
        );
        return;
    }
    match MigrationStateMachine::load(&state) {
        Ok(mut machine) => {
            machine.set_graph_build_completed();
            if let Err(e) = machine.save(&state) {
                warnings.push(format!("标记 graph_build_completed 失败: {e}"));
            }
        }
        Err(e) => warnings.push(format!(
            "加载状态文件失败，未标记 graph_build_completed: {e}"
        )),
    }
}

/// `graph topo-sort`：拓扑排序输出迁移顺序；有环则非零退出（退出码 2）并列出环。
///
/// 单独处理退出码：成功 0，环 2，其他错误 1。
fn cmd_graph_topo_sort<W: Write>(writer: &mut W) -> i32 {
    let graph = match load_graph() {
        Ok(g) => g,
        Err(err) => return emit(writer, Err(err)),
    };

    match topological_sort(&graph) {
        Ok(order) => {
            let order_strs: Vec<String> = order.iter().map(|id| id.to_string()).collect();
            let resp = Response::ok(json!({ "order": order_strs }));
            write_json(writer, &resp);
            0
        }
        Err(MigrateError::CyclicDependency { .. }) => {
            // 列出完整环路径，非零退出（设计：检测到环则非零退出并列出环路径）。
            let cycles = detect_cycles(&graph);
            let cycle_paths: Vec<Vec<String>> = cycles
                .iter()
                .map(|c| c.iter().map(|id| id.to_string()).collect())
                .collect();
            let resp = Response {
                status: Status::Error,
                data: json!({
                    "error": "cyclic_dependency",
                    "cycles": cycle_paths,
                    "suggestion": "存在循环依赖，无法生成拓扑序；请打破环后重试",
                }),
                warnings: Vec::new(),
            };
            write_json(writer, &resp);
            2
        }
        Err(err) => emit(writer, Err(err)),
    }
}

/// `graph deps <module>`：模块正向依赖（imports 边的传递闭包，BFS）。
fn cmd_graph_deps(module: &str) -> CmdResult {
    let graph = load_graph()?;
    let start = resolve_file_node(&graph, module)?;

    // BFS 遍历 imports 出边，收集传递依赖（不含自身）。
    let mut visited: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut queue: std::collections::VecDeque<NodeId> = std::collections::VecDeque::new();
    queue.push_back(start.clone());
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    seen.insert(start.as_str().to_owned());

    while let Some(cur) = queue.pop_front() {
        for (target, edge_type) in graph.outgoing_edges(&cur) {
            if edge_type != EdgeType::Imports {
                continue;
            }
            let tid = target.id.as_str().to_owned();
            if seen.insert(tid.clone()) {
                visited.insert(tid);
                queue.push_back(target.id.clone());
            }
        }
    }

    let deps: Vec<String> = visited.into_iter().collect();
    Ok((
        json!({
            "module": start.to_string(),
            "dependencies": deps,
        }),
        Vec::new(),
    ))
}

/// `graph interfaces <module> [--deps-of <target>]`：模块导出接口签名。
///
/// 输出该模块内 `is_exported=true` 的符号节点（名称 + 类型 + 行号 + token 估算）。
/// `--deps-of` 批量模式推迟 M2。
fn cmd_graph_interfaces(module: &str, deps_of: Option<&str>) -> CmdResult {
    let mut warnings: Vec<String> = Vec::new();
    if deps_of.is_some() {
        // TODO(M2): --deps-of 批量输出 target 的直接依赖模块接口（imports 1-hop 邻居）。
        warnings
            .push("--deps-of 批量接口输出尚未实现（推迟 M2），本次仅输出指定模块接口".to_owned());
    }

    let graph = load_graph()?;
    let file_node = resolve_file_node(&graph, module)?;
    let file_path = file_node
        .file_path()
        .ok_or_else(|| MigrateError::Graph {
            message: format!("无法解析模块文件路径: {module}"),
            file: module.to_owned(),
        })?
        .to_owned();

    // 收集该文件下导出的符号（File 节点本身不算接口）。
    let mut interfaces: Vec<serde_json::Value> = graph
        .symbols_in_file(&file_path)
        .into_iter()
        .filter(|n| n.is_exported)
        .map(|n| {
            // token 估算：签名近似为 name 的字节数 / 4（设计：bytes/4）。
            let token_estimate = n.name.len().div_ceil(4);
            json!({
                "name": n.name,
                "node_type": n.node_type.to_string(),
                "line_range": n.line_range,
                "token_estimate": token_estimate,
            })
        })
        .collect();
    // 确定性排序（symbols_in_file 顺序依赖图遍历）。
    interfaces.sort_by(|a, b| {
        a["name"]
            .as_str()
            .unwrap_or_default()
            .cmp(b["name"].as_str().unwrap_or_default())
    });

    Ok((
        json!({
            "module": file_path,
            "interfaces": interfaces,
        }),
        warnings,
    ))
}

/// `graph stats`：图统计信息。
fn cmd_graph_stats() -> CmdResult {
    let graph = load_graph()?;
    let stats = graph.stats();
    Ok((serde_json::to_value(&stats)?, Vec::new()))
}

/// `validate state`：校验 `migration-state.json`。
fn cmd_validate_state() -> CmdResult {
    let machine = MigrationStateMachine::load(&state_path())?;
    let warnings = validate_state(machine.state_file())?;
    Ok((json!({ "valid": true }), warnings))
}

/// `state get <module>`：查询指定模块迁移状态。
fn cmd_state_get(module: &str) -> CmdResult {
    let machine = MigrationStateMachine::load(&state_path())?;
    let state_file = machine.state_file();
    match state_file.modules.get(module) {
        Some(m) => Ok((
            json!({
                "module": module,
                "status": m.status.to_string(),
                "state": serde_json::to_value(m)?,
            }),
            Vec::new(),
        )),
        None => Err(MigrateError::Config(format!("模块不存在: {module}"))),
    }
}

/// `state transition --module <m> --to <status>`：模块级迁移状态转换。
///
/// 设计（`09-appendix-schemas.md` § 状态机）要求本命令为【模块级】转换：
/// `--to` 取 ModuleStatus（translating/compile_fixing/testing/reviewing/done…），
/// 校验合法转换路径，并支持 `--substatus`/`--reason` 与 tmp-fsync-rename 原子写。
/// 这是一块独立的模块级状态机功能（设计有专门附录），不属于本 PR 的命令路由接线范围。
fn cmd_state_transition(module: &str, to: &str) -> CmdResult {
    // TODO(M1-STATE): 完整模块级状态转换 —— ModuleStatus 解析 + 合法性前置校验
    // + substatus/reason 参数 + 原子写（复用 core `update_module`）。
    // 当前诚实返回未实现，避免用项目级 ProjectState 语义冒充模块级转换。
    Ok((
        json!({
            "message": "state transition 模块级状态转换尚未实现",
            "implemented": false,
            "module": module,
            "requested_to": to,
        }),
        vec![
            "state transition 需模块级状态机完整实现（合法性校验/substatus/reason/原子写），本 PR 未实现"
                .to_owned(),
        ],
    ))
}

/// `stats loc`：迁移进度统计（按模块状态分类）。
fn cmd_stats_loc() -> CmdResult {
    let machine = MigrationStateMachine::load(&state_path())?;
    let stats = compute_stats(machine.state_file());
    Ok((serde_json::to_value(&stats)?, Vec::new()))
}

/// `stats compare`：源码与 Rust 结构复杂度对比（推迟 M2）。
fn cmd_stats_compare() -> CmdResult {
    // TODO(M2): 复用 tokei + tree-sitter 函数计数做结构对比（见 06 § CLI 表 stats compare）。
    Ok((
        json!({
            "message": "stats compare 尚未实现（推迟 M2）",
            "implemented": false,
        }),
        vec!["stats compare 推迟 M2，当前返回占位响应".to_owned()],
    ))
}

/// `scaffold workspace`：生成 Cargo lib 项目骨架。
fn cmd_scaffold_workspace(name: &str, target: &Path) -> CmdResult {
    scaffold_project(name, target)?;
    Ok((
        json!({
            "name": name,
            "target": target.to_string_lossy(),
        }),
        Vec::new(),
    ))
}

// === 图加载辅助 ===

/// 从 `.rust-migration/source-graph.db` 加载图。
///
/// db 不存在时返回 FileNotFound（设计：build 写、下游读）。
fn load_graph() -> Result<SourceGraph, MigrateError> {
    load_from_db(&db_path())
}

/// 将模块名解析为图中的 File 节点 ID，兼容多种写法：
/// 直接 NodeId、`file:` 前缀、相对路径（含/不含 `src/` 前缀）。
fn resolve_file_node(graph: &SourceGraph, module: &str) -> Result<NodeId, MigrateError> {
    // 候选 NodeId 形式（按优先级）。
    let candidates = [
        NodeId::new(module.to_owned()),
        NodeId::file(module),
        NodeId::file(&format!("src/{module}")),
    ];
    for cand in &candidates {
        if graph.node_index(cand).is_some() {
            return Ok(cand.clone());
        }
    }
    // 退一步：按文件名后缀匹配唯一 File 节点。
    let matches: Vec<&NodeId> = graph
        .nodes_by_type(NodeType::File)
        .into_iter()
        .map(|n| &n.id)
        .filter(|id| {
            id.file_path()
                .map(|p| p == module || p.ends_with(&format!("/{module}")))
                .unwrap_or(false)
        })
        .collect();
    match matches.as_slice() {
        [single] => Ok((*single).clone()),
        [] => Err(MigrateError::Graph {
            message: format!("图中找不到模块: {module}"),
            file: module.to_owned(),
        }),
        _ => Err(MigrateError::Graph {
            message: format!("模块名歧义，匹配到多个文件: {module}"),
            file: module.to_owned(),
        }),
    }
}
