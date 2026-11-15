//! `rename-session` subcommand: an agent naming the session it runs in.
//!
//! ```text
//! tddy-tools rename-session "Fix the login flow"
//! tddy-tools rename-session ""          # clear the name
//! ```
//!
//! The session it names, and the credential it presents, come from the environment a
//! daemon-launched claude-cli or cursor-cli session's agent is given — the same one `tddy-tools
//! notify` reads ([`NotifySession`]). It calls `RenameOwnSession` over the embedded daemon's
//! agent-tool socket when `TDDY_DAEMON_SOCKET` is set, over `TDDY_DAEMON_URL` otherwise.
//!
//! Not fail-quiet: a missing environment, an unreachable daemon or a refused name exits non-zero
//! with the reason; success prints the name as recorded.

use clap::Args;
use prost::Message as _;
use tddy_service::proto::activity::{RenameOwnSessionRequest, RenameOwnSessionResponse};

use crate::notify_cli::NotifySession;

/// The method this subcommand calls on the activity service.
const METHOD: &str = "RenameOwnSession";

#[derive(Debug, Args)]
pub struct RenameSessionArgs {
    /// The session's new name: one line, at most 80 characters once trimmed. Empty clears it, so
    /// the session is called by its derived label again.
    pub name: String,
}

/// Name the session the environment names; print the name as recorded.
pub async fn run_rename_session(args: RenameSessionArgs) -> anyhow::Result<()> {
    let session = NotifySession::from_lookup(|name| std::env::var(name).ok())?;
    let answer = session
        .transport
        .call(METHOD, request(&session, &args).encode_to_vec())
        .await?;
    let recorded = RenameOwnSessionResponse::decode(answer.as_slice())
        .map_err(|e| anyhow::anyhow!("{METHOD}: the daemon's answer does not decode: {e}"))?;
    println!("{}", confirmation(&recorded.display_name));
    Ok(())
}

/// The request `args` become for `session`. The name is sent as typed: the daemon trims it and
/// holds it to the rules, so there is one place that decides.
#[must_use]
pub fn request(session: &NotifySession, args: &RenameSessionArgs) -> RenameOwnSessionRequest {
    RenameOwnSessionRequest {
        session_id: session.session_id.clone(),
        hook_token: session.hook_token.clone(),
        os_user: session.os_user.clone(),
        display_name: args.name.clone(),
    }
}

/// What the command prints for the name the daemon recorded.
fn confirmation(recorded: &str) -> String {
    if recorded.is_empty() {
        "Session name cleared".to_string()
    } else {
        format!("Session renamed: {recorded}")
    }
}
