use anyhow::Result;
use clap::Parser;
use tddy_vm_build::{run_build_image, BuildImageArgs};

#[tokio::main]
async fn main() -> Result<()> {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .try_init();
    let args = BuildImageArgs::parse();
    match run_build_image(args).await {
        Ok(path) => {
            println!("{}", path.display());
            Ok(())
        }
        Err(err) => {
            eprintln!("Error: {err:#}");
            std::process::exit(1);
        }
    }
}
