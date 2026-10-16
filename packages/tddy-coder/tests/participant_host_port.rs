//! The coder's session participant is served through the `SessionParticipantHost` port — PRD
//! `docs/ft/coder/1-WIP/PRD-2026-10-16-livekit-satellite-coder-participant.md`. In memory: no
//! LiveKit.

use std::sync::{Arc, Mutex};

use pretty_assertions::assert_eq;
use tddy_coder::participant_host::{
    ParticipantAuth, ParticipantJoin, ParticipantOptions, ServedParticipant, SessionParticipantHost,
};
use tddy_coder::session_participant::{
    spawn_session_participant_with, SessionParticipantOptions, ToolDef, ToolExecutor, ToolOutcome,
};

struct EchoExecutor;

#[async_trait::async_trait]
impl ToolExecutor for EchoExecutor {
    async fn execute(&self, _tool_name: &str, args_json: &str) -> ToolOutcome {
        ToolOutcome {
            result_json: args_json.to_string(),
            is_error: false,
            error_message: String::new(),
            job_id: String::new(),
            job_running: false,
        }
    }
}

/// Records what it was asked to serve and serves nothing.
#[derive(Default)]
struct RecordingHost {
    served: Mutex<Vec<(ParticipantJoin, bool, bool)>>,
}

#[async_trait::async_trait]
impl SessionParticipantHost for RecordingHost {
    async fn serve(
        &self,
        join: ParticipantJoin,
        _surface: tddy_rpc::MultiRpcService,
        options: ParticipantOptions,
    ) -> Result<ServedParticipant, String> {
        self.served
            .lock()
            .unwrap()
            .push((join, options.reconnect, options.metadata.is_some()));
        Ok(ServedParticipant::new(tokio::spawn(async {})))
    }
}

#[tokio::test]
async fn a_session_participant_serves_once_under_its_session_identity_with_its_token() {
    // Given a host, and a session's participant options
    let host = Arc::new(RecordingHost::default());
    let tool_calls = tempfile::tempdir().unwrap();
    let opts = SessionParticipantOptions {
        session_id: "1780828020298-abc".to_string(),
        daemon_instance_id: "host".to_string(),
        session_token: "a-session-token".to_string(),
        tool_calls_path: tool_calls.path().join("tool-calls.jsonl"),
        tools: vec![ToolDef {
            name: "Echo".to_string(),
            description: "Echo a message".to_string(),
            input_schema_json: r#"{"type":"object"}"#.to_string(),
        }],
        executor: Arc::new(EchoExecutor),
        worktree: tool_calls.path().to_path_buf(),
    };
    let (_metadata_tx, metadata_rx) = tokio::sync::watch::channel(String::new());

    // When the session participant is spawned through the host
    spawn_session_participant_with(
        "ws://10.0.0.5:7880",
        "a-room-token",
        "session-host-1780828020298-abc",
        opts,
        metadata_rx,
        &*host,
    )
    .await
    .expect("the host serves it");

    // Then it served once, at that URL, as that identity, with the token it was given, publishing
    // its metadata
    assert_eq!(
        *host.served.lock().unwrap(),
        vec![(
            ParticipantJoin {
                url: "ws://10.0.0.5:7880".to_string(),
                identity: "session-host-1780828020298-abc".to_string(),
                auth: ParticipantAuth::Token("a-room-token".to_string()),
            },
            false,
            true
        )]
    );
}
