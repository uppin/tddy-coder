//! The replay RPCs addressed at a subagent conversation of the session (`conversation_id`).
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-conversation-bubble.md

use super::*;
use futures_util::StreamExt;
use std::time::Duration;
use tddy_core::agent_activity::{AgentActivityRecord, STATUS_COMPLETED};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon_kernel::conversation_frames::{ConversationFrame, ConversationFrames};
use tddy_daemon_kernel::{SessionUserResolver, SessionsBaseResolver};
use tddy_service::acp_replay::{agent_text_frame, append_acp_frame, frame_for_agent_activity};
use tddy_service::proto::activity::{
    ActivityService as _, GetAcpReplayPageRequest, GetAcpToolCallDetailRequest,
    StreamAcpReplayRequest, StreamMode,
};
use tddy_service::subagent_conversation_replay::conversation_transcript_dir;

const SESSION_ID: &str = "1780828020298-conversations";

fn a_service(sessions_base: std::path::PathBuf) -> DaemonSessionHost {
    let yaml = "users:\n  - github_user: \"u\"\n    os_user: \"u\"\n";
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).unwrap();
    let config = crate::config::DaemonConfig::load(&path).unwrap();
    let base = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver = Arc::new(move |_| Some(base.clone()));
    let user_resolver: SessionUserResolver =
        Arc::new(|token| (token == "valid").then(|| "u".to_string()));
    DaemonSessionHost::new(
        config.clone(),
        sessions_base_resolver,
        sessions_base,
        user_resolver,
        None,
        tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        crate::peer_routing::PeerRouting::local_only(&config),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        Arc::new(CliSessionManager::new()),
    )
}

/// A session whose own transcript says one thing and whose conversation `conv-1` another, so a
/// reply can only be right if it read the conversation's.
fn a_session_with_a_recorded_conversation(sessions_base: &std::path::Path) {
    let session_dir = unified_session_dir_path(sessions_base, SESSION_ID);
    std::fs::create_dir_all(&session_dir).unwrap();
    append_acp_frame(
        &session_dir,
        &agent_text_frame("The main agent's own text.", 500),
    )
    .unwrap();
    let conversation_dir = conversation_transcript_dir(&session_dir, "conv-1");
    std::fs::create_dir_all(&conversation_dir).unwrap();
    append_acp_frame(
        &conversation_dir,
        &agent_text_frame("The subagent's answer.", 1_000),
    )
    .unwrap();
    append_acp_frame(
        &conversation_dir,
        &frame_for_agent_activity(&AgentActivityRecord {
            call_id: "sub-read-1".to_string(),
            tool_name: "Read".to_string(),
            input: serde_json::json!({ "file_path": "src/parser.rs" }),
            status: STATUS_COMPLETED.to_string(),
            result: serde_json::json!({ "content": "fn parse() {}" }),
            error_message: String::new(),
            started_unix_ms: 1_100,
            completed_unix_ms: 1_200,
            source: "coding-agent".to_string(),
            head_commit: String::new(),
            activity_seq: 0,
            changed_paths: Vec::new(),
        }),
    )
    .unwrap();
}

fn replaying(conversation_id: &str) -> StreamAcpReplayRequest {
    StreamAcpReplayRequest {
        session_token: "valid".to_string(),
        session_id: SESSION_ID.to_string(),
        daemon_instance_id: String::new(),
        mode: StreamMode::SnapshotThenLive as i32,
        page_size: 0,
        conversation_id: conversation_id.to_string(),
    }
}

fn agent_text(frame: &tddy_service::proto::acp::AcpAgentMessage) -> String {
    use tddy_service::proto::acp::{acp_agent_message, content_block, session_update};
    match &frame.msg {
        Some(acp_agent_message::Msg::SessionUpdate(n)) => {
            match n.update.as_ref().and_then(|u| u.update.as_ref()) {
                Some(session_update::Update::AgentMessageChunk(c)) => {
                    match c.content.as_ref().and_then(|b| b.block.as_ref()) {
                        Some(content_block::Block::Text(t)) => t.text.clone(),
                        other => panic!("expected text content, got {other:?}"),
                    }
                }
                other => panic!("expected AgentMessageChunk, got {other:?}"),
            }
        }
        other => panic!("expected a SessionUpdate frame, got {other:?}"),
    }
}

