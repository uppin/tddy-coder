//! Acceptance: what the daemon's `session_files.SessionFilesService` coordinate does with a
//! request — serve it, or hand it to the daemon that holds the bytes.
//!
//! `#unbundle` node 6 moved the thirteen session-file methods into `tddy-session-files`, which
//! deliberately implements no routing: forwarding needs the eligible-daemon roster, the common room
//! and the LiveKit clients, all of which are the daemon's transport layer. The daemon therefore
//! registers the crate's entry behind a routing wrapper, and these tests are the evidence for both
//! halves of that — a request this host owns is served by the crate against *this* host's data dir
//! and staging root, and a request naming another host leaves this one rather than being answered
//! from it.
//!
//! The routing half is the one that cannot be left to a manual check: `HostDocumentPicker` browses
//! a peer by `browsedDaemonInstanceId`, and a coordinate that quietly served every request locally
//! would answer such a browse with an empty list indistinguishable from a genuinely empty directory.

use std::path::PathBuf;
use std::sync::Arc;

use prost::Message as _;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon::connection_service::PeerRoutedSessionFiles;
use tddy_daemon::multi_host::{DaemonInstanceId, EligibleDaemonInfo, EligibleDaemonSource};
use tddy_daemon::test_util::{test_service, TestDaemon, TEST_TOKEN};
use tddy_rpc::{Code, RpcMessage, RpcResult, RpcService, Status};
use tddy_service::proto::session_files::{
    DeleteStagedAttachmentRequest, ListSessionWorkflowFilesRequest,
    ListSessionWorkflowFilesResponse, ListStagedAttachmentsRequest, ListStagedAttachmentsResponse,
    UploadStagedAttachmentChunkRequest, UploadStagedAttachmentChunkResponse,
};
use tddy_testing_commons::{a_session_metadata, fs::write_session_yaml};

/// The generated server the daemon's entry wraps around its routing surface — the thing that
/// actually answers at a coordinate, and whose `NAME` comes from `session_files.proto` rather than
/// from a caller.
type ServedCoordinate = tddy_service::SessionFilesServiceServer<PeerRoutedSessionFiles>;

/// A peer this daemon can see in the common room, and therefore route to.
const A_PEER_DAEMON: &str = "the-other-host";
/// The OS user `test_service`'s config maps its GitHub login to. Every path this coordinate
/// resolves is under *this* user, never the GitHub one the token names.
const THE_OS_USER: &str = "testdev";
/// The batch a start-session form would stage its documents under.
const STAGING_ID: &str = "5c9d2f10-1c2b-4c9a-9f0e-3a6f2b7d1e44";

/// A common room holding one peer, so a request naming it has somewhere to be routed.
///
/// Without a peer in the roster the request would be refused as unroutable and prove nothing about
/// forwarding: what has to be observable is the fork taken *after* the daemon agrees the id names
/// another host.
struct ACommonRoomHolding {
    peer_instance_id: &'static str,
}

impl EligibleDaemonSource for ACommonRoomHolding {
    fn list_eligible_daemons(&self) -> Vec<EligibleDaemonInfo> {
        vec![EligibleDaemonInfo {
            instance_id: DaemonInstanceId(self.peer_instance_id.to_string()),
            label: self.peer_instance_id.to_string(),
        }]
    }
}

/// A daemon whose data dir and staging area are this test's own directories.
///
/// `test_service` maps [`TEST_TOKEN`] to [`THE_OS_USER`] and points `tddy_data_dir` at
/// `sessions_base`, which is what makes an assertion about *which* directory the coordinate read
/// or wrote meaningful.
fn a_daemon_rooted_at(sessions_base: PathBuf, staging_base: PathBuf) -> TestDaemon {
    test_service(sessions_base).with_staging_base_dir(staging_base)
}

