//! Acceptance: a project is renamed, soft-deleted and restored on **every host owning it**, and a
//! soft-deleted project keeps all of its data, hides from listings, refuses new work, and stays
//! deleted when a host that missed the delete comes back.
//!
//! Two daemons, `host-a` and `host-b`, run in this process and reach each other through an
//! in-memory [`PeerForwarder`] that hands each forwarded call to the other daemon's own service,
//! stamped as arriving over LiveKit. A third eligible host, `host-c`, has no daemon behind it: it
//! never answers. A host "goes offline" by being left out of the transport.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Renaming, deleting and restoring a project.
//! Changeset: docs/dev/1-WIP/2026-11-15-project-update-and-soft-delete.md

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use prost::Message;
use tddy_daemon_extensions::PeerForwarder;
use tddy_daemon_kernel::peer_forwarding::CommonRoom;
use tddy_daemon_rpc::test_util::TestDaemon;
use tddy_host_service::multi_host::{DaemonInstanceId, EligibleDaemonInfo, EligibleDaemonSource};
use tddy_projects::project_storage::{self, ProjectData, ProjectLifecycle};
use tddy_rpc::{Code, Request, RequestMetadata, RequestTransport, Status};
use tddy_service::proto::project::{
    AddProjectToHostRequest, CreateProjectRequest, DeleteProjectRequest, DeleteProjectResponse,
    ListProjectsRequest, ProjectEntry, ProjectService as ProjectServiceTrait,
    RestoreProjectRequest, RestoreProjectResponse, UpdateProjectRequest, UpdateProjectResponse,
};
use tddy_service::proto::session::{
    EnsureProjectHomeSessionRequest, ResumeSessionRequest, SessionService as SessionServiceTrait,
    StartSessionRequest,
};
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::peer_routing::PeerRouting;
use tddy_session_lifecycle::test_util::{test_config, TEST_TOKEN, TEST_USER};
use tddy_session_lifecycle::CliSessionManager;
use tokio::sync::mpsc;

const THE_PROJECT: &str = "019d105b-ac0f-78d3-9a89-409731145a3b";
const UNKNOWN_PROJECT: &str = "00000000-0000-4000-8000-000000000000";
const THE_GIT_URL: &str = "https://github.com/acme/the-project.git";
const HOST_A: &str = "host-a";
const HOST_B: &str = "host-b";
const SILENT_HOST: &str = "host-c";
const PROJECT_SERVICE: &str = "project.ProjectService";

// ── the hosts ────────────────────────────────────────────────────────────────────────────────

/// The transport between the in-process daemons. A host with no daemon registered here is
/// unreachable — `host-c` always, `host-a` or `host-b` while a test keeps it offline.
#[derive(Default)]
struct InProcessPeers {
    daemons: RwLock<HashMap<String, Arc<TestDaemon>>>,
}

impl InProcessPeers {
    fn register(&self, instance_id: &str, daemon: Arc<TestDaemon>) {
        self.daemons
            .write()
            .unwrap()
            .insert(instance_id.to_string(), daemon);
    }

    fn take_offline(&self, instance_id: &str) {
        self.daemons.write().unwrap().remove(instance_id);
    }

    fn daemon(&self, peer_id: &str) -> Result<Arc<TestDaemon>, Status> {
        self.daemons
            .read()
            .unwrap()
            .get(peer_id)
            .cloned()
            .ok_or_else(|| Status::unavailable(format!("daemon {peer_id} does not answer")))
    }
}

fn arriving_from_a_peer<T>(inner: T) -> Request<T> {
    Request::with_metadata(inner, RequestMetadata::over(RequestTransport::LiveKit))
}

#[async_trait::async_trait]
impl PeerForwarder for InProcessPeers {
    async fn is_connected(&self) -> bool {
        true
    }

