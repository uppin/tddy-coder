//! Acceptance, end to end on the real daemon: a host terminal session of an agent declared in YAML
//! alone has usage on the Inspector's stream. Its start records the usage context, the daemon's
//! `ActivityService` looks the agent's `usage:` binding up in the host's own registry, and
//! `StreamSessionUsage` serves the snapshot, then each change (`#usage-binding 2/5`). No LiveKit
//! room and no `tddy-coder` take part.
//!
//! Feature: docs/ft/web/session-usage-inspector.md
//!
//! The daemon is `agent_registry_support`'s: the real `DaemonSessionHost`, its agent `acme` a stub
//! binary. The binding reads `{session_dir}/{conversation_id}.jsonl`, so the fixture transcript lives
//! in the session's own directory rather than in the OS user's real home.

mod agent_registry_support;
mod support;

use std::io::Write as _;
use std::time::Duration;

use agent_registry_support::{
    a_registry_daemon, acme_yaml_declaring, ARegistryDaemon, VALID_TOKEN,
};
use futures_util::StreamExt as _;
use pretty_assertions::assert_eq;
use tddy_rpc::Request;
use tddy_service::proto::activity::{ActivityService as _, StreamSessionUsageRequest};
use tddy_session_lifecycle::connection_service::PeerRoutedActivity;

/// How long the test waits for a snapshot the daemon's poller owes it.
const DEADLINE: Duration = Duration::from_secs(10);

/// `acme`'s conversation is the session id, and its `usage:` binding reads each `turn` line of
/// `{session_dir}/{conversation_id}.jsonl`.
const ACME_USAGE: &str = r#"    conversation: { kind: daemon_session_id }
    usage:
      sources:
        - format: jsonl
          main:
            path: "{session_dir}/{conversation_id}.jsonl"
          record:
            where: [{ field: type, equals: turn }]
            tokens:
              input: in
              output: out
"#;

fn a_daemon_whose_acme_declares_usage() -> ARegistryDaemon {
    a_registry_daemon()
        .with_tool("acme.yaml", &acme_yaml_declaring(ACME_USAGE))
        .build()
}

/// `acme` completes a turn of `input` / `output` tokens in session `session_id`.
fn acme_takes_a_turn(daemon: &ARegistryDaemon, session_id: &str, input: u64, output: u64) {
    let transcript = daemon
        .session_dir(session_id)
        .join(format!("{session_id}.jsonl"));
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&transcript)
        .expect("open the transcript");
    writeln!(file, r#"{{"type":"turn","in":{input},"out":{output}}}"#).expect("write a turn");
}

type UsageStream = <PeerRoutedActivity as tddy_service::proto::activity::ActivityService>::StreamSessionUsageStream;

/// Subscribe to `session_id`'s usage on the activity surface the daemon mounts.
async fn the_usage_stream_of(daemon: &ARegistryDaemon, session_id: &str) -> UsageStream {
    daemon
        .service
        .activity_service()
        .stream_session_usage(Request::direct(StreamSessionUsageRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
        }))
        .await
        .expect("the usage stream opens")
        .into_inner()
}

/// The next snapshot, as `(agent, id, input, output)` per conversation.
async fn next_snapshot(stream: &mut UsageStream) -> Vec<(String, String, u64, u64)> {
    tokio::time::timeout(DEADLINE, stream.next())
        .await
        .expect("a snapshot within the deadline")
        .expect("the stream is still open")
        .expect("a snapshot, not an error")
        .conversations
        .into_iter()
        .map(|c| (c.agent, c.id, c.input_tokens, c.output_tokens))
        .collect()
}

#[tokio::test]
#[serial_test::serial]
async fn a_host_terminal_sessions_usage_reaches_the_usage_stream() {
    // Given — a host terminal session of `acme` that has taken one turn
    let daemon = a_daemon_whose_acme_declares_usage();
    let (session_id, _host) = daemon
        .start(daemon.a_start("acme-cli", false))
        .await
        .expect("a host acme-cli start succeeds");
    acme_takes_a_turn(&daemon, &session_id, 100, 10);

    // When
    let mut stream = the_usage_stream_of(&daemon, &session_id).await;
    let first = next_snapshot(&mut stream).await;
    acme_takes_a_turn(&daemon, &session_id, 50, 5);
    let after_a_turn = next_snapshot(&mut stream).await;

    // Then — the main conversation is the session's own, read from acme's binding
    assert_eq!(
        (first, after_a_turn),
        (
            vec![("acme".to_string(), session_id.clone(), 100, 10)],
            vec![("acme".to_string(), session_id.clone(), 150, 15)],
        )
    );
}