/// The same daemon, able to see [`A_PEER_DAEMON`] and holding no common room to reach it through.
///
/// The missing room is what makes the forward *observable* without a LiveKit server: a request this
/// daemon decides to forward cannot be sent and says so, while a request it serves locally succeeds
/// and leaves a file behind. The two outcomes are never confusable.
fn a_daemon_that_can_see_a_peer_but_cannot_reach_it(
    sessions_base: PathBuf,
    staging_base: PathBuf,
) -> TestDaemon {
    test_service(sessions_base)
        .with_eligible_daemon_source(Arc::new(ACommonRoomHolding {
            peer_instance_id: A_PEER_DAEMON,
        }) as Arc<dyn EligibleDaemonSource>)
        .with_staging_base_dir(staging_base)
}

/// The `session_files.SessionFilesService` this daemon registers, as the host mounts it.
fn the_registered_session_files_service(daemon: TestDaemon) -> Arc<dyn RpcService> {
    let entry = daemon.as_arc().session_files_entry();
    assert_eq!(
        entry.name, "session_files.SessionFilesService",
        "the entry under test must be the session-files coordinate"
    );
    entry.service
}

/// One unary call, at the wire: encoded request in, encoded response or the refusal out.
async fn calling<Req: prost::Message>(
    service: &Arc<dyn RpcService>,
    method: &str,
    request: &Req,
) -> Result<Vec<u8>, Status> {
    let message = RpcMessage::new(request.encode_to_vec(), Default::default());
    match service
        .handle_rpc("session_files.SessionFilesService", method, &message)
        .await
    {
        RpcResult::Unary(answer) => answer,
        RpcResult::ServerStream(_) => panic!("{method} is a unary RPC but answered with a stream"),
    }
}

/// A session of [`THE_OS_USER`]'s, holding the workflow files a recipe wrote.
fn a_session_with_workflow_files(sessions_base: &std::path::Path, session_id: &str) {
    let session_dir = unified_session_dir_path(sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).expect("the session directory could not be created");
    write_session_yaml(
        &session_dir,
        &a_session_metadata().with_session_id(session_id).build(),
    );
    std::fs::write(session_dir.join("PRD.md"), "# Plan\n").expect("PRD.md could not be written");
    std::fs::write(session_dir.join("TODO.md"), "- [ ] item\n")
        .expect("TODO.md could not be written");
}

/// Where this host keeps [`THE_OS_USER`]'s staged attachments.
fn the_staged_file(staging_base: &std::path::Path, file_name: &str) -> PathBuf {
    tddy_daemon::session_attachment_staging::staging_root_for(THE_OS_USER, staging_base)
        .join(STAGING_ID)
        .join(file_name)
}

/// A document already staged on this host, put there through the coordinate under test.
///
/// Staged over the wire rather than written to [`the_staged_file`] directly, so a listing or a
/// delete is asked about a batch this host's own writer produced — layout, mtime and all.
async fn a_staged_attachment(service: &Arc<dyn RpcService>, file_name: &str, bytes: &[u8]) {
    calling(
        service,
        "UploadStagedAttachmentChunk",
        &UploadStagedAttachmentChunkRequest {
            session_token: TEST_TOKEN.to_string(),
            daemon_instance_id: String::new(),
            staging_id: STAGING_ID.to_string(),
            file_name: file_name.to_string(),
            data: bytes.to_vec(),
            last: true,
        },
    )
    .await
    .expect("the fixture's own staged document must upload");
}

/// A listing of one batch of this host's staged attachments, as a client browsing the batch asks.
fn a_listing_of_the_staged_batch(daemon_instance_id: &str) -> ListStagedAttachmentsRequest {
    ListStagedAttachmentsRequest {
        session_token: TEST_TOKEN.to_string(),
        daemon_instance_id: daemon_instance_id.to_string(),
        staging_id: STAGING_ID.to_string(),
    }
}

/// A delete of one staged attachment, as a client removing it from the start-session form does.
fn a_delete_of_the_staged(
    daemon_instance_id: &str,
    file_name: &str,
) -> DeleteStagedAttachmentRequest {
    DeleteStagedAttachmentRequest {
        session_token: TEST_TOKEN.to_string(),
        daemon_instance_id: daemon_instance_id.to_string(),
        staging_id: STAGING_ID.to_string(),
        file_name: file_name.to_string(),
    }
}

