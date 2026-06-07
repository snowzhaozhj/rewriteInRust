use std::process::ExitCode;

fn main() -> ExitCode {
    let code = rustmigrate_cli::run_with_args(std::env::args_os(), &mut std::io::stdout().lock());
    ExitCode::from(code as u8)
}