    async fn call_unary(
        &self,
        peer_id: &str,
        service: &str,
        method: &str,
        body: Vec<u8>,
    ) -> Result<Vec<u8>, Status> {
        let daemon = self.daemon(peer_id)?;
        match (service, method) {
            (PROJECT_SERVICE, "ListProjects") => daemon
                .list_projects(arriving_from_a_peer(
                    ListProjectsRequest::decode(body.as_slice()).unwrap(),
                ))
                .await
                .map(|r| r.into_inner().encode_to_vec()),
            (PROJECT_SERVICE, "UpdateProject") => daemon
                .update_project(arriving_from_a_peer(
                    UpdateProjectRequest::decode(body.as_slice()).unwrap(),
                ))
                .await
                .map(|r| r.into_inner().encode_to_vec()),
            (PROJECT_SERVICE, "DeleteProject") => daemon
                .delete_project(arriving_from_a_peer(
                    DeleteProjectRequest::decode(body.as_slice()).unwrap(),
                ))
                .await
                .map(|r| r.into_inner().encode_to_vec()),
            (PROJECT_SERVICE, "RestoreProject") => daemon
                .restore_project(arriving_from_a_peer(
                    RestoreProjectRequest::decode(body.as_slice()).unwrap(),
                ))
                .await
                .map(|r| r.into_inner().encode_to_vec()),
            other => Err(Status::unimplemented(format!(
                "the in-process peers do not serve {other:?}"
            ))),
        }
    }

    async fn call_server_stream(
        &self,
        _peer_id: &str,
        service: &str,
        method: &str,
        _body: Vec<u8>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        Err(Status::unimplemented(format!(
            "the in-process peers serve no stream ({service}/{method})"
        )))
    }
}

/// Every eligible host, as each daemon's roster lists them, and the peer listing a daemon merges
/// into its own — gathered the way the LiveKit discovery gathers it: each reachable peer's own
/// rows, tombstones included.
struct TheCommonRoom {
    me: &'static str,
    peers: Arc<InProcessPeers>,
}

#[async_trait::async_trait]
impl EligibleDaemonSource for TheCommonRoom {
    fn list_eligible_daemons(&self) -> Vec<EligibleDaemonInfo> {
        [HOST_A, HOST_B, SILENT_HOST]
            .into_iter()
            .map(|id| EligibleDaemonInfo {
                instance_id: DaemonInstanceId(id.to_string()),
                label: id.to_string(),
            })
            .collect()
    }

    async fn peer_project_entries(&self, session_token: &str) -> Vec<ProjectEntry> {
        let mut rows = Vec::new();
        for peer in [HOST_A, HOST_B, SILENT_HOST] {
            if peer == self.me {
                continue;
            }
            let Ok(daemon) = self.peers.daemon(peer) else {
                continue;
            };
            let listed = daemon
                .list_projects(arriving_from_a_peer(ListProjectsRequest {
                    session_token: session_token.to_string(),
                    local_only: true,
                    include_deleted: true,
                }))
                .await
                .expect("a reachable peer lists its projects");
            rows.extend(listed.into_inner().projects);
        }
        rows
    }
}

fn a_daemon_named(
    instance_id: &'static str,
    data_dir: &Path,
    peers: Arc<InProcessPeers>,
) -> TestDaemon {
    let mut config = test_config();
    config.daemon_instance_id = Some(instance_id.to_string());
    let base = data_dir.to_path_buf();
    let routing = PeerRouting::new(
        config.clone(),
        Arc::new(TheCommonRoom {
            me: instance_id,
            peers: Arc::clone(&peers),
        }),
        Some(CommonRoom::new(peers, &config)),
    );
    let host = DaemonSessionHost::new(
        config.clone(),
        Arc::new(move |_| Some(base.clone())),
        data_dir.to_path_buf(),
        Arc::new(|token| (token == TEST_TOKEN).then(|| TEST_USER.to_string())),
        None,
        tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        routing,
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        Arc::new(CliSessionManager::new()),
        tddy_session_lifecycle::test_util::builtin_coding_agents(),
    );
    TestDaemon::from_host(host)
}

/// One daemon's half of the world: its data directory (registry, sessions, stacks) and the main
/// checkout its project row points at.
struct Host {
    id: &'static str,
    daemon: Arc<TestDaemon>,
    data: tempfile::TempDir,
    checkout: tempfile::TempDir,
}

fn a_project_row(checkout: &Path) -> ProjectData {
    ProjectData {
        project_id: THE_PROJECT.to_string(),
        name: "the-project".to_string(),
        git_url: THE_GIT_URL.to_string(),
        main_repo_path: checkout.display().to_string(),
        main_branch_ref: None,
        remote_name: None,
        host_repo_paths: Default::default(),
        accounts: Vec::new(),
        local_gh: false,
        home_session_id: None,
        orchestrator_defaults: None,
        lifecycle: ProjectLifecycle::default(),
    }
}