/// The name the entry is registered under and the name the server *inside* it answers to have to
/// be one value: the generated `handle_rpc` compares `service` against its own
/// [`ServedCoordinate::NAME`] and refuses anything else, so an entry mounted under a different
/// constant is reachable by nobody — a mismatch no compiler sees, because both ends are `&str`.
/// That is how a forwarded session-file call already reached a host that did not serve it once
/// during this stack.
#[test]
fn registers_the_coordinate_its_own_generated_server_answers_to() {
    // Given a daemon that can see a peer, and so may route a request away from itself
    let sessions = tempfile::tempdir().expect("a temp sessions base");
    let staging = tempfile::tempdir().expect("a temp staging base");
    let daemon = a_daemon_that_can_see_a_peer_but_cannot_reach_it(
        sessions.path().to_path_buf(),
        staging.path().to_path_buf(),
    );

    // When reading the coordinate it registers its session-file entry at
    let registered = daemon.as_arc().session_files_entry().name;

    // Then it is the name generated from `session_files.proto` — which is also where a forward is
    // addressed on the peer, since the peer serves the same generated server
    assert_eq!(registered, ServedCoordinate::NAME);
}

/// A request this host owns is served by `tddy-session-files`, against this host's own data dir.
#[tokio::test]
async fn serves_a_request_this_daemon_owns_from_its_own_sessions() {
    // Given
    let sessions = tempfile::tempdir().expect("a temp sessions base");
    let staging = tempfile::tempdir().expect("a temp staging base");
    let session_id = "0199a6d2-1f6a-7b3c-8e21-4c5d6e7f8a90";
    a_session_with_workflow_files(sessions.path(), session_id);
    let service = the_registered_session_files_service(a_daemon_rooted_at(
        sessions.path().to_path_buf(),
        staging.path().to_path_buf(),
    ));

    // When
    let answer = calling(
        &service,
        "ListSessionWorkflowFiles",
        &ListSessionWorkflowFilesRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: session_id.to_string(),
        },
    )
    .await
    .expect("the registered coordinate must serve a request this daemon owns");

    // Then
    let mut listed: Vec<String> = ListSessionWorkflowFilesResponse::decode(answer.as_slice())
        .expect("the answer must be a ListSessionWorkflowFilesResponse")
        .files
        .into_iter()
        .map(|file| file.basename)
        .collect();
    listed.sort();
    assert_eq!(
        listed,
        vec![
            ".session.yaml".to_string(),
            "PRD.md".to_string(),
            "TODO.md".to_string()
        ],
        "the coordinate must list the session's workflow files from this daemon's data dir"
    );
}

/// A staged upload naming no daemon is this host's, and its bytes land in this host's staging root.
#[tokio::test]
async fn stages_an_unaddressed_upload_in_this_daemons_own_staging_root() {
    // Given
    let sessions = tempfile::tempdir().expect("a temp sessions base");
    let staging = tempfile::tempdir().expect("a temp staging base");
    let service =
        the_registered_session_files_service(a_daemon_that_can_see_a_peer_but_cannot_reach_it(
            sessions.path().to_path_buf(),
            staging.path().to_path_buf(),
        ));

    // When — an empty `daemon_instance_id` is the protocol's spelling for "the daemon called"
    let answer = calling(
        &service,
        "UploadStagedAttachmentChunk",
        &UploadStagedAttachmentChunkRequest {
            session_token: TEST_TOKEN.to_string(),
            daemon_instance_id: String::new(),
            staging_id: STAGING_ID.to_string(),
            file_name: "notes.md".to_string(),
            data: b"staged here".to_vec(),
            last: true,
        },
    )
    .await
    .expect("an unaddressed staged upload must be served locally");

    // Then
    let staged = the_staged_file(staging.path(), "notes.md");
    assert_eq!(
        std::fs::read(&staged).ok(),
        Some(b"staged here".to_vec()),
        "the bytes must land in this host's staging root at {staged:?}"
    );
    let entry = UploadStagedAttachmentChunkResponse::decode(answer.as_slice())
        .expect("the answer must be an UploadStagedAttachmentChunkResponse")
        .entry
        .expect("a final chunk must be answered with the staged entry");
    // Canonicalised on both sides: the writer resolves symlinks (on macOS `/tmp` is one), so the
    // question worth asking is whether the entry names the same file, not the same spelling.
    assert_eq!(
        std::fs::canonicalize(&entry.host_path).ok(),
        std::fs::canonicalize(&staged).ok(),
        "the entry must name the file this host actually wrote; it named {:?}",
        entry.host_path
    );
}

