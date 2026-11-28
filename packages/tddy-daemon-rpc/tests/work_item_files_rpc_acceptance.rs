//! Acceptance: attaching and detaching a work item's files over the daemon's RPCs.
//!
//! PRD: `docs/ft/coder/work-item-files.md`
//! Changeset: `docs/dev/1-WIP/2026-11-28-work-item-files.md`
//!
//! The queue is seeded straight into the host's store and addressed by its address, as the
//! annotations suite does. A staged upload is written the same way `UploadStagedAttachmentChunk`
//! leaves it — final chunk, `.staged-complete` marker beside it — because promoting a *completed*
//! upload is the only thing an attach may consume.

mod support;

use pretty_assertions::assert_eq;
use support::{a_host, Host, InProcessPeers, HOST_A, THIS_PROJECT};
use tddy_core::changeset::{WorkItem, WorkQueue};
use tddy_core::session_work_queue::SessionWorkQueues;
use tddy_rpc::{Code, Request};
use tddy_service::proto::session::StagedAttachmentRef;
use tddy_service::proto::work_queue::work_queue_file_source::Origin;
use tddy_service::proto::work_queue::{
    AttachWorkItemFileRequest, DetachWorkItemFileRequest, WorkQueueAddress, WorkQueueFileSource,
    WorkQueueService,
};
use tddy_session_files::session_attachment_staging::{staging_root_for, write_staged_chunk};
use tddy_session_lifecycle::test_util::TEST_TOKEN;

const OWNER: &str = "queue-owner-files";
/// The OS user `TEST_TOKEN` maps to under the test config (`test_util::USERS_YAML`).
const OS_USER: &str = "testdev";
const ITEM: &str = "n1";

fn a_host_holding_a_queue_with_one_item() -> (Host, WorkQueueAddress) {
    let host = a_host(HOST_A, &std::sync::Arc::new(InProcessPeers::default()));
    let address = queue_address(&host);
    (host, address)
}

fn queue_address(host: &Host) -> WorkQueueAddress {
    let queues = SessionWorkQueues::new(host.sessions.path().join("sessions"));
    queues
        .create_for_session(OWNER, THIS_PROJECT, 0)
        .expect("the owner gets its queue");
    queues
        .update(OWNER, |queue| {
            *queue = WorkQueue {
                version: 1,
                nodes: vec![WorkItem {
                    node_id: ITEM.to_string(),
                    title: "Wire it".to_string(),
                    ..WorkItem::default()
                }],
                ..WorkQueue::default()
            }
        })
        .expect("the queue is filled");
    WorkQueueAddress {
        project_id: THIS_PROJECT.to_string(),
        queue_id: OWNER.to_string(),
        daemon_instance_id: HOST_A.to_string(),
    }
}

/// A staged upload exactly as the final chunk of `UploadStagedAttachmentChunk` leaves it —
/// bytes and `.staged-complete` marker — under a staging batch id this test alone uses, and
/// answers that id.
fn a_completed_staged_upload(_host: &Host, file_name: &str, bytes: &[u8]) -> String {
    let staging_id = format!("staging-{}", uuid::Uuid::new_v4());
    let root = staging_root_for(
        OS_USER,
        &tddy_session_files::session_attachment_staging::default_staging_base_dir(),
    );
    write_staged_chunk(&root, &staging_id, file_name, bytes, true)
        .expect("the final chunk writes the file and its marker");
    staging_id
}

fn staged_source(staging_id: &str) -> Option<WorkQueueFileSource> {
    Some(WorkQueueFileSource {
        origin: Some(Origin::Staged(StagedAttachmentRef {
            daemon_instance_id: HOST_A.to_string(),
            staging_id: staging_id.to_string(),
            file_name: String::new(),
        })),
    })
}

async fn attach(
    host: &Host,
    address: &WorkQueueAddress,
    item: &str,
    basename: &str,
    source: Option<WorkQueueFileSource>,
    file_name: &str,
) -> Result<String, tddy_rpc::Status> {
    let mut source = source.expect("a source");
    if let Some(Origin::Staged(staged)) = source.origin.as_mut() {
        staged.file_name = file_name.to_string();
    }
    host.daemon
        .attach_work_item_file(Request::direct(AttachWorkItemFileRequest {
            session_token: TEST_TOKEN.to_string(),
            queue: Some(address.clone()),
            item_id: item.to_string(),
            basename: basename.to_string(),
            source: Some(source),
        }))
        .await
        .map(|answer| answer.into_inner().work_queue_json)
}

async fn detach(
    host: &Host,
    address: &WorkQueueAddress,
    item: &str,
    basename: &str,
) -> Result<String, tddy_rpc::Status> {
    host.daemon
        .detach_work_item_file(Request::direct(DetachWorkItemFileRequest {
            session_token: TEST_TOKEN.to_string(),
            queue: Some(address.clone()),
            item_id: item.to_string(),
            basename: basename.to_string(),
        }))
        .await
        .map(|answer| answer.into_inner().work_queue_json)
}