/// A host whose registry holds the project, with a file in its main checkout and a stack record —
/// the data a delete must keep.
fn a_host_owning_the_project(id: &'static str, peers: &Arc<InProcessPeers>) -> Host {
    let host = a_host_without_the_project(id, peers);
    std::fs::write(host.checkout.path().join("README.md"), "kept").unwrap();
    project_storage::write_projects(&host.projects_dir(), &[a_project_row(host.checkout.path())])
        .unwrap();
    let stacks = host.projects_dir().join(THE_PROJECT).join("stacks");
    std::fs::create_dir_all(&stacks).unwrap();
    std::fs::write(stacks.join("stack-1.yaml"), "stack_id: stack-1\n").unwrap();
    host
}

fn a_host_without_the_project(id: &'static str, peers: &Arc<InProcessPeers>) -> Host {
    let data = tempfile::tempdir().unwrap();
    let checkout = tempfile::tempdir().unwrap();
    let daemon = Arc::new(a_daemon_named(id, data.path(), Arc::clone(peers)));
    peers.register(id, Arc::clone(&daemon));
    Host {
        id,
        daemon,
        data,
        checkout,
    }
}

impl Host {
    fn projects_dir(&self) -> PathBuf {
        self.data.path().join("projects")
    }

    fn stored_row(&self) -> ProjectData {
        project_storage::find_project(&self.projects_dir(), THE_PROJECT)
            .expect("read registry")
            .unwrap_or_else(|| panic!("{} holds a row for the project", self.id))
    }

    fn session_dir(&self, session_id: &str) -> PathBuf {
        tddy_core::session_lifecycle::unified_session_dir_path(self.data.path(), session_id)
    }

    /// A session of the project on this host, recorded with `pid` and `role`.
    fn a_session(&self, session_id: &str, pid: Option<u32>, role: Option<&str>) {
        let dir = self.session_dir(session_id);
        std::fs::create_dir_all(&dir).unwrap();
        let mut yaml = format!(
            "session_id: {session_id}\nproject_id: {THE_PROJECT}\ncreated_at: '2026-11-15T10:00:00Z'\nupdated_at: '2026-11-15T10:00:00Z'\nstatus: active\nsession_type: workspace\n"
        );
        if let Some(pid) = pid {
            yaml.push_str(&format!("pid: {pid}\n"));
        }
        if let Some(role) = role {
            yaml.push_str(&format!("session_role: {role}\n"));
        }
        std::fs::write(dir.join(".session.yaml"), yaml).unwrap();
    }

    /// The project's main session on this host, run by a real process so stopping it is visible.
    fn a_running_main(&self, session_id: &str) -> std::process::Child {
        let child = std::process::Command::new("sleep")
            .arg("120")
            .spawn()
            .expect("spawn a stand-in main process");
        self.a_session(session_id, Some(child.id()), Some("project-home"));
        project_storage::set_project_home_session(&self.projects_dir(), THE_PROJECT, Some(session_id))
            .unwrap();
        child
    }

    async fn listed(&self, include_deleted: bool) -> Vec<ProjectEntry> {
        self.daemon
            .list_projects(Request::direct(ListProjectsRequest {
                session_token: TEST_TOKEN.to_string(),
                local_only: false,
                include_deleted,
            }))
            .await
            .expect("ListProjects answers")
            .into_inner()
            .projects
            .into_iter()
            .filter(|p| p.project_id == THE_PROJECT)
            .collect()
    }

    async fn rename(&self, name: &str) -> Result<UpdateProjectResponse, Status> {
        self.daemon
            .update_project(Request::direct(UpdateProjectRequest {
                session_token: TEST_TOKEN.to_string(),
                project_id: THE_PROJECT.to_string(),
                name: name.to_string(),
                local_only: false,
            }))
            .await
            .map(|r| r.into_inner())
    }

    async fn delete(&self, project_id: &str) -> Result<DeleteProjectResponse, Status> {
        self.daemon
            .delete_project(Request::direct(DeleteProjectRequest {
                session_token: TEST_TOKEN.to_string(),
                project_id: project_id.to_string(),
                ..Default::default()
            }))
            .await
            .map(|r| r.into_inner())
    }

