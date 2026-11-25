//! `send` subcommand: a session sends another session of its project a message
//! (`#activity-wake` 5/5).
//!
//! ```text
//! tddy-tools send --to <session-id> "Tests pass; pushed 3f2a1c. Want me to open the PR?"
//! tddy-tools send --to <session-id> --reply-to <message-id> -     # the text from stdin
//! tddy-tools send --to <session-id> --host <daemon-instance-id> "Rebase on main."  # on another host
//! ```
//!
//! It calls `PostSessionMessage` through the session and transport every session tool call uses
//! ([`crate::session_tool_client::dispatch_session_tool`]) and prints the daemon's answer —
//! `{ message_id, to_session_id, sent_at_unix_ms }` — as one JSON line. Nothing is typed into the
//! recipient's terminal: it reads the message with `ReadSessionMessages`, or streams it with
//! `tddy-tools monitor --kinds message`.
//!
//! Not fail-quiet: a refusal (the daemon names who the caller may address), an unreachable daemon
//! or an unreadable stdin exits non-zero with the reason on stderr.
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-11-17-session-messages.md

use clap::Args;
use serde_json::{json, Value};

/// How long `PostSessionMessage` may take before `send` gives up on it.
const SEND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// What `tddy-tools send` sends.
#[derive(Debug, Args)]
pub struct SendArgs {
    /// The session to message.
    #[arg(long = "to", value_name = "SESSION_ID")]
    pub to_session_id: String,

    /// The `message_id` of a message you received that this one answers.
    #[arg(long = "reply-to", value_name = "MESSAGE_ID")]
    pub reply_to: Option<String>,

    /// The host the recipient runs on, as `ListProjectSessions` reports it. Omit it to have the
    /// recipient found: on this host, as your orchestrator's host, as the host of the message you
    /// reply to, or by asking the connected hosts.
    #[arg(long = "host", value_name = "DAEMON_INSTANCE_ID")]
    pub daemon_instance_id: Option<String>,

    /// The message, at most 8 KiB; `-` reads it from stdin.
    pub text: String,
}

/// Send the message `args` describe, from the session the environment names; the process exit code.
pub async fn run_send(args: SendArgs) -> i32 {
    match send(args).await {
        Ok(()) => 0,
        Err(message) => {
            eprintln!("tddy-tools send: {message}");
            1
        }
    }
}

/// Send the message, printing the daemon's answer as one JSON line on stdout.
async fn send(args: SendArgs) -> Result<(), String> {
    let text = the_text_of(&args)?;
    let mut call = json!({
        "to_session_id": args.to_session_id,
        "text": text,
    });
    if let Some(reply_to) = args.reply_to {
        call["reply_to"] = json!(reply_to);
    }
    if let Some(host) = args.daemon_instance_id {
        call["daemon_instance_id"] = json!(host);
    }
    let dispatched = crate::session_tool_client::dispatch_session_tool("PostSessionMessage", call);
    let answer = tokio::time::timeout(SEND_TIMEOUT, dispatched)
        .await
        .map_err(|_| "PostSessionMessage did not answer in 30 s: the call timed out".to_string())?;
    let value: Value = serde_json::from_str(&answer)
        .map_err(|e| format!("PostSessionMessage answered unreadable JSON ({e}): {answer}"))?;
    if value["is_error"].as_bool() == Some(true) {
        return Err(value["error"]
            .as_str()
            .unwrap_or("the message was not sent: the tool call was refused")
            .to_string());
    }
    println!("{}", serde_json::to_string(&value).unwrap_or_default());
    Ok(())
}

/// The message `args` carries, or reads from stdin for `-`.
fn the_text_of(args: &SendArgs) -> Result<String, String> {
    if args.text != "-" {
        return Ok(args.text.clone());
    }
    use std::io::Read as _;
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .map_err(|e| format!("could not read the message from stdin: {e}"))?;
    // A trailing line break is the shell's, not the message's.
    Ok(text
        .strip_suffix('\n')
        .map(|stripped| stripped.to_string())
        .unwrap_or(text))
}