#[tokio::test]
async fn stream_acp_replay_with_a_conversation_replays_that_conversations_transcript() {
    // Given
    let temp = tempfile::tempdir().unwrap();
    a_session_with_a_recorded_conversation(temp.path());
    let service = a_service(temp.path().to_path_buf());

    // When
    let mut stream = service
        .activity_service()
        .stream_acp_replay(Request::direct(replaying("conv-1")))
        .await
        .expect("the conversation replays")
        .into_inner();

    // Then
    let envelope = tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .expect("a frame in time")
        .expect("a frame")
        .expect("not an error");
    let first: tddy_service::proto::acp::AcpAgentMessage =
        prost::Message::decode(&envelope.acp_agent_message[..]).expect("decode");
    assert_eq!(agent_text(&first), "The subagent's answer.");
}

#[tokio::test]
async fn a_conversation_that_recorded_nothing_is_not_found_naming_it() {
    // Given
    let temp = tempfile::tempdir().unwrap();
    a_session_with_a_recorded_conversation(temp.path());
    let service = a_service(temp.path().to_path_buf());

    // When
    let refusal = service
        .activity_service()
        .stream_acp_replay(Request::direct(replaying("conv-in-jail")))
        .await
        .expect_err("nothing was recorded for it");

    // Then
    assert_eq!(refusal.code(), tddy_rpc::Code::NotFound);
    assert!(
        refusal.message().contains("conv-in-jail"),
        "{}",
        refusal.message()
    );
}

#[tokio::test]
async fn a_conversation_page_is_served_from_the_conversations_transcript() {
    // Given
    let temp = tempfile::tempdir().unwrap();
    a_session_with_a_recorded_conversation(temp.path());
    let service = a_service(temp.path().to_path_buf());

    // When
    let page = service
        .activity_service()
        .get_acp_replay_page(Request::direct(GetAcpReplayPageRequest {
            session_token: "valid".to_string(),
            session_id: SESSION_ID.to_string(),
            daemon_instance_id: String::new(),
            // `before_seq` is exclusive (activity.proto): the page before seq 1 is the text at seq 0.
            before_seq: 1,
            page_size: 1,
            conversation_id: "conv-1".to_string(),
        }))
        .await
        .expect("the page is served")
        .into_inner();

    // Then
    assert_eq!(page.frames.len(), 1);
    let frame: tddy_service::proto::acp::AcpAgentMessage =
        prost::Message::decode(&page.frames[0][..]).expect("decode");
    assert_eq!(agent_text(&frame), "The subagent's answer.");
    assert!(page.at_oldest);
}

#[tokio::test]
async fn a_conversation_tool_call_detail_is_served_by_conversation_id() {
    // Given
    let temp = tempfile::tempdir().unwrap();
    a_session_with_a_recorded_conversation(temp.path());
    let service = a_service(temp.path().to_path_buf());

    // When
    let detail = service
        .activity_service()
        .get_acp_tool_call_detail(Request::direct(GetAcpToolCallDetailRequest {
            session_token: "valid".to_string(),
            session_id: SESSION_ID.to_string(),
            daemon_instance_id: String::new(),
            tool_call_id: "sub-read-1".to_string(),
            conversation_id: "conv-1".to_string(),
        }))
        .await
        .expect("the conversation's call is found")
        .into_inner();

    // Then
    let raw_input: serde_json::Value =
        serde_json::from_str(&detail.raw_input.expect("raw_input")).expect("JSON");
    assert_eq!(
        raw_input,
        serde_json::json!({ "file_path": "src/parser.rs" })
    );
}

