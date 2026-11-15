//! `rename-session` subcommand: an agent naming the session it runs in.
//!
//! ```text
//! tddy-tools rename-session "Fix the login flow"
//! tddy-tools rename-session ""          # clear the name
//! ```
//!
//! The session it names, and the credential it presents, come from the environment a
//! daemon-launched claude-cli or cursor-cli session's agent is given — the same one `tddy-tools
//! notify` reads ([`crate::notify_cli::NotifySession`]). It calls `RenameOwnSession` over the
//! embedded daemon's agent-tool socket when `TDDY_DAEMON_SOCKET` is set, over `TDDY_DAEMON_URL`
//! otherwise.
//!
//! Not fail-quiet: a missing environment, an unreachable daemon or a refused name exits non-zero
//! with the reason; success prints the name as recorded.

use clap::Args;

#[derive(Debug, Args)]
pub struct RenameSessionArgs {
    /// The session's new name: one line, at most 80 characters once trimmed. Empty clears it, so
    /// the session is called by its derived label again.
    pub name: String,
}

/// Name the session the environment names; print the name as recorded.
pub async fn run_rename_session(args: RenameSessionArgs) -> anyhow::Result<()> {
    // TODO(session-display-name): green phase.
    let _ = args;
    anyhow::bail!("tddy-tools rename-session: not implemented")
}
