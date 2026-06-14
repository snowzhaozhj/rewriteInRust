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
use rustmigrate_core::profile::{
    check_adapter_tools, check_cargo_nextest, load_analysis_tools, profile_project, ToolStatus,
};
use rustmigrate_core::response::{ErrorData, Response, Status};
use rustmigrate_core::scaffold::scaffold_project;
use rustmigrate_core::state::MigrationStateMachine;
use rustmigrate_core::stats::count_loc;
use rustmigrate_core::types::common::NodeId;
use rustmigrate_core::types::graph::{EdgeType, NodeType};
use rustmigrate_core::types::state::ModuleStatus;
use rustmigrate_core::validate::validate_state;

/// `.rust-migration/` 工作目录名（见 `docs/design/04-toolchain.md § 5.7.3`）。
const WORK_DIR: &str = ".rust-migration";
/// 源码图数据库文件名。
const DB_FILE: &str = "source-graph.db";
/// 迁移状态文件名。
const STATE_FILE: &str = "migration-state.json";
/// 项目根配置文件名（见 `06-plugin-structure.md` § CLI 命令概览）。
const CONFIG_FILE: &str = ".rustmigrate.toml";

/// CLI 顶层入口。
#[derive(Parser)]
// color=Never：CLI 输出统一 JSON，clap 错误/help 文本不应含 ANSI 色码（tty 下会污染 JSON message）。
#[command(
    name = "rustmigrate",
    version,
    about = "Rust 迁移验证工作台 CLI",
    color = clap::ColorChoice::Never
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// 顶层子命令。
#[derive(clap::Subcommand)]
pub enum Commands {
    /// 初始化 `.rust-migration/` 目录与迁移状态文件。
    Init,
    /// 项目画像分析（语言检测 + 复杂度 + 外部工具可用性检测）。
    Profile {
        /// 项目根目录（默认当前目录）。
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// 适配器 `analysis-tools.json` 路径（用于 ADAPTER_TOOL_MISSING 检测）。
        /// 由 Plugin SKILL 传入 `${CLAUDE_PLUGIN_ROOT}/skills/migrate/adapters/<lang>/analysis-tools.json`；
        /// 省略则跳过适配器工具检测（仍检测 cargo-nextest）。
        #[arg(long)]
        adapter_tools: Option<PathBuf>,
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
    /// 执行模块级状态机转换（带合法性前置检查）。
    Transition {
        #[arg(long)]
        module: String,
        /// 目标 ModuleStatus（translating/compile_fixing/testing/reviewing/done/degrade_*/
        /// paused/blocked）。省略则仅更新 substatus（status 不变，见 09-appendix § Step 2/4）。
        #[arg(long)]
        to: Option<String>,
        /// 子状态（如 phase_a_complete_awaiting_review），见 09-appendix-schemas.md。
        #[arg(long)]
        substatus: Option<String>,
        /// 转换原因说明（追加到模块 attempts 审计序列）。
        #[arg(long)]
        reason: Option<String>,
        /// 强制恢复：degrade_* → translating 须显式 --force（降级恢复是人类决策，
        /// 见设计 § Step 0.3）。其余转换忽略。
        #[arg(long)]
        force: bool,
    },
}

/// Stats 子命令。
#[derive(clap::Subcommand)]
pub enum StatsCommands {
    /// 源码与 Rust 代码行数统计（嵌入 tokei）。
    Loc {
        /// 源码根目录（省略则取 `.rustmigrate.toml` 的 `project.source_root`）。
        #[arg(long)]
        source: Option<PathBuf>,
        /// Rust 代码根目录（省略则取 `.rustmigrate.toml` 的 `project.rust_root`）。
        #[arg(long)]
        rust: Option<PathBuf>,
    },
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
            use clap::error::ErrorKind;
            // --help / --version 是正常输出，原样写出并退出 0。
            if matches!(
                e.kind(),
                ErrorKind::DisplayHelp
                    | ErrorKind::DisplayVersion
                    | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
            ) {
                let _ = write!(writer, "{e}");
                return 0;
            }
            // 真正的解析错误：包成统一 JSON 错误响应，不输出 clap 裸文本
            // （契约：所有输出统一 {status,data,warnings}）。
            let resp = Response::<ErrorData> {
                status: Status::Error,
                data: ErrorData {
                    kind: "cli_parse".to_owned(),
                    message: e.to_string(),
                    context: None,
                },
                warnings: Vec::new(),
            };
            write_json(writer, &resp);
            return 1;
        }
    };

    execute(&cli.command, writer)
}

