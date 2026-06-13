use clap::Parser;
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
            let _ = write!(writer, "{e}");
            return if e.use_stderr() { 1 } else { 0 };
        }
    };

    match execute(&cli.command, writer) {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(
                writer,
                r#"{{"status":"error","data":{{"message":"{}"}}}}"#,
                e.to_string().replace('"', "\\\"")
            );
            1
        }
    }
}

fn execute<W: Write>(command: &Commands, writer: &mut W) -> anyhow::Result<()> {
    match command {
        Commands::Init => {
            writeln!(
                writer,
                r#"{{"status":"ok","data":{{"message":"initialized"}}}}"#
            )?;
            Ok(())
        }
        Commands::Profile => todo!("Phase 2: profile 命令集成"),
        Commands::Graph { action } => match action {
            GraphCommands::Build => todo!("Phase 2: graph build 命令集成"),
            GraphCommands::TopoSort => todo!("Phase 2: graph topo-sort 命令集成"),
            GraphCommands::Deps { .. } => todo!("Phase 2: graph deps 命令集成"),
            GraphCommands::Interfaces { .. } => todo!("Phase 2: graph interfaces 命令集成"),
            GraphCommands::Stats => todo!("Phase 2: graph stats 命令集成"),
        },
        Commands::Validate { action } => match action {
            ValidateCommands::State => todo!("Phase 2: validate state 命令集成"),
        },
        Commands::State { action } => match action {
            StateCommands::Get { .. } => todo!("Phase 2: state get 命令集成"),
            StateCommands::Transition { .. } => todo!("Phase 2: state transition 命令集成"),
        },
        Commands::Stats { action } => match action {
            StatsCommands::Loc => todo!("Phase 2: stats loc 命令集成"),
            StatsCommands::Compare => todo!("Phase 2: stats compare 命令集成"),
        },
        Commands::Scaffold { action } => match action {
            ScaffoldCommands::Workspace => todo!("Phase 2: scaffold workspace 命令集成"),
        },
    }
}