    async fn restore(&self) -> Result<RestoreProjectResponse, Status> {
        self.daemon
            .restore_project(Request::direct(RestoreProjectRequest {
                session_token: TEST_TOKEN.to_string(),
                project_id: THE_PROJECT.to_string(),
                ..Default::default()
            }))
            .await
            .map(|r| r.into_inner())
    }
}

struct World {
    peers: Arc<InProcessPeers>,
    a: Host,
    b: Host,
}

fn two_hosts_owning_the_project() -> World {
    let peers = Arc::new(InProcessPeers::default());
    let a = a_host_owning_the_project(HOST_A, &peers);
    let b = a_host_owning_the_project(HOST_B, &peers);
    World { peers, a, b }
}

impl World {
    fn take_offline(&self, host: &Host) {
        self.peers.take_offline(host.id);
    }

    fn bring_back(&self, host: &Host) {
        self.peers.register(host.id, Arc::clone(&host.daemon));
    }
}

fn sorted(mut hosts: Vec<String>) -> Vec<String> {
    hosts.sort();
    hosts
}

fn unconfirmed_ids(hosts: &[tddy_service::proto::project::UnconfirmedHost]) -> Vec<String> {
    sorted(hosts.iter().map(|h| h.daemon_instance_id.clone()).collect())
}

fn wait_for_exit(child: &mut std::process::Child) -> bool {
    for _ in 0..100 {
        if child.try_wait().expect("poll the main process").is_some() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    false
}

// ── update ───────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn renaming_a_project_renames_it_on_every_host_owning_it() {
    // Given a project on host-a and host-b, and a host-c that never answers
    let world = two_hosts_owning_the_project();

    // When host-a renames it
    let renamed = world.a.rename("  The Project ").await.expect("the rename succeeds");

    // Then both hosts' rows carry the new name, trimmed
    assert_eq!(renamed.project.expect("a row").name, "The Project");
    assert_eq!(world.a.stored_row().name, "The Project");
    assert_eq!(world.b.stored_row().name, "The Project");
    // And the answer says which hosts applied it and which could not be reached
    assert_eq!(sorted(renamed.applied_hosts), vec![HOST_A, HOST_B]);
    assert_eq!(unconfirmed_ids(&renamed.unconfirmed_hosts), vec![SILENT_HOST]);
}

#[tokio::test]
async fn renaming_keeps_the_repository_identity() {
    let world = two_hosts_owning_the_project();
    let before = world.b.stored_row();

    world.a.rename("renamed").await.unwrap();

    assert_eq!(
        world.b.stored_row(),
        ProjectData {
            name: "renamed".to_string(),
            ..before
        }
    );
}

#[tokio::test]
async fn a_rename_reaches_a_project_this_host_does_not_own() {
    // Given host-a holds no row for the project, and host-b does
    let peers = Arc::new(InProcessPeers::default());
    let a = a_host_without_the_project(HOST_A, &peers);
    let b = a_host_owning_the_project(HOST_B, &peers);

    // When host-a is asked to rename it
    let renamed = a.rename("from-a").await.expect("the rename is forwarded");

    // Then host-b's row is renamed, and its row is the answer
    assert_eq!(b.stored_row().name, "from-a");
    assert_eq!(renamed.applied_hosts, vec![HOST_B.to_string()]);
    assert_eq!(renamed.project.expect("a row").daemon_instance_id, HOST_B);
}

#[tokio::test]
async fn a_name_the_create_rules_refuse_is_refused_before_any_host_changes() {
    let world = two_hosts_owning_the_project();

    let refusal = world.a.rename("a/b").await.unwrap_err();

    assert_eq!(refusal.code, Code::InvalidArgument);
    assert_eq!(world.a.stored_row().name, "the-project");
    assert_eq!(world.b.stored_row().name, "the-project");
}

#[tokio::test]
async fn renaming_a_project_no_reachable_host_owns_is_not_found() {
    let world = two_hosts_owning_the_project();

    let refusal = world
        .a
        .daemon
        .update_project(Request::direct(UpdateProjectRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: UNKNOWN_PROJECT.to_string(),
            name: "x".to_string(),
            local_only: false,
        }))
        .await
        .unwrap_err();

    assert_eq!(refusal.code, Code::NotFound);
}

