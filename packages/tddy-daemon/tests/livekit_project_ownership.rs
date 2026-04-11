//! Project-data ownership and replica registry sync — acceptance tests (PRD Testing Plan).
//!
//! Uses `tddy-livekit-testkit`: respects `LIVEKIT_TESTKIT_WS_URL` when set (see AGENTS.md),
//! otherwise starts a LiveKit container via testcontainers.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use livekit::prelude::*;
use tokio::sync::mpsc;
use uuid::Uuid;

use tddy_daemon::project_data_ownership::{
    apply_owner_project_registry_snapshot_to_replica,
    converge_replica_project_registry_with_elected_owner, count_remote_project_data_owners,
    join_common_room_and_publish_project_ownership_metadata,
    metadata_claims_active_project_data_owner, project_data_owner_flag_from_metadata,
    refresh_project_data_ownership_metadata,
};
use tddy_daemon::project_storage::{add_project, read_projects, ProjectData};
use tddy_livekit_testkit::LiveKitTestkit;

const STABILIZE: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(200);

async fn start_testkit() -> anyhow::Result<(LiveKitTestkit, String)> {
    let kit = LiveKitTestkit::start().await?;
    let ws_url = kit.get_ws_url();
    Ok((kit, ws_url))
}

/// Holds a LiveKit room open using the same connect path the daemon will use once ownership is implemented.
fn spawn_placeholder_daemon_room(ws_url: String, token: String) {
    tokio::spawn(async move {
        let (room, mut events) =
            join_common_room_and_publish_project_ownership_metadata(&ws_url, &token)
                .await
                .expect("daemon room connect");
        let until = tokio::time::Instant::now() + Duration::from_secs(90);
        while tokio::time::Instant::now() < until {
            let _ = tokio::time::timeout(POLL, events.recv()).await;
            let _ = refresh_project_data_ownership_metadata(&room).await;
        }
        drop(room);
    });
}

async fn wait_for_remote_count(
    room: &Room,
    events: &mut mpsc::UnboundedReceiver<RoomEvent>,
    want: usize,
) -> anyhow::Result<()> {
    let deadline = Instant::now() + STABILIZE;
    loop {
        if room.remote_participants().len() >= want {
            return Ok(());
        }
        if Instant::now() > deadline {
            anyhow::bail!(
                "timed out waiting for {want} remote participants (have {})",
                room.remote_participants().len()
            );
        }
        let _ = tokio::time::timeout(POLL, events.recv()).await;
    }
}

/// PRD: one eligible and one ineligible daemon in `common_room` → exactly one elected owner in metadata with required JSON keys.
#[tokio::test]
async fn livekit_metadata_contains_project_owner_fields_for_elected_daemon() -> anyhow::Result<()> {
    let (kit, ws_url) = start_testkit().await?;
    let room_name = format!("proj-own-meta-{}", Uuid::new_v4());
    let tok_eligible = kit.generate_token(&room_name, "daemon-eligible")?;
    let tok_ineligible = kit.generate_token(&room_name, "daemon-ineligible")?;

    spawn_placeholder_daemon_room(ws_url.clone(), tok_eligible);
    spawn_placeholder_daemon_room(ws_url.clone(), tok_ineligible);
    tokio::time::sleep(Duration::from_millis(400)).await;

    let tok_obs = kit.generate_token(&room_name, "observer-meta")?;
    let (obs, mut ev) = Room::connect(&ws_url, &tok_obs, RoomOptions::default())
        .await
        .map_err(|e| anyhow::anyhow!("observer connect: {e}"))?;

    wait_for_remote_count(&obs, &mut ev, 2).await?;

    let deadline = Instant::now() + STABILIZE;
    loop {
        let owner_count = count_remote_project_data_owners(&obs);
        if owner_count == 1 {
            let remotes = obs.remote_participants();
            let owners: Vec<_> = remotes
                .values()
                .filter(|p| metadata_claims_active_project_data_owner(&p.metadata()))
                .collect();
            assert_eq!(owners.len(), 1);
            let elected = owners[0];
            assert!(
                metadata_claims_active_project_data_owner(&elected.metadata()),
                "elected participant metadata must include daemon_instance_id, project_data_owner true, and schema version"
            );
            for p in obs.remote_participants().values() {
                if p.identity().as_str() == "daemon-ineligible" {
                    assert_ne!(
                        project_data_owner_flag_from_metadata(&p.metadata()),
                        Some(true),
                        "daemon with project-data ownership disabled must never advertise project_data_owner true"
                    );
                }
            }
            return Ok(());
        }
        if Instant::now() > deadline {
            anyhow::bail!(
                "expected metadata_owner_count == 1 after stabilization; got {owner_count} (remotes={})",
                obs.remote_participants().len()
            );
        }
        let _ = tokio::time::timeout(POLL, ev.recv()).await;
    }
}

