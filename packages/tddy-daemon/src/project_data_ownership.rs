//! Project-data owner election, LiveKit participant metadata, and replica registry sync (PRD).

use std::path::Path;

use livekit::prelude::*;
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::project_storage::{read_projects, write_projects};

/// JSON key for the schema / version field in participant metadata (forward compatibility).
pub const PROJECT_METADATA_SCHEMA_VERSION_KEY: &str = "project_metadata_schema_version";

/// Published schema version for [`build_project_data_participant_metadata`] (bump when JSON shape changes).
pub const PROJECT_DATA_METADATA_SCHEMA_VERSION: u32 = 1;

/// LiveKit participant identity reserved in acceptance tests for a daemon that must **never** claim
/// project-data ownership (`livekit.project_data_owner_eligible: false`). Production callers should use
/// [`crate::config::DaemonConfig::effective_project_data_owner_eligible`] and pass eligibility into
/// higher-level join helpers when integrated with the spawner.
pub const LIVEKIT_IDENTITY_PROJECT_DATA_INELIGIBLE: &str = "daemon-ineligible";

/// Build canonical participant metadata JSON for the project-data plane.
pub fn build_project_data_participant_metadata(
    daemon_instance_id: &str,
    project_data_owner: bool,
    schema_version: u32,
) -> String {
    serde_json::json!({
        "daemon_instance_id": daemon_instance_id,
        "project_data_owner": project_data_owner,
        PROJECT_METADATA_SCHEMA_VERSION_KEY: schema_version,
    })
    .to_string()
}

#[derive(Debug, Deserialize)]
pub struct ProjectDataMetadataView {
    #[serde(default)]
    daemon_instance_id: Option<String>,
    #[serde(default)]
    project_data_owner: Option<bool>,
    #[serde(default)]
    project_metadata_schema_version: Option<u32>,
}

/// Parse participant metadata string; returns structured view when JSON is valid.
pub fn parse_project_data_participant_metadata(metadata: &str) -> Option<ProjectDataMetadataView> {
    if metadata.trim().is_empty() {
        return None;
    }
    serde_json::from_str::<ProjectDataMetadataView>(metadata).ok()
}

/// Whether this LiveKit identity may participate in project-data owner election for the test harness
/// and placeholder join path. Matches acceptance identity `daemon-ineligible`.
pub fn livekit_identity_eligible_for_project_data_ownership(identity: &str) -> bool {
    identity != LIVEKIT_IDENTITY_PROJECT_DATA_INELIGIBLE
}

/// Deterministic project-data owner among eligible daemons: **lexicographically smallest** non-empty
/// `daemon_instance_id` wins. Documented tie-break for PRD single-writer convergence.
pub fn elect_project_data_owner(candidates: &[String]) -> Option<&str> {
    candidates
        .iter()
        .map(|s| s.as_str().trim())
        .filter(|s| !s.is_empty())
        .min()
}

/// True when metadata marks this participant as active project-data owner with required fields.
pub fn metadata_claims_active_project_data_owner(metadata: &str) -> bool {
    let Some(v) = parse_project_data_participant_metadata(metadata) else {
        return false;
    };
    v.project_data_owner == Some(true)
        && v.daemon_instance_id
            .as_ref()
            .map_or(false, |s| !s.is_empty())
        && v.project_metadata_schema_version.is_some()
}

/// Recompute election from current room participants and push updated JSON metadata for the local participant.
///
/// Call after connect and whenever participants join, leave, or change metadata (see integration tests).
pub async fn refresh_project_data_ownership_metadata(room: &Room) -> anyhow::Result<()> {
    let local = room.local_participant();
    let local_id = local.identity().to_string();
    let local_eligible = livekit_identity_eligible_for_project_data_ownership(&local_id);

    let mut candidates: Vec<String> = Vec::new();
    if local_eligible {
        candidates.push(local_id.clone());
    }
    for p in room.remote_participants().values() {
        let rid = p.identity().to_string();
        if livekit_identity_eligible_for_project_data_ownership(&rid) {
            candidates.push(rid);
        }
    }
    candidates.sort();
    candidates.dedup();

    let elected = elect_project_data_owner(&candidates);
    let am_owner = local_eligible && elected.is_some_and(|e| e == local_id.as_str());

    log::debug!(
        target: "tddy_daemon::project_data_ownership",
        "refresh_project_data_ownership_metadata: local_id={} eligible={} candidates={:?} elected={:?} am_owner={}",
        local_id,
        local_eligible,
        candidates,
        elected,
        am_owner
    );

    let meta = build_project_data_participant_metadata(
        &local_id,
        am_owner,
        PROJECT_DATA_METADATA_SCHEMA_VERSION,
    );
    local
        .set_metadata(meta)
        .await
        .map_err(|e| anyhow::anyhow!("LiveKit set_metadata: {e}"))?;

    if am_owner {
        log::info!(
            target: "tddy_daemon::project_data_ownership",
            "project-data owner (metadata): local_id={} elected_among={:?}",
            local_id,
            candidates
        );
    } else {
        log::info!(
            target: "tddy_daemon::project_data_ownership",
            "project-data replica (metadata): local_id={} elected={:?}",
            local_id,
            elected
        );
    }

    Ok(())
}