// ── delete ───────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn deleting_a_project_tombstones_it_on_every_host_and_keeps_its_data() {
    // Given a project on two hosts, with a stopped session on host-b
    let world = two_hosts_owning_the_project();
    world.b.a_session("stopped-1", None, None);

    // When host-a deletes it
    let deleted = world.a.delete(THE_PROJECT).await.expect("the delete succeeds");

    // Then both rows are tombstoned by the caller
    for host in [&world.a, &world.b] {
        let row = host.stored_row();
        assert!(row.is_deleted(), "{} tombstoned", host.id);
        assert_eq!(row.lifecycle.deleted_by.as_deref(), Some(TEST_USER));
    }
    // And with the same tombstone time, stamped once by host-a
    assert_eq!(
        world.a.stored_row().lifecycle.deleted_at,
        world.b.stored_row().lifecycle.deleted_at
    );
    assert_eq!(sorted(deleted.applied_hosts), vec![HOST_A, HOST_B]);
    assert_eq!(unconfirmed_ids(&deleted.unconfirmed_hosts), vec![SILENT_HOST]);
    // And nothing is removed from disk: checkout, stack record, session directory
    for host in [&world.a, &world.b] {
        assert!(host.checkout.path().join("README.md").is_file());
        assert!(host
            .projects_dir()
            .join(THE_PROJECT)
            .join("stacks")
            .join("stack-1.yaml")
            .is_file());
    }
    assert!(world.b.session_dir("stopped-1").join(".session.yaml").is_file());
}

#[tokio::test]
async fn a_deleted_project_is_listed_only_when_deleted_projects_are_asked_for() {
    let world = two_hosts_owning_the_project();
    world.a.delete(THE_PROJECT).await.unwrap();

    // Then the default listing on either host leaves it out
    assert_eq!(world.a.listed(false).await, vec![]);
    assert_eq!(world.b.listed(false).await, vec![]);

    // And asking for deleted projects lists every host's row with its tombstone
    let with_deleted = world.a.listed(true).await;
    assert_eq!(
        sorted(with_deleted.iter().map(|p| p.daemon_instance_id.clone()).collect()),
        vec![HOST_A, HOST_B]
    );
    for row in &with_deleted {
        assert!(!row.deleted_at.is_empty(), "{row:?}");
        assert_eq!(row.deleted_by, TEST_USER);
    }
}

#[tokio::test]
async fn deleting_twice_keeps_the_first_tombstone() {
    let world = two_hosts_owning_the_project();
    world.a.delete(THE_PROJECT).await.unwrap();
    let first = world.b.stored_row().lifecycle;

    world.b.delete(THE_PROJECT).await.expect("a repeated delete is a no-op");

    assert_eq!(world.b.stored_row().lifecycle, first);
}

#[tokio::test]
async fn deleting_a_project_no_reachable_host_owns_is_not_found() {
    let world = two_hosts_owning_the_project();

    let refusal = world.a.delete(UNKNOWN_PROJECT).await.unwrap_err();

    assert_eq!(refusal.code, Code::NotFound);
}

#[tokio::test]
async fn a_delete_is_refused_while_another_host_runs_one_of_the_projects_sessions() {
    // Given an ordinary session of the project running on host-b
    let world = two_hosts_owning_the_project();
    world.b.a_session("busy-1", Some(std::process::id()), None);

    // When host-a deletes the project
    let refusal = world.a.delete(THE_PROJECT).await.unwrap_err();

    // Then it is refused, naming the session and its host
    assert_eq!(refusal.code, Code::FailedPrecondition);
    assert!(refusal.message.contains("busy-1"), "{}", refusal.message);
    assert!(refusal.message.contains(HOST_B), "{}", refusal.message);
    // And no host was tombstoned
    assert!(!world.a.stored_row().is_deleted());
    assert!(!world.b.stored_row().is_deleted());
}

#[tokio::test]
async fn deleting_stops_the_project_home_session_and_keeps_its_record() {
    // Given the project's main session running on host-a
    let world = two_hosts_owning_the_project();
    let mut main = world.a.a_running_main("main-1");

    // When the project is deleted
    let deleted = world.a.delete(THE_PROJECT).await.expect("main does not block a delete");

    // Then main's process is stopped, and the answer names it
    assert!(wait_for_exit(&mut main), "main's process was stopped");
    assert_eq!(deleted.stopped_session_ids, vec!["main-1".to_string()]);
    // And its session record and the project's pointer to it are kept for a restore
    assert!(world.a.session_dir("main-1").join(".session.yaml").is_file());
    assert_eq!(world.a.stored_row().home_session_id.as_deref(), Some("main-1"));
}

