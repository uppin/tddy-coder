//! `tddy-remote-git-repo` — git's `GIT_SSH_COMMAND` for a tddy-daemon project.
//!
//! ```text
//! export GIT_SSH_COMMAND=tddy-remote-git-repo
//! export TDDY_DAEMON_URL=http://udoo-1.example:8899 TDDY_REFRESH_TOKEN=…
//! git clone udoo-1780828020298:my-app
//! ```

use std::collections::HashMap;

use clap::error::ErrorKind;
use clap::Parser;
use tddy_remote_git_repo::credentials::{resolve_credentials, CredentialArgs};
use tddy_remote_git_repo::relay::TRANSPORT_FAILURE_EXIT_CODE;
use tddy_remote_git_repo::{relay, ssh_argv};

/// Flags precede the `<host> <command>` pair git appends. Every one has an environment fallback.
///
/// `hide_env_values` is set on every credential: clap prints the *current* value of an
/// `env`-backed argument in `--help`, and this binary is configured through `GIT_SSH_COMMAND`
/// with its tokens exported — so a bare `--help` would print a daemon refresh token to whatever
/// terminal, screen share or CI log asked for usage.
#[derive(Parser)]
#[command(
    name = "tddy-remote-git-repo",
    about = "Serve a tddy-daemon project as a git remote over LiveKit"
)]
struct Cli {
    /// Base URL of the daemon's Connect-HTTP surface (e.g. http://udoo-1.example:8899). The
    /// daemon mints the LiveKit room token; this client holds no LiveKit credential.
    #[arg(long, env = "TDDY_DAEMON_URL")]
    daemon_url: Option<String>,

    /// Daemon access token. Lives 5 minutes — prefer --refresh-token for a configured remote.
    #[arg(long, env = "TDDY_SESSION_TOKEN", hide_env_values = true)]
    session_token: Option<String>,

    /// Daemon refresh token (7 days), exchanged for an access token before anything else runs.
    #[arg(long, env = "TDDY_REFRESH_TOKEN", hide_env_values = true)]
    refresh_token: Option<String>,

    /// Seconds to wait for the daemon before reporting it unreachable. Falls back to the
    /// TDDY_CONNECT_TIMEOUT_SECS environment variable, then to 30.
    //
    // Read from the environment by `resolve_credentials` rather than by clap, so a value that is
    // not a number is refused with the same wording — and the same exit code — as every other
    // unusable setting, instead of clap's own bare "invalid value".
    #[arg(long)]
    connect_timeout_secs: Option<u64>,

    /// The ssh-style tail git appends: `[options] <host> <command>`.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    ssh_tail: Vec<String>,
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let cli = parse_cli();

    let request = match ssh_argv::parse_ssh_invocation(&cli.ssh_tail) {
        Ok(request) => request,
        Err(e) => {
            eprintln!("tddy-remote-git-repo: {e}");
            std::process::exit(e.exit_code());
        }
    };

    let env: HashMap<String, String> = std::env::vars().collect();
    let args = CredentialArgs {
        daemon_url: cli.daemon_url,
        session_token: cli.session_token,
        refresh_token: cli.refresh_token,
        connect_timeout_secs: cli.connect_timeout_secs,
    };
    let credentials = match resolve_credentials(&args, &env) {
        Ok(credentials) => credentials,
        Err(e) => {
            eprintln!("tddy-remote-git-repo: {e}");
            std::process::exit(e.exit_code());
        }
    };

    match relay::run(request, credentials).await {
        Ok(exit_code) => std::process::exit(exit_code),
        Err(e) => {
            eprintln!("tddy-remote-git-repo: {e}");
            std::process::exit(e.exit_code());
        }
    }
}

/// Parse argv, exiting `255` on a rejected command line rather than clap's default `2`.
///
/// Git reads this binary's exit code the way it reads `ssh`'s: `255` is "the remote could not be
/// reached", and `2` is nothing in particular — it would surface as an unexplained failure with no
/// hint that the command line itself was at fault. `--help` and `--version` are not failures and
/// still exit `0`.
fn parse_cli() -> Cli {
    match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            let _ = e.print();
            std::process::exit(match e.kind() {
                ErrorKind::DisplayHelp
                | ErrorKind::DisplayVersion
                | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => 0,
                _ => TRANSPORT_FAILURE_EXIT_CODE,
            });
        }
    }
}
