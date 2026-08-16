//! `tddy-session-sync` — mirror a tddy session's worktree locally.
//!
//! ```bash
//! export LIVEKIT_URL=ws://…  LIVEKIT_API_KEY=…  LIVEKIT_API_SECRET=…
//! export TDDY_DAEMON_URL=http://udoo-1.example:8899  TDDY_REFRESH_TOKEN=…
//! tddy-session-sync --session-id 1780828020298-abc --dest ~/mirrors/my-app
//! ```
//!
//! Product contract: `docs/ft/daemon/session-worktree-sync.md`.

use std::path::PathBuf;

use clap::Parser;
use tddy_session_sync::{attach, layered_environment, resolve_credentials, run, SyncArgs};

/// Every secret carries `hide_env_values`: clap would otherwise print the live token in `--help`,
/// which is the one place a user is guaranteed to look while someone is watching their screen.
///
/// `LIVEKIT_API_SECRET` has no flag at all. It is the key a daemon signs every session token with,
/// and an argv is world-readable through `/proc` — so it is read from the environment (or the
/// repo-root `.env`) and nowhere else.
#[derive(Parser, Debug)]
#[command(
    name = "tddy-session-sync",
    about = "Mirror a tddy session's worktree locally by watching its LiveKit room"
)]
struct Cli {
    /// The session to mirror.
    #[arg(long)]
    session_id: Option<String>,

    /// The directory to keep equal to the session's worktree. Owned by this tool: local edits
    /// under it are discarded by the next sync.
    #[arg(long)]
    dest: Option<PathBuf>,

    /// LiveKit server URL.
    #[arg(long, env = "LIVEKIT_URL")]
    livekit_url: Option<String>,

    /// LiveKit API key, used to mint this client's own room token.
    #[arg(long, env = "LIVEKIT_API_KEY", hide_env_values = true)]
    livekit_api_key: Option<String>,

    /// The daemon's Connect-HTTP root — the one `/rpc/…` hangs off.
    #[arg(long, env = "TDDY_DAEMON_URL")]
    daemon_url: Option<String>,

    /// A daemon access token (5 minutes).
    #[arg(long, env = "TDDY_SESSION_TOKEN", hide_env_values = true)]
    session_token: Option<String>,

    /// A daemon refresh token (7 days), exchanged for an access token before anything else runs.
    #[arg(long, env = "TDDY_REFRESH_TOKEN", hide_env_values = true)]
    refresh_token: Option<String>,

    /// Parsed here rather than by clap, so an unparsable value is refused instead of silently
    /// becoming the default.
    #[arg(long)]
    connect_timeout_secs: Option<String>,
}

/// The per-developer environment file, read from the working directory. Gitignored, so its absence
/// is the normal case rather than a misconfiguration.
const ENV_FILENAME: &str = ".env";

/// What a run that could not attach, or could not keep the mirror equal, exits with.
///
/// One code for every such failure, as [`tddy_session_sync::CredentialError::exit_code`] is one for
/// every credential failure: a caller tells them apart by reading the message, not by switching on
/// a number. What matters is only that it is not zero — a syncer that exited 0 having mirrored
/// nothing is a scripted workflow that carries on against an empty directory.
const RUN_FAILURE_EXIT_CODE: i32 = 1;

#[tokio::main]
async fn main() {
    env_logger::init();

    let cli = Cli::parse();
    let args = SyncArgs {
        session_id: cli.session_id,
        dest: cli.dest,
        livekit_url: cli.livekit_url,
        livekit_api_key: cli.livekit_api_key,
        // Environment only — deliberately not a flag. This is the same value a daemon signs every
        // session token with, and a flag lands in `/proc/<pid>/cmdline`, readable by any local
        // user for the life of the process. `hide_env_values` protects `--help`; nothing protects
        // an argv.
        livekit_api_secret: None,
        daemon_url: cli.daemon_url,
        session_token: cli.session_token,
        refresh_token: cli.refresh_token,
        connect_timeout_secs: cli.connect_timeout_secs,
    };

    // The process environment first, then the repo-root `.env` for whatever it left unset. Read
    // from the working directory because that is where a developer running this from a checkout
    // keeps it; an absent file simply contributes nothing.
    let env_file = match std::fs::read_to_string(ENV_FILENAME) {
        Ok(contents) => Some(contents),
        // An absent `.env` is the normal case — it is gitignored and per-developer.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        // Anything else is reported rather than treated as absent. A `.env` that exists but cannot
        // be read would otherwise contribute nothing and the run would fail complaining about a
        // missing credential, sending the user to look for the wrong problem.
        Err(e) => {
            eprintln!("tddy-session-sync: cannot read {ENV_FILENAME}: {e}");
            std::process::exit(2);
        }
    };
    let env = layered_environment(std::env::vars().collect(), env_file.as_deref());
    let credentials = match resolve_credentials(&args, &env) {
        Ok(credentials) => credentials,
        Err(e) => {
            eprintln!("tddy-session-sync: {e}");
            std::process::exit(e.exit_code());
        }
    };

    // Resolve the session and join its room before anything is written locally, so a session id
    // nobody knows fails naming itself rather than after a directory has been taken over for it.
    let attached = match attach(&credentials).await {
        Ok(attached) => attached,
        Err(e) => {
            eprintln!("tddy-session-sync: {e}");
            std::process::exit(RUN_FAILURE_EXIT_CODE);
        }
    };

    if let Err(failure) = run(&credentials, attached).await {
        eprintln!("tddy-session-sync: {failure}");
    }
    // Non-zero either way. A session being mirrored has no completion, so returning from `run` at
    // all means the mirror stopped being kept equal — and reporting that as success is exactly the
    // half-written-mirror-that-looks-fine AC32 refuses.
    std::process::exit(RUN_FAILURE_EXIT_CODE);
}
