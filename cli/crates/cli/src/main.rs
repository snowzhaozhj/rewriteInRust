use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "rustmigrate", version, about = "Rust 迁移验证工作台 CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 初始化 .rust-migration/ 目录
    Init,
    /// 分析源码项目画像
    Profile,
    /// 源码图操作
    Graph {
        #[command(subcommand)]
        action: GraphCommands,
    },
    /// 校验 migration-state.json
    Validate {
        #[command(subcommand)]
        action: ValidateCommands,
    },
    /// 状态机操作
    State {
        #[command(subcommand)]
        action: StateCommands,
    },
    /// 代码统计
    Stats {
        #[command(subcommand)]
        action: StatsCommands,
    },
    /// 生成 Cargo workspace 骨架
    Scaffold {
        #[command(subcommand)]
        action: ScaffoldCommands,
    },
}

#[derive(Subcommand)]
enum GraphCommands {
    /// 构建源码图
    Build,
    /// 拓扑排序
    TopoSort,
    /// 查询模块依赖
    Deps { module: String },
    /// 输出模块接口签名
    Interfaces { module: String },
    /// 图统计
    Stats,
}

#[derive(Subcommand)]
enum ValidateCommands {
    /// 校验 migration-state.json
    State,
}

#[derive(Subcommand)]
enum StateCommands {
    /// 查询模块状态
    Get { module: String },
    /// 执行状态转换
    Transition {
        #[arg(long)]
        module: String,
        #[arg(long)]
        to: String,
    },
}

#[derive(Subcommand)]
enum StatsCommands {
    /// 代码行数统计
    Loc,
    /// 结构复杂度对比
    Compare,
}

#[derive(Subcommand)]
enum ScaffoldCommands {
    /// 生成 Cargo workspace
    Workspace,
}

fn main() {
    let cli = Cli::parse();

    let result: Result<(), anyhow::Error> = match cli.command {
        Commands::Init => {
            println!(r#"{{"status":"ok","data":{{"message":"initialized"}}}}"#);
            Ok(())
        }
        Commands::Profile => {
            rustmigrate_core::profile::analyze();
            Ok(())
        }
        Commands::Graph { action } => match action {
            GraphCommands::Build => { rustmigrate_core::graph::build(); Ok(()) }
            GraphCommands::TopoSort => { todo!() }
            GraphCommands::Deps { .. } => { todo!() }
            GraphCommands::Interfaces { .. } => { todo!() }
            GraphCommands::Stats => { todo!() }
        },
        Commands::Validate { action } => match action {
            ValidateCommands::State => { rustmigrate_core::validate::validate_state(); Ok(()) }
        },
        Commands::State { action } => match action {
            StateCommands::Get { .. } => { rustmigrate_core::state::get(); Ok(()) }
            StateCommands::Transition { .. } => { rustmigrate_core::state::transition(); Ok(()) }
        },
        Commands::Stats { action } => match action {
            StatsCommands::Loc => { todo!() }
            StatsCommands::Compare => { todo!() }
        },
        Commands::Scaffold { action } => match action {
            ScaffoldCommands::Workspace => { rustmigrate_core::scaffold::workspace(); Ok(()) }
        },
    };

    if let Err(e) = result {
        eprintln!(r#"{{"status":"error","data":{{"message":"{}"}}}}"#, e);
        std::process::exit(1);
    }
}
