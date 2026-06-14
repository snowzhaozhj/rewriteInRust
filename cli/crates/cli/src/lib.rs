use clap::Parser;
use rustmigrate_core::error::MigrateError;
use rustmigrate_core::response::{ErrorData, Response};
use std::io::Write;

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
    Init,
    Profile,
    Graph {
        #[command(subcommand)]
        action: GraphCommands,
    },
    Validate {
        #[command(subcommand)]
        action: ValidateCommands,
    },
    State {
        #[command(subcommand)]
        action: StateCommands,
    },
    Stats {
        #[command(subcommand)]
        action: StatsCommands,
    },
    Scaffold {
        #[command(subcommand)]
        action: ScaffoldCommands,
    },
}

/// Graph 子命令。
#[derive(clap::Subcommand)]
pub enum GraphCommands {
    Build,
    TopoSort,
    Deps { module: String },
    Interfaces { module: String },
    Stats,
}

/// Validate 子命令。
#[derive(clap::Subcommand)]
pub enum ValidateCommands {
    State,
}

/// State 子命令。
#[derive(clap::Subcommand)]
pub enum StateCommands {
    Get {
        module: String,
    },
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
    Loc,
    Compare,
}

/// Scaffold 子命令。
#[derive(clap::Subcommand)]
pub enum ScaffoldCommands {
    Workspace,
}

/// CLI 入口：解析参数并执行，结果写入 writer。
/// 测试中传 Vec<u8> 捕获输出；生产中传 stdout。
pub fn run_with_args<I, S, W>(args: I, writer: &mut W) -> i32
where
    I: IntoIterator<Item = S>,
    S: Into<std::ffi::OsString> + Clone,
    W: Write,
{
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(e) => {
            // --help / --version 等正常退出场景（use_stderr=false）保留 clap 原样人类可读输出；
            // 真正的解析错误统一包成 JSON，保证「所有命令输出可被工具链解析」的契约。
            if e.use_stderr() {
                emit_error(writer, MigrateError::Config(e.to_string()));
                return 1;
            }
            let _ = write!(writer, "{e}");
            return 0;
        }
    };

    match execute(&cli.command, writer) {
        Ok(()) => 0,
        Err(e) => {
            emit_error(writer, e);
            1
        }
    }
}

/// 将错误序列化为统一 JSON 响应写入 writer，避免手工转义产生非法 JSON。
fn emit_error<W: Write>(writer: &mut W, err: MigrateError) {
    let response: Response<ErrorData> = Response::from(err);
    let _ = serde_json::to_writer(&mut *writer, &response);
    let _ = writeln!(writer);
}

fn execute<W: Write>(command: &Commands, writer: &mut W) -> Result<(), MigrateError> {
    match command {
        Commands::Init => {
            let response = Response::ok(serde_json::json!({ "message": "initialized" }));
            serde_json::to_writer(&mut *writer, &response)?;
            writeln!(writer)?;
            Ok(())
        }
        Commands::Profile => Err(not_impl("profile")),
        Commands::Graph { action } => match action {
            GraphCommands::Build => Err(not_impl("graph build")),
            GraphCommands::TopoSort => Err(not_impl("graph topo-sort")),
            GraphCommands::Deps { .. } => Err(not_impl("graph deps")),
            GraphCommands::Interfaces { .. } => Err(not_impl("graph interfaces")),
            GraphCommands::Stats => Err(not_impl("graph stats")),
        },
        Commands::Validate { action } => match action {
            ValidateCommands::State => Err(not_impl("validate state")),
        },
        Commands::State { action } => match action {
            StateCommands::Get { .. } => Err(not_impl("state get")),
            StateCommands::Transition { .. } => Err(not_impl("state transition")),
        },
        Commands::Stats { action } => match action {
            StatsCommands::Loc => Err(not_impl("stats loc")),
            StatsCommands::Compare => Err(not_impl("stats compare")),
        },
        Commands::Scaffold { action } => match action {
            ScaffoldCommands::Workspace => Err(not_impl("scaffold workspace")),
        },
    }
}

/// 构造「命令尚未实现（Phase 2 接线）」错误。
fn not_impl(command: &str) -> MigrateError {
    MigrateError::NotImplemented(format!("{command}（Phase 2 接线）"))
}
