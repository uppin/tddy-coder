//! Each RPC family, served by its own handler rather than by `DaemonSessionHost`.
//!
//! These drive the four handlers exactly the way the daemon serves them — built from a host with
//! `from_host`, wrapped in the family's own `*ServiceImpl` — and assert the answers the host used to
//! give. The families' full behaviour is pinned by their own suites, which move here with them;
//! what these add is that the handler **is** the family now, and that it shares the host's state
//! instead of a copy of it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::user_paths::projects_path_for_user;
use tddy_daemon_kernel::{SessionUserResolver, SessionsBaseResolver};
use tddy_daemon_rpc::{
    CatalogRpcHandler, ExecToolRpcHandler, PrStackRpcHandler, ProjectRpcHandler,
};
use tddy_discovery::CatalogServiceImpl;
use tddy_projects::project_storage::{self, ProjectData};
use tddy_projects::ProjectServiceImpl;
use tddy_rpc::{Code, Request};
use tddy_service::proto::catalog::{CatalogService, ListToolsRequest};
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest};
use tddy_service::proto::pr_stack::{PrStackService, QueryBranchRequest};
use tddy_service::proto::project::{ListProjectsRequest, ProjectService};
use tddy_session_lifecycle::cli_session_manager::CliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::relay_idle::IdleTimeoutTracker;
use tddy_session_lifecycle::PrStackServiceImpl;
use tddy_tool_engine::ExecToolServiceImpl;

const A_KNOWN_TOKEN: &str = "a-known-token";
const AN_UNKNOWN_TOKEN: &str = "a-token-no-daemon-issued";
const GITHUB_USER: &str = "octocat";
const OS_USER: &str = "octodev";

// ---------------------------------------------------------------------------------------------
// Given: a daemon host
// ---------------------------------------------------------------------------------------------

/// A daemon host rooted in its own temp directory, knowing one caller.
struct ADaemon {
    tools: Vec<String>,
    idle_tracker: Option<Arc<IdleTimeoutTracker>>,
}

fn a_daemon() -> ADaemon {
    ADaemon {
        tools: Vec::new(),
        idle_tracker: None,
    }
}

impl ADaemon {
    fn allowing_the_tool(mut self, path: &str) -> Self {
        self.tools.push(path.to_string());
        self
    }

    fn with_the_idle_tracker(mut self, tracker: &Arc<IdleTimeoutTracker>) -> Self {
        self.idle_tracker = Some(Arc::clone(tracker));
        self
    }

    fn running(self) -> ARunningDaemon {
        let home = tempfile::tempdir().expect("a temp tddy home");
        let tddy_data_dir = home.path().to_path_buf();
        let config = a_config(home.path(), &self.tools);

        let sessions_base = tddy_data_dir.clone();
        let sessions_base_resolver: SessionsBaseResolver =
            Arc::new(move |_| Some(sessions_base.clone()));
        let user_resolver: SessionUserResolver =
            Arc::new(|token| (token == A_KNOWN_TOKEN).then(|| GITHUB_USER.to_string()));

        let host = DaemonSessionHost::new(
            config,
            sessions_base_resolver,
            tddy_data_dir.clone(),
            user_resolver,
            None,
            None,
            None,
            Arc::new(CliSessionManager::new()),
        );
        let host = match self.idle_tracker {
            Some(tracker) => host.with_idle_tracker(tracker),
            None => host,
        };

        ARunningDaemon {
            host,
            tddy_data_dir,
            _home: home,
        }
    }
}

/// The daemon's config, mapping the one caller to an OS user and allowing `tools`.
fn a_config(home: &Path, tools: &[String]) -> DaemonConfig {
    let allowed_tools: String = tools
        .iter()
        .map(|path| format!("  - path: \"{path}\"\n"))
        .collect();
    let yaml = format!(
        "users:\n  - github_user: \"{GITHUB_USER}\"\n    os_user: \"{OS_USER}\"\nallowed_tools:\n{allowed_tools}"
    );
    let path = home.join("daemon.yaml");
    std::fs::write(&path, yaml).expect("write the daemon config");
    DaemonConfig::load(&path).expect("the daemon config did not load")
}

struct ARunningDaemon {
    host: DaemonSessionHost,
    tddy_data_dir: PathBuf,
    _home: tempfile::TempDir,
}

impl ARunningDaemon {
    /// Registers a project for the one caller, the way `CreateProject` would have.
    fn with_a_registered_project(self, name: &str) -> Self {
        let projects_dir = projects_path_for_user(OS_USER, Some(&self.tddy_data_dir))
            .expect("the caller's projects directory");
        project_storage::add_project(
            &projects_dir,
            ProjectData {
                project_id: format!("{name}-id"),
                name: name.to_string(),
                git_url: format!("https://example.com/{name}.git"),
                main_repo_path: self.tddy_data_dir.join(name).display().to_string(),
                main_branch_ref: None,
                remote_name: None,
                host_repo_paths: HashMap::new(),
            },
        )
        .expect("register the project");
        self
    }

