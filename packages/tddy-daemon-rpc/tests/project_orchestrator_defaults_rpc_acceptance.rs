//! Acceptance: `SetProjectOrchestratorDefaults` saves what a new orchestrator for a project starts
//! with on one host — model, effort, provider account — and `ListProjects` carries them on that
//! host's `ProjectEntry`, which is where the "+" composer reads them (`#orchestrators` 11/11).
//!
//! Defaults are **per host**, as main is. A request naming another host is routed there and never
//! written here. No LiveKit and no peer: the daemon is put in a common room with a host it can
//! name and **no room connected**, so a request that reaches the forwarding path is refused there
//! with the one message only that path produces — what is under test is which host answers.
//!
//! The second half pins the handler's own contract: clearing, replacing, refusals.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-11-10-project-settings-pane.md
//! Changeset: docs/dev/1-WIP/2026-11-10-project-settings-pane.md

use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_daemon_kernel::{SessionUserResolver, SessionsBaseResolver};
use tddy_daemon_livekit::livekit_peer_discovery::LiveKitDiscoveryHandles;
use tddy_daemon_rpc::test_util::TestDaemon;
use tddy_host_service::multi_host::{DaemonInstanceId, EligibleDaemonInfo, EligibleDaemonSource};
use tddy_projects::project_storage;
use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::project::{
    ListProjectsRequest, OrchestratorDefaults, ProjectEntry, ProjectService,
    SetProjectOrchestratorDefaultsRequest,
};
use tddy_session_lifecycle::cli_session_manager::CliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;

/// The daemon under test.
const THIS_HOST: &str = "laptop-a";
/// A host in the same common room, holding the same logical project.
const OTHER_HOST: &str = "workstation-b";

const PROJECT: &str = "11111111-2222-4333-8444-555555555555";
const UNKNOWN_PROJECT: &str = "00000000-0000-4000-8000-000000000000";

const VALID_TOKEN: &str = "valid-token";

/// What a forwarded unary call reports when this daemon has no common-room connection — the one
/// observable only the forwarding path produces.
const WENT_TO_THE_PEER: &str = "LiveKit common room is not connected on this daemon";

// ---------------------------------------------------------------------------
// World
// ---------------------------------------------------------------------------

/// The common room this daemon sees: itself and `OTHER_HOST`.
struct ACommonRoomWithAnotherHost;

#[async_trait::async_trait]
impl EligibleDaemonSource for ACommonRoomWithAnotherHost {
    fn list_eligible_daemons(&self) -> Vec<EligibleDaemonInfo> {
        [THIS_HOST, OTHER_HOST]
            .into_iter()
            .map(|id| EligibleDaemonInfo {
                instance_id: DaemonInstanceId(id.to_string()),
                label: format!("{id} (common room)"),
            })
            .collect()
    }
}

/// `THIS_HOST`, holding `PROJECT` in its registry, in a common room with no room connected.
struct ADaemonHoldingTheProject {
    service: TestDaemon,
    data_dir: tempfile::TempDir,
}

impl ADaemonHoldingTheProject {
    fn projects_dir(&self) -> std::path::PathBuf {
        self.data_dir.path().join("projects")
    }

    /// Ask this daemon to save `defaults` for `project_id` on `host` (empty: this one).
    async fn set_defaults(
        &self,
        project_id: &str,
        host: &str,
        defaults: Option<OrchestratorDefaults>,
    ) -> Result<ProjectEntry, Status> {
        self.set_defaults_as(VALID_TOKEN, project_id, host, defaults)
            .await
    }

    async fn set_defaults_as(
        &self,
        session_token: &str,
        project_id: &str,
        host: &str,
        defaults: Option<OrchestratorDefaults>,
    ) -> Result<ProjectEntry, Status> {
        self.service
            .set_project_orchestrator_defaults(Request::direct(
                SetProjectOrchestratorDefaultsRequest {
                    session_token: session_token.to_string(),
                    project_id: project_id.to_string(),
                    daemon_instance_id: host.to_string(),
                    defaults,
                },
            ))
            .await
            .map(|r| {
                r.into_inner()
                    .project
                    .expect("the response carries the project")
            })
    }

    /// This host's own `ListProjects` row for `PROJECT`.
    async fn the_listed_project(&self) -> ProjectEntry {
        self.service
            .list_projects(Request::direct(ListProjectsRequest {
                session_token: VALID_TOKEN.to_string(),
                local_only: true,
                ..ListProjectsRequest::default()
            }))
            .await
            .expect("list_projects succeeds")
            .into_inner()
            .projects
            .into_iter()
            .find(|p| p.project_id == PROJECT)
            .expect("the project is listed")
    }