/// A staged upload naming another daemon is forwarded to it — never written here.
///
/// The regression this guards is silent: were the wrapper missing, this call would be served
/// locally and answer `Ok`, leaving the caller's document on the wrong host with nothing in the
/// response to say so.
#[tokio::test]
async fn forwards_a_staged_upload_addressed_to_another_daemon_instead_of_staging_it_here() {
    // Given
    let sessions = tempfile::tempdir().expect("a temp sessions base");
    let staging = tempfile::tempdir().expect("a temp staging base");
    let service =
        the_registered_session_files_service(a_daemon_that_can_see_a_peer_but_cannot_reach_it(
            sessions.path().to_path_buf(),
            staging.path().to_path_buf(),
        ));

    // When
    let refusal = calling(
        &service,
        "UploadStagedAttachmentChunk",
        &UploadStagedAttachmentChunkRequest {
            session_token: TEST_TOKEN.to_string(),
            daemon_instance_id: A_PEER_DAEMON.to_string(),
            staging_id: STAGING_ID.to_string(),
            file_name: "for-the-peer.md".to_string(),
            data: b"belongs on the other host".to_vec(),
            last: true,
        },
    )
    .await
    .expect_err("an upload addressed to an unreachable peer cannot be answered");

    // Then — the call left for the peer and failed there, rather than being served here
    assert_eq!(
        refusal.code,
        Code::FailedPrecondition,
        "a forward with no common-room connection must be refused as a precondition, not served \
         locally; got {:?}: {}",
        refusal.code,
        refusal.message
    );
    assert!(
        refusal.message.contains("forward"),
        "the refusal must say the call was being forwarded; got {:?}",
        refusal.message
    );
    let staged = the_staged_file(staging.path(), "for-the-peer.md");
    assert!(
        !staged.exists(),
        "a request addressed to another daemon must not be staged here, but {staged:?} was written"
    );
}

/// A listing naming no daemon is this host's, and it names this host's own staged bytes.
#[tokio::test]
async fn lists_this_daemons_own_staged_attachments_when_no_daemon_is_named() {
    // Given
    let sessions = tempfile::tempdir().expect("a temp sessions base");
    let staging = tempfile::tempdir().expect("a temp staging base");
    let service =
        the_registered_session_files_service(a_daemon_that_can_see_a_peer_but_cannot_reach_it(
            sessions.path().to_path_buf(),
            staging.path().to_path_buf(),
        ));
    a_staged_attachment(&service, "notes.md", b"staged here").await;

    // When — an empty `daemon_instance_id` is the protocol's spelling for "the daemon called"
    let answer = calling(
        &service,
        "ListStagedAttachments",
        &a_listing_of_the_staged_batch(""),
    )
    .await
    .expect("an unaddressed listing must be served locally");

    // Then — canonicalised because this host's writer resolves symlinks (on macOS `/tmp` is one),
    // so the question is whether the entry names the same file, not the same spelling
    let listed: Vec<(String, u64, Option<PathBuf>)> =
        ListStagedAttachmentsResponse::decode(answer.as_slice())
            .expect("the answer must be a ListStagedAttachmentsResponse")
            .attachments
            .into_iter()
            .map(|attachment| {
                (
                    attachment.file_name,
                    attachment.size_bytes,
                    std::fs::canonicalize(&attachment.host_path).ok(),
                )
            })
            .collect();
    assert_eq!(
        listed,
        vec![(
            "notes.md".to_string(),
            "staged here".len() as u64,
            std::fs::canonicalize(the_staged_file(staging.path(), "notes.md")).ok()
        )],
        "the listing must be this host's staging root, read from disk"
    );
}

