//! `monitor` subcommand: stream an orchestrator's session watches as one JSON object per stdout
//! line (`#activity-wake` 4/5).
//!
//! ```text
//! tddy-tools monitor --started-by-me --kinds permission,question --ttl 3600
//! tddy-tools monitor --watch <watch_id>
//! ```
//!
//! Made for Claude Code's `Monitor` tool, which turns each stdout line of a command into a
//! notification. Without `--watch` it creates a watch through `CreateSessionWatch` and prints a
//! `watch` line; with it, it attaches to the caller's existing watches. It then loops
//! `WaitSessionWatchEvents`, acknowledging each batch once it is printed, and prints every event,
//! `lost` count and ended watch as it comes.
//!
//! **stdout carries only [`MonitorLine`]s**, each one compact JSON object, flushed as it is
//! written; logs go to stderr. It exits 0 once every streamed watch has ended, after an `ended`
//! line for each, and non-zero after an `error` line on any refusal, transport error or per-call
//! deadline lapse — so a `Monitor` never mistakes silence for success.
//!
//! The session and the transport are the ones every session tool call uses
//! ([`crate::session_tool_client::dispatch_session_tool`]).
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-11-17-session-watch-monitor.md

use std::io::Write as _;

use clap::Args;
use serde::Serialize;
use serde_json::Value;

/// What `tddy-tools monitor` streams.
#[derive(Debug, Args)]
pub struct MonitorArgs {
    /// Stream this existing watch of yours instead of creating one (repeatable): the `watch_id` of
    /// an earlier monitor's `watch` line, to re-arm it.
    #[arg(long = "watch", value_name = "WATCH_ID")]
    pub watch_ids: Vec<String>,

    /// Watch this session of the project (repeatable), as `CreateSessionWatch`'s `session_ids`.
    #[arg(
        long = "session",
        value_name = "SESSION_ID",
        conflicts_with = "watch_ids"
    )]
    pub session_ids: Vec<String>,

    /// Watch every session you started, including ones you start later.
    #[arg(long, conflicts_with = "watch_ids")]
    pub started_by_me: bool,

    /// Which events, comma-separated: question, permission, turn_finished, waiting_for_input,
    /// agent_notification, ended. Default all six.
    #[arg(long, value_delimiter = ',', conflicts_with = "watch_ids")]
    pub kinds: Vec<String>,

    /// How long the created watch lives, in seconds (at most 86400).
    #[arg(long = "ttl", value_name = "SECONDS", conflicts_with = "watch_ids")]
    pub ttl_seconds: Option<u64>,

    /// Add the full log record to each event line. Without it a line is compact: every line becomes
    /// a message in the agent's conversation.
    #[arg(long)]
    pub full: bool,
}

/// One line of the monitor's stdout.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MonitorLine {
    /// The watch this monitor created.
    Watch {
        watch_id: String,
        filter: Value,
        expires_at_unix_ms: u64,
    },
    /// One matched record.
    Event {
        seq: u64,
        watch_ids: Vec<String>,
        kind: String,
        session_id: String,
        session: String,
        summary: String,
        /// The id `AnswerSessionPermission` / `AnswerSessionQuestion` take, on a `permission` or
        /// `question` event.
        #[serde(skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
        /// The log record, with `--full`.
        #[serde(skip_serializing_if = "Option::is_none")]
        record: Option<Value>,
    },
    /// Matched records the log evicted before they could be streamed.
    Lost { count: u64 },
    /// A streamed watch ended.
    Ended { watch_id: String, reason: String },
    /// Why the monitor stops with a non-zero exit.
    Error { message: String },
}

impl MonitorLine {
    /// The line as written to stdout: one compact JSON object, no newline inside it.
    pub fn to_line(&self) -> String {
        serde_json::to_string(self)
            .expect("a monitor line serialises: it holds only strings, numbers and JSON values")
    }
}

/// Run the monitor `args` describes, for the session the environment names; the process exit code.
pub async fn run_monitor(args: MonitorArgs) -> i32 {
    // TODO(watch-monitor): implement — create or attach, loop WaitSessionWatchEvents with the last
    // printed seq as ack_seq, print each line and flush, bound each call by timeout_ms + 10 s, and
    // exit 0 once every streamed watch has ended.
    let _ = args;
    print_line(&MonitorLine::Error {
        message: "TODO(watch-monitor): tddy-tools monitor is not implemented yet".to_string(),
    });
    1
}

/// Write `line` to stdout and flush it, so a `Monitor` sees it at once.
fn print_line(line: &MonitorLine) {
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{}", line.to_line());
    let _ = stdout.flush();
}
