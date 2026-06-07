use clap::Parser;
use std::io::Write;

#[derive(Parser)]
#[command(name = "rustmigrate", version, about = "Rust 迁移验证工作台 CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

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

#[derive(clap::Subcommand)]
pub enum GraphCommands {
    Build,
    TopoSort,
    Deps { module: String },
    Interfaces { module: String },
    Stats,
}

#[derive(clap::Subcommand)]
pub enum ValidateCommands {
    State,
}

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

#[derive(clap::Subcommand)]
pub enum StatsCommands {
    Loc,
    Compare,
}

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
        Commands::Profile => {
            rustmigrate_core::profile::analyze();
            Ok(())
        }
        Commands::Graph { action } => match action {
            GraphCommands::Build => {
                rustmigrate_core::graph::build();
                Ok(())
            }
            GraphCommands::TopoSort => todo!("M1-TOPO"),
            GraphCommands::Deps { .. } => todo!("M1-DEPS"),
            GraphCommands::Interfaces { .. } => todo!("M1-INTERFACES"),
            GraphCommands::Stats => todo!("M1-STATS"),
        },
        Commands::Validate { action } => match action {
            ValidateCommands::State => {
                rustmigrate_core::validate::validate_state();
                Ok(())
            }
        },
        Commands::State { action } => match action {
            StateCommands::Get { .. } => {
                rustmigrate_core::state::get();
                Ok(())
            }
            StateCommands::Transition { .. } => {
                rustmigrate_core::state::transition();
                Ok(())
            }
        },
        Commands::Stats { action } => match action {
            StatsCommands::Loc => todo!("M1-LOC"),
            StatsCommands::Compare => todo!("M1-COMPARE"),
        },
        Commands::Scaffold { action } => match action {
            ScaffoldCommands::Workspace => {
                rustmigrate_core::scaffold::workspace();
                Ok(())
            }
        },
    }
}
