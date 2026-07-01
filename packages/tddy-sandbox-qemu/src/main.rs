use anyhow::Result;
use clap::Parser;
use tddy_sandbox_qemu::{run_sandbox_qemu, SandboxQemuArgs};

#[tokio::main]
async fn main() -> Result<()> {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .try_init();
    let args = SandboxQemuArgs::parse();
    let code = run_sandbox_qemu(args).await?;
    std::process::exit(code);
}
