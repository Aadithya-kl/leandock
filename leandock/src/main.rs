use std::process::ExitCode;

use owo_colors::OwoColorize;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("leandock=info")),
        )
        .with_target(false)
        .without_time()
        .init();

    match leandock::cli::run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{} {}", "Error:".red().bold(), err);
            ExitCode::FAILURE
        }
    }
}
