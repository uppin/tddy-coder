//! `send` subcommand: a session sends another session of its project a message
//! (`#activity-wake` 5/5).
//!
//! ```text
//! tddy-tools send --to <session-id> "Tests pass; pushed 3f2a1c. Want me to open the PR?"
//! tddy-tools send --to <session-id> --reply-to <message-id> -     # the text from stdin
//! ```
//!
//! It calls `PostSessionMessage` through the session and transport every session tool call uses
//! ([`crate::session_tool_client::dispatch_session_tool`]) and prints the daemon's answer —
//! `{ message_id, to_session_id, sent_at_unix_ms }` — as one JSON line. Nothing is typed into the
//! recipient's terminal: it reads the message with `ReadSessionMessages`, or streams it with
//! `tddy-tools monitor --inbox`.
//!
//! Not fail-quiet: a refusal (the daemon names who the caller may address), an unreachable daemon
//! or an unreadable stdin exits non-zero with the reason on stderr.
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-11-17-session-messages.md

use clap::Args;

/// What `tddy-tools send` sends.
#[derive(Debug, Args)]
pub struct SendArgs {
    /// The session to message.
    #[arg(long = "to", value_name = "SESSION_ID")]
    pub to_session_id: String,

    /// The `message_id` of a message you received that this one answers.
    #[arg(long = "reply-to", value_name = "MESSAGE_ID")]
    pub reply_to: Option<String>,

    /// The message, at most 8 KiB; `-` reads it from stdin.
    pub text: String,
}

/// Send the message `args` describe, from the session the environment names; the process exit code.
pub async fn run_send(args: SendArgs) -> i32 {
    // TODO(session-messages): implement — read the text (stdin for `-`), call PostSessionMessage,
    // print its answer as one JSON line, or the refusal on stderr and exit 1.
    let _ = args;
    eprintln!("tddy-tools send: TODO(session-messages): not implemented yet");
    1
}
