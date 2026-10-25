//! `notify` subcommand: an agent notifying its operator in its own words.
//!
//! ```text
//! tddy-tools notify "Parallel run finished" --body "4 done, **1 blocked**" --level success --sound
//! ```
//!
//! The session it notifies about, and the credential it presents, come from the environment a
//! daemon-launched claude-cli or cursor-cli session's agent is given
//! ([`tddy_service::session_activity::notify_env`]). It calls `ReportOperatorNotification` over the
//! embedded daemon's agent-tool socket when `TDDY_DAEMON_SOCKET` is set, over `TDDY_DAEMON_URL`
//! otherwise.
//!
//! **Not fail-quiet**, unlike `session-hook`: an agent that asked to notify must learn that it did
//! not. A missing environment, an unreachable daemon or a refusal exits non-zero with the reason;
//! success prints `Notification sent`.

use clap::{Args, ValueEnum};

/// How the agent grades what it is telling its operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum NotifyLevel {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Args)]
pub struct NotifyArgs {
    /// One-line title, 1–200 characters after trimming.
    pub title: String,

    /// Markdown body, at most 2,000 characters. `-` reads it from stdin.
    #[arg(long)]
    pub body: Option<String>,

    /// How the operator should read it.
    #[arg(long, value_enum, default_value_t = NotifyLevel::Info)]
    pub level: NotifyLevel,

    /// Alert the operator audibly. Without it the notification arrives silently where a surface
    /// can send one silently.
    #[arg(long)]
    pub sound: bool,
}

/// Send the notification `args` describes, for the session the environment names.
///
/// `Ok` only once the daemon accepted it; every other outcome is an `Err` naming why, which the
/// binary prints and exits non-zero on.
pub async fn run_notify(args: NotifyArgs) -> anyhow::Result<()> {
    // TODO(agent-notify): implement
    let _ = args;
    anyhow::bail!("TODO(agent-notify): notify is not implemented")
}
