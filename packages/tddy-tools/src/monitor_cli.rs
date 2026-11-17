//! `monitor` subcommand: stream an orchestrator's session watches as one JSON object per stdout
//! line (`#activity-wake` 4/5).
//!
//! ```text
//! tddy-tools monitor --started-by-me --kinds permission,question --ttl 3600
//! tddy-tools monitor --watch <watch_id>
//! tddy-tools monitor --kinds message --ttl 86400   # the messages addressed to you
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
//! A matched message between sessions is printed as a `message` line rather than an `event` line
//! (`#activity-wake` 5/5). `--kinds message` with no `--session` or `--started-by-me` creates the
//! caller's **inbox watch** — the messages addressed to it — which any session may stream, not only
//! an orchestrator.
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
    /// agent_notification, ended, message. Default all seven.
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
    /// A message between sessions a streamed watch matched (`#activity-wake` 5/5): one addressed
    /// to the caller (its inbox watch), or one a watched session sent. Its `text` is whole; JSON
    /// escapes its newlines, so it stays one line.
    Message {
        seq: u64,
        watch_ids: Vec<String>,
        message_id: String,
        from_session_id: String,
        /// The sender's display label.
        from: String,
        to_session_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        reply_to: Option<String>,
        text: String,
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

/// The `timeout_ms` of each stream call: the tool's cap, so a call is held as long as it allows.
const WAIT_TIMEOUT_MS: u64 = 20_000;

/// How long past a stream call's `timeout_ms` the monitor waits for its answer before it calls
/// the call lapsed and stops (docs/dev/todo: no LiveKit RPC call has a client-side deadline).
const CALL_DEADLINE_MARGIN: std::time::Duration = std::time::Duration::from_secs(10);

/// Run the monitor `args` describes, for the session the environment names; the process exit code.
pub async fn run_monitor(args: MonitorArgs) -> i32 {
    match stream(&args).await {
        Ok(code) => code,
        Err(message) => {
            print_line(&MonitorLine::Error { message });
            1
        }
    }
}

/// Create or attach, then loop the stream until every streamed watch has ended.
async fn stream(args: &MonitorArgs) -> Result<i32, String> {
    let watch_ids = if args.watch_ids.is_empty() {
        vec![create_watch(args).await?]
    } else {
        args.watch_ids.clone()
    };
    let mut remaining: std::collections::HashSet<String> = watch_ids.iter().cloned().collect();
    let mut stream_id: Option<String> = None;
    let mut ack_seq: Option<u64> = None;
    loop {
        let mut call = serde_json::json!({
            "watch_ids": watch_ids,
            "timeout_ms": WAIT_TIMEOUT_MS,
        });
        if let Some(stream_id) = &stream_id {
            call["stream_id"] = Value::String(stream_id.clone());
        }
        if let Some(ack_seq) = ack_seq {
            call["ack_seq"] = serde_json::json!(ack_seq);
        }
        let deadline = std::time::Duration::from_millis(WAIT_TIMEOUT_MS) + CALL_DEADLINE_MARGIN;
        let batch = call_tool("WaitSessionWatchEvents", call, deadline)
            .await
            .map_err(|message| match message {
                message if message == LAPSED => {
                    format!(
                        "WaitSessionWatchEvents did not answer within {} ms: the call lapsed",
                        WAIT_TIMEOUT_MS
                            + u64::try_from(CALL_DEADLINE_MARGIN.as_millis()).unwrap_or(u64::MAX)
                    )
                }
                message => message,
            })?;
        stream_id = Some(
            batch["stream_id"]
                .as_str()
                .filter(|id| !id.is_empty())
                .ok_or_else(|| format!("WaitSessionWatchEvents answered no stream_id: {batch}"))?
                .to_string(),
        );
        for event in batch["events"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
            match message_line_of(event) {
                Some(line) => print_line(&line),
                None => print_line(&event_line(event, args.full)?),
            }
        }
        if let Some(lost) = batch["lost"].as_u64().filter(|lost| *lost > 0) {
            print_line(&MonitorLine::Lost { count: lost });
        }
        for end in batch["ended"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
            let watch_id = end["watch_id"].as_str().unwrap_or_default().to_string();
            let reason = end["reason"].as_str().unwrap_or_default().to_string();
            print_line(&MonitorLine::Ended {
                watch_id: watch_id.clone(),
                reason,
            });
            remaining.remove(&watch_id);
        }
        // Only now, once the batch is printed, may the next call acknowledge it.
        ack_seq = batch["next_ack_seq"].as_u64().or(ack_seq);
        if remaining.is_empty() {
            return Ok(0);
        }
    }
}

/// The marker [`call_tool`] returns when a call outlives its deadline.
const LAPSED: &str = "lapsed";

/// Create the watch `args` describes through `CreateSessionWatch`, and print its `watch` line.
async fn create_watch(args: &MonitorArgs) -> Result<String, String> {
    let mut call = serde_json::json!({
        "ttl_seconds": args.ttl_seconds.unwrap_or(3_600),
    });
    if !args.session_ids.is_empty() {
        call["session_ids"] = serde_json::json!(args.session_ids);
    }
    if args.started_by_me {
        call["started_by_me"] = Value::Bool(true);
    }
    if !args.kinds.is_empty() {
        call["kinds"] = serde_json::json!(args.kinds);
    }
    let watch = call_tool(
        "CreateSessionWatch",
        call,
        std::time::Duration::from_millis(WAIT_TIMEOUT_MS) + CALL_DEADLINE_MARGIN,
    )
    .await?;
    let watch_id = watch["watch_id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| format!("CreateSessionWatch answered no watch_id: {watch}"))?
        .to_string();
    let mut filter = serde_json::Map::new();
    if let Some(session_ids) = watch.get("session_ids") {
        filter.insert("session_ids".to_string(), session_ids.clone());
    }
    if let Some(started_by_me) = watch.get("started_by_me") {
        filter.insert("started_by_me".to_string(), started_by_me.clone());
    }
    if let Some(kinds) = watch.get("kinds") {
        filter.insert("kinds".to_string(), kinds.clone());
    }
    print_line(&MonitorLine::Watch {
        watch_id: watch_id.clone(),
        filter: Value::Object(filter),
        expires_at_unix_ms: watch["expires_at_unix_ms"].as_u64().unwrap_or(0),
    });
    Ok(watch_id)
}

/// One stream event as a monitor line, its record carried only with `--full`.
fn event_line(event: &Value, full: bool) -> Result<MonitorLine, String> {
    let strings = |key: &str| {
        event[key]
            .as_array()
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    Ok(MonitorLine::Event {
        seq: event["seq"].as_u64().unwrap_or(0),
        watch_ids: strings("watch_ids"),
        kind: event["kind"].as_str().unwrap_or("event").to_string(),
        session_id: event["session_id"].as_str().unwrap_or_default().to_string(),
        session: event["session"].as_str().unwrap_or_default().to_string(),
        summary: event["summary"].as_str().unwrap_or_default().to_string(),
        request_id: event["request_id"].as_str().map(str::to_string),
        record: full
            .then(|| event["record"].clone())
            .filter(|record| !record.is_null()),
    })
}

/// One tool call through the session's dispatch, bounded by `deadline`: its result object, or
/// the refusal's message. A lapse is [`LAPSED`].
async fn call_tool(
    tool_name: &str,
    args: Value,
    deadline: std::time::Duration,
) -> Result<Value, String> {
    let dispatched = crate::session_tool_client::dispatch_session_tool(tool_name, args);
    let answer = tokio::time::timeout(deadline, dispatched)
        .await
        .map_err(|_| LAPSED.to_string())?;
    let value: Value = serde_json::from_str(&answer)
        .map_err(|e| format!("{tool_name} answered unreadable JSON ({e}): {answer}"))?;
    if value["is_error"].as_bool() == Some(true) {
        return Err(value["error"]
            .as_str()
            .unwrap_or("the tool call was refused")
            .to_string());
    }
    Ok(value)
}

/// The `message` line for a streamed event of kind `message`, or `None` for any other event
/// (`#activity-wake` 5/5). The message's fields are read from the event's log record, so its text
/// is whole.
pub fn message_line_of(event: &Value) -> Option<MonitorLine> {
    if event["kind"].as_str()? != "message" {
        return None;
    }
    let message = &event["record"]["message"];
    Some(MonitorLine::Message {
        seq: event["seq"].as_u64().unwrap_or(0),
        watch_ids: event["watch_ids"]
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        message_id: message.get("message_id")?.as_str()?.to_string(),
        from_session_id: event["session_id"].as_str().unwrap_or_default().to_string(),
        from: event["session"].as_str().unwrap_or_default().to_string(),
        to_session_id: message.get("to_session_id")?.as_str()?.to_string(),
        reply_to: message
            .get("reply_to")
            .and_then(Value::as_str)
            .map(str::to_string),
        text: message.get("text")?.as_str()?.to_string(),
    })
}

/// Write `line` to stdout and flush it, so a `Monitor` sees it at once.
fn print_line(line: &MonitorLine) {
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{}", line.to_line());
    let _ = stdout.flush();
}
