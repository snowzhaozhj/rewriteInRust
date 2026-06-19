//! `rustmigrate` CLI 入口与命令路由。
//!
//! 所有命令统一输出 JSON：`{"status":"ok|error|warning","data":{...},"warnings":[...]}`。
//! 命令权威定义见 `docs/design/06-plugin-structure.md` 的「CLI 命令概览」。
//!
//! 数据流约定（见 `docs/design/04-toolchain.md § 5.7.3`）：
//! - `graph build` 解析源码写入 `.rust-migration/source-graph.db`（SQLite）；
//! - `graph topo-sort` / `deps` / `rdeps` / `interfaces` / `stats` 从该 db 读取；
//! - 迁移状态文件位于 `.rust-migration/migration-state.json`。

use std::io::Write;
use std::path::{Path, PathBuf};

use clap::Parser;
use serde::Serialize;
use serde_json::json;

use rustmigrate_core::detect::detect_tier;
use rustmigrate_core::error::MigrateError;
use rustmigrate_core::graph::build::{build_graph_ts, build_graph_ts_profiled};
use rustmigrate_core::graph::export::{export_dot, export_json, export_mermaid};
use rustmigrate_core::graph::persist::{load_from_db, save_to_db};
use rustmigrate_core::graph::topo::{detect_cycles, migration_sequence, topological_sort};
use rustmigrate_core::graph::SourceGraph;
use rustmigrate_core::profile::{
    check_adapter_tools, check_cargo_nextest, load_analysis_tools, profile_project, ToolStatus,
};
use rustmigrate_core::response::{ErrorData, Response, Status};
use rustmigrate_core::scaffold::scaffold_project;
use rustmigrate_core::state::{MigrationStateMachine, SprintAdvanceResult};
use rustmigrate_core::stats::{compare_structure, count_loc};
use rustmigrate_core::types::common::{NodeId, Timestamp};
use rustmigrate_core::types::graph::{EdgeType, NodeType, SourceNode};
use rustmigrate_core::types::state::{
    humanize_module_key, ModuleState, ModuleStatus, ModuleTier, ProjectState, SprintState,
};
use rustmigrate_core::validate::{
    auto_unblock_modules, check_blocked_modules, detect_blocked_cycles, validate_state,
};

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
    /// 项目级毕业评估：所有模块 done/degrade_* 时，推进 ProjectState 到 Graduate 并产出报告。
    Graduate,
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
    /// 查询模块的反向依赖（imports 入边的传递闭包）。
    Rdeps { module: String },
    /// 输出模块的导出接口签名。
    Interfaces {
        module: String,
        /// 批量输出 target 的直接依赖模块接口（M2 落地）。
        #[arg(long)]
        deps_of: Option<String>,
    },
    /// 图统计信息（节点/边计数、分类计数）。
    Stats,
    /// 循环依赖检测：完整 SCC 环路径输出。
    Cycles,
    /// 导出依赖图为 JSON/DOT/Mermaid 格式。
    Export {
        /// 导出格式（json/dot/mermaid，默认 json）。
        #[arg(long, default_value = "json")]
        format: String,
    },
}

/// Validate 子命令。
#[derive(clap::Subcommand)]
pub enum ValidateCommands {
    /// 校验 `migration-state.json` 合法性。
    State {
        /// 检查 blocked 模块的依赖是否已就绪（blocked_by 全部终态则 ready）。
        #[arg(long)]
        check_blocked: bool,
        /// 自动恢复就绪的 blocked 模块到 pre_blocked_status（需配合 --check-blocked）。
        #[arg(long)]
        auto_unblock: bool,
    },
    /// 校验 `.rustmigrate.toml` 配置文件合法性。
    Config,
}

