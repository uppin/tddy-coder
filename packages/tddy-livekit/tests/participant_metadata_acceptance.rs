//! Acceptance tests: LiveKit participant metadata includes merged project registry cardinality (PRD).
//!
//! Run: `cargo test -p tddy-livekit --test participant_metadata_acceptance`
//! With shared kit: `eval $(./run-livekit-testkit-server | grep '^export ')` then same command.

use anyhow::Result;
use livekit::prelude::*;
use serde::Serialize;
use serde_json::Value;
use serial_test::serial;
use std::time::Duration;
use tddy_livekit::{
    merge_participant_metadata_json, LiveKitParticipant, OWNED_PROJECT_COUNT_METADATA_KEY,
};
use tddy_livekit_testkit::LiveKitTestkit;
use tddy_service::{EchoServiceImpl, EchoServiceServer};

const SERVER_IDENTITY: &str = "server";
const CLIENT_IDENTITY: &str = "client";
const PARTICIPANT_TIMEOUT: Duration = Duration::from_secs(10);
const METADATA_POLL_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Serialize)]
struct ProjectsFile {
    projects: Vec<ProjectRow>,
}

#[derive(Serialize)]
struct ProjectRow {
    project_id: String,
    name: String,
    git_url: String,
    main_repo_path: String,
}

