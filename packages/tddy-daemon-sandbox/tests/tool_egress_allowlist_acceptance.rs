//! Acceptance: a jail reaches exactly the hosts its tools declare.
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-tool-egress-allowlist.md` (`#agent-tools 3/12`).
//!
//! Two levels. The plan tests pin what a workspace jail built from declared tools asks its runner
//! for — the lowered policy, a shim only when something may leave, and the tool set a refusal
//! names. The shim test runs the real runner in-process with its real egress shim and the real host
//! relay against two loopback listeners, and drives `curl` through the shim the way a jailed tool
//! does. It runs on every platform: the enforcement is the runner's code, not the jail backend's,
//! so a Seatbelt or cgroups jail around it would prove nothing more about the policy. The relay's
//! own refusal — the bypass of the shim — is `tddy-sandbox-runner`'s
//! `egress_allowlist_relay_acceptance`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_actions::tool::{Egress, HostPattern, NetworkSection, ResolvedTool};
use tddy_daemon_sandbox::workspace_tool_sandbox::{
    build_workspace_tool_plan, WorkspaceSandboxLayout, WorkspaceToolPlanRequest,
};
use tddy_sandbox::{EgressPolicy, SandboxPlan};
use tddy_sandbox_recipes::lower_network;

const SESSION_ID: &str = "tool-egress-allowlist";

fn a_tool(id: &str, egress: Egress) -> ResolvedTool {
    ResolvedTool {
        id: id.into(),
        command: id.into(),
        binary: PathBuf::from(format!("/usr/bin/{id}")),
        reads: vec![],
        network: NetworkSection {
            egress,
            oauth_inbound: false,
        },
    }
}

fn hosts(patterns: &[&str]) -> Egress {
    Egress::Hosts(
        patterns
            .iter()
            .map(|p| HostPattern::parse(p).expect("a valid pattern"))
            .collect(),
    )
}

/// A session directory with a checkout, as a workspace start leaves them.
struct ASession {
    session_dir: PathBuf,
    worktree: PathBuf,
    _tmp: tempfile::TempDir,
}

fn a_session() -> ASession {
    let tmp = tempfile::tempdir().expect("tempdir");
    let session_dir = tmp.path().join("sessions").join(SESSION_ID);
    let worktree = tmp.path().join("worktree");
    std::fs::create_dir_all(&session_dir).expect("session dir");
    std::fs::create_dir_all(&worktree).expect("worktree");
    ASession {
        session_dir,
        worktree,
        _tmp: tmp,
    }
}

fn a_workspace_jail_plan_running(session: &ASession, tools: Vec<ResolvedTool>) -> SandboxPlan {
    build_workspace_tool_plan(WorkspaceToolPlanRequest {
        layout: WorkspaceSandboxLayout::under_session_dir(&session.session_dir),
        worktree_path: session.worktree.clone(),
        session_id: SESSION_ID.into(),
        runner_path: "/opt/tddy/bin/tddy-sandbox-runner".into(),
        tddy_tools_path: "/opt/tddy/bin/tddy-tools".into(),
        cgroup: Default::default(),
        tools,
    })
    .expect("the plan builds")
}

/// Every value the runner's argv gives `flag`, in order.
fn the_runner_values_of(plan: &SandboxPlan, flag: &str) -> Vec<String> {
    plan.spec
        .command
        .windows(2)
        .filter(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
        .collect()
}

/// The proxy variables the plan sets on the jail, by name.
fn the_proxy_env_of(plan: &SandboxPlan) -> Vec<String> {
    plan.env
        .vars
        .keys()
        .filter(|name| name.to_ascii_lowercase().ends_with("_proxy"))
        .cloned()
        .collect()
}

#[test]
fn a_jail_whose_tools_declare_egress_none_starts_no_shim_and_has_no_proxy_env() {
    // Given — two tools, neither of which needs the network
    let session = a_session();
    let tools = vec![a_tool("rg", Egress::None), a_tool("jq", Egress::None)];

    // When
    let plan = a_workspace_jail_plan_running(&session, tools);

    // Then
    assert_eq!(plan.network.egress, EgressPolicy::None);
    assert_eq!(
        the_runner_values_of(&plan, "--egress-shim-port"),
        Vec::<String>::new()
    );
    assert_eq!(the_proxy_env_of(&plan), Vec::<String>::new());
}

#[test]
fn a_jail_whose_tool_declares_hosts_passes_its_runner_exactly_those_hosts_and_the_tool_set() {
    // Given — the positive control for the jail above: one tool that needs two hosts
    let session = a_session();
    let tools = vec![
        a_tool("rg", Egress::None),
        a_tool(
            "gh",
            hosts(&["api.github.com", "*.githubusercontent.com:8443"]),
        ),
    ];

    // When
    let plan = a_workspace_jail_plan_running(&session, tools);

    // Then — a shim, told exactly the declared hosts and which tools declared them
    assert_eq!(
        plan.network.egress,
        EgressPolicy::Hosts(vec![
            tddy_sandbox::HostPattern::new("api.github.com", 443),
            tddy_sandbox::HostPattern::new("*.githubusercontent.com", 8443),
        ])
    );
    assert_eq!(the_runner_values_of(&plan, "--egress-shim-port").len(), 1);
    assert_eq!(
        the_runner_values_of(&plan, "--egress-policy"),
        vec!["hosts:api.github.com:443,*.githubusercontent.com:8443".to_string()]
    );
    assert_eq!(
        the_runner_values_of(&plan, "--egress-tool"),
        vec!["rg".to_string(), "gh".to_string()]
    );
}

#[test]
fn one_tool_declaring_any_makes_the_jails_policy_any() {
    // Given — one unrestricted tool among a host-listing one and a networkless one
    let session = a_session();
    let tools = vec![
        a_tool("rg", Egress::None),
        a_tool("gh", hosts(&["api.github.com"])),
        a_tool("curl", Egress::Any),
    ];

    // When
    let plan = a_workspace_jail_plan_running(&session, tools);

    // Then
    assert_eq!(plan.network.egress, EgressPolicy::Any);
    assert_eq!(the_runner_values_of(&plan, "--egress-shim-port").len(), 1);
    assert_eq!(
        the_runner_values_of(&plan, "--egress-policy"),
        vec!["any".to_string()]
    );
}

// ---------------------------------------------------------------------------
// The shim, live: the real runner and host relay, `curl` through the shim
// ---------------------------------------------------------------------------

/// A loopback HTTP server answering every request with `body`.
async fn a_local_server_answering(body: &'static str) -> u16 {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf).await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
            });
        }
    });
    port
}

