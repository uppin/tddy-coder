//! `StreamAgentActivityDelta` at the RPC — AC6-AC14 of
//! `docs/ft/daemon/session-worktree-sync.md`.
//!
//! `session_activity_delta_acceptance.rs` pins what a tick measures and what the ring retains.
//! What is pinned here is how one of those deltas reaches a client: the frames it is cut into, and
//! the answers a caller gets when there is no delta to serve.
//!
//! A hosted session room needs LiveKit, so the two lookup failures a room *can* produce — an
//! unknown call and an aged-out one — are pinned against `SessionDeltaStore` in that suite rather
//! than through this handler. What the handler adds over the store is what is pinned here: the
//! authorization that runs before any lookup, the absence of a room at all, and the framing.

use pretty_assertions::assert_eq;
use tddy_daemon::connection_service::{activity_delta_frames, HOST_DOCUMENT_FRAME_BYTES};
use tddy_daemon::session_room::ActivityDelta;
use tddy_daemon::test_util::{test_service, TEST_TOKEN};
use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::connection::{
    AgentActivityDeltaChunk, AgentActivityDeltaRequest,
    ConnectionService as ConnectionServiceTrait, DeltaScope,
};

const A_SESSION: &str = "1780828020298-delta";
const A_CALL: &str = "0199c7a4-6c2c-7c9a-9d1e-3f0a1b2c3d4e";
const A_BASE_COMMIT: &str = "9f1c0b6d2a4e8f37c5b1d9e0a2c4f6b8d0e2a4c6";
const A_SCOPED_PATH: &str = "src/main.rs";

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

fn a_delta_request() -> AgentActivityDeltaRequest {
    AgentActivityDeltaRequest {
        session_token: TEST_TOKEN.to_string(),
        session_id: A_SESSION.to_string(),
        daemon_instance_id: String::new(),
        call_id: A_CALL.to_string(),
        scope: DeltaScope::Call as i32,
    }
}

/// A tick delta of `patch_len` bytes, credited to one file.
fn a_delta_of(patch_len: usize) -> ActivityDelta {
    ActivityDelta {
        seq: 7,
        prev_seq: 6,
        base_commit: A_BASE_COMMIT.to_string(),
        patch: (0..patch_len).map(|i| (i % 256) as u8).collect(),
        scoped_paths: vec![A_SCOPED_PATH.to_string()],
    }
}

/// Everything a frame says about the tick it belongs to — the fields that must repeat on all of
/// them, so a reader knows what it is receiving without a header frame to special-case.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FrameDescription {
    seq: u64,
    prev_seq: u64,
    base_commit: String,
    total_byte_size: u64,
    scoped_paths: Vec<String>,
}

fn a_description_of(total_byte_size: u64) -> FrameDescription {
    FrameDescription {
        seq: 7,
        prev_seq: 6,
        base_commit: A_BASE_COMMIT.to_string(),
        total_byte_size,
        scoped_paths: vec![A_SCOPED_PATH.to_string()],
    }
}

fn descriptions(frames: &[AgentActivityDeltaChunk]) -> Vec<FrameDescription> {
    frames
        .iter()
        .map(|frame| FrameDescription {
            seq: frame.seq,
            prev_seq: frame.prev_seq,
            base_commit: frame.base_commit.clone(),
            total_byte_size: frame.total_byte_size,
            scoped_paths: frame.scoped_paths.clone(),
        })
        .collect()
}

fn patches(frames: &[AgentActivityDeltaChunk]) -> Vec<Vec<u8>> {
    frames.iter().map(|frame| frame.patch.clone()).collect()
}

/// The status of a refused lookup, against a daemon hosting no session room at all.
///
/// `expect_err` is the assertion as much as the setup: a refusal must fail the call rather than
/// open a stream that ends empty, because an empty stream is what "this call changed nothing"
/// looks like.
async fn a_refused_lookup(request: AgentActivityDeltaRequest) -> Status {
    let sessions = tempfile::tempdir().expect("sessions tempdir");
    let service = test_service(sessions.path().to_path_buf());
    service
        .stream_agent_activity_delta(Request::new(request))
        .await
        .expect_err("expected the lookup to be refused rather than streamed")
}

