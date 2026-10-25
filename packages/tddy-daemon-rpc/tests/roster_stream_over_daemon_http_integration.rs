//! The roster stream opens over the daemon-HTTP transport and yields the roster the daemon holds.
//!
//! Feature: docs/ft/daemon/session-agent-roster.md § The roster stream
//! Feature: docs/ft/daemon/remote-codebase-mode.md § Coding subagents on the codebase host
//!
//! The server is the codebase daemon's own family-B mount, `DaemonSessionHost::session_agents_entry`
//! (the entry `runtime::mount_entries` registers on `/rpc`), served on a real loopback port behind
//! `connect_router`. The client is the public follower the MCP server awaits, run in the environment
//! a remote-codebase session is spawned with, so the transport it opens is the one
//! `detect_session_tool_transport` chooses rather than one the test constructs itself.

use std::path::Path;

use pretty_assertions::assert_eq;
use tddy_connectrpc::connect_router;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::SessionMetadata;
use tddy_daemon_rpc::test_util::{test_service, TestDaemon};
use tddy_discovery::roster::{follow_session_agent_roster, session_agent_roster};
use tddy_rpc::{MultiRpcService, Request, RpcBridge};
use tddy_service::proto::catalog::{CatalogService, ListSubagentsRequest};
use tddy_service::proto::session_agents_svc::{
    AttachSessionAgentRequest, ListSessionAgentsRequest, SessionAgentRoster,
    SessionAgentService as _,
};
use tddy_session_lifecycle::test_util::TEST_TOKEN;

#[tokio::test(flavor = "multi_thread")]
async fn the_roster_stream_over_daemon_http_yields_the_roster_the_daemon_holds() {
    // Given
    let sessions = tempfile::tempdir().expect("sessions tempdir");
    write_agent_defs(sessions.path(), &[("linter", "")]);
    let session_id = "1780828020298-remote-roster";
    write_managed_session(sessions.path(), session_id);
    let daemon = test_service(sessions.path().to_path_buf());
    let agent_id = agent_id_for(&daemon, "linter").await;
    let daemon_instance_id = local_daemon_instance_id(&daemon).await;
    attach(&daemon, session_id, &agent_id).await;
    let held = list(&daemon, session_id).await;
    let relay_url = serve_over_http(&daemon).await;
    a_session_reaching_its_daemon_over_http(&relay_url, session_id, &daemon_instance_id);

    // When
    follow_session_agent_roster(|| {}).await;

    // Then
    let report = session_agent_roster().status_report();
    assert_eq!(report.refusal, None);
    assert_eq!(report.applied_rev, Some(held.rev));
    let followed: Vec<String> = report
        .agents
        .iter()
        .map(|agent| agent.entry.agent_id.clone())
        .collect();
    assert_eq!(followed, vec![agent_id]);
}

/// `<tddyhome>/agents/<name>.yaml` for each named def. `test_service` uses the sessions base as the
/// data dir, so this is the directory the daemon resolves against.
fn write_agent_defs(tddy_data_dir: &Path, agents: &[(&str, &str)]) {
    let agents_dir = tddy_data_dir.join("agents");
    std::fs::create_dir_all(&agents_dir).expect("create agents dir");
    for (name, replaces) in agents {
        std::fs::write(
            agents_dir.join(format!("{name}.yaml")),
            format!(
                "name: {name}\nlabel: \"{name} (local)\"\nmodel: qwen2.5-coder:7b\n\
                 base_url: http://localhost:11434\ntools: [READ, GLOB, GREP]\n\
                 replaces: [{replaces}]\n"
            ),
        )
        .expect("write agent def");
    }
}