/// PRD: owner creates a project; replica registry matches (sorted project ids) within a bounded wait.
#[tokio::test]
async fn replica_project_registry_matches_owner_after_create() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let owner_dir = temp.path().join("owner_projects");
    let replica_dir = temp.path().join("replica_projects");
    std::fs::create_dir_all(&owner_dir)?;
    std::fs::create_dir_all(&replica_dir)?;

    let p = ProjectData {
        project_id: "proj-accept-replica-1".to_string(),
        name: "accept-replica".to_string(),
        git_url: "https://github.com/org/accept-replica.git".to_string(),
        main_repo_path: "/tmp/accept-replica".to_string(),
        main_branch_ref: None,
        host_repo_paths: HashMap::new(),
    };
    add_project(&owner_dir, p)?;

    let deadline = Instant::now() + STABILIZE;
    loop {
        apply_owner_project_registry_snapshot_to_replica(&replica_dir, &owner_dir)?;
        let mut owner_ids: Vec<_> = read_projects(&owner_dir)?
            .into_iter()
            .map(|x| x.project_id)
            .collect();
        let mut replica_ids: Vec<_> = read_projects(&replica_dir)?
            .into_iter()
            .map(|x| x.project_id)
            .collect();
        owner_ids.sort();
        replica_ids.sort();
        if owner_ids == replica_ids && !owner_ids.is_empty() {
            return Ok(());
        }
        if Instant::now() > deadline {
            anyhow::bail!(
                "replica projects.yaml did not match owner after {:?}; owner={owner_ids:?} replica={replica_ids:?}",
                STABILIZE
            );
        }
        tokio::time::sleep(POLL).await;
    }
}

/// PRD: two ownership-eligible daemons → single metadata owner and no divergent authoritative registry files after convergence.
#[tokio::test]
async fn dual_owner_eligibility_converges_to_single_writer() -> anyhow::Result<()> {
    let (kit, ws_url) = start_testkit().await?;
    let room_name = format!("proj-dual-{}", Uuid::new_v4());
    let tok_a = kit.generate_token(&room_name, "daemon-owner-a")?;
    let tok_b = kit.generate_token(&room_name, "daemon-owner-b")?;

    spawn_placeholder_daemon_room(ws_url.clone(), tok_a);
    spawn_placeholder_daemon_room(ws_url.clone(), tok_b);
    tokio::time::sleep(Duration::from_millis(400)).await;

    let tok_obs = kit.generate_token(&room_name, "observer-dual")?;
    let (obs, mut ev) = Room::connect(&ws_url, &tok_obs, RoomOptions::default())
        .await
        .map_err(|e| anyhow::anyhow!("observer connect: {e}"))?;

    wait_for_remote_count(&obs, &mut ev, 2).await?;

    let deadline = Instant::now() + STABILIZE;
    loop {
        let n = count_remote_project_data_owners(&obs);
        if n == 1 {
            break;
        }
        if Instant::now() > deadline {
            anyhow::bail!("metadata_owner_count expected 1, got {n}");
        }
        let _ = tokio::time::timeout(POLL, ev.recv()).await;
    }

    let temp = tempfile::tempdir()?;
    let dir_a = temp.path().join("writer_a");
    let dir_b = temp.path().join("writer_b");
    std::fs::create_dir_all(&dir_a)?;
    std::fs::create_dir_all(&dir_b)?;

    let pa = ProjectData {
        project_id: "proj-dual-a".to_string(),
        name: "a".to_string(),
        git_url: "https://github.com/org/a.git".to_string(),
        main_repo_path: "/tmp/a".to_string(),
        main_branch_ref: None,
        host_repo_paths: HashMap::new(),
    };
    let pb = ProjectData {
        project_id: "proj-dual-b".to_string(),
        name: "b".to_string(),
        git_url: "https://github.com/org/b.git".to_string(),
        main_repo_path: "/tmp/b".to_string(),
        main_branch_ref: None,
        host_repo_paths: HashMap::new(),
    };
    add_project(&dir_a, pa)?;
    add_project(&dir_b, pb)?;
    converge_replica_project_registry_with_elected_owner(&dir_a, &dir_b)?;

    let mut sa: Vec<_> = read_projects(&dir_a)?
        .into_iter()
        .map(|p| p.project_id)
        .collect();
    let mut sb: Vec<_> = read_projects(&dir_b)?
        .into_iter()
        .map(|p| p.project_id)
        .collect();
    sa.sort();
    sb.sort();
    assert_eq!(
        sa, sb,
        "deterministic election must yield a single authoritative registry view on disk (harness dirs)"
    );

    drop(obs);
    drop(kit);
    Ok(())
}