// ---------------------------------------------------------------------------
// Framing — AC6, AC9
// ---------------------------------------------------------------------------

#[test]
fn cuts_a_patch_spanning_several_frames_into_whole_frames_and_a_remainder() {
    // Given a patch two frames and a bit long
    let delta = a_delta_of(HOST_DOCUMENT_FRAME_BYTES * 2 + 512);

    // When it is framed for the wire
    let frames = activity_delta_frames(&delta);

    // Then no frame approaches what the transport chunk-frames, and the patch rebuilds exactly
    assert_eq!(
        frames.iter().map(|f| f.patch.len()).collect::<Vec<_>>(),
        vec![HOST_DOCUMENT_FRAME_BYTES, HOST_DOCUMENT_FRAME_BYTES, 512]
    );
    assert_eq!(patches(&frames).concat(), delta.patch);
}

#[test]
fn describes_the_tick_on_every_frame_rather_than_in_a_header() {
    // Given a patch spanning more than one frame
    let delta = a_delta_of(HOST_DOCUMENT_FRAME_BYTES + 1);

    // When it is framed for the wire
    let frames = activity_delta_frames(&delta);

    // Then every frame places the patch and names what it was scoped to, so a client can check the
    // server scoped the way it asked rather than trusting that it did
    let expected = a_description_of(HOST_DOCUMENT_FRAME_BYTES as u64 + 1);
    assert_eq!(descriptions(&frames), vec![expected.clone(), expected]);
}

#[test]
fn frames_a_call_that_changed_nothing_as_one_empty_patch() {
    // Given a call credited with paths its tick's diff never touched
    let delta = a_delta_of(0);

    // When it is framed for the wire
    let frames = activity_delta_frames(&delta);

    // Then "nothing changed" stays distinguishable from "the stream failed"
    assert_eq!(patches(&frames), vec![Vec::<u8>::new()]);
    assert_eq!(descriptions(&frames), vec![a_description_of(0)]);
}

// ---------------------------------------------------------------------------
// Refusals — AC10, AC14
// ---------------------------------------------------------------------------

#[tokio::test]
async fn refuses_an_unknown_session_token_before_looking_up_any_session() {
    // Given a request carrying a token the daemon does not know, for a session it hosts no room for
    let request = AgentActivityDeltaRequest {
        session_token: "not-a-token".to_string(),
        ..a_delta_request()
    };

    // When the delta is asked for
    let status = a_refused_lookup(request).await;

    // Then the caller learns nothing about which sessions this daemon hosts: authorization is
    // answered first, so the same request cannot be told apart by its NOT_FOUND.
    assert_eq!(status.code(), Code::Unauthenticated);
}

#[tokio::test]
async fn reports_a_session_this_daemon_hosts_no_room_for_by_name() {
    // Given an authorized caller asking about a session whose room is elsewhere
    let request = a_delta_request();

    // When the delta is asked for
    let status = a_refused_lookup(request).await;

    // Then the absence is named, so a client can tell "wrong daemon" from "unknown call"
    assert_eq!(status.code(), Code::NotFound);
    assert!(
        status.message().contains(A_SESSION),
        "the refusal must name the session it is about, was {:?}",
        status.message()
    );
}

#[tokio::test]
async fn refuses_a_request_that_names_no_call() {
    // Given a request with an empty call_id — there is deliberately no whole-worktree delta
    let request = AgentActivityDeltaRequest {
        call_id: String::new(),
        ..a_delta_request()
    };

    // When the delta is asked for
    let status = a_refused_lookup(request).await;

    // Then
    assert_eq!(status.code(), Code::InvalidArgument);
}
