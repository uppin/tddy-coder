//! Session-participant module — the tddy-coder process serves session-scoped RPCs (tools,
//! terminal control) from its own LiveKit participant and publishes `session` metadata.
//!
//! Three coordinates, registered together by [`session_service_entries`]:
//!
//! * `exec_tools.ExecToolService` — the session's tools and its tool calls, served here.
//! * `terminal_session.TerminalSessionService` — the terminal family, served by
//!   [`terminal_session_service`] from `tddy-terminal-rpc`'s own handlers, so this process and the
//!   daemon answer one session identically. `#unbundle` node 6 moved the terminal streaming off
//!   `connection.ConnectionService` here for that reason.
//! * `activity.ActivityService` — the session's activity stream and its ACP replay, served by
//!   [`activity_service`]. `#unbundle` node 7 moved families M and N off
//!   `connection.ConnectionService`, and this participant moved with them in the same PR: the two
//!   servers of one session have to answer it the same way.
//!
//! `DeleteSession` / `SignalSession` are **not** served here: the web routes them directly to the
//! daemon participant (`daemon-{instanceId}`), which owns process teardown and must be reachable
//! even when the coder participant is stuck (changeset `2026-07-12-fast-session-change`).

pub mod acp_transcript;
pub mod activity_service;
pub mod connection_service_participant;
pub mod exec_tool_service;
pub mod metadata_publisher;
pub mod terminal_manager;
pub mod terminal_session_adapter;
pub mod terminal_session_service;

pub use acp_transcript::{append_frames_for_event, frame_for_event, spawn_acp_transcript_writer};
pub use activity_service::coder_activity_entry;
pub use connection_service_participant::{
    coder_session_tool_catalog, coder_session_tool_catalog_full, CoderSessionToolExecutor,
    SessionConnectionService, ToolDef, ToolExecutor, ToolOutcome,
};
pub use exec_tool_service::coder_exec_tool_entry;
pub use metadata_publisher::{
    session_metadata_json, spawn_session_metadata_tap, SessionMetadata, SessionMetadataSeed,
};
pub use terminal_session_service::coder_terminal_session_entry;

use std::sync::Arc;

use tokio::sync::watch;

use tddy_rpc::ServiceEntry;

/// Buffer size for the `StreamSessionActivity` server-stream bridge (snapshot rows + live tail).
pub(crate) const AGENT_ACTIVITY_CHANNEL_CAPACITY: usize = 256;

/// Options for spawning a session participant. `tools` + `executor` are injected by `run.rs`
/// (production wires the shared tool engine; tests wire a fake).
#[derive(Clone)]
pub struct SessionParticipantOptions {
    pub session_id: String,
    pub daemon_instance_id: String,
    pub session_token: String,
    pub tool_calls_path: std::path::PathBuf,
    pub tools: Vec<ToolDef>,
    pub executor: Arc<dyn ToolExecutor>,
    /// Session worktree where started bash terminals are spawned (the coder's agent working dir).
    pub worktree: std::path::PathBuf,
}

/// Handle returned by `spawn_session_participant`. Dropping it does **not** cancel the participant —
/// the connection + metadata watcher run in spawned tasks. The handle keeps the `JoinHandle` for a
/// future graceful-shutdown wiring.
pub struct SessionParticipantHandle {
    _run: tokio::task::JoinHandle<()>,
}

/// Spawn the session's LiveKit participant, serving the session-scoped RPC coordinates and
/// publishing `session` metadata from `metadata_rx`.
///
/// The participant identity is `session-{daemon_instance_id}-{session_id}` (built by the caller and
/// passed as `identity`). The token must authorize that identity for the target room.
pub async fn spawn_session_participant(
    ws_url: &str,
    session_token: &str,
    identity: &str,
    opts: SessionParticipantOptions,
    metadata_rx: watch::Receiver<String>,
) -> anyhow::Result<SessionParticipantHandle> {
    // The agent-activity log lives alongside `tool-calls.jsonl` in the session directory.
    let agent_activity_dir = opts
        .tool_calls_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let svc = Arc::new(SessionConnectionService {
        session_id: opts.session_id.clone(),
        session_token: opts.session_token.clone(),
        tool_calls_path: opts.tool_calls_path.clone(),
        tools: opts.tools.clone(),
        executor: opts.executor.clone(),
        worktree: opts.worktree.clone(),
        terminal_manager: Arc::new(terminal_manager::TerminalManager::new()),
        agent_activity_dir,
        // This spawn path has no presenter broadcast wired; served as snapshot-only. `run.rs`
        // (the production coder participant) wires the live presenter channel directly.
        presenter_events: None,
    });
    let mut entries = session_service_entries_from(svc);
    let names: Vec<&str> = entries.iter().map(|e| e.name).collect();
    entries.push(tddy_service::reflection_entry_from(&names));
    let multi = tddy_rpc::MultiRpcService::new(entries);

    let participant = tddy_livekit::LiveKitParticipant::connect(
        ws_url,
        session_token,
        multi,
        tddy_livekit::RoomOptions::default(),
        None,
        None,
    )
    .await
    .map_err(|e| anyhow::anyhow!("session participant connect (identity={identity}): {e}"))?;

    let local = participant.room().local_participant().clone();
    let lock = participant.metadata_publish_lock();
    let _meta_handle =
        tddy_livekit::spawn_local_participant_metadata_watcher(metadata_rx, local, lock);

    log::info!(
        target: "tddy_coder::session_participant",
        "session participant '{}' connected for session {}",
        identity,
        opts.session_id
    );

    let run = tokio::spawn(async move {
        participant.run().await;
    });
    Ok(SessionParticipantHandle { _run: run })
}

/// The three [`ServiceEntry`]s a coder session participant registers, backed by one `svc`.
///
/// Used by `run.rs`, where the coder's own participant identity *is* the session participant
/// (`daemon-{instanceId}-{sessionId}`), and by [`spawn_session_participant`].
///
/// One entry per coordinate rather than one service registered under three names, because each
/// dispatches on the method alone: a single service answering at every coordinate would serve
/// `ListExecTools` on `terminal_session.TerminalSessionService` and `StreamAcpReplay` on
/// `exec_tools.ExecToolService`, which is the opposite of moving the family. All are built from
/// the same `svc`, so they address one terminal manager, one lease and one transcript.
#[must_use]
pub fn session_service_entries(svc: SessionConnectionService) -> Vec<ServiceEntry> {
    session_service_entries_from(Arc::new(svc))
}

/// [`session_service_entries`] over a service the caller already shares.
fn session_service_entries_from(svc: Arc<SessionConnectionService>) -> Vec<ServiceEntry> {
    vec![
        exec_tool_service::coder_exec_tool_entry(Arc::clone(&svc)),
        activity_service::coder_activity_entry(Arc::clone(&svc)),
        terminal_session_service::coder_terminal_session_entry(svc),
    ]
}
