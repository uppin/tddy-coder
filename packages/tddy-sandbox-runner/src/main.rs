//! Run the in-jail sandbox gRPC server + claude PTY (`tddy-sandbox-runner` binary).
use anyhow::Result;
use clap::Parser;

use tddy_sandbox_runner::{run_sandbox_runner, SandboxRunnerArgs};

#[tokio::main]
async fn main() -> Result<()> {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .try_init();
    let args = SandboxRunnerArgs::parse();
    if let Err(err) = run_sandbox_runner(args).await {
        eprintln!("Error: {err:#}");
        std::process::exit(1);
    }
    Ok(())
}