    /// The defaults this host's registry file holds for `PROJECT`.
    fn stored_defaults(&self) -> Option<project_storage::OrchestratorDefaults> {
        project_storage::find_project(&self.projects_dir(), PROJECT)
            .expect("read registry")
            .expect("the project is registered")
            .orchestrator_defaults
    }
}

/// The OS user the test process runs as — the config must map a real user.
fn current_os_user() -> String {
    let pw = unsafe { libc::getpwuid(libc::getuid()) };
    assert!(!pw.is_null(), "current uid must resolve to a passwd entry");
    unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) }
        .to_string_lossy()
        .into_owned()
}

fn a_daemon_holding_the_project() -> ADaemonHoldingTheProject {
    let data_dir = tempfile::tempdir().expect("data dir");
    project_storage::write_projects(
        &data_dir.path().join("projects"),
        &[project_storage::ProjectData {
            project_id: PROJECT.to_string(),
            name: "alpha".to_string(),
            git_url: "https://example.com/alpha.git".to_string(),
            main_repo_path: "/home/dev/repos/alpha".to_string(),
            main_branch_ref: Some("origin/main".to_string()),
            remote_name: Some("origin".to_string()),
            host_repo_paths: Default::default(),
            accounts: Vec::new(),
            local_gh: false,
            home_session_id: None,
            orchestrator_defaults: None,
            lifecycle: Default::default(),
        }],
    )
    .expect("seed the registry");

    let user = current_os_user();
    let config_path = data_dir.path().join("daemon.yaml");
    std::fs::write(
        &config_path,
        format!("daemon_instance_id: \"{THIS_HOST}\"\n"),
    )
    .expect("write daemon config");
    std::fs::write(
        data_dir.path().join("users.yaml"),
        format!("- github_user: \"{user}\"\n  os_user: \"{user}\"\n"),
    )
    .expect("write users");
    let config =
        tddy_daemon_kernel::config::DaemonConfig::load(&config_path).expect("load daemon config");

    let base = data_dir.path().to_path_buf();
    let sessions_base_resolver: SessionsBaseResolver = Arc::new(move |_| Some(base.clone()));
    let user_resolver: SessionUserResolver =
        Arc::new(move |token| (token == VALID_TOKEN).then(|| user.clone()));
    let service = TestDaemon::from_host(DaemonSessionHost::new(
        config.clone(),
        sessions_base_resolver,
        data_dir.path().to_path_buf(),
        user_resolver,
        None,
        tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        tddy_daemon_livekit::peer_routing::peer_routing_over(
            &config,
            Some(LiveKitDiscoveryHandles {
                eligible_daemon_source: Arc::new(ACommonRoomWithAnotherHost),
                common_room_livekit_room: Arc::new(tokio::sync::RwLock::new(None)),
            }),
        ),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        Arc::new(CliSessionManager::new()),
        tddy_session_lifecycle::test_util::builtin_coding_agents(),
    ));

    ADaemonHoldingTheProject { service, data_dir }
}

fn the_defaults(model: &str, effort: &str, provider_account_id: &str) -> OrchestratorDefaults {
    OrchestratorDefaults {
        model: model.to_string(),
        effort: effort.to_string(),
        provider_account_id: provider_account_id.to_string(),
    }
}

fn stored(model: &str, effort: &str, account: &str) -> project_storage::OrchestratorDefaults {
    let some = |v: &str| (!v.is_empty()).then(|| v.to_string());
    project_storage::OrchestratorDefaults {
        model: some(model),
        effort: some(effort),
        provider_account_id: some(account),
    }
}

// ---------------------------------------------------------------------------
// Acceptance
// ---------------------------------------------------------------------------

#[tokio::test]
async fn set_project_orchestrator_defaults_is_listed_on_the_project_entry() {
    // Given
    let daemon = a_daemon_holding_the_project();

    // When the defaults are saved on this host
    let answered = daemon
        .set_defaults(
            PROJECT,
            "",
            Some(the_defaults("sonnet", "high", "acct-meta")),
        )
        .await
        .expect("saving the defaults succeeds");

    // Then the answer, the listing and the registry file agree
    let expected = Some(the_defaults("sonnet", "high", "acct-meta"));
    assert_eq!(answered.orchestrator_defaults, expected);
    assert_eq!(answered.daemon_instance_id, THIS_HOST);
    assert_eq!(
        daemon.the_listed_project().await.orchestrator_defaults,
        expected
    );
    assert_eq!(
        daemon.stored_defaults(),
        Some(stored("sonnet", "high", "acct-meta"))
    );
}

