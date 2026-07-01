//! Acceptance test for `--stdio` on `tddy-coder`: the remote-control surface (`tddy.v1.TddyRemote`)
//! served over the process's own stdin/stdout via `tddy-stdio`, as an alternative to `--grpc`.
//!
//! See docs/ft/coder/1-WIP/PRD-2026-07-01-stdio-transport-for-grpc-binaries.md (Milestone 2).

use std::time::Duration;

use async_trait::async_trait;
use prost::Message;
use tddy_rpc::{RpcMessage, RpcResult, RpcService, Status};
use tddy_service::gen::{
    client_message, server_message, ClientMessage, ServerMessage, SubmitFeatureInput,
};
use tddy_stdio::spawn_child_endpoint;
use tokio::process::Command;
use tokio::time::timeout;

/// Bounded safety net around calls otherwise driven entirely by async channels (see fluent-tests
/// "Testing Async Code"). Generous enough to absorb `tddy-coder` process startup under CI load,
/// but still well under the 10s E2E ceiling.
const CALL_TIMEOUT: Duration = Duration::from_secs(8);

/// `tddy-coder --stdio` never calls back into the test process for this scenario — any inbound
/// request here would be a bug, so it fails loudly rather than silently no-op'ing.
struct NoCallbackService;

#[async_trait]
impl RpcService for NoCallbackService {
    async fn handle_rpc(&self, service: &str, method: &str, _message: &RpcMessage) -> RpcResult {
        RpcResult::Unary(Err(Status::unimplemented(format!(
            "test process hosts no callback service, got {service}/{method}"
        ))))
    }
}

/// Path to the `tddy-coder` binary. `CARGO_BIN_EXE_tddy-coder` is only set by Cargo for binaries
/// of the *current* package; `tddy-e2e` doesn't declare a `tddy-coder` bin target itself, so this
/// falls back to deriving the path from the test binary's own location (mirrors
/// `terminal_service_livekit.rs`'s existing fallback for the same reason).
fn tddy_coder_exe_path() -> String {
    std::env::var("CARGO_BIN_EXE_tddy-coder").unwrap_or_else(|_| {
        let exe = std::env::current_exe().expect("current exe");
        let deps = exe.parent().expect("exe parent");
        let debug = deps.parent().expect("deps parent");
        debug.join("tddy-coder").display().to_string()
    })
}

fn tddy_coder_stdio_command(tddy_data_dir: &std::path::Path) -> Command {
    let mut command = Command::new(tddy_coder_exe_path());
    command.env_clear().args([
        "--agent",
        "stub",
        "--stdio",
        "--tddy-data-dir",
        tddy_data_dir.to_str().expect("utf8 tmp path"),
    ]);
    command
}

#[tokio::test]
async fn submits_a_feature_input_over_stdio_and_receives_a_goal_started_event() {
    // Given `tddy-coder --stdio` spawned as a child, driven entirely over its stdin/stdout
    let tddy_data_dir =
        std::env::temp_dir().join(format!("tddy-stdio-e2e-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&tddy_data_dir).expect("create tddy data dir");
    let endpoint =
        spawn_child_endpoint(tddy_coder_stdio_command(&tddy_data_dir), NoCallbackService)
            .await
            .expect("spawn tddy-coder --stdio");
    let (mut sender, mut responses) = endpoint
        .client
        .start_bidi_stream("tddy.v1.TddyRemote", "Stream")
        .expect("start TddyRemote.Stream bidi call");

    // When submitting a feature input as the first message on the stream
    let submit = ClientMessage {
        intent: Some(client_message::Intent::SubmitFeatureInput(
            SubmitFeatureInput {
                text: "Build auth".to_string(),
            },
        )),
    };
    sender
        .send(submit.encode_to_vec(), false)
        .await
        .expect("send SubmitFeatureInput frame");

    // Then a decodable ServerMessage carrying GoalStarted arrives — proving the bytes on the wire
    // are clean RPC frames the peer's FrameDecoder can parse, not corrupted by stray stdout writes
    // from logging, TUI rendering, or a plain-mode fallback running concurrently with --stdio
    let mut seen_goal_started = false;
    for _ in 0..50 {
        let next = timeout(CALL_TIMEOUT, responses.recv())
            .await
            .expect("ServerMessage frame timed out");
        let Some(frame) = next else {
            break;
        };
        let bytes = frame.expect("stream item error");
        let message = ServerMessage::decode(bytes.as_slice()).expect("decode ServerMessage");
        if matches!(message.event, Some(server_message::Event::GoalStarted(_))) {
            seen_goal_started = true;
            break;
        }
    }

    assert!(
        seen_goal_started,
        "expected a GoalStarted event over the stdio RPC channel"
    );
}
