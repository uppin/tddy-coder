//! Acceptance: the host relay reports a jail's terminal output and size changes on one channel, in
//! the order the jail produced them — the order a host-side screen model needs to read every byte
//! at the grid it was drawn for.

mod common;

use std::time::Duration;

use bytes::Bytes;
use common::{serve_fake_over_tcp, Mode};
use tddy_sandbox_runner::{
    run_host_relay, ExecuteToolResponse, HostRelayConfig, HostTerminalEvent, HostToolHandler,
};
use tddy_service::proto::sandbox::session_frame::Payload as SessionPayload;
use tddy_service::proto::sandbox::{SandboxTerminalOutput, SandboxTerminalResized};
use tddy_service::tonic_sandbox::sandbox_service_client::SandboxServiceClient;
use tokio::sync::mpsc;

const SESSION_ID: &str = "host-relay-terminal-events";

/// A handler for a test that dispatches no tools.
#[derive(Clone)]
struct NoTools;

#[async_trait::async_trait]
impl HostToolHandler for NoTools {
    async fn execute(&self, _: &str, _: &str, tool_name: &str, _: &str) -> ExecuteToolResponse {
        panic!("no tool call was expected, got {tool_name}");
    }
}

fn output(text: &str) -> SessionPayload {
    SessionPayload::TerminalOutput(SandboxTerminalOutput {
        data: text.as_bytes().to_vec(),
        session_id: SESSION_ID.to_string(),
        terminal_id: "main".to_string(),
    })
}

fn resized(cols: u32, rows: u32) -> SessionPayload {
    SessionPayload::TerminalResized(SandboxTerminalResized {
        cols,
        rows,
        session_id: SESSION_ID.to_string(),
        terminal_id: "main".to_string(),
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_host_relay_reports_the_jails_output_and_size_changes_in_order() {
    // Given a jail that opened at 100x30, drew, was resized to 140x40, and drew again
    let (endpoint, _captured) = serve_fake_over_tcp(Mode::PushTerminalFrames(vec![
        resized(100, 30),
        output("first frame"),
        resized(140, 40),
        output("redrawn"),
    ]))
    .await;
    let (terminal_tx, _bytes_only) = mpsc::unbounded_channel::<Bytes>();
    let (events_tx, mut events) = mpsc::unbounded_channel::<HostTerminalEvent>();
    let config = HostRelayConfig::new(SESSION_ID, terminal_tx).with_terminal_events(events_tx);
    let (_stdin_tx, stdin_rx) = mpsc::unbounded_channel::<Bytes>();

    // When the host relay runs against it
    let client = SandboxServiceClient::connect(endpoint)
        .await
        .expect("connect fake sandbox grpc");
    let _relay = run_host_relay(client, NoTools, config, stdin_rx)
        .await
        .expect("start host relay");

    // Then the events arrive in exactly the jail's order
    let mut received = Vec::new();
    while received.len() < 4 {
        match tokio::time::timeout(Duration::from_secs(5), events.recv()).await {
            Ok(Some(event)) => received.push(event),
            other => panic!("expected 4 terminal events, got {received:?} then {other:?}"),
        }
    }
    assert_eq!(
        received,
        vec![
            HostTerminalEvent::Resized {
                cols: 100,
                rows: 30
            },
            HostTerminalEvent::Output(Bytes::from_static(b"first frame")),
            HostTerminalEvent::Resized {
                cols: 140,
                rows: 40
            },
            HostTerminalEvent::Output(Bytes::from_static(b"redrawn")),
        ]
    );
}