// ── a host that missed the change ────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_host_that_missed_the_delete_does_not_bring_the_project_back() {
    // Given host-b is offline while host-a deletes the project
    let world = two_hosts_owning_the_project();
    world.take_offline(&world.b);
    let deleted = world.a.delete(THE_PROJECT).await.expect("the delete succeeds");
    assert_eq!(deleted.applied_hosts, vec![HOST_A.to_string()]);
    assert_eq!(unconfirmed_ids(&deleted.unconfirmed_hosts), vec![HOST_B, SILENT_HOST]);
    assert!(!world.b.stored_row().is_deleted());

    // When host-b comes back and re-advertises its live row
    world.bring_back(&world.b);

    // Then neither host lists the project
    assert_eq!(world.a.listed(false).await, vec![]);
    assert_eq!(world.b.listed(false).await, vec![]);
    // And host-b has adopted the tombstone as host-a stamped it
    assert_eq!(world.b.stored_row().lifecycle, world.a.stored_row().lifecycle);
}

#[tokio::test]
async fn a_restore_wins_over_a_stale_tombstone_on_a_host_that_missed_it() {
    // Given a project deleted on both hosts, then restored on host-a while host-b was offline
    let world = two_hosts_owning_the_project();
    world.a.delete(THE_PROJECT).await.unwrap();
    world.take_offline(&world.b);
    world.a.restore().await.expect("the restore succeeds");

    // When host-b comes back still holding its tombstone
    world.bring_back(&world.b);

    // Then both hosts list the project again, and host-b has adopted the restore
    assert_eq!(world.b.listed(false).await.len(), 2);
    assert!(!world.b.stored_row().is_deleted());
}

// ── restore ──────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn restoring_a_project_clears_its_tombstone_on_every_host() {
    let world = two_hosts_owning_the_project();
    world.a.delete(THE_PROJECT).await.unwrap();

    let restored = world.b.restore().await.expect("the restore succeeds");

    assert_eq!(sorted(restored.applied_hosts), vec![HOST_A, HOST_B]);
    for host in [&world.a, &world.b] {
        let row = host.stored_row();
        assert!(!row.is_deleted(), "{} restored", host.id);
        assert!(row.lifecycle.restored_at.is_some());
    }
    assert_eq!(world.a.listed(false).await.len(), 2);
}

#[tokio::test]
async fn restoring_a_live_project_changes_nothing() {
    let world = two_hosts_owning_the_project();
    let before = world.a.stored_row();

    world.a.restore().await.expect("a restore of a live project is a no-op");

    assert_eq!(world.a.stored_row(), before);
}

#[tokio::test]
async fn creating_a_project_for_a_deleted_projects_repository_restores_it() {
    // Given the project was deleted
    let world = two_hosts_owning_the_project();
    world.a.delete(THE_PROJECT).await.unwrap();

    // When a project is created on host-a for the same repository
    let created = world
        .a
        .daemon
        .create_project(Request::direct(CreateProjectRequest {
            session_token: TEST_TOKEN.to_string(),
            name: "another-name".to_string(),
            git_url: format!(" {THE_GIT_URL} "),
            user_relative_path: String::new(),
        }))
        .await
        .expect("the create succeeds")
        .into_inner()
        .project
        .expect("a row");

    // Then it is the deleted project, restored on every host, and no second row was added
    assert_eq!(created.project_id, THE_PROJECT);
    assert!(created.deleted_at.is_empty());
    assert!(!world.a.stored_row().is_deleted());
    assert!(!world.b.stored_row().is_deleted());
    assert_eq!(
        project_storage::read_projects(&world.a.projects_dir()).unwrap().len(),
        1
    );
}

#[tokio::test]
async fn adding_a_deleted_project_to_a_host_that_holds_its_tombstone_restores_it() {
    let world = two_hosts_owning_the_project();
    world.a.delete(THE_PROJECT).await.unwrap();

    let added = world
        .a
        .daemon
        .add_project_to_host(Request::direct(AddProjectToHostRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: THE_PROJECT.to_string(),
            name: "the-project".to_string(),
            git_url: THE_GIT_URL.to_string(),
            daemon_instance_id: HOST_B.to_string(),
            ..Default::default()
        }))
        .await
        .expect("the add succeeds")
        .into_inner()
        .project
        .expect("a row");

    assert_eq!(added.daemon_instance_id, HOST_B);
    assert!(added.deleted_at.is_empty());
    assert!(!world.a.stored_row().is_deleted());
    assert!(!world.b.stored_row().is_deleted());
}