#[tokio::test]
async fn a_conversation_replay_delivers_a_frame_recorded_after_it_subscribed_once() {
    // Given a conversation whose two recorded frames the replay sends, and a stream open on it
    let temp = tempfile::tempdir().unwrap();
    a_session_with_a_recorded_conversation(temp.path());
    let service = a_service(temp.path().to_path_buf());
    let frames = service.conversation_frames();
    let mut stream = service
        .activity_service()
        .stream_acp_replay(Request::direct(replaying("conv-1")))
        .await
        .expect("the conversation replays")
        .into_inner();
    for _ in 0..2 {
        tokio::time::timeout(Duration::from_secs(1), stream.next())
            .await
            .expect("a recorded frame in time")
            .expect("a frame")
            .expect("not an error");
    }

    // When the recorder publishes a copy of a frame the replay already sent (seq 1), then a frame recorded
    // after the stream opened (seq 2)
    frames.publish(
        SESSION_ID,
        "conv-1",
        ConversationFrame {
            seq: 1,
            frame: agent_text_frame("A copy the replay already sent.", 1_000),
        },
    );
    frames.publish(
        SESSION_ID,
        "conv-1",
        ConversationFrame {
            seq: 2,
            frame: agent_text_frame("A new answer.", 2_000),
        },
    );

    // Then the next frame is the new one, at its position, and the copy was not sent a second time
    let envelope = tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .expect("the live frame in time")
        .expect("a frame")
        .expect("not an error");
    assert_eq!(envelope.seq, 2);
    let live: tddy_service::proto::acp::AcpAgentMessage =
        prost::Message::decode(&envelope.acp_agent_message[..]).expect("decode");
    assert_eq!(agent_text(&live), "A new answer.");
}

#[tokio::test]
async fn a_conversation_replay_that_falls_behind_skips_ahead_and_keeps_streaming() {
    // Given a conversation stream open on its two recorded frames
    let temp = tempfile::tempdir().unwrap();
    a_session_with_a_recorded_conversation(temp.path());
    let service = a_service(temp.path().to_path_buf());
    let frames = service.conversation_frames();
    let mut stream = service
        .activity_service()
        .stream_acp_replay(Request::direct(replaying("conv-1")))
        .await
        .expect("the conversation replays")
        .into_inner();
    for _ in 0..2 {
        tokio::time::timeout(Duration::from_secs(1), stream.next())
            .await
            .expect("a recorded frame in time")
            .expect("a frame")
            .expect("not an error");
    }

    // When the recorder publishes more frames than the channel holds before the reader runs (the test runtime
    // is single-threaded, so the relay cannot drain the channel mid-burst), and the burst's seqs are 2 to
    // `burst_last_seq`
    let burst_len = ConversationFrames::CAPACITY as u64 + 10;
    let burst_last_seq = 1 + burst_len;
    for seq in 2..=burst_last_seq {
        frames.publish(
            SESSION_ID,
            "conv-1",
            ConversationFrame {
                seq,
                frame: agent_text_frame(&format!("A burst frame, seq {seq}."), seq as i64),
            },
        );
    }

    // Then the reader skips the frames the channel dropped and reads on from the oldest one it still holds, in order
    let oldest_held_seq = burst_last_seq + 1 - ConversationFrames::CAPACITY as u64;
    for expected in oldest_held_seq..=burst_last_seq {
        let envelope = tokio::time::timeout(Duration::from_secs(1), stream.next())
            .await
            .expect("a burst frame in time")
            .expect("the stream is still open")
            .expect("not an error");
        assert_eq!(envelope.seq, expected);
    }

    // And a frame recorded after the skip still reaches the reader, so the fall-behind did not end the stream
    frames.publish(
        SESSION_ID,
        "conv-1",
        ConversationFrame {
            seq: burst_last_seq + 1,
            frame: agent_text_frame("A frame after the skip.", 3_000),
        },
    );
    let envelope = tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .expect("the frame after the skip in time")
        .expect("the stream is still open")
        .expect("not an error");
    assert_eq!(envelope.seq, burst_last_seq + 1);
    let live: tddy_service::proto::acp::AcpAgentMessage =
        prost::Message::decode(&envelope.acp_agent_message[..]).expect("decode");
    assert_eq!(agent_text(&live), "A frame after the skip.");
}