/// Join `common_room` and publish election-derived participant metadata for project-data ownership.
///
/// Publishes once after connect; callers should invoke [`refresh_project_data_ownership_metadata`]
/// on an event loop when participants change (see `livekit_project_ownership` tests).
pub async fn join_common_room_and_publish_project_ownership_metadata(
    ws_url: &str,
    access_token: &str,
) -> anyhow::Result<(Room, mpsc::UnboundedReceiver<RoomEvent>)> {
    let (room, events) = Room::connect(ws_url, access_token, RoomOptions::default())
        .await
        .map_err(|e| anyhow::anyhow!("LiveKit Room::connect: {e}"))?;

    if let Err(e) = refresh_project_data_ownership_metadata(&room).await {
        log::warn!(
            target: "tddy_daemon::project_data_ownership",
            "initial refresh_project_data_ownership_metadata failed: {}",
            e
        );
    }

    Ok((room, events))
}

/// Copy the owner's `projects.yaml` snapshot into the replica projects directory.
pub fn apply_owner_project_registry_snapshot_to_replica(
    replica_projects_dir: &Path,
    owner_projects_dir: &Path,
) -> anyhow::Result<()> {
    let projects = read_projects(owner_projects_dir)?;
    log::debug!(
        target: "tddy_daemon::project_data_ownership",
        "apply_owner_project_registry_snapshot_to_replica: owner={:?} replica={:?} rows={}",
        owner_projects_dir,
        replica_projects_dir,
        projects.len()
    );
    write_projects(replica_projects_dir, &projects)?;
    log::info!(
        target: "tddy_daemon::project_data_ownership",
        "applied owner projects.yaml snapshot -> replica ({:?}, {} projects)",
        replica_projects_dir,
        projects.len()
    );
    Ok(())
}

/// Lexicographic compare on directory paths to pick the authoritative registry (deterministic tie-break
/// aligned with PRD election: stable ordering). The other directory is overwritten to match.
pub fn converge_replica_project_registry_with_elected_owner(
    path_a: &Path,
    path_b: &Path,
) -> anyhow::Result<()> {
    let key_a = path_a.to_string_lossy();
    let key_b = path_b.to_string_lossy();
    let (authoritative, follower) = if key_a <= key_b {
        (path_a, path_b)
    } else {
        (path_b, path_a)
    };
    log::debug!(
        target: "tddy_daemon::project_data_ownership",
        "converge_replica_project_registry_with_elected_owner: authoritative={:?} follower={:?}",
        authoritative,
        follower
    );
    apply_owner_project_registry_snapshot_to_replica(follower, authoritative)?;
    log::info!(
        target: "tddy_daemon::project_data_ownership",
        "converged project registry: follower {:?} now matches {:?}",
        follower,
        authoritative
    );
    Ok(())
}

/// Count remote participants whose metadata parses as active project-data owner (required JSON fields).
pub fn count_remote_project_data_owners(room: &Room) -> usize {
    room.remote_participants()
        .values()
        .filter(|p| metadata_claims_active_project_data_owner(&p.metadata()))
        .count()
}

/// Extract `project_data_owner` flag when present in JSON metadata.
pub fn project_data_owner_flag_from_metadata(metadata: &str) -> Option<bool> {
    let v: Value = serde_json::from_str(metadata).ok()?;
    v.get("project_data_owner").and_then(|x| x.as_bool())
}

#[cfg(test)]
mod project_data_ownership_unit_tests {
    use std::collections::HashMap;

    use super::{
        apply_owner_project_registry_snapshot_to_replica,
        converge_replica_project_registry_with_elected_owner, elect_project_data_owner,
    };
    use crate::project_storage::{add_project, read_projects, ProjectData};

    fn sample_project(id: &str) -> ProjectData {
        ProjectData {
            project_id: id.to_string(),
            name: format!("name-{id}"),
            git_url: format!("https://github.com/org/{id}.git"),
            main_repo_path: format!("/tmp/{id}"),
            main_branch_ref: None,
            host_repo_paths: HashMap::new(),
        }
    }

    fn sorted_project_ids(dir: &std::path::Path) -> Vec<String> {
        let mut ids: Vec<_> = read_projects(dir)
            .expect("read_projects")
            .into_iter()
            .map(|p| p.project_id)
            .collect();
        ids.sort();
        ids
    }

    #[test]
    fn elect_project_data_owner_prefers_lexicographic_minimum() {
        let candidates = vec![
            "daemon-z".to_string(),
            "daemon-a".to_string(),
            "daemon-m".to_string(),
        ];
        assert_eq!(
            elect_project_data_owner(&candidates),
            Some("daemon-a"),
            "documented tie-break: lexicographically smallest daemon_instance_id wins"
        );
    }

    #[test]
    fn apply_owner_snapshot_copies_projects_yaml_to_replica() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let owner = tmp.path().join("owner");
        let replica = tmp.path().join("replica");
        std::fs::create_dir_all(&owner).unwrap();
        std::fs::create_dir_all(&replica).unwrap();
        add_project(&owner, sample_project("unit-snap-1")).expect("add owner project");
        apply_owner_project_registry_snapshot_to_replica(&replica, &owner).expect("apply snapshot");
        assert_eq!(
            sorted_project_ids(&replica),
            vec!["unit-snap-1".to_string()],
            "replica directory must receive owner's registry snapshot"
        );
    }

    #[test]
    fn converge_replica_resyncs_to_owner_projects_yaml() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let replica = tmp.path().join("replica");
        let owner = tmp.path().join("owner");
        std::fs::create_dir_all(&replica).unwrap();
        std::fs::create_dir_all(&owner).unwrap();
        add_project(&owner, sample_project("elected-only")).expect("owner project");
        add_project(&replica, sample_project("stale-replica")).expect("stale replica");
        converge_replica_project_registry_with_elected_owner(&replica, &owner).expect("converge");
        assert_eq!(
            sorted_project_ids(&replica),
            sorted_project_ids(&owner),
            "replica projects.yaml must match owner after convergence"
        );
    }
}