/// State 子命令。
#[derive(clap::Subcommand)]
pub enum StateCommands {
    /// 查询指定模块的当前迁移状态。
    Get {
        module: String,
        /// 在输出 data 中附加人类友好显示名（`human` 字段；不改变内部 module key）。
        #[arg(long)]
        human: bool,
    },
    /// 执行状态机转换（带合法性前置检查）。
    ///
    /// 提供 `--module` 为**模块级**转换（ModuleStatus）；省略 `--module` 为**项目级**转换
    /// （ProjectState：init/profile/plan/scaffold/sprint_loop/graduate，见 02-architecture § 3.4）。
    /// 项目级转换是 `/migrate analyze`→`/migrate run` 衔接（profile→…→sprint_loop）的接入点。
    Transition {
        /// 模块名。省略则执行项目级 ProjectState 转换（此时 `--to` 必填、`--substatus`/`--force` 不适用）。
        #[arg(long)]
        module: Option<String>,
        /// 目标状态。有 `--module` 时为 ModuleStatus（translating/compile_fixing/testing/reviewing/
        /// done/degrade_*/paused/blocked）；无 `--module` 时为 ProjectState（profile/plan/scaffold/
        /// sprint_loop/graduate）。模块级省略则仅更新 substatus（status 不变，见 09-appendix § Step 2/4）。
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
    /// 用源码图的迁移序列填充 `migration-state.json` 的 `modules`/`sprint`（PLAN 操作）。
    ///
    /// 读取 `source-graph.db` → `migration_sequence()` 拓扑序 → 为每个文件模块写入
    /// `ModuleState{status:pending, sprint:<按parallel_groups>, tier:auto}` 并设 `sprint{current:1}`，原子落盘。
    /// module key 用 NodeId 原值（与 `graph deps` 输出一致，保证 run 阶段依赖门禁匹配）。
    /// 环图（`migration_sequence().has_cycles()`）拒绝填充，须先打破环（对齐 topo-sort 门禁）。
    /// 是 `/migrate analyze`→`/migrate run` 衔接的缺失 PLAN 步骤（见 PLAN.md §9.5 M1-PLAN-01）。
    PopulateModules {
        /// 源码根目录，用于 per-module 复杂度分档（M2-TIER-01a）。
        /// 省略则尝试从当前目录解析源文件。
        #[arg(long)]
        root: Option<PathBuf>,
        /// 所有模块统一分配 sprint=1（兼容 M1 单 sprint 模式）。
        /// 省略时按 parallel_groups 拓扑层级分配 sprint 号（组 0→sprint 1，组 1→sprint 2...）。
        #[arg(long)]
        single_sprint: bool,
    },
    /// 追加一条 SubAgent 调用记录到顶层 `subagent_calls`（诊断卡死 / 统计重试，append-only）。
    ///
    /// 对齐 `09-appendix-schemas.md § subagent_calls 字段说明`：每次 SubAgent 调用（含重试）
    /// 追加一条 `{step_index, subagent_name, started_at, ended_at, status, error_message}`。
    /// `--started-at` / `--ended-at` 取 ISO8601 字符串；`--started-at` 省略时由 CLI 取当前 UTC 时间
    /// （schema 中 started_at 必填，给出合理缺省以便编排器在调用开始时即可记录）。
    RecordSubagentCall {
        /// 编排步骤序号（见 06 § 10.5 编排调度路径）。
        #[arg(long)]
        step_index: u32,
        /// SubAgent 名称（如 translator / verifier / analyzer / scaffolder）。
        #[arg(long)]
        subagent_name: String,
        /// 调用结果状态（如 success / timeout / failed）。
        #[arg(long)]
        status: String,
        /// 调用开始时间（ISO8601）。省略则取当前 UTC 时间。
        #[arg(long)]
        started_at: Option<String>,
        /// 调用结束时间（ISO8601）。进行中 / 卡死场景可省略。
        #[arg(long)]
        ended_at: Option<String>,
        /// 失败 / 超时原因（成功时省略）。
        #[arg(long)]
        error_message: Option<String>,
    },
    /// 推进 sprint：当前 sprint 所有模块终态时，自动推进到下一 sprint。
    ///
    /// sprint N 全模块 done/degrade_* → current=N+1 + history 回填。
    /// 无可推进 sprint 或尚有非终态模块时返回 status:ok + advanced:false。
    AdvanceSprint,
    /// 乐观锁状态更新（`--cas-version` 比较并写入，版本不匹配返回冲突）。
    ///
    /// 读取 `metadata.version` 与 `--cas-version` 比较，匹配时执行模块状态转换并递增 version；
    /// 不匹配时返回 `lock_conflict` 错误。M2 并发安全设计预留（见 06-plugin-structure.md §10.8）。
    Update {
        /// 模块名。
        #[arg(long)]
        module: String,
        /// 目标状态（ModuleStatus）。
        #[arg(long)]
        status: String,
        /// CAS 版本号（从 state file 的 metadata.version 读取）。
        #[arg(long)]
        cas_version: u64,
        /// 子状态（可选）。
        #[arg(long)]
        substatus: Option<String>,
        /// 转换原因（可选，追加到 attempts 审计序列）。
        #[arg(long)]
        reason: Option<String>,
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
    /// 源码与 Rust 结构复杂度对比（LOC + 函数数 + 控制流嵌套）。
    Compare {
        /// 源码根目录（省略则取 `.rustmigrate.toml` 的 `project.source_root`）。
        #[arg(long)]
        source: Option<PathBuf>,
        /// Rust 代码根目录（省略则取 `.rustmigrate.toml` 的 `project.rust_root`）。
        #[arg(long)]
        rust: Option<PathBuf>,
    },
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
                    error_code: String::new(),
                    message: e.to_string(),
                    retryable: false,
                    suggestion: String::new(),
                    context: None,
                    details: None,
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
            GraphCommands::Rdeps { module } => emit(writer, cmd_graph_rdeps(module)),
            GraphCommands::Interfaces { module, deps_of } => {
                emit(writer, cmd_graph_interfaces(module, deps_of.as_deref()))
            }
            GraphCommands::Stats => emit(writer, cmd_graph_stats()),
            GraphCommands::Cycles => emit(writer, cmd_graph_cycles()),
            GraphCommands::Export { format } => emit(writer, cmd_graph_export(format)),
        },
        Commands::Validate { action } => match action {
            ValidateCommands::State {
                check_blocked,
                auto_unblock,
            } => emit(writer, cmd_validate_state(*check_blocked, *auto_unblock)),
            ValidateCommands::Config => emit(writer, cmd_validate_config()),
        },
        Commands::State { action } => match action {
            StateCommands::Get { module, human } => emit(writer, cmd_state_get(module, *human)),
            StateCommands::Transition {
                module,
                to,
                substatus,
                reason,
                force,
            } => emit(
                writer,
                cmd_state_transition(
                    module.as_deref(),
                    to.as_deref(),
                    substatus.as_deref(),
                    reason.as_deref(),
                    *force,
                ),
            ),
            StateCommands::PopulateModules {
                root,
                single_sprint,
            } => emit(
                writer,
                cmd_state_populate_modules(root.as_deref(), *single_sprint),
            ),
            StateCommands::RecordSubagentCall {
                step_index,
                subagent_name,
                status,
                started_at,
                ended_at,
                error_message,
            } => emit(
                writer,
                cmd_state_record_subagent_call(
                    *step_index,
                    subagent_name,
                    status,
                    started_at.as_deref(),
                    ended_at.as_deref(),
                    error_message.as_deref(),
                ),
            ),
            StateCommands::AdvanceSprint => emit(writer, cmd_state_advance_sprint()),
            StateCommands::Update {
                module,
                status,
                cas_version,
                substatus,
                reason,
            } => emit(
                writer,
                cmd_state_update(
                    module,
                    status,
                    *cas_version,
                    substatus.as_deref(),
                    reason.as_deref(),
                ),
            ),
        },
        Commands::Stats { action } => match action {
            StatsCommands::Loc { source, rust } => {
                emit(writer, cmd_stats_loc(source.as_deref(), rust.as_deref()))
            }
            StatsCommands::Compare { source, rust } => emit(
                writer,
                cmd_stats_compare(source.as_deref(), rust.as_deref()),
            ),
        },
        Commands::Scaffold { action } => match action {
            ScaffoldCommands::Workspace { target, name } => {
                emit(writer, cmd_scaffold_workspace(name, target))
            }
        },
        Commands::Graduate => emit(writer, cmd_graduate()),
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

/// 加载状态机，并在因主文件损坏回退 `.backup` 时返回告警。
///
/// 见 [`MigrationStateMachine::recovered_from_backup`]：回退意味着拿到的是上一次保存前的
/// 旧状态，最近进度可能丢失，必须经统一响应（warning 降级 status）显式告知用户。
fn load_state_with_warnings(
    path: &Path,
) -> Result<(MigrationStateMachine, Vec<String>), MigrateError> {
    let mut machine = MigrationStateMachine::load(path)?;
    let mut warnings = Vec::new();
    if machine.recovered_from_backup() {
        warnings.push(
            "migration-state.json 主文件损坏，已从 .backup 恢复——最近一次保存的进度可能丢失，\
             请核对状态后再继续"
                .to_owned(),
        );
    }
    // 注入持久化配置：从 .rustmigrate.toml 读取 [persistence] 段。
    let cfg = load_config_or_default(&mut warnings);
    machine.set_persistence_config(cfg.persistence);
    Ok((machine, warnings))
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
    if let Some(err) = status.probe_error() {
        msg.push_str(&format!("探测失败（{err}），无法确认是否安装"));
    } else if !status.available() {
        msg.push_str("未安装");
    } else if status.min_version.is_some() && status.detected_version().is_none() {
        // 命令成功但版本号无法解析：不应断言「版本不足」。
        if let Some(min) = &status.min_version {
            msg.push_str(&format!("版本无法解析，无法确认是否满足 ≥{min}"));
        }
    } else {
        msg.push_str("版本不足");
        if let Some(min) = &status.min_version {
            msg.push_str(&format!("（需 ≥{min}"));
            if let Some(det) = status.detected_version() {
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
/// 当前以 TypeScript adapter 全量构建。`--full` 在 M1 等价默认（增量推 M2）。
/// `--profile` 开启时输出 per-phase 耗时的性能画像 JSON（见 04-toolchain.md § 5.7.4.1）。
fn cmd_graph_build(root: &Path, _full: bool, profile: bool) -> CmdResult {
    let mut warnings: Vec<String> = Vec::new();

    // TODO(M2): 增量构建（file_fingerprints 跳过未变更文件）；当前 --full 无差异。
    // M1 暂无增量构建：无论 --full 与否都是全量。下方 `full` 字段恒 true 反映真实构建模式。

    let (graph, build_profile) = if profile {
        let (g, p) = build_graph_ts_profiled(root)?;
        (g, Some(p))
    } else {
        (build_graph_ts(root)?, None)
    };
    warnings.extend(graph.warnings().iter().cloned());

    // 确保 `.rust-migration/` 存在后再写 db。
    std::fs::create_dir_all(work_dir())?;
    let db = db_path();

    // 持久化计时（仅 --profile 开启时插桩）
    let persist_ms = if profile {
        let t = std::time::Instant::now();
        save_to_db(&graph, &db)?;
        Some(t.elapsed().as_millis() as u64)
    } else {
        save_to_db(&graph, &db)?;
        None
    };

    // 标记 graph 构建完成（若状态文件存在）。
    mark_graph_built(&mut warnings);

    let mut data = json!({
        "db_path": db.to_string_lossy(),
        "node_count": graph.node_count(),
        "edge_count": graph.edge_count(),
        "full": true,
    });

    // --profile：将性能画像嵌入 data.profile
    if let (Some(bp), Some(pm)) = (build_profile, persist_ms) {
        data["profile"] = json!({
            "parse_ms": bp.parse_ms,
            "edge_build_ms": bp.edge_build_ms,
            "persist_ms": pm,
            "total_ms": bp.total_ms + pm,
        });
    }

    Ok((data, warnings))
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
    match load_state_with_warnings(&state) {
        Ok((mut machine, backup_warnings)) => {
            warnings.extend(backup_warnings);
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
            // 走统一 ErrorData 类型（REFAC-14）：`cycle_path` 经 `details` 的 flatten
            // 提升到 `data` 顶层，路径保持 `data.cycle_path` 不变——对齐设计
            // 09-appendix § Step 2.8 + plugin analyze.md（SKILL 直接读 `data.cycle_path`）。
            let resp = Response {
                status: Status::Error,
                data: ErrorData::new(
                    "cyclic_dependency",
                    "存在循环依赖，无法生成拓扑序；请打破环后重试",
                    Some(json!({ "cycle_path": cycle_paths })),
                ),
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
    let dependencies = collect_imports_closure(&graph, &start, DependencyDirection::Forward);

    Ok((
        json!({
            "module": start.to_string(),
            "dependencies": dependencies,
        }),
        Vec::new(),
    ))
}

/// `graph rdeps <module>`：模块反向依赖（imports 入边的传递闭包，BFS）。
fn cmd_graph_rdeps(module: &str) -> CmdResult {
    let graph = load_graph()?;
    let warnings = graph.warnings().to_vec();
    let start = resolve_file_node(&graph, module)?;
    let dependents = collect_imports_closure(&graph, &start, DependencyDirection::Reverse);

    Ok((
        json!({
            "module": start.to_string(),
            "dependents": dependents,
        }),
        warnings,
    ))
}

#[derive(Clone, Copy)]
enum DependencyDirection {
    Forward,
    Reverse,
}

fn collect_imports_closure(
    graph: &SourceGraph,
    start: &NodeId,
    direction: DependencyDirection,
) -> Vec<String> {
    let mut visited: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut queue: std::collections::VecDeque<NodeId> = std::collections::VecDeque::new();
    queue.push_back(start.clone());
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    seen.insert(start.as_str().to_owned());

    while let Some(cur) = queue.pop_front() {
        let edges = match direction {
            DependencyDirection::Forward => graph.outgoing_edges(&cur),
            DependencyDirection::Reverse => graph.incoming_edges(&cur),
        };
        for (node, edge_type) in edges {
            if edge_type != EdgeType::Imports {
                continue;
            }
            let id = node.id.as_str().to_owned();
            if seen.insert(id.clone()) {
                visited.insert(id);
                queue.push_back(node.id.clone());
            }
        }
    }

    visited.into_iter().collect()
}

/// `graph interfaces <module> [--deps-of <target>]`：模块导出接口签名。
///
/// 无 `--deps-of`：输出 `<module>` 内 `is_exported=true` 的符号节点。
/// 有 `--deps-of`：以 `<target>` 为中心，取其 imports 边的 1-hop 出边邻居（直接依赖模块），
/// 批量输出每个依赖模块的导出接口签名（区别于 `graph deps` 的 BFS 传递闭包）。
fn cmd_graph_interfaces(module: &str, deps_of: Option<&str>) -> CmdResult {
    let graph = load_graph()?;

    // --deps-of 批量模式：输出 target 直接依赖模块的导出接口。
    if let Some(target) = deps_of {
        return cmd_graph_interfaces_deps_of(&graph, target);
    }

    // 单模块模式：输出指定模块自身的导出接口。
    let file_path = resolve_module_file_path(&graph, module)?;
    let interfaces = collect_exported_interfaces(&graph, &file_path);

    Ok((
        json!({
            "module": file_path,
            "interfaces": interfaces,
        }),
        Vec::new(),
    ))
}

/// `--deps-of` 批量模式：获取 target 的 1-hop imports 出边邻居，收集每个邻居的导出接口。
fn cmd_graph_interfaces_deps_of(
    graph: &rustmigrate_core::graph::SourceGraph,
    target: &str,
) -> CmdResult {
    let target_node = resolve_file_node(graph, target)?;
    let target_path = target_node
        .file_path()
        .map(|p| p.to_owned())
        .ok_or_else(|| MigrateError::Graph {
            message: format!("无法解析模块文件路径: {target}"),
            file: target.to_owned(),
        })?;

    // 取 imports 边的 1-hop 出边邻居（仅 File 节点）。
    let dep_nodes: Vec<&SourceNode> = graph
        .outgoing_edges(&target_node)
        .into_iter()
        .filter(|(_, edge_type)| *edge_type == EdgeType::Imports)
        .map(|(node, _)| node)
        .filter(|node| node.node_type == NodeType::File)
        .collect();

    // 去重并排序（BTreeSet 保证有序迭代，确定性输出）。
    let dep_paths: Vec<String> = dep_nodes
        .iter()
        .map(|n| n.file_path.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();

    // 对每个依赖模块收集导出接口。
    let dependencies: Vec<serde_json::Value> = dep_paths
        .iter()
        .map(|path| {
            let exports = collect_exported_interfaces(graph, path);
            json!({
                "module": path,
                "exports": exports,
            })
        })
        .collect();

    Ok((
        json!({
            "target": target_path,
            "dependencies": dependencies,
        }),
        Vec::new(),
    ))
}

/// 解析模块名为文件路径字符串。
fn resolve_module_file_path(
    graph: &rustmigrate_core::graph::SourceGraph,
    module: &str,
) -> Result<String, MigrateError> {
    let file_node = resolve_file_node(graph, module)?;
    file_node
        .file_path()
        .map(|p| p.to_owned())
        .ok_or_else(|| MigrateError::Graph {
            message: format!("无法解析模块文件路径: {module}"),
            file: module.to_owned(),
        })
}

/// 收集指定文件下的导出接口（`is_exported=true` 的符号），按名称排序。
fn collect_exported_interfaces(
    graph: &rustmigrate_core::graph::SourceGraph,
    file_path: &str,
) -> Vec<serde_json::Value> {
    let mut interfaces: Vec<serde_json::Value> = graph
        .symbols_in_file(file_path)
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
    // 确定性排序（symbols_in_file 顺序依赖图遍历，需显式排序）。
    interfaces.sort_by(|a, b| {
        a["name"]
            .as_str()
            .unwrap_or_default()
            .cmp(b["name"].as_str().unwrap_or_default())
    });
    interfaces
}

/// `graph stats`：图统计信息。
fn cmd_graph_stats() -> CmdResult {
    let graph = load_graph()?;
    let stats = graph.stats();
    Ok((serde_json::to_value(&stats)?, Vec::new()))
}

/// `graph cycles`：循环依赖检测，输出完整 SCC 环路径。
///
/// 使用 Tarjan SCC 算法检测所有强连通分量（大小 > 1 或自环），
/// 将每个 SCC 中的 NodeId 转为字符串输出。无环时 `has_cycles: false`。
fn cmd_graph_cycles() -> CmdResult {
    let graph = load_graph()?;
    let cycles = detect_cycles(&graph);
    let cycle_paths: Vec<Vec<String>> = cycles
        .iter()
        .map(|c| c.iter().map(|id| id.to_string()).collect())
        .collect();
    Ok((
        json!({
            "has_cycles": !cycle_paths.is_empty(),
            "cycle_count": cycle_paths.len(),
            "cycles": cycle_paths,
        }),
        Vec::new(),
    ))
}

/// `graph export --format <json|dot|mermaid>`：导出依赖图。
///
/// 设计（`06-plugin-structure.md`）：导出依赖图为 JSON/DOT/Mermaid 格式。
/// json 格式时 `data.content` 为对象（nodes + edges）；
/// dot/mermaid 格式时 `data.content` 为字符串。
fn cmd_graph_export(format: &str) -> CmdResult {
    let graph = load_graph()?;

    match format {
        "json" => {
            let content = export_json(&graph);
            Ok((
                json!({
                    "format": "json",
                    "content": content,
                }),
                Vec::new(),
            ))
        }
        "dot" => {
            let content = export_dot(&graph);
            Ok((
                json!({
                    "format": "dot",
                    "content": content,
                }),
                Vec::new(),
            ))
        }
        "mermaid" => {
            let content = export_mermaid(&graph);
            Ok((
                json!({
                    "format": "mermaid",
                    "content": content,
                }),
                Vec::new(),
            ))
        }
        _ => Err(MigrateError::Config(format!(
            "不支持的导出格式: {format}（合法值: json/dot/mermaid）"
        ))),
    }
}

/// `validate state [--check-blocked] [--auto-unblock]`：校验 `migration-state.json`。
///
/// 基础模式（无 flag）：执行 schema 版本、历史链、前置条件等完整性检查。
///
/// `--check-blocked`：额外检查所有 blocked 模块的 `blocked_by` 依赖是否已进入终态，
/// 并执行 DFS 环检测（blocked_by 关系图中的环路会导致死锁）。
///
/// `--auto-unblock`（需配合 `--check-blocked`）：对就绪的 blocked 模块自动恢复到
/// `pre_blocked_status`（无则默认 `pending`），通过 `transition_module` 落盘。
fn cmd_validate_state(check_blocked: bool, auto_unblock: bool) -> CmdResult {
    // --auto-unblock 须配合 --check-blocked。
    if auto_unblock && !check_blocked {
        return Err(MigrateError::Config(
            "--auto-unblock 须配合 --check-blocked 使用".to_owned(),
        ));
    }

    let path = state_path();
    let (mut machine, mut warnings) = load_state_with_warnings(&path)?;

    // 基础 validate。
    warnings.extend(validate_state(machine.state_file())?);

    if !check_blocked {
        return Ok((json!({ "valid": true }), warnings));
    }

    // --check-blocked：检查 blocked 模块依赖就绪状态 + DFS 环检测。
    let checks = check_blocked_modules(machine.state_file());
    let cycles = detect_blocked_cycles(machine.state_file());

    let blocked_count = checks.len();
    let ready_modules: Vec<String> = checks
        .iter()
        .filter(|r| r.ready)
        .map(|r| r.module.clone())
        .collect();
    let still_blocked: Vec<String> = checks
        .iter()
        .filter(|r| !r.ready)
        .map(|r| r.module.clone())
        .collect();
    let cycle_paths: Vec<Vec<String>> = cycles;

    let mut data = json!({
        "valid": true,
        "blocked_count": blocked_count,
        "ready_to_unblock": ready_modules,
        "still_blocked": still_blocked,
        "cycles": cycle_paths,
    });

    // --auto-unblock：自动恢复就绪的 blocked 模块。
    if auto_unblock {
        // 有环时拒绝自动解除（设计 09-appendix：环检测 → 报错中止）。
        if !cycle_paths.is_empty() {
            return Err(MigrateError::Config(format!(
                "blocked_by 关系图存在环路，无法自动解除；请先打破环后重试。环路径: {cycle_paths:?}"
            )));
        }

        let unblocked = auto_unblock_modules(&mut machine, &checks, &mut warnings);
        if !unblocked.is_empty() {
            machine.save(&path)?;
        }
        data["unblocked"] = json!(unblocked);
    }

    Ok((data, warnings))
}

/// `validate config`：校验 `.rustmigrate.toml` 配置文件合法性。
///
/// 三种场景：
/// - 文件不存在：status=ok，warning 提示"未找到配置文件，使用默认值"
/// - 文件存在且合法：status=ok，输出各字段检查数
/// - 文件存在但有不合理字段：status=warning，附带具体问题
fn cmd_validate_config() -> CmdResult {
    use rustmigrate_core::types::config::MigrateConfig;

    let path = config_path();
    let mut warnings: Vec<String> = Vec::new();

    // 配置文件不存在：正常场景，使用默认值。
    if !path.exists() {
        warnings.push("未找到配置文件，使用默认值".to_owned());
        return Ok((
            json!({
                "config_path": CONFIG_FILE,
                "valid": true,
                "fields_checked": 0,
            }),
            warnings,
        ));
    }

    // 读取并解析配置文件。
    let content = std::fs::read_to_string(&path)?;
    let config: MigrateConfig = match toml::from_str(&content) {
        Ok(cfg) => cfg,
        Err(e) => {
            return Err(MigrateError::Config(format!("{CONFIG_FILE} 解析失败: {e}")));
        }
    };

    // 逐字段合理性校验。
    let mut fields_checked: u32 = 0;

    // 1. project.source_root 路径检查
    fields_checked += 1;
    let source_root = Path::new(&config.project.source_root);
    if !source_root.exists() {
        warnings.push(format!(
            "project.source_root 目录不存在: {}",
            config.project.source_root
        ));
    }

    // 2. project.rust_root 路径检查
    fields_checked += 1;
    let rust_root = Path::new(&config.project.rust_root);
    if !rust_root.exists() {
        warnings.push(format!(
            "project.rust_root 目录不存在: {}（迁移早期属正常）",
            config.project.rust_root
        ));
    }

    // 3. strategy.max_retry_rounds 合理性
    fields_checked += 1;
    if config.strategy.max_retry_rounds == 0 {
        warnings.push("strategy.max_retry_rounds 为 0，翻译失败后不会重试".to_owned());
    }

    // 4. testing.coverage_threshold 范围检查（0-100）
    fields_checked += 1;
    if config.testing.coverage_threshold > 100 {
        warnings.push(format!(
            "testing.coverage_threshold 超出合理范围（0-100）: {}",
            config.testing.coverage_threshold
        ));
    }

    // 5. orchestration.subagent_timeout_secs 合理性
    fields_checked += 1;
    if config.orchestration.subagent_timeout_secs == 0 {
        warnings.push("orchestration.subagent_timeout_secs 为 0，SubAgent 将立即超时".to_owned());
    }

    let valid = warnings.is_empty();

    Ok((
        json!({
            "config_path": CONFIG_FILE,
            "valid": valid,
            "fields_checked": fields_checked,
        }),
        warnings,
    ))
}

/// `state get <module> [--human]`：查询指定模块迁移状态。
///
/// `--human` 开启时在 data 中附加人类友好显示名（`human` 字段，由
/// [`humanize_module_key`] 从内部 key 派生）；内部 `module` key 保持不变，向后兼容。
fn cmd_state_get(module: &str, human: bool) -> CmdResult {
    let (machine, warnings) = load_state_with_warnings(&state_path())?;
    let state_file = machine.state_file();
    match state_file.modules.get(module) {
        Some(m) => {
            let mut data = json!({
                "module": module,
                "status": m.status.to_string(),
                "state": serde_json::to_value(m)?,
            });
            if human {
                data["human"] = json!(humanize_module_key(module));
            }
            Ok((data, warnings))
        }
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
    module: Option<&str>,
    to: Option<&str>,
    substatus: Option<&str>,
    reason: Option<&str>,
    force: bool,
) -> CmdResult {
    // 无 --module：项目级 ProjectState 转换（profile→…→sprint_loop→graduate）。
    let Some(module) = module else {
        return cmd_state_transition_project(to, substatus, reason, force);
    };
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
    let (mut machine, warnings) = load_state_with_warnings(&path)?;
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
        warnings,
    ))
}

/// `state transition --to <ProjectState>`（无 `--module`）：项目级状态机转换。
///
/// 驱动 `init→profile→plan→scaffold→sprint_loop→graduate`（合法性由
/// [`ProjectState::can_transition_to`] 校验），是 `/migrate analyze`（推进到 `profile`）
/// 与 `/migrate run`（前置要求 `sprint_loop`）之间的衔接接入点。`--substatus`/`--reason`/`--force`
/// 为模块级概念（substatus 是 Phase 进度、reason 落 attempts 审计、force 是 degrade 恢复），
/// 项目级 `transition` 不落这些字段——显式拒绝以免静默吞参。
fn cmd_state_transition_project(
    to: Option<&str>,
    substatus: Option<&str>,
    reason: Option<&str>,
    force: bool,
) -> CmdResult {
    if substatus.is_some() || reason.is_some() || force {
        return Err(MigrateError::Config(
            "项目级 state transition（无 --module）不支持 --substatus / --reason / --force（仅模块级适用）"
                .to_owned(),
        ));
    }
    let to = to.ok_or_else(|| {
        MigrateError::Config("项目级 state transition 必须指定 --to <ProjectState>".to_owned())
    })?;
    // 提示仅列「可作为转换目标」的状态：init 是初始态，无任何 can_transition_to 规则以其为 target，
    // 故不列入（照其转换必失败，徒增误导）。
    let target = to.parse::<ProjectState>().map_err(|_| {
        MigrateError::Config(format!(
            "非法 ProjectState: {to}（合法值: profile/plan/scaffold/sprint_loop/graduate）"
        ))
    })?;

    // graduate 须走 `rustmigrate graduate` 命令（含模块终态前置检查 + 报告产出），
    // 不允许通过 state transition 绕过。
    if target == ProjectState::Graduate {
        return Err(MigrateError::Config(
            "graduate 须通过 `rustmigrate graduate` 命令执行（含模块终态检查 + 毕业报告），\
             不支持 state transition --to graduate 直接推进"
                .to_owned(),
        ));
    }

    let path = state_path();
    let (mut machine, warnings) = load_state_with_warnings(&path)?;
    let from = machine.current_state();
    machine.transition(target)?;
    machine.save(&path)?;

    Ok((
        json!({
            "from": from.to_string(),
            "state": target.to_string(),
        }),
        warnings,
    ))
}

/// `state populate-modules`：用源码图迁移序列填充 `modules`/`sprint`（PLAN 操作）。
///
/// 这是 `/migrate analyze`→`/migrate run` 衔接的缺失环节（见 PLAN.md §9.5 M1-PLAN-01）：
/// analyze 构建源码图后，需把拓扑序"落"成可执行的模块清单，run 阶段才有 `modules[target]`
/// 可读、依赖门禁（`graph deps` + `modules[dep].status`）才成立。
///
/// 流程：`load_graph` → `migration_sequence()` →（有环则拒绝，对齐 topo-sort 门禁）→
/// 清理孤儿 pending（见下）→ 为每个文件模块写
/// `ModuleState{status:pending, sprint:<parallel_groups层级>, tier:auto}`（module key = NodeId 原值，
/// 与 `graph deps` 输出一致）→ `set_sprint(current:1)` → 原子落盘。
///
/// **幂等保护**：已有模块处于非 `pending` 活跃态（迁移进行中）时拒绝覆盖，避免把进度重置回
/// `pending`（断点续传安全）；仅当全部模块仍为 pending（或全新）时才整体重填。
///
/// **孤儿 pending 清理**：源码图删文件后重填时，上一轮登记、本轮序列已不含的 pending 模块会成为
/// "孤儿"（状态中存在但源码图已无对应节点）。重填只新增/覆盖序列内节点，故先用
/// [`MigrationStateMachine::retain_modules`] 剔除孤儿，保持 `modules` 与当前迁移序列一致，
/// 避免不存在的模块被 `state report` / 依赖门禁误计入进度；被清理的 key 经 warning 告知用户。
fn cmd_state_populate_modules(root: Option<&Path>, single_sprint: bool) -> CmdResult {
    let graph = load_graph()?;
    let sequence = migration_sequence(&graph);

    // 环图拒绝：与 topo-sort 门禁一致——有环无法生成可靠迁移序，须先打破环。
    if sequence.has_cycles() {
        let cycle_paths: Vec<Vec<String>> = sequence
            .cycles
            .iter()
            .map(|c| c.iter().map(|id| id.to_string()).collect())
            .collect();
        return Err(MigrateError::Graph {
            message: format!(
                "源码图存在循环依赖，无法填充迁移序列；请先打破环后重试。环路径: {cycle_paths:?}"
            ),
            file: String::new(),
        });
    }

    let path = state_path();
    let (mut machine, mut warnings) = load_state_with_warnings(&path)?;

    // 断点续传保护：任一模块已离开 pending（迁移进行中/已完成）则拒绝重填，避免重置进度。
    if let Some(active) = machine
        .state_file()
        .modules
        .iter()
        .find(|(_, m)| m.status != ModuleStatus::Pending)
    {
        return Err(MigrateError::Config(format!(
            "模块 `{}` 已处于 `{}`（非 pending），拒绝重填以免重置迁移进度；\
             如需重建请先清空 modules",
            active.0, active.1.status
        )));
    }

    if sequence.order.is_empty() {
        // 空源码图：跳过孤儿清理，避免误执行一次空 graph build 后把已登记的 pending 模块整表清空。
        warnings.push("源码图无文件模块，填充结果为空（请确认已运行 graph build）".to_owned());
    } else {
        // 孤儿 pending 清理：剔除 key 不在本轮迁移序列中的残留模块（源码图删文件后重填）。
        let live_keys: std::collections::HashSet<String> =
            sequence.order.iter().map(|id| id.to_string()).collect();
        let orphans = machine.retain_modules(&live_keys);
        if !orphans.is_empty() {
            warnings.push(format!(
                "已清理 {} 个孤儿 pending 模块（源码图已无对应节点）: {:?}",
                orphans.len(),
                orphans
            ));
        }
    }

    // 复杂度分档（M2-TIER-01a）：按 AST 语义特征自动评估每个模块的 tier。
    let tier_map = detect_tiers_for_modules(&sequence.order, root, &mut warnings);

    // Sprint 分配（M2-SCALE-P）：按 parallel_groups 拓扑层级分配 sprint 号。
    // 组 0（叶节点）→ sprint 1，组 1 → sprint 2，...
    // --single-sprint 模式下所有模块统一 sprint=1（M1 兼容）。
    let sprint_map: std::collections::HashMap<String, u32> = if single_sprint {
        sequence
            .order
            .iter()
            .map(|id| (id.to_string(), 1))
            .collect()
    } else {
        let mut map = std::collections::HashMap::new();
        for (group_idx, group) in sequence.parallel_groups.iter().enumerate() {
            let sprint = (group_idx as u32) + 1;
            for node_id in group {
                map.insert(node_id.to_string(), sprint);
            }
        }
        map
    };

    let total_sprints = if single_sprint {
        1
    } else {
        sequence.parallel_groups.len().max(1) as u32
    };

    for node_id in &sequence.order {
        let tier = tier_map.get(node_id.as_str()).copied().flatten();
        let sprint = sprint_map.get(node_id.as_str()).copied().unwrap_or(1);
        machine.update_module(
            node_id.as_str(),
            ModuleState {
                status: ModuleStatus::Pending,
                substatus: None,
                sprint: Some(sprint),
                attempts: Vec::new(),
                test_pass_rate: None,
                coverage: None,
                known_differences: 0,
                tier,
                phase_a_version: None,
                phase_a_audit_passed: None,
                blocked_by: None,
                pre_blocked_status: None,
            },
        );
    }

    machine.set_sprint(SprintState {
        current: 1,
        history: Vec::new(),
    });
    machine.save(&path)?;

    let modules: Vec<serde_json::Value> = sequence
        .order
        .iter()
        .map(|id| {
            let tier = tier_map
                .get(id.as_str())
                .and_then(|t| t.map(|t| t.to_string()));
            let sprint = sprint_map.get(id.as_str()).copied().unwrap_or(1);
            json!({ "id": id.to_string(), "tier": tier, "sprint": sprint })
        })
        .collect();
    Ok((
        json!({
            "module_count": modules.len(),
            "modules": modules,
            "total_sprints": total_sprints,
        }),
        warnings,
    ))
}

/// 对迁移序列中的所有模块运行复杂度分档检测（M2-TIER-01a）。
///
/// 从 NodeId 解析文件路径 → 在 `root`（或 CWD）下查找源文件 → `detect_tier` 评估。
/// 文件不存在时静默跳过（tier 为 best-effort，不阻塞 populate）；
/// 仅在检测逻辑本身失败时记 warning。
fn detect_tiers_for_modules(
    order: &[NodeId],
    root: Option<&Path>,
    warnings: &mut Vec<String>,
) -> std::collections::HashMap<String, Option<ModuleTier>> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let base = root.unwrap_or(&cwd);

    let mut tiers = std::collections::HashMap::new();
    for node_id in order {
        let file_path = match node_id.file_path() {
            Some(p) => p,
            None => {
                tiers.insert(node_id.to_string(), None);
                continue;
            }
        };

        let full_path = base.join(file_path);
        if full_path.exists() {
            match detect_tier(&full_path) {
                Ok(tier) => {
                    tiers.insert(node_id.to_string(), Some(tier));
                }
                Err(e) => {
                    warnings.push(format!(
                        "模块 `{node_id}` 复杂度检测失败（默认不分档）: {e}"
                    ));
                    tiers.insert(node_id.to_string(), None);
                }
            }
        } else {
            tiers.insert(node_id.to_string(), None);
        }
    }
    tiers
}

/// `state advance-sprint`：当前 sprint 所有模块终态时推进到下一 sprint。
///
/// 检查当前 sprint 下所有模块是否终态。推进成功返回 `advanced:true`；
/// 全部 sprint 已完成返回 `all_completed:true`；尚有非终态模块返回 `advanced:false`。
fn cmd_state_advance_sprint() -> CmdResult {
    let path = state_path();
    let (mut machine, warnings) = load_state_with_warnings(&path)?;

    match machine.try_advance_sprint() {
        SprintAdvanceResult::Advanced(new_sprint) => {
            machine.save(&path)?;
            Ok((
                json!({
                    "advanced": true,
                    "new_sprint": new_sprint,
                }),
                warnings,
            ))
        }
        SprintAdvanceResult::AllCompleted => {
            machine.save(&path)?;
            Ok((
                json!({
                    "advanced": false,
                    "all_completed": true,
                }),
                warnings,
            ))
        }
        SprintAdvanceResult::NotReady => {
            let current = machine
                .state_file()
                .sprint
                .as_ref()
                .map(|s| s.current)
                .unwrap_or(0);
            Ok((
                json!({
                    "advanced": false,
                    "current_sprint": current,
                }),
                warnings,
            ))
        }
    }
}

/// `state update --module <m> --status <s> --cas-version <v> [--substatus <s>] [--reason <r>]`：
/// 乐观锁状态更新（CAS：Compare-And-Swap）。
///
/// 设计（`06-plugin-structure.md` M2 扩展 §10.8）：`metadata` 增加 `version` 字段支持乐观锁；
/// 写入时携带 `--cas-version`，版本不匹配返回冲突错误，防止并发写覆盖。
fn cmd_state_update(
    module: &str,
    status: &str,
    cas_version: u64,
    substatus: Option<&str>,
    reason: Option<&str>,
) -> CmdResult {
    // 解析目标状态（ModuleStatus）。
    let target = status.parse::<ModuleStatus>().map_err(|_| {
        MigrateError::Config(format!(
            "非法 ModuleStatus: {status}（合法值: pending/translating/compile_fixing/testing/\
             reviewing/done/degrade_ffi/degrade_manual/degrade_skip/paused/blocked）"
        ))
    })?;

    let path = state_path();
    let (mut machine, warnings) = load_state_with_warnings(&path)?;
    let (previous_status, new_version) =
        machine.update_with_cas(module, target, cas_version, substatus, reason)?;
    machine.save(&path)?;

    Ok((
        json!({
            "module": module,
            "previous_status": previous_status.to_string(),
            "new_status": target.to_string(),
            "new_version": new_version,
        }),
        warnings,
    ))
}

/// [`SubAgentCall`] 后经 core [`MigrationStateMachine::push_subagent_call`] 入库，落盘走 tmp-fsync-rename
/// 原子写。`started_at` schema 必填——省略时取当前 UTC 时间（编排器在调用开始即可记录、结束再补登一条）。
fn cmd_state_record_subagent_call(
    step_index: u32,
    subagent_name: &str,
    status: &str,
    started_at: Option<&str>,
    ended_at: Option<&str>,
    error_message: Option<&str>,
) -> CmdResult {
    let path = state_path();
    let (mut machine, warnings) = load_state_with_warnings(&path)?;
    // started_at 缺省由 core 取当前 UTC 时间（schema 必填）。
    let count = machine.push_subagent_call(
        step_index,
        subagent_name.to_owned(),
        status.to_owned(),
        started_at.map(Timestamp::from),
        ended_at.map(Timestamp::from),
        error_message.map(str::to_owned),
    );
    machine.save(&path)?;

    Ok((
        json!({
            "recorded": true,
            "subagent_calls_count": count,
        }),
        warnings,
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

/// `stats compare`：源码与 Rust 结构复杂度对比（LOC + 函数数 + 控制流嵌套）。
///
/// 路径解析与 `stats loc` 同口径：CLI 参数 > 配置文件 > 默认值，并复用包含关系告警。
/// 任一侧目录不存在时返回 `Null` data + warning（迁移早期 Rust 端常未生成，属正常），
/// 与 `stats loc` 的「缺目录不报错只告警」行为一致。
fn cmd_stats_compare(source: Option<&Path>, rust: Option<&Path>) -> CmdResult {
    let mut warnings: Vec<String> = Vec::new();
    let cfg = load_config_or_default(&mut warnings);
    let source_root = source
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(&cfg.project.source_root));
    let rust_root = rust
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(&cfg.project.rust_root));

    // stats compare 源侧走 tree-sitter-typescript 解析（measure_source 强绑 TS）。非 TS 项目若放行，
    // 源侧会收集到 0 个文件、静默给出半残比值（functions/nesting 全 0、ratio 为 null）——显式报错
    // 而非静默半残。Python/C/Go 前端在 M3 按 source_language 分派解析器。
    if cfg.project.source_language != rustmigrate_core::types::common::SourceLang::TypeScript {
        return Err(MigrateError::Config(format!(
            "stats compare 当前仅支持 TypeScript 源（配置 source_language={}）；\
             Python/C/Go 源的结构对比在 M3 实现",
            cfg.project.source_language
        )));
    }

    // 与 stats loc 同：源/Rust 目录互相包含时统计会污染，显式告警不静默。
    if let Some(outer) = roots_overlap(&source_root, &rust_root) {
        warnings.push(format!(
            "source 与 rust 目录存在包含关系（{outer} 包含另一侧），结构对比可能重复计数；\
             建议将 source_root / rust_root 配置为互不包含的目录"
        ));
    }

    match compare_structure(&source_root, &rust_root) {
        Ok(report) => match serde_json::to_value(&report) {
            Ok(v) => Ok((v, warnings)),
            Err(e) => {
                warnings.push(format!("结构对比结果序列化失败: {e}"));
                Ok((serde_json::Value::Null, warnings))
            }
        },
        Err(MigrateError::FileNotFound(p)) => {
            warnings.push(format!(
                "源码或 Rust 目录不存在，跳过结构对比: {}",
                p.display()
            ));
            Ok((serde_json::Value::Null, warnings))
        }
        Err(e) => {
            warnings.push(format!("结构对比失败: {e}"));
            Ok((serde_json::Value::Null, warnings))
        }
    }
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

/// `graduate`：项目级毕业评估。
///
/// 前置条件：ProjectState == SprintLoop 且所有模块为终态（done/degrade_*）。
/// 成功时推进 ProjectState 到 Graduate，产出 graduation-report.json
/// 到 `.rust-migration/reports/`。不新增 `graduated` 模块状态（设计文档 09 附录 A
/// 将 GRADUATE 映射为项目级概念）。
fn cmd_graduate() -> CmdResult {
    let path = state_path();
    let (mut machine, mut warnings) = load_state_with_warnings(&path)?;

    // 前置检查：必须处于 SprintLoop 状态。
    if machine.current_state() != ProjectState::SprintLoop {
        return Err(MigrateError::Config(format!(
            "graduate 需在 sprint_loop 状态执行，当前状态: {}",
            machine.current_state()
        )));
    }

    // 前置检查：所有模块必须为终态。
    let modules = &machine.state_file().modules;
    if modules.is_empty() {
        return Err(MigrateError::Config(
            "无模块记录，请先运行 populate-modules".to_owned(),
        ));
    }
    let non_terminal: Vec<(&String, String)> = modules
        .iter()
        .filter(|(_, m)| !m.status.is_terminal())
        .map(|(k, m)| (k, m.status.to_string()))
        .collect();
    if !non_terminal.is_empty() {
        let summary: Vec<String> = non_terminal
            .iter()
            .take(5)
            .map(|(k, s)| format!("{k}={s}"))
            .collect();
        return Err(MigrateError::Config(format!(
            "{} 个模块尚未终态，无法毕业: {}{}",
            non_terminal.len(),
            summary.join(", "),
            if non_terminal.len() > 5 { " ..." } else { "" }
        )));
    }

    // 统计毕业报告数据。
    let total = modules.len();
    let done_count = modules
        .values()
        .filter(|m| m.status == ModuleStatus::Done)
        .count();
    let degraded_count = modules.values().filter(|m| m.status.is_degraded()).count();
    let degraded_modules: Vec<serde_json::Value> = modules
        .iter()
        .filter(|(_, m)| m.status.is_degraded())
        .map(|(k, m)| json!({"module": k, "status": m.status.to_string()}))
        .collect();

    // 推进项目状态到 Graduate。
    machine.transition(ProjectState::Graduate)?;
    machine.save(&path)?;

    // 产出 graduation-report.json。
    let report = json!({
        "project": machine.state_file().project.as_ref().map(|p| &p.name),
        "total_modules": total,
        "done": done_count,
        "degraded": degraded_count,
        "degraded_modules": degraded_modules,
        "graduated_at": Timestamp::now().as_str(),
    });
    let reports_dir = work_dir().join("reports");
    if let Err(e) = std::fs::create_dir_all(&reports_dir) {
        warnings.push(format!("创建 reports 目录失败: {e}"));
    } else {
        let report_path = reports_dir.join("graduation-report.json");
        match serde_json::to_string_pretty(&report) {
            Ok(content) => {
                if let Err(e) = std::fs::write(&report_path, content) {
                    warnings.push(format!("写入 graduation-report.json 失败: {e}"));
                }
            }
            Err(e) => warnings.push(format!("序列化毕业报告失败: {e}")),
        }
    }

    Ok((report, warnings))
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