/// 执行命令，返回进程退出码。
fn execute<W: Write>(command: &Commands, writer: &mut W) -> i32 {
    match command {
        Commands::Init => emit(writer, cmd_init()),
        Commands::Profile {
            root,
            adapter_tools,
        } => emit(writer, cmd_profile(root, adapter_tools.as_deref())),
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
            StateCommands::Transition {
                module,
                to,
                substatus,
                reason,
                force,
            } => emit(
                writer,
                cmd_state_transition(
                    module,
                    to.as_deref(),
                    substatus.as_deref(),
                    reason.as_deref(),
                    *force,
                ),
            ),
        },
        Commands::Stats { action } => match action {
            StatsCommands::Loc { source, rust } => {
                emit(writer, cmd_stats_loc(source.as_deref(), rust.as_deref()))
            }
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

/// 项目根 `.rustmigrate.toml` 配置文件路径。
fn config_path() -> PathBuf {
    PathBuf::from(CONFIG_FILE)
}

// === 命令实现 ===

/// `init`：创建 `.rust-migration/` 目录、初始 `migration-state.json`
/// 与项目根 `.rustmigrate.toml` 配置文件（见 `06-plugin-structure.md` § CLI 命令概览）。
///
/// 已存在状态文件 / 配置文件时不覆盖（幂等），仅返回已初始化标记。
fn cmd_init() -> CmdResult {
    let dir = work_dir();
    std::fs::create_dir_all(&dir)?;

    let state = state_path();
    let already = state.exists();
    // 主语言在 init 阶段未知，先用 profile 探测；探测失败回退 TypeScript。
    let lang = rustmigrate_core::profile::detect_language(Path::new("."))
        .unwrap_or(rustmigrate_core::types::common::SourceLang::TypeScript);
    if !already {
        let machine = MigrationStateMachine::init_new(&project_name(), lang);
        machine.save(&state)?;
    }

    // 项目根 `.rustmigrate.toml`：不存在时按默认配置写入（幂等，存在不覆盖）。
    let config = config_path();
    let config_already = config.exists();
    if !config_already {
        let mut cfg = rustmigrate_core::types::config::MigrateConfig::default();
        cfg.project.name = project_name();
        cfg.project.source_language = lang;
        let toml_str = toml::to_string(&cfg)?;
        std::fs::write(&config, toml_str)?;
    }

    Ok((
        json!({
            "message": "initialized",
            "work_dir": dir.to_string_lossy(),
            "state_file": state.to_string_lossy(),
            "config_file": config.to_string_lossy(),
            "already_initialized": already && config_already,
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

/// `profile --root <path> [--adapter-tools <json>]`：项目画像分析 + 外部工具可用性检测。
///
/// 设计（06-plugin-structure.md § CLI / 错误码表）：
/// - `--adapter-tools` 指向适配器 `analysis-tools.json`，逐项验证安装与最低版本，
///   必需工具缺失/版本不足产出 `ADAPTER_TOOL_MISSING` 警告（含 install_hint）；
/// - 始终检测 Tier 0 Rust 二进制 `cargo-nextest`，缺失产出 `RUST_TOOL_MISSING` 警告。
///
/// 检测结果（含已满足项）写入 `data.tool_checks`，缺失项同时降级为 warning。
fn cmd_profile(root: &Path, adapter_tools: Option<&Path>) -> CmdResult {
    let profile = profile_project(root)?;
    let mut warnings: Vec<String> = Vec::new();
    let mut checks: Vec<ToolStatus> = Vec::new();

    // 适配器工具检测（按 analysis-tools.json）。
    match adapter_tools {
        Some(path) => match load_analysis_tools(path) {
            Ok(tools) => {
                for status in check_adapter_tools(&tools) {
                    if status.is_missing() {
                        warnings.push(format_tool_missing("ADAPTER_TOOL_MISSING", &status));
                    }
                    checks.push(status);
                }
            }
            // 清单文件本身读不了/损坏 ≠ 工具缺失，用独立的 MANIFEST 码，
            // 并区分「路径错」与「内容损坏」，避免污染 ADAPTER_TOOL_MISSING 计数。
            Err(MigrateError::FileNotFound(_)) => warnings.push(format!(
                "ADAPTER_TOOLS_MANIFEST_UNREADABLE: analysis-tools.json 不存在（检查 --adapter-tools 路径）：{}",
                path.display()
            )),
            Err(e) => warnings.push(format!(
                "ADAPTER_TOOLS_MANIFEST_UNREADABLE: analysis-tools.json 解析失败（文件可能损坏）（{}）：{e}",
                path.display()
            )),
        },
        None => warnings
            .push("未提供 --adapter-tools，跳过适配器工具检测（ADAPTER_TOOL_MISSING）".to_owned()),
    }

    // Tier 0 Rust 工具检测（cargo-nextest）。
    let nextest = check_cargo_nextest();
    if nextest.is_missing() {
        warnings.push(format_tool_missing("RUST_TOOL_MISSING", &nextest));
    }
    checks.push(nextest);

    let mut data = serde_json::to_value(&profile)?;
    if let Some(obj) = data.as_object_mut() {
        obj.insert("tool_checks".to_owned(), serde_json::to_value(&checks)?);
    }
    Ok((data, warnings))
}

/// 构造工具缺失警告文案，按根因给出**精确**结论，避免误导用户：
/// - 探测失败（命令存在但执行异常）→「探测失败（<原因>），无法确认是否安装」；
/// - 命令不存在 →「未安装」；
/// - 已安装但有 min_version 却解析不出版本 →「版本无法解析…无法确认是否满足 ≥<min>」；
/// - 已安装但版本不足 →「版本不足（需 ≥<min>，探测到 <det>）」。
///
/// 末尾附 `install_hint`（如有）。
fn format_tool_missing(code: &str, status: &ToolStatus) -> String {
    let mut msg = format!("{code}: {} ", status.display_name);
    if let Some(err) = &status.probe_error {
        msg.push_str(&format!("探测失败（{err}），无法确认是否安装"));
    } else if !status.available {
        msg.push_str("未安装");
    } else if status.min_version.is_some() && status.detected_version.is_none() {
        // 命令成功但版本号无法解析：不应断言「版本不足」。
        if let Some(min) = &status.min_version {
            msg.push_str(&format!("版本无法解析，无法确认是否满足 ≥{min}"));
        }
    } else {
        msg.push_str("版本不足");
        if let Some(min) = &status.min_version {
            msg.push_str(&format!("（需 ≥{min}"));
            if let Some(det) = &status.detected_version {
                msg.push_str(&format!("，探测到 {det}"));
            }
            msg.push('）');
        }
    }
    if let Some(hint) = &status.install_hint {
        msg.push_str(&format!("；安装: {hint}"));
    }
    msg
}

/// `graph build --root <path> [--full] [--profile]`：构建图并写入 db。
///
/// 当前以 TypeScript adapter 全量构建。`--full` 在 M1 等价默认（增量推 M2），
/// `--profile` 性能画像推 M2。
fn cmd_graph_build(root: &Path, _full: bool, profile: bool) -> CmdResult {
    let mut warnings: Vec<String> = Vec::new();

    // TODO(M2): 增量构建（file_fingerprints 跳过未变更文件）；当前 --full 无差异。
    // TODO(M2): --profile 性能画像 JSON（见 04-toolchain.md § 5.7.4.1）。
    if profile {
        warnings.push("--profile 性能画像尚未实现（推迟 M2），本次按普通构建处理".to_owned());
    }
    // M1 暂无增量构建：无论 --full 与否都是全量。下方 `full` 字段恒 true 反映真实构建模式
    // （原 `full: full` 默认 false 会让上层误判为增量结果）。增量推 M2。

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
            "full": true,
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
            // data 形状对齐统一 ErrorData 的 `kind`/`message` 命名（原手搓 `error`/`suggestion`）。
            // 环路径字段名用 `cycle_path` 对齐设计 09-appendix § Step 2.8（SKILL 据此解析）；
            // 值为环路径数组（可能多个环），ErrorData 无对应字段故仍用 json! 自构造。
            // TODO(M2): ErrorData 增加 structured context（如 details: Value），让环路径走统一类型。
            let resp = Response {
                status: Status::Error,
                data: json!({
                    "kind": "cyclic_dependency",
                    "message": "存在循环依赖，无法生成拓扑序；请打破环后重试",
                    "cycle_path": cycle_paths,
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

/// `state transition --module <m> [--to <status>] [--substatus <s>] [--reason <r>]`：
/// 模块级迁移状态转换。
///
/// 设计（`09-appendix-schemas.md` § 合法状态转换）：`--to` 取 ModuleStatus
/// （translating/compile_fixing/testing/reviewing/done/degrade_*/paused/blocked），
/// 经 [`ModuleStatus::can_transition_to`] 校验合法转换路径；省略 `--to` 时仅更新
/// `--substatus`（status 不变，对应 Step 2/4 的 Phase 进度记录）。`degrade_* → translating`
/// 恢复须 `--force`。转换的 blocked 恢复、degrade 重置等副作用由 core
/// [`MigrationStateMachine::transition_module`] 处理，落盘走 tmp-fsync-rename 原子写。
fn cmd_state_transition(
    module: &str,
    to: Option<&str>,
    substatus: Option<&str>,
    reason: Option<&str>,
    force: bool,
) -> CmdResult {
    if to.is_none() && substatus.is_none() {
        return Err(MigrateError::Config(
            "state transition 至少需指定 --to 或 --substatus 之一".to_owned(),
        ));
    }
    // 解析目标状态（ModuleStatus 派生 EnumString，snake_case）。
    let target = match to {
        Some(s) => Some(s.parse::<ModuleStatus>().map_err(|_| {
            MigrateError::Config(format!(
                "非法 ModuleStatus: {s}（合法值: pending/translating/compile_fixing/testing/\
                 reviewing/done/degrade_ffi/degrade_manual/degrade_skip/paused/blocked）"
            ))
        })?),
        None => None,
    };

    let path = state_path();
    let mut machine = MigrationStateMachine::load(&path)?;
    machine.transition_module(module, target, substatus, reason, force)?;
    machine.save(&path)?;

    let updated = machine
        .state_file()
        .modules
        .get(module)
        .expect("转换成功后模块必存在");
    Ok((
        json!({
            "module": module,
            "status": updated.status.to_string(),
            "substatus": updated.substatus,
            "state": serde_json::to_value(updated)?,
        }),
        Vec::new(),
    ))
}

/// `stats loc [--source <p>] [--rust <p>]`：源码 / Rust 代码行数统计（嵌入 tokei）。
///
/// 设计（`06-plugin-structure.md:99`）：统计源码和 Rust 代码行数。路径优先取命令行参数，
/// 否则读 `.rustmigrate.toml` 的 `project.source_root` / `project.rust_root`，再退默认值
/// （`src` / `rust-src`）。某一侧目录不存在时该侧报 null 并降级 warning（Rust 端在迁移
/// 早期通常尚未生成，属正常情形，不应整命令失败）。
fn cmd_stats_loc(source: Option<&Path>, rust: Option<&Path>) -> CmdResult {
    let mut warnings: Vec<String> = Vec::new();
    // 解析路径：CLI 参数 > 配置文件 > 默认值（配置读取/解析失败会进 warnings）。
    let cfg = load_config_or_default(&mut warnings);
    let source_root = source
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(&cfg.project.source_root));
    let rust_root = rust
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(&cfg.project.rust_root));

    // 包含关系检测：一侧是另一侧的子目录时，被包含侧的文件会被外侧 tokei 递归计入，
    // 造成跨 source/rust 重复计数 + 源码侧混入 Rust（EXCLUDED_DIRS 不含 rust_root）。
    // M1 仅显式告警（不静默），完整去重（扫外侧时排除内侧）留待 M2。
    if let Some(outer) = roots_overlap(&source_root, &rust_root) {
        warnings.push(format!(
            "source 与 rust 目录存在包含关系（{outer} 包含另一侧），LOC 可能重复计数且源码侧会混入 Rust；\
             建议将 source_root / rust_root 配置为互不包含的目录"
        ));
    }

    let source_loc = count_loc_side(&source_root, "source", &mut warnings);
    let rust_loc = count_loc_side(&rust_root, "rust", &mut warnings);

    Ok((
        json!({
            "source": source_loc,
            "rust": rust_loc,
        }),
        warnings,
    ))
}

/// 检测两个 root 是否存在包含关系（含相等）。返回**外层**目录的展示路径（供告警）。
///
/// 优先按 [`std::fs::canonicalize`] 比较真实路径（解析符号链接/`.`/`..`），任一侧无法
/// 规范化（如目录不存在）时回退到原始路径的 [`Path::starts_with`] 词法比较。无包含返回 `None`。
fn roots_overlap(source: &Path, rust: &Path) -> Option<String> {
    let cs = std::fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf());
    let cr = std::fs::canonicalize(rust).unwrap_or_else(|_| rust.to_path_buf());
    if cs == cr || cs.starts_with(&cr) {
        Some(rust.display().to_string())
    } else if cr.starts_with(&cs) {
        Some(source.display().to_string())
    } else {
        None
    }
}

/// 统计单侧 LOC。三种结果均产生**可区分**的输出/告警，不静默：
/// - 成功：返回序列化后的 `LocReport`；若 `files == 0`（目录存在但未统计到受支持文件）
///   追加可疑提示。
/// - 目录不存在：返回 `Null` + warning（迁移早期 Rust 端常未生成，属正常）。
/// - 序列化失败 / 其余错误：返回 `Null` + warning（**不与"目录不存在"混淆**）。
fn count_loc_side(root: &Path, label: &str, warnings: &mut Vec<String>) -> serde_json::Value {
    match count_loc(root) {
        Ok(report) => {
            if report.files == 0 {
                warnings.push(format!(
                    "{label} 目录存在但未统计到任何受支持语言文件（可能为空/权限不足/全被排除）: {}",
                    root.display()
                ));
            }
            match serde_json::to_value(&report) {
                Ok(v) => v,
                Err(e) => {
                    warnings.push(format!("{label} LOC 结果序列化失败: {e}"));
                    serde_json::Value::Null
                }
            }
        }
        Err(MigrateError::FileNotFound(_)) => {
            warnings.push(format!(
                "{label} 目录不存在，跳过 LOC 统计: {}",
                root.display()
            ));
            serde_json::Value::Null
        }
        Err(e) => {
            warnings.push(format!("{label} LOC 统计失败: {e}"));
            serde_json::Value::Null
        }
    }
}

/// 读取项目根 `.rustmigrate.toml` 回退默认配置。**区分三种情形**，避免静默吞错：
/// - 文件不存在：静默回退默认（正常，配置可选）。
/// - 读取失败（权限等非 NotFound IO 错误）/ TOML 解析失败：回退默认并**追加 warning**，
///   避免用户精心配置的 `source_root` 因 typo 被无声丢弃。
fn load_config_or_default(
    warnings: &mut Vec<String>,
) -> rustmigrate_core::types::config::MigrateConfig {
    use rustmigrate_core::types::config::MigrateConfig;
    match std::fs::read_to_string(config_path()) {
        Ok(s) => match toml::from_str::<MigrateConfig>(&s) {
            Ok(cfg) => cfg,
            Err(e) => {
                warnings.push(format!(
                    "{CONFIG_FILE} 解析失败，回退默认配置（路径将用默认值）: {e}"
                ));
                MigrateConfig::default()
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => MigrateConfig::default(),
        Err(e) => {
            warnings.push(format!("{CONFIG_FILE} 读取失败，回退默认配置: {e}"));
            MigrateConfig::default()
        }
    }
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