fn item_json(work_queue_json: &str) -> serde_json::Value {
    let plan: serde_json::Value = serde_json::from_str(work_queue_json).expect("JSON");
    plan["nodes"][0].clone()
}

fn stored_bytes(host: &Host, item: &str, basename: &str) -> Option<Vec<u8>> {
    std::fs::read(
        host.sessions
            .path()
            .join("sessions")
            .join(OWNER)
            .join("work-queue-files")
            .join(item)
            .join(basename),
    )
    .ok()
}

fn staged_copy(staging_id: &str, file_name: &str) -> Option<Vec<u8>> {
    std::fs::read(
        staging_root_for(
            OS_USER,
            &tddy_session_files::session_attachment_staging::default_staging_base_dir(),
        )
        .join(staging_id)
        .join(file_name),
    )
    .ok()
}

fn project_repo_of(host: &Host) -> std::path::PathBuf {
    let project = tddy_projects::project_storage::find_project(
        &host.sessions.path().join("projects"),
        THIS_PROJECT,
    )
    .expect("the project registry is readable")
    .expect("the project is registered");
    std::path::PathBuf::from(project.main_repo_path)
}

#[tokio::test]
async fn attaching_a_completed_staged_upload_records_the_file_and_consumes_the_staged_copy() {
    // Given
    let (host, address) = a_host_holding_a_queue_with_one_item();
    let staging_id = a_completed_staged_upload(&host, "failure.log", b"# failure log\n");

    // When
    let answered = attach(
        &host,
        &address,
        ITEM,
        "failure.log",
        staged_source(&staging_id),
        "failure.log",
    )
    .await
    .expect("attached");

    // Then
    assert_eq!(
        item_json(&answered)["files"],
        serde_json::json!([{ "basename": "failure.log", "size_bytes": 14 }]),
        "the answered queue names the attached file with its size"
    );
    assert_eq!(
        stored_bytes(&host, ITEM, "failure.log").expect("the bytes are stored"),
        b"# failure log\n".to_vec()
    );
    assert!(
        staged_copy(&staging_id, "failure.log").is_none(),
        "the consumed staged upload is deleted, not left to leak"
    );
}

#[tokio::test]
async fn attaching_a_basename_the_item_already_carries_is_refused_and_writes_nothing() {
    // Given
    let (host, address) = a_host_holding_a_queue_with_one_item();
    let first = a_completed_staged_upload(&host, "spec.md", b"first");
    attach(
        &host,
        &address,
        ITEM,
        "spec.md",
        staged_source(&first),
        "spec.md",
    )
    .await
    .expect("first attach");
    let second = a_completed_staged_upload(&host, "spec.md", b"second");

    // When
    let refusal = attach(
        &host,
        &address,
        ITEM,
        "spec.md",
        staged_source(&second),
        "spec.md",
    )
    .await
    .expect_err("the duplicate must be refused");

    // Then
    assert_eq!(refusal.code, Code::InvalidArgument);
    assert_eq!(
        stored_bytes(&host, ITEM, "spec.md"),
        Some(b"first".to_vec()),
        "the first file is untouched"
    );
}

#[tokio::test]
async fn attaching_to_an_unknown_item_is_refused() {
    // Given
    let (host, address) = a_host_holding_a_queue_with_one_item();
    let staging_id = a_completed_staged_upload(&host, "spec.md", b"x");

    // When
    let refusal = attach(
        &host,
        &address,
        "n9",
        "spec.md",
        staged_source(&staging_id),
        "spec.md",
    )
    .await
    .expect_err("an unknown item must be refused");

    // Then
    assert_eq!(refusal.code, Code::NotFound);
    assert!(stored_bytes(&host, "n9", "spec.md").is_none());
}

#[tokio::test]
async fn an_upload_that_never_completed_is_refused_and_nothing_is_written() {
    // Given — chunks arrived but no final one: no `.staged-complete` marker
    let (host, address) = a_host_holding_a_queue_with_one_item();
    let root = staging_root_for(
        OS_USER,
        &tddy_session_files::session_attachment_staging::default_staging_base_dir(),
    );
    let staging_id = format!("staging-{}", uuid::Uuid::new_v4());
    write_staged_chunk(&root, &staging_id, "partial.md", b"half", false).expect("chunk");

    // When
    let refusal = attach(
        &host,
        &address,
        ITEM,
        "partial.md",
        staged_source(&staging_id),
        "partial.md",
    )
    .await
    .expect_err("an incomplete upload must be refused");

    // Then
    assert_eq!(refusal.code, Code::FailedPrecondition);
    assert!(stored_bytes(&host, ITEM, "partial.md").is_none());
}

