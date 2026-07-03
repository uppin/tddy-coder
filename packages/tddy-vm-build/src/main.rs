use anyhow::Result;
use clap::Parser;
use tddy_vm_build::{run_build_image, run_cloud_init_build, Cli, Command};

#[tokio::main]
async fn main() -> Result<()> {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .try_init();
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Build(args) => run_build_image(args).await,
        Command::CloudInit(args) => run_cloud_init_build(args).await,
    };
    match result {
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