#[tokio::test]
async fn defaults_set_for_another_host_are_forwarded_there_not_written_here() {
    // Given
    let daemon = a_daemon_holding_the_project();

    // When the request names the other host
    let outcome = daemon
        .set_defaults(PROJECT, OTHER_HOST, Some(the_defaults("opus", "low", "")))
        .await;

    // Then it went to that host — the forwarding path's own refusal, with no room connected —
    // and this host's row is untouched
    let status = outcome.expect_err("no room is connected, so the forward cannot complete");
    assert_eq!(status.code(), Code::FailedPrecondition, "{status:?}");
    assert!(
        status.message().contains(WENT_TO_THE_PEER),
        "refused by the forwarding path: {status:?}"
    );
    assert_eq!(daemon.stored_defaults(), None);
    assert_eq!(
        daemon.the_listed_project().await.orchestrator_defaults,
        None
    );
}

// ---------------------------------------------------------------------------
// Handler contract
// ---------------------------------------------------------------------------

#[tokio::test]
async fn naming_this_host_saves_here() {
    // Given
    let daemon = a_daemon_holding_the_project();

    // When
    daemon
        .set_defaults(PROJECT, THIS_HOST, Some(the_defaults("opus", "", "")))
        .await
        .expect("saving the defaults succeeds");

    // Then
    assert_eq!(daemon.stored_defaults(), Some(stored("opus", "", "")));
}

#[tokio::test]
async fn absent_defaults_clear_the_saved_ones() {
    // Given defaults saved on this host
    let daemon = a_daemon_holding_the_project();
    daemon
        .set_defaults(PROJECT, "", Some(the_defaults("opus", "high", "acct-meta")))
        .await
        .expect("saving the defaults succeeds");

    // When
    let answered = daemon
        .set_defaults(PROJECT, "", None)
        .await
        .expect("clearing the defaults succeeds");

    // Then
    assert_eq!(answered.orchestrator_defaults, None);
    assert_eq!(
        daemon.the_listed_project().await.orchestrator_defaults,
        None
    );
    assert_eq!(daemon.stored_defaults(), None);
}

#[tokio::test]
async fn defaults_with_every_control_empty_are_listed_as_none() {
    // Given
    let daemon = a_daemon_holding_the_project();

    // When
    daemon
        .set_defaults(PROJECT, "", Some(the_defaults("", "", "")))
        .await
        .expect("saving succeeds");

    // Then — nothing was chosen, so there is nothing to pre-fill
    assert_eq!(
        daemon.the_listed_project().await.orchestrator_defaults,
        None
    );
}

#[tokio::test]
async fn a_project_this_host_does_not_hold_is_not_found() {
    // Given
    let daemon = a_daemon_holding_the_project();

    // When
    let outcome = daemon
        .set_defaults(UNKNOWN_PROJECT, "", Some(the_defaults("opus", "", "")))
        .await;

    // Then
    let status = outcome.expect_err("an unknown project is refused");
    assert_eq!(status.code(), Code::NotFound, "{status:?}");
}

#[tokio::test]
async fn an_empty_project_id_is_an_invalid_argument() {
    // Given
    let daemon = a_daemon_holding_the_project();

    // When
    let outcome = daemon
        .set_defaults("  ", "", Some(the_defaults("opus", "", "")))
        .await;

    // Then
    let status = outcome.expect_err("an empty project id is refused");
    assert_eq!(status.code(), Code::InvalidArgument, "{status:?}");
}

#[tokio::test]
async fn an_invalid_session_is_unauthenticated_and_writes_nothing() {
    // Given
    let daemon = a_daemon_holding_the_project();

    // When
    let outcome = daemon
        .set_defaults_as(
            "not-a-token",
            PROJECT,
            "",
            Some(the_defaults("opus", "", "")),
        )
        .await;

    // Then
    let status = outcome.expect_err("an invalid session is refused");
    assert_eq!(status.code(), Code::Unauthenticated, "{status:?}");
    assert_eq!(daemon.stored_defaults(), None);
}