// ── new work on a deleted project ────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_deleted_project_starts_no_session() {
    let world = two_hosts_owning_the_project();
    world.a.delete(THE_PROJECT).await.unwrap();

    let refusal = world
        .a
        .daemon
        .start_session(Request::direct(StartSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: THE_PROJECT.to_string(),
            session_type: "workspace".to_string(),
            ..Default::default()
        }))
        .await
        .unwrap_err();

    assert_eq!(refusal.code, Code::FailedPrecondition, "{}", refusal.message);
    assert!(refusal.message.contains("deleted"), "{}", refusal.message);
}

#[tokio::test]
async fn a_deleted_project_sets_up_no_main() {
    let world = two_hosts_owning_the_project();
    world.a.delete(THE_PROJECT).await.unwrap();

    let refusal = world
        .a
        .daemon
        .ensure_project_home_session(Request::direct(EnsureProjectHomeSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: THE_PROJECT.to_string(),
            ..Default::default()
        }))
        .await
        .unwrap_err();

    assert_eq!(refusal.code, Code::FailedPrecondition, "{}", refusal.message);
    assert!(refusal.message.contains("deleted"), "{}", refusal.message);
}

#[tokio::test]
async fn a_session_of_a_deleted_project_is_not_resumed() {
    // Given a stopped session of the project, which the delete keeps
    let world = two_hosts_owning_the_project();
    world.a.a_session("stopped-1", None, None);
    world.a.delete(THE_PROJECT).await.unwrap();

    let refusal = world
        .a
        .daemon
        .resume_session(Request::direct(ResumeSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: "stopped-1".to_string(),
        }))
        .await
        .unwrap_err();

    assert_eq!(refusal.code, Code::FailedPrecondition, "{}", refusal.message);
    assert!(refusal.message.contains("deleted"), "{}", refusal.message);
    assert!(world.a.session_dir("stopped-1").join(".session.yaml").is_file());
}

#[tokio::test]
async fn a_host_without_the_project_does_not_provision_a_deleted_one_from_its_peers() {
    // Given host-a's project was deleted, and host-b has never held it
    let peers = Arc::new(InProcessPeers::default());
    let a = a_host_owning_the_project(HOST_A, &peers);
    let b = a_host_without_the_project(HOST_B, &peers);
    a.delete(THE_PROJECT).await.unwrap();

    // When a session is started for it on host-b
    let outcome = b
        .daemon
        .start_session(Request::direct(StartSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: THE_PROJECT.to_string(),
            session_type: "workspace".to_string(),
            ..Default::default()
        }))
        .await;

    // Then it is refused, and host-b registered nothing
    assert!(outcome.is_err(), "a deleted project is not provisioned");
    assert_eq!(
        project_storage::find_project(&b.projects_dir(), THE_PROJECT).unwrap(),
        None
    );
}

#[tokio::test]
async fn a_deleted_projects_main_reaches_no_project_home_tool() {
    use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest};

    // Given the project's main, stopped, and the project deleted
    let world = two_hosts_owning_the_project();
    world.a.a_session("main-1", None, Some("project-home"));
    project_storage::set_project_home_session(&world.a.projects_dir(), THE_PROJECT, Some("main-1"))
        .unwrap();
    world.a.delete(THE_PROJECT).await.unwrap();

    // When main's agent lists the project's sessions
    let refusal = world
        .a
        .daemon
        .execute_tool(Request::direct(ExecuteToolRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: "main-1".to_string(),
            tool_name: tddy_tool_engine::project_home_tools::LIST_PROJECT_SESSIONS.to_string(),
            args_json: "{}".to_string(),
            daemon_instance_id: String::new(),
            conversation_id: String::new(),
        }))
        .await
        .unwrap_err();

    // Then the tool is refused because the project is deleted
    assert_eq!(refusal.code, Code::FailedPrecondition, "{}", refusal.message);
    assert!(refusal.message.contains("deleted"), "{}", refusal.message);
}