fn a_free_loopback_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// The real runner, in generic-PTY mode with nothing to run but a sleep, its shim on `shim_port`
/// enforcing `policy` for the `tool_set`; and the real host relay attached to it.
async fn a_runner_with_its_shim_enforcing(
    dir: &Path,
    shim_port: u16,
    policy: &EgressPolicy,
    tool_set: &[&str],
) {
    let context_dir = dir.join("context");
    std::fs::create_dir_all(&context_dir).unwrap();
    let ready_marker = dir.join("sandbox.ready");
    let args = tddy_sandbox_runner::SandboxRunnerArgs {
        session_id: SESSION_ID.to_string(),
        context_dir,
        cwd: None,
        grpc_socket: None,
        tool_ipc_socket: dir.join("tool_ipc.sock"),
        tddy_tools_path: None,
        claude_binary: "claude".to_string(),
        agent_kind: "claude".to_string(),
        agent_binary: None,
        model: String::new(),
        ready_marker: ready_marker.clone(),
        permission_mode: "auto".to_string(),
        dangerously_skip_permissions: false,
        resume: false,
        append_system_prompt_file: None,
        grpc_listen_port: None,
        egress_shim_port: Some(shim_port),
        egress_policy: policy.to_runner_arg(),
        egress_tool: tool_set.iter().map(|t| t.to_string()).collect(),
        grpc_uds: None,
        pty_command: vec!["/bin/sleep".to_string(), "60".to_string()],
        claude_arg: vec![],
        agent_arg: vec![],
        mcp_log_level: None,
        stdio: false,
        initial_cols: 80,
        initial_rows: 24,
    };
    tokio::spawn(async move {
        let _ = tddy_sandbox_runner::run_sandbox_runner(args).await;
    });

    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    // The marker holds the gRPC port; it is complete once it parses.
    while std::fs::read_to_string(&ready_marker)
        .ok()
        .and_then(|port| port.trim().parse::<u16>().ok())
        .is_none()
    {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the runner never became ready"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let client = tddy_sandbox_runner::connect_sandbox_client(&ready_marker)
        .await
        .expect("connect the runner");
    let (terminal_tx, _terminal_rx) = tokio::sync::mpsc::unbounded_channel::<bytes::Bytes>();
    let (_stdin_tx, stdin_rx) = tokio::sync::mpsc::unbounded_channel::<bytes::Bytes>();
    // The relay admits everything here, so whatever is refused below is the shim's refusal.
    let _relay = tddy_sandbox_runner::run_host_relay(
        client,
        tddy_sandbox_runner::NullToolHandler,
        tddy_sandbox_runner::HostRelayConfig::new(SESSION_ID, terminal_tx),
        stdin_rx,
    )
    .await
    .expect("start host relay");
}

/// `curl` through the shim as a jailed tool reaches the network: `--proxytunnel` makes it `CONNECT`
/// even to an `http://` target. Prints the CONNECT status, then the body.
async fn curl_through_the_shim(shim_port: u16, target_port: u16) -> String {
    let out = tokio::process::Command::new("curl")
        .args([
            "--silent",
            "--max-time",
            "10",
            "--proxytunnel",
            "--proxy",
            &format!("http://127.0.0.1:{shim_port}"),
            "--write-out",
            "CONNECT=%{http_connect} ",
            &format!("http://127.0.0.1:{target_port}/"),
        ])
        .output()
        .await
        .expect("curl runs");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tool_declaring_one_local_host_reaches_it_through_the_shim_and_another_local_port_is_refused_403(
) {
    // Given — a tool declaring one local host, and a second local server it never declared
    let declared = a_local_server_answering("declared").await;
    let undeclared = a_local_server_answering("undeclared").await;
    let tools = [a_tool("curl", hosts(&[&format!("127.0.0.1:{declared}")]))];
    let (policy, _oauth_inbound) = lower_network(&tools);
    let dir = tempfile::tempdir().unwrap();
    let shim_port = a_free_loopback_port();
    a_runner_with_its_shim_enforcing(dir.path(), shim_port, &policy, &["curl"]).await;

    // When
    let reached = curl_through_the_shim(shim_port, declared).await;
    let refused = curl_through_the_shim(shim_port, undeclared).await;

    // Then
    assert_eq!(reached, "declaredCONNECT=200 ");
    assert_eq!(refused, "CONNECT=403 ");
}
