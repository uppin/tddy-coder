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
    let baseline =
        r#"{"codex_oauth":{"pending":true,"authorize_url":"https://auth.example.com/oauth"}}"#;
    let update = format!(r#"{{"{key}":3}}"#, key = OWNED_PROJECT_COUNT_METADATA_KEY);
    let merged =
        merge_participant_metadata_json(baseline, &update).expect("merge returns JSON string");
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

#[tokio::test]
#[serial]
async fn livekit_participant_metadata_includes_project_count() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let projects_dir = tmp.path();
    let expected: u64 = 3;
    write_projects_yaml(projects_dir, expected as usize)?;

    let _ = env_logger::Builder::new()
        .parse_default_env()
        .is_test(true)
        .try_init();

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
