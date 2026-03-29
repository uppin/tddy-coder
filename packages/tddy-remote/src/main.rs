//! tddy-remote — SSH-like / rsync `--rsh` client for per-connection remote sandboxes.

use std::ffi::OsString;
use std::path::PathBuf;

use clap::{Parser, Subcommand};

fn init_tracing() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .try_init();
}

// Ensure Connect stack crate is linked for upcoming HTTP client (PRD F3).
use tddy_connectrpc as _;

#[derive(Parser, Debug)]
#[command(name = "tddy-remote")]
#[command(about = "Remote sandbox client for tddy-daemon (Connect / LiveKit)")]
struct Cli {
    /// Path to client config (authorities, daemon URL, auth).
    #[arg(long, global = true, env = "TDDY_REMOTE_CONFIG")]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// List configured authorities / identities.
    Authorities {
        #[command(subcommand)]
        sub: AuthoritiesCmd,
    },
    /// Run a non-interactive remote command; remote exit code becomes the CLI exit code.
    Exec {
        #[arg(value_name = "HOST")]
        host: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        remote_cmd: Vec<String>,
    },
    /// Interactive shell; use `--pty` to allocate a PTY (bash in the sandbox).
    Shell {
        #[arg(long)]
        pty: bool,
        #[arg(value_name = "HOST")]
        host: String,
    },
    /// Rsync `--rsh` mode: `tddy-remote [options] <authority-or-host> <remote program> ...`
    #[command(external_subcommand)]
    Rsh(Vec<OsString>),
}

#[derive(Subcommand, Debug)]
enum AuthoritiesCmd {
    List,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    // rsync speaks the rsync protocol on our stdin/stdout; never attach env_logger to stderr here.
    if !matches!(&cli.command, Commands::Rsh(_)) {
        init_tracing();
    }
    match cli.command {
        Commands::Authorities {
            sub: AuthoritiesCmd::List,
        } => match cli.config.as_deref() {
            None => {
                eprintln!("tddy-remote: --config is required for authorities list");
                std::process::ExitCode::from(2)
            }
            Some(cfg) => match tddy_remote::load_authority_ids_from_path(cfg) {
                Ok(ids) => {
                    for id in ids {
                        println!("{id}");
                    }
                    std::process::ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("tddy-remote: {e}");
                    std::process::ExitCode::from(2)
                }
            },
        },
        Commands::Exec { host, remote_cmd } => {
            tddy_remote::run_exec(cli.config.as_deref(), &host, &remote_cmd)
                .await
                .unwrap_or_else(|e| {
                    eprintln!("tddy-remote: {e}");
                    std::process::ExitCode::from(2)
                })
        }
        Commands::Shell { pty, host } => {
            tddy_remote::run_shell_pty(cli.config.as_deref(), &host, pty)
                .await
                .unwrap_or_else(|e| {
                    eprintln!("tddy-remote: {e}");
                    std::process::ExitCode::from(2)
                })
        }
        Commands::Rsh(args) => tddy_remote::run_rsync_rsh(cli.config.as_deref(), args)
            .await
            .unwrap_or_else(|e| {
                eprintln!("tddy-remote: {e}");
                std::process::ExitCode::from(2)
            }),
    }
}