async fn wait_for_participant(
    room: &Room,
    events: &mut tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
    identity: &str,
) -> Result<()> {
    let target: ParticipantIdentity = identity.to_string().into();
    if room.remote_participants().contains_key(&target) {
        return Ok(());
    }
    tokio::time::timeout(PARTICIPANT_TIMEOUT, async {
        while let Some(event) = events.recv().await {
            if let RoomEvent::ParticipantConnected(p) = event {
                if p.identity() == target {
                    return;
                }
            }
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("Timed out waiting for participant '{}'", identity))?;
    Ok(())
}

fn write_projects_yaml(dir: &std::path::Path, n: usize) -> Result<()> {
    let projects: Vec<ProjectRow> = (0..n)
        .map(|i| ProjectRow {
            project_id: format!("proj-{i}"),
            name: format!("Project {i}"),
            git_url: format!("https://example.com/repo-{i}.git"),
            main_repo_path: format!("/tmp/repo-{i}"),
        })
        .collect();
    let yaml = serde_yaml::to_string(&ProjectsFile { projects })?;
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("projects.yaml"), yaml)?;
    Ok(())
}

#[test]
fn metadata_merge_preserves_codex_oauth_and_project_count() {
    // Given a baseline metadata JSON with codex_oauth fields and an update with project count
    let baseline =
        r#"{"codex_oauth":{"pending":true,"authorize_url":"https://auth.example.com/oauth"}}"#;
    let update = format!(r#"{{"{key}":3}}"#, key = OWNED_PROJECT_COUNT_METADATA_KEY);

    // When merging the update into the baseline
    let merged =
        merge_participant_metadata_json(baseline, &update).expect("merge returns JSON string");

    // Then the result contains both the project count and the preserved codex_oauth fields
    let v: Value = serde_json::from_str(&merged).expect("merged parses as JSON");
    assert_eq!(
        v.get(OWNED_PROJECT_COUNT_METADATA_KEY)
            .and_then(|x| x.as_u64()),
        Some(3),
        "merged metadata must include {} from the update fragment",
        OWNED_PROJECT_COUNT_METADATA_KEY
    );
    assert_eq!(
        v.pointer("/codex_oauth/pending"),
        Some(&Value::Bool(true)),
        "merge must preserve codex_oauth from baseline"
    );
    assert_eq!(
        v.pointer("/codex_oauth/authorize_url"),
        Some(&Value::String("https://auth.example.com/oauth".to_string())),
        "merge must preserve authorize_url from baseline"
    );
}

#[test]
fn metadata_merge_preserves_session_key_and_owned_project_count_mutually() {
    // Regression guard for the `session` metadata key (changeset 2026-07-12-fast-session-change).
    // The merge helper is generic and shallow-merges top-level keys, so a pre-existing `session`
    // block must survive an `owned_project_count` update and vice versa. This test locks that
    // contract so a future narrower merge can't silently drop the `session` block.

    // Given a baseline carrying a `session` block, and an update carrying owned_project_count
    let baseline = r#"{"session":{"workflow_goal":"acceptance-tests","workflow_state":"Red","agent":"claude","model":"sonnet-4"}}"#;
    let update = format!(r#"{{"{key}":3}}"#, key = OWNED_PROJECT_COUNT_METADATA_KEY);

    // When merging the update into the baseline
    let merged =
        merge_participant_metadata_json(baseline, &update).expect("merge returns JSON string");

    // Then the `session` block survives alongside the new owned_project_count
    let v: Value = serde_json::from_str(&merged).expect("merged parses as JSON");
    assert_eq!(
        v.get(OWNED_PROJECT_COUNT_METADATA_KEY)
            .and_then(|x| x.as_u64()),
        Some(3),
        "merge must include owned_project_count from the update"
    );
    assert_eq!(
        v.pointer("/session/workflow_goal").and_then(|x| x.as_str()),
        Some("acceptance-tests"),
        "merge must preserve the baseline `session` block"
    );
    assert_eq!(
        v.pointer("/session/workflow_state")
            .and_then(|x| x.as_str()),
        Some("Red"),
        "merge must preserve workflow_state from the baseline `session` block"
    );

    // And vice versa — a baseline with owned_project_count survives a `session` update
    let baseline2 = format!(r#"{{"{key}":3}}"#, key = OWNED_PROJECT_COUNT_METADATA_KEY);
    let update2 = r#"{"session":{"workflow_goal":"plan","workflow_state":"Plan"}}"#;
    let merged2 =
        merge_participant_metadata_json(&baseline2, update2).expect("merge returns JSON string");
    let v2: Value = serde_json::from_str(&merged2).expect("merged parses as JSON");
    assert_eq!(
        v2.get(OWNED_PROJECT_COUNT_METADATA_KEY)
            .and_then(|x| x.as_u64()),
        Some(3),
        "merge must preserve owned_project_count from the baseline when a `session` update arrives"
    );
    assert_eq!(
        v2.pointer("/session/workflow_state")
            .and_then(|x| x.as_str()),
        Some("Plan"),
        "merge must include the new `session` block from the update"
    );
}

#[tokio::test]
#[serial]
async fn livekit_participant_metadata_includes_project_count() -> Result<()> {
    // Given a LiveKit server with a server participant configured with 3 projects
    let tmp = tempfile::tempdir()?;
    let projects_dir = tmp.path();
    let expected: u64 = 3;
    write_projects_yaml(projects_dir, expected as usize)?;

    let _ = env_logger::Builder::new()
        .parse_default_env()
        .is_test(true)
        .try_init();

    // When the server participant connects and a client observes its metadata
    let livekit = LiveKitTestkit::start().await?;
    let url = livekit.get_ws_url();
    let room_name = "acceptance-owned-project-count";

    let server_token = livekit.generate_token(room_name, SERVER_IDENTITY)?;
    let server = LiveKitParticipant::connect(
        &url,
        &server_token,
        EchoServiceServer::new(EchoServiceImpl),
        RoomOptions::default(),
        None,
        Some(projects_dir.to_path_buf()),
    )
    .await?;
    let server_handle = tokio::spawn(async move { server.run().await });

    let client_token = livekit.generate_token(room_name, CLIENT_IDENTITY)?;
    let (client_room, mut client_events) =
        Room::connect(&url, &client_token, RoomOptions::default())
            .await
            .map_err(|e| anyhow::anyhow!("client connect: {}", e))?;
    wait_for_participant(&client_room, &mut client_events, SERVER_IDENTITY).await?;

    let target: ParticipantIdentity = SERVER_IDENTITY.to_string().into();
    let deadline = tokio::time::Instant::now() + METADATA_POLL_TIMEOUT;
    let mut last_meta = String::new();
    loop {
        if let Some(remote) = client_room.remote_participants().get(&target) {
            last_meta = remote.metadata();
            if let Ok(v) = serde_json::from_str::<Value>(&last_meta) {
                if let Some(n) = v
                    .get(OWNED_PROJECT_COUNT_METADATA_KEY)
                    .and_then(|x| x.as_u64())
                {
                    // Then the metadata project count matches the number of rows in projects.yaml
                    assert_eq!(
                        n, expected,
                        "{} must match read_projects row count for the session projects directory ({})",
                        OWNED_PROJECT_COUNT_METADATA_KEY,
                        projects_dir.display()
                    );
                    server_handle.abort();
                    return Ok(());
                }
            }
        }
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    server_handle.abort();
    panic!(
        "timed out waiting for server metadata to include {}={} (same cardinality as projects.yaml under {}). Last metadata: {:?}",
        OWNED_PROJECT_COUNT_METADATA_KEY,
        expected,
        projects_dir.display(),
        last_meta
    );
}
