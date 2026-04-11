//! Acceptance: aggregated `ListProjects` merges registry rows from eligible daemons so the same
//! `project_id` can appear twice with distinct `daemon_instance_id` tags (PRD multi-daemon).

use std::path::PathBuf;
use std::sync::Arc;

use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::multi_host::{DaemonInstanceId, EligibleDaemonInfo, EligibleDaemonSource};
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ListProjectsRequest,
    ProjectEntry as ProtoProjectEntry,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

fn test_config_for_os_user(os_user: &str) -> DaemonConfig {
    let yaml = format!(
        r#"
users:
  - github_user: "testuser"
    os_user: "{os_user}"
"#
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).unwrap();
    DaemonConfig::load(&path).unwrap()
}

fn test_service(
    sessions_base: PathBuf,
    os_user: &str,
    eligible: Arc<dyn EligibleDaemonSource>,
) -> ConnectionServiceImpl {
    let config = test_config_for_os_user(os_user);
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver = Arc::new(|token| {
        if token == "valid-token" {
            Some("testuser".to_string())
        } else {
            None
        }
    });
    ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        user_resolver,
        None,
        Some(eligible),
        None,
    )
}

/// Drives `peer_project_entries` so merge produces two rows for the same `project_id`.
struct TestPeerProjectsSource;

impl EligibleDaemonSource for TestPeerProjectsSource {
    fn list_eligible_daemons(&self) -> Vec<EligibleDaemonInfo> {
        vec![
            EligibleDaemonInfo {
                instance_id: DaemonInstanceId("workstation-1".to_string()),
                label: "workstation-1".to_string(),
            },
            EligibleDaemonInfo {
                instance_id: DaemonInstanceId("server-2".to_string()),
                label: "server-2".to_string(),
            },
        ]
    }

    fn peer_project_entries(&self, session_token: &str) -> Vec<ProtoProjectEntry> {
        if session_token != "valid-token" {
            return vec![];
        }
        let marker = "bbbbbbbb-cccc-4ddd-8eee-ffffffffffff";
        vec![
            ProtoProjectEntry {
                project_id: marker.to_string(),
                name: "peer-a".to_string(),
                git_url: "https://example.com/a.git".to_string(),
                main_repo_path: "/peer/a".to_string(),
                daemon_instance_id: "workstation-1".to_string(),
            },
            ProtoProjectEntry {
                project_id: marker.to_string(),
                name: "peer-b".to_string(),
                git_url: "https://example.com/b.git".to_string(),
                main_repo_path: "/peer/b".to_string(),
                daemon_instance_id: "server-2".to_string(),
            },
        ]
    }
}

#[tokio::test]
async fn list_projects_merges_entries_tagged_with_daemon_instance_id() {
    let os_user = std::env::var("USER").expect("USER must be set for passwd-backed projects path");
    let service = test_service(
        tempfile::tempdir().unwrap().path().to_path_buf(),
        &os_user,
        Arc::new(TestPeerProjectsSource),
    );

    let marker = "bbbbbbbb-cccc-4ddd-8eee-ffffffffffff";
    let response = service
        .list_projects(Request::new(ListProjectsRequest {
            session_token: "valid-token".to_string(),
        }))
        .await
        .expect("list_projects succeeds");

    let rows: Vec<_> = response
        .into_inner()
        .projects
        .into_iter()
        .filter(|p| p.project_id == marker)
        .collect();

    assert_eq!(
        rows.len(),
        2,
        "aggregated ListProjects must return one row per (project_id, hosting daemon); \
         expected two rows for the same project_id from two eligible daemons (PRD)"
    );
    assert_ne!(
        rows[0].daemon_instance_id, rows[1].daemon_instance_id,
        "merged duplicate project_id rows must keep distinct daemon_instance_id tags"
    );
}