/// A managed claude-cli session on disk, the type an agent attaches to.
fn write_managed_session(sessions_base: &Path, session_id: &str) {
    let session_dir = unified_session_dir_path(sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).expect("create session dir");
    tddy_core::write_session_metadata(
        &session_dir,
        &SessionMetadata {
            session_id: session_id.to_string(),
            project_id: "project-under-roster".to_string(),
            created_at: "2026-08-16T10:00:00Z".to_string(),
            updated_at: "2026-08-16T10:00:00Z".to_string(),
            status: "active".to_string(),
            repo_path: Some("/tmp/worktrees/roster".to_string()),
            pid: None,
            tool: None,
            livekit_room: None,
            pending_elicitation: false,
            previous_session_id: None,
            session_type: Some("claude-cli".to_string()),
            model: None,
            cursor_chat_id: None,
            harness_resume_token: None,
            activity_status: None,
            hook_token: None,
            sandbox: Some(true),
            agent: None,
            recipe: None,
            agents: Vec::new(),
            agents_rev: 0,
            legacy_specialized_agents: Vec::new(),
            codebase_daemon_instance_id: None,
            codebase_session_id: None,
            agent_daemon_instance_id: None,
            agent_session_id: None,
            ssh_config_host: None,
            managed_codebase: false,
            session_role: None,
        },
    )
    .expect("write session metadata");
}

/// The qualified id this daemon advertises for a def named `name`, read the way a client reads it.
async fn agent_id_for(daemon: &TestDaemon, name: &str) -> String {
    daemon
        .list_subagents(Request::direct(ListSubagentsRequest {}))
        .await
        .expect("listing subagents must succeed")
        .into_inner()
        .subagents
        .into_iter()
        .find(|subagent| subagent.name == name)
        .unwrap_or_else(|| panic!("this daemon must advertise a def named '{name}'"))
        .agent_id
}

/// The local daemon's own instance id, read from a stamped `ListSubagents` row.
async fn local_daemon_instance_id(daemon: &TestDaemon) -> String {
    daemon
        .list_subagents(Request::direct(ListSubagentsRequest {}))
        .await
        .expect("listing subagents must succeed")
        .into_inner()
        .subagents
        .first()
        .expect("the fixture must define at least one agent to read the daemon id from")
        .daemon_instance_id
        .clone()
}

async fn attach(daemon: &TestDaemon, session_id: &str, agent_id: &str) {
    daemon
        .session_agents_service()
        .attach_session_agent(Request::direct(AttachSessionAgentRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
            agent_id: agent_id.to_string(),
            placement: 0,
        }))
        .await
        .expect("attaching a def the daemon advertises must succeed");
}

/// The roster the daemon holds for the session, read through its own service.
async fn list(daemon: &TestDaemon, session_id: &str) -> SessionAgentRoster {
    daemon
        .session_agents_service()
        .list_session_agents(Request::direct(ListSessionAgentsRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
        }))
        .await
        .expect("listing a session's agents must succeed")
        .into_inner()
}

/// The daemon's `/rpc` mount on a loopback port, as a session's relay URL names it.
async fn serve_over_http(daemon: &TestDaemon) -> String {
    let router = connect_router(RpcBridge::new(MultiRpcService::new(vec![
        daemon.session_agents_entry()
    ])));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the daemon endpoint");
    let port = listener
        .local_addr()
        .expect("daemon endpoint local addr")
        .port();
    tokio::spawn(async move {
        // Not swallowed: a daemon that stopped serving would read as an empty roster otherwise.
        axum::serve(listener, router)
            .await
            .expect("the daemon endpoint served");
    });
    format!("http://127.0.0.1:{port}/rpc")
}

/// Points this process's session tool transport at `relay_url`, as a remote-codebase session's
/// environment does. Clears the variables that would select another transport, and the static-roster
/// declaration that would switch the stream off.
fn a_session_reaching_its_daemon_over_http(
    relay_url: &str,
    session_id: &str,
    daemon_instance_id: &str,
) {
    for key in [
        "TDDY_SANDBOX_TOOL_IPC",
        "TDDY_REMOTE_DAEMON_SOCKET",
        "TDDY_REMOTE_LIVEKIT_URL",
        "TDDY_REMOTE_LIVEKIT_ROOM",
        "TDDY_REMOTE_LIVEKIT_TOKEN",
        "TDDY_REMOTE_SERVER_IDENTITY",
        "TDDY_SUBAGENT_ROSTER_STATIC",
    ] {
        std::env::remove_var(key);
    }
    std::env::set_var("TDDY_REMOTE_SESSION_ID", session_id);
    std::env::set_var("TDDY_REMOTE_SESSION_TOKEN", TEST_TOKEN);
    std::env::set_var("TDDY_REMOTE_DAEMON_INSTANCE_ID", daemon_instance_id);
    std::env::set_var("TDDY_REMOTE_DAEMON_URL", relay_url);
}