    fn project_service(&self) -> ProjectServiceImpl<ProjectRpcHandler> {
        ProjectServiceImpl::new(Arc::new(ProjectRpcHandler::from_host(&self.host)))
    }

    fn catalog_service(&self) -> CatalogServiceImpl<CatalogRpcHandler> {
        CatalogServiceImpl::new(Arc::new(CatalogRpcHandler::from_host(&self.host)))
    }

    fn exec_tool_service(&self) -> ExecToolServiceImpl<ExecToolRpcHandler> {
        ExecToolServiceImpl::new(Arc::new(ExecToolRpcHandler::from_host(&self.host)))
    }

    fn pr_stack_service(&self) -> PrStackServiceImpl<PrStackRpcHandler> {
        PrStackServiceImpl::new(Arc::new(PrStackRpcHandler::from_host(&self.host)))
    }
}

/// An idle tracker whose timeout has already elapsed, so any recorded activity is observable as
/// `should_shutdown()` turning false.
fn an_idle_tracker_that_has_already_timed_out() -> Arc<IdleTimeoutTracker> {
    let tracker = Arc::new(IdleTimeoutTracker::new(Duration::from_millis(1)));
    std::thread::sleep(Duration::from_millis(10));
    assert!(
        tracker.should_shutdown(),
        "the fixture's tracker must start out idle, or activity would not be observable"
    );
    tracker
}

// ---------------------------------------------------------------------------------------------
// Project — family D
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_project_handler_lists_the_projects_registered_for_the_caller() {
    // Given
    let daemon = a_daemon().running().with_a_registered_project("widgets");

    // When
    let listed = daemon
        .project_service()
        .list_projects(Request::new(ListProjectsRequest {
            session_token: A_KNOWN_TOKEN.to_string(),
            local_only: true,
        }))
        .await
        .expect("the caller's projects were not listed")
        .into_inner();

    // Then
    let names: Vec<&str> = listed.projects.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["widgets"]);
}

#[tokio::test]
async fn a_project_handler_refuses_a_caller_whose_session_token_is_unknown() {
    // Given
    let daemon = a_daemon().running().with_a_registered_project("widgets");

    // When
    let refusal = daemon
        .project_service()
        .list_projects(Request::new(ListProjectsRequest {
            session_token: AN_UNKNOWN_TOKEN.to_string(),
            local_only: true,
        }))
        .await
        .expect_err("an unknown caller was shown projects");

    // Then
    assert_eq!(refusal.code(), Code::Unauthenticated);
}

// ---------------------------------------------------------------------------------------------
// Catalog — family A
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_catalog_handler_lists_the_tools_the_daemon_allows() {
    // Given
    let daemon = a_daemon()
        .allowing_the_tool("target/debug/tddy-coder")
        .running();

    // When
    let listed = daemon
        .catalog_service()
        .list_tools(Request::new(ListToolsRequest {}))
        .await
        .expect("the allowed tools were not listed")
        .into_inner();

    // Then
    let paths: Vec<&str> = listed.tools.iter().map(|t| t.path.as_str()).collect();
    assert_eq!(paths, vec!["target/debug/tddy-coder"]);
}

/// The handler is not a copy of the host: activity it records lands on the tracker the host was
/// built with, which is what keeps a relay daemon from shutting itself down mid-session.
#[tokio::test]
async fn a_catalog_handler_records_rpc_activity_on_the_hosts_own_idle_tracker() {
    // Given
    let tracker = an_idle_tracker_that_has_already_timed_out();
    let daemon = a_daemon().with_the_idle_tracker(&tracker).running();

    // When
    daemon
        .catalog_service()
        .list_tools(Request::new(ListToolsRequest {}))
        .await
        .expect("the tools were not listed");

    // Then
    assert!(
        !tracker.should_shutdown(),
        "the call did not bump the host's idle tracker — the handler holds a tracker of its own"
    );
}

// ---------------------------------------------------------------------------------------------
// Exec tools — family L
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn an_exec_tool_handler_refuses_a_caller_whose_session_token_is_unknown() {
    // Given
    let daemon = a_daemon().running();

    // When
    let refusal = daemon
        .exec_tool_service()
        .execute_tool(Request::new(ExecuteToolRequest {
            session_token: AN_UNKNOWN_TOKEN.to_string(),
            session_id: "a-session".to_string(),
            tool_name: "Read".to_string(),
            args_json: "{}".to_string(),
            ..Default::default()
        }))
        .await
        .expect_err("an unknown caller ran a tool");

    // Then
    assert_eq!(refusal.code(), Code::Unauthenticated);
}

// ---------------------------------------------------------------------------------------------
// PR stack — family P
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_pr_stack_handler_refuses_a_caller_whose_session_token_is_unknown() {
    // Given
    let daemon = a_daemon().running();

    // When
    let refusal = daemon
        .pr_stack_service()
        .query_branch(Request::new(QueryBranchRequest {
            session_token: AN_UNKNOWN_TOKEN.to_string(),
            branch: "feature/widgets".to_string(),
            ..Default::default()
        }))
        .await
        .expect_err("an unknown caller queried a branch");

    // Then
    assert_eq!(refusal.code(), Code::Unauthenticated);
}