/// A listing naming another daemon is forwarded to it — never answered from this host's batch.
///
/// The regression this guards is the one `HostDocumentPicker` shows the user: a coordinate that
/// quietly listed locally would answer a browse of the peer's staging area with *this* host's
/// documents, which a client cannot tell from the peer's own.
#[tokio::test]
async fn forwards_a_staged_listing_addressed_to_another_daemon_instead_of_listing_its_own() {
    // Given
    let sessions = tempfile::tempdir().expect("a temp sessions base");
    let staging = tempfile::tempdir().expect("a temp staging base");
    let service =
        the_registered_session_files_service(a_daemon_that_can_see_a_peer_but_cannot_reach_it(
            sessions.path().to_path_buf(),
            staging.path().to_path_buf(),
        ));
    a_staged_attachment(&service, "staged-on-this-host.md", b"belongs to this host").await;

    // When
    let refusal = calling(
        &service,
        "ListStagedAttachments",
        &a_listing_of_the_staged_batch(A_PEER_DAEMON),
    )
    .await
    .expect_err("a listing addressed to an unreachable peer cannot be answered");

    // Then — the call left for the peer and failed there, rather than being answered from here
    assert_eq!(
        (refusal.code, refusal.message.contains("forward")),
        (Code::FailedPrecondition, true),
        "a forward with no common-room connection must be refused as a precondition naming the \
         forward, not served from this host's own staging root; got {:?}: {}",
        refusal.code,
        refusal.message
    );
}

/// A delete naming no daemon is this host's, and it removes this host's staged bytes.
#[tokio::test]
async fn deletes_a_staged_attachment_this_daemon_holds_when_no_daemon_is_named() {
    // Given
    let sessions = tempfile::tempdir().expect("a temp sessions base");
    let staging = tempfile::tempdir().expect("a temp staging base");
    let service =
        the_registered_session_files_service(a_daemon_that_can_see_a_peer_but_cannot_reach_it(
            sessions.path().to_path_buf(),
            staging.path().to_path_buf(),
        ));
    a_staged_attachment(&service, "notes.md", b"staged here").await;

    // When
    calling(
        &service,
        "DeleteStagedAttachment",
        &a_delete_of_the_staged("", "notes.md"),
    )
    .await
    .expect("an unaddressed delete must be served locally");

    // Then
    let staged = the_staged_file(staging.path(), "notes.md");
    assert!(
        !staged.exists(),
        "the delete must remove the file from this host's staging root, but {staged:?} is still there"
    );
}

/// A delete naming another daemon is forwarded to it — never applied to this host's batch.
///
/// The regression this guards destroys data: were the wrapper missing, removing a document from
/// the peer's staging area would delete the same-named one here and answer `Ok`, with nothing in
/// the response to say which host lost a file.
#[tokio::test]
async fn forwards_a_staged_delete_addressed_to_another_daemon_instead_of_deleting_here() {
    // Given
    let sessions = tempfile::tempdir().expect("a temp sessions base");
    let staging = tempfile::tempdir().expect("a temp staging base");
    let service =
        the_registered_session_files_service(a_daemon_that_can_see_a_peer_but_cannot_reach_it(
            sessions.path().to_path_buf(),
            staging.path().to_path_buf(),
        ));
    a_staged_attachment(&service, "notes.md", b"staged here").await;

    // When
    let refusal = calling(
        &service,
        "DeleteStagedAttachment",
        &a_delete_of_the_staged(A_PEER_DAEMON, "notes.md"),
    )
    .await
    .expect_err("a delete addressed to an unreachable peer cannot be answered");

    // Then — the call left for the peer and failed there
    assert_eq!(
        (refusal.code, refusal.message.contains("forward")),
        (Code::FailedPrecondition, true),
        "a forward with no common-room connection must be refused as a precondition naming the \
         forward, not applied here; got {:?}: {}",
        refusal.code,
        refusal.message
    );

    // Then — and this host's own copy of that name is untouched
    let staged = the_staged_file(staging.path(), "notes.md");
    assert_eq!(
        std::fs::read(&staged).ok(),
        Some(b"staged here".to_vec()),
        "a delete addressed to another daemon must leave {staged:?} alone"
    );
}