#[tokio::test]
async fn detaching_removes_the_entry_and_the_bytes() {
    // Given
    let (host, address) = a_host_holding_a_queue_with_one_item();
    let staging_id = a_completed_staged_upload(&host, "spec.md", b"# the spec\n");
    attach(
        &host,
        &address,
        ITEM,
        "spec.md",
        staged_source(&staging_id),
        "spec.md",
    )
    .await
    .expect("attached");

    // When
    let answered = detach(&host, &address, ITEM, "spec.md")
        .await
        .expect("detached");

    // Then
    // An empty list is omitted from the queue JSON entirely (`skip_serializing_if`), so the field
    // reads back absent rather than `[]`.
    assert_eq!(item_json(&answered)["files"], serde_json::json!(null));
    assert!(stored_bytes(&host, ITEM, "spec.md").is_none());
}

#[tokio::test]
async fn detaching_a_file_the_item_does_not_carry_is_not_found() {
    // Given
    let (host, address) = a_host_holding_a_queue_with_one_item();

    // When
    let refusal = detach(&host, &address, ITEM, "absent.md")
        .await
        .expect_err("an unknown file must be refused");

    // Then
    assert_eq!(refusal.code, Code::NotFound);
}

#[tokio::test]
async fn the_file_after_the_cap_is_refused_naming_the_cap() {
    // Given — the cap is exactly full
    let (host, address) = a_host_holding_a_queue_with_one_item();
    for i in 0..20 {
        let name = format!("file-{i}.md");
        let staging_id = a_completed_staged_upload(&host, &name, format!("file {i}").as_bytes());
        attach(
            &host,
            &address,
            ITEM,
            &name,
            staged_source(&staging_id),
            &name,
        )
        .await
        .expect("within the cap");
    }
    let staging_id = a_completed_staged_upload(&host, "one-too-many.md", b"x");

    // When
    let refusal = attach(
        &host,
        &address,
        ITEM,
        "one-too-many.md",
        staged_source(&staging_id),
        "one-too-many.md",
    )
    .await
    .expect_err("the cap must refuse");

    // Then
    assert_eq!(refusal.code, Code::InvalidArgument);
    assert!(
        refusal.message.contains("20"),
        "the refusal names the limit: {}",
        refusal.message
    );
    assert!(stored_bytes(&host, ITEM, "one-too-many.md").is_none());
}

#[tokio::test]
async fn attaching_the_owners_own_session_artifact_records_it() {
    // Given — the owner session has an artifact beside its queue
    let (host, address) = a_host_holding_a_queue_with_one_item();
    let artifacts = host
        .sessions
        .path()
        .join("sessions")
        .join(OWNER)
        .join("artifacts");
    std::fs::create_dir_all(&artifacts).expect("artifacts root");
    std::fs::write(artifacts.join("design-note.md"), b"# Design\n").expect("artifact");

    // When
    let answered = attach(
        &host,
        &address,
        ITEM,
        "design-note.md",
        Some(WorkQueueFileSource {
            origin: Some(Origin::ArtifactRelativePath("design-note.md".to_string())),
        }),
        "",
    )
    .await
    .expect("attached");

    // Then
    assert_eq!(
        item_json(&answered)["files"],
        serde_json::json!([{ "basename": "design-note.md", "size_bytes": 9 }])
    );
    assert_eq!(
        stored_bytes(&host, ITEM, "design-note.md").expect("stored"),
        b"# Design\n".to_vec()
    );
}

#[tokio::test]
async fn attaching_a_repo_file_records_it() {
    // Given — the project's repo holds a checked-in document
    let (host, address) = a_host_holding_a_queue_with_one_item();
    let repo = project_repo_of(&host);
    std::fs::write(repo.join("NOTES.md"), b"# Notes\n").expect("repo file");

    // When
    let answered = attach(
        &host,
        &address,
        ITEM,
        "NOTES.md",
        Some(WorkQueueFileSource {
            origin: Some(Origin::RepoFilePath("NOTES.md".to_string())),
        }),
        "",
    )
    .await
    .expect("attached");

    // Then
    assert_eq!(
        item_json(&answered)["files"],
        serde_json::json!([{ "basename": "NOTES.md", "size_bytes": 8 }])
    );
}

#[tokio::test]
async fn a_path_that_escapes_its_source_root_is_refused() {
    // Given
    let (host, address) = a_host_holding_a_queue_with_one_item();

    // When
    let refusal = attach(
        &host,
        &address,
        ITEM,
        "spec.md",
        Some(WorkQueueFileSource {
            origin: Some(Origin::RepoFilePath("../outside.md".to_string())),
        }),
        "",
    )
    .await
    .expect_err("an escaping path must be refused");

    // Then
    assert_eq!(refusal.code, Code::InvalidArgument);
    assert!(stored_bytes(&host, ITEM, "spec.md").is_none());
}
