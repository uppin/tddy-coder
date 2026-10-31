//! Acceptance: a sandboxed `claude-cli` session's jail is built from the `claude` tool definition
//! (`tddy-coding-agent-specs/specs/claude.yaml`) by the shared jail launcher — the runner is told
//! what to exec, never that the process is Claude, and Claude sees an ordinary environment.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-claude-cli-jail-from-spec.md` (`#agent-tools 5/12`).
//!
//! The daemon is the real `DaemonSessionHost` with its sandbox RPC bridge installed and its coding
//! agents given as the daemon runtime gives them — the agent registry from `<tddyhome>/tools`,
//! resolved against a fixture host whose `PATH` is a stub `claude` and the real `tddy-tools`. The
//! jail is a real Seatbelt jail running the real `tddy-sandbox-runner`; the stub reports on its
//! terminal what it was started with (see `claude_jail_support`). Hosts a test declares are local
//! stand-ins: a `<tddyhome>/tools/claude.yaml` override maps them through the pattern list.
//!
//! The jail is a real Seatbelt jail, so this suite is macOS-only; CI (ubuntu) never runs it.
#![cfg(target_os = "macos")]

mod claude_jail_support;

use std::collections::BTreeSet;
use std::path::Path;

use claude_jail_support::{
    a_jailed_claude_daemon, the_builtin_claude_yaml, ADaemonSpec, AJailedClaudeDaemon, AStubReport,
    TEST_MODEL,
};
use pretty_assertions::assert_eq;
use tddy_agent_launch::jail_launch::EXEC_PLAN_FILE;
use tddy_sandbox_runner::ExecPlan;
use tddy_testing_commons::a_stub_http_endpoint_answering_ok;

const CREDENTIALS: &str = r#"{"claudeAiOauth":{"accessToken":"from-the-persistent-home"}}"#;
const OAUTH_TOKEN: &str = "CLAUDE_CODE_OAUTH_TOKEN";

/// The runner flags only the Claude agent mode reads.
const CLAUDE_RUNNER_FLAGS: &[&str] = &[
    "--claude-binary",
    "--claude-arg",
    "--permission-mode",
    "--dangerously-skip-permissions",
    "--resume",
    "--append-system-prompt-file",
];

/// Write `contents` to `<home>/.claude/.credentials.json`.
fn a_home_holding_credentials(home: &Path, contents: &str) {
    let credentials = home.join(".claude/.credentials.json");
    std::fs::create_dir_all(credentials.parent().unwrap()).unwrap();
    std::fs::write(credentials, contents).unwrap();
}

/// A daemon whose `<tddyhome>/homes/claude` already holds [`CREDENTIALS`].
fn a_daemon_with_a_claude_home(spec: ADaemonSpec) -> AJailedClaudeDaemon {
    let daemon = a_jailed_claude_daemon(spec);
    a_home_holding_credentials(&daemon.tddy_home.join("homes/claude"), CREDENTIALS);
    daemon
}

/// The built-in `claude.yaml` with `network.egress.hosts` replaced by `hosts`.
fn claude_yaml_declaring_hosts(hosts: &[String]) -> serde_yaml::Value {
    let mut claude = the_builtin_claude_yaml();
    let network = serde_yaml::to_value(serde_yaml::Mapping::from_iter([(
        "egress".into(),
        serde_yaml::to_value(serde_yaml::Mapping::from_iter([(
            "hosts".into(),
            serde_yaml::to_value(hosts).unwrap(),
        )]))
        .unwrap(),
    )]))
    .unwrap();
    claude
        .as_mapping_mut()
        .unwrap()
        .insert("network".into(), network);
    claude
}

/// `claude` with its OAuth secret read from `token_file` instead of the daemon's environment, still
/// guarded `when: unrouted`.
fn with_the_oauth_token_from(
    mut claude: serde_yaml::Value,
    token_file: &Path,
) -> serde_yaml::Value {
    let secrets: serde_yaml::Value = serde_yaml::from_str(&format!(
        "- env: {OAUTH_TOKEN}\n  source: {{ kind: host-file, path: {} }}\n  when: unrouted\n",
        token_file.display()
    ))
    .unwrap();
    claude
        .as_mapping_mut()
        .unwrap()
        .insert("secrets".into(), secrets);
    claude
}

impl AJailedClaudeDaemon {
    /// The plan the jail launcher wrote for `session_id`'s runner.
    fn exec_plan_of(&self, session_id: &str) -> ExecPlan {
        let path = self
            .session_dir(session_id)
            .join("sandbox")
            .join(EXEC_PLAN_FILE);
        let json = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("the jail launcher wrote {}: {e}", path.display()));
        serde_json::from_str(&json).expect("the exec plan is an ExecPlan")
    }

    fn jail_tmp_of(&self, session_id: &str) -> String {
        self.session_dir(session_id)
            .join("sandbox/.work/tmp")
            .to_string_lossy()
            .into_owned()
    }

    fn claude_home(&self) -> String {
        self.tddy_home
            .join("homes/claude")
            .to_string_lossy()
            .into_owned()
    }
}

/// `TDDY_*` names in `names`.
fn tddy_names<'a>(names: impl IntoIterator<Item = &'a String>) -> BTreeSet<String> {
    names
        .into_iter()
        .filter(|name| name.starts_with("TDDY_"))
        .cloned()
        .collect()
}

fn the_proxy_env(report: &AStubReport) -> [String; 4] {
    [
        report.env("HTTPS_PROXY"),
        report.env("https_proxy"),
        report.env("HTTP_PROXY"),
        report.env("http_proxy"),
    ]
}

#[tokio::test]
async fn a_sandboxed_claude_cli_start_runs_a_stub_claude_whose_argv_home_path_proxy_env_mcp_config_and_credentials_match_the_spec(
) {
    // Given
    let daemon = a_daemon_with_a_claude_home(ADaemonSpec::default());

    // When
    let (session_id, _jail) = daemon.start(false).await;
    let report = daemon.stub_report(&session_id).await;

    // Then — Claude runs with its declared home, a PATH of its declared tools, and its credentials
    assert_eq!(
        (
            report.env("HOME"),
            report.path_bin(),
            report.credentials(),
            report.env("CLAUDE_CODE_TMPDIR"),
        ),
        (
            daemon.claude_home(),
            vec!["claude".to_string(), "tddy-tools".to_string()],
            CREDENTIALS.to_string(),
            daemon.jail_tmp_of(&session_id),
        )
    );
    // And — its proxy env is set in both casings, to the jail's loopback shim
    let proxies = the_proxy_env(&report);
    assert_eq!(
        proxies.clone(),
        std::array::from_fn(|_| proxies[0].clone()),
        "one shim in every casing"
    );
    assert!(
        proxies[0].starts_with("http://127.0.0.1:"),
        "the shim is on the jail's loopback: {}",
        proxies[0]
    );
    // And — the argv is the plan's, rendered from claude.yaml with its MCP config in the jail tmp
    let plan = daemon.exec_plan_of(&session_id);
    let mcp_config = format!("{}/claude-mcp-config.json", daemon.jail_tmp_of(&session_id));
    assert_eq!(
        (
            report.argv(),
            plan.argv[1..7].to_vec(),
            plan.argv[plan.argv.len() - 2..].to_vec(),
            report.mcp_config()["mcpServers"]
                .as_object()
                .map(|servers| servers.keys().cloned().collect::<Vec<_>>()),
        ),
        (
            plan.argv.clone(),
            vec![
                "--model".to_string(),
                TEST_MODEL.to_string(),
                "--session-id".to_string(),
                session_id.clone(),
                "--permission-mode".to_string(),
                "auto".to_string(),
            ],
            vec!["--mcp-config".to_string(), mcp_config],
            Some(vec!["tddy-tools".to_string()]),
        )
    );
}

#[tokio::test]
async fn the_agent_process_needs_no_tddy_sandbox_variable_and_sees_only_the_tddy_vars_its_declared_tddy_tools_server_needs(
) {
    // Given
    let daemon = a_daemon_with_a_claude_home(ADaemonSpec::default());

    // When
    let (session_id, _jail) = daemon.start(false).await;
    let report = daemon.stub_report(&session_id).await;

    // Then
    let server = report.mcp_config()["mcpServers"]["tddy-tools"].clone();
    let server_env: Vec<String> = server["env"]
        .as_object()
        .map(|env| env.keys().cloned().collect())
        .unwrap_or_default();
    let agent_names = report.env_names();
    let sandbox_names: BTreeSet<String> = agent_names
        .iter()
        .filter(|name| name.starts_with("TDDY_SANDBOX_"))
        .cloned()
        .collect();
    let tddy_tools = std::fs::canonicalize(claude_jail_support::tddy_tools_binary()).unwrap();
    assert_eq!(
        (
            server["command"].as_str().map(str::to_string),
            sandbox_names,
            tddy_names(&agent_names),
        ),
        (
            Some(tddy_tools.to_string_lossy().into_owned()),
            BTreeSet::new(),
            tddy_names(&server_env),
        )
    );
}

#[tokio::test]
async fn a_relaunch_uses_the_same_launcher_and_renders_the_same_spec() {
    // Given — a sandboxed session, as the launcher started it
    let daemon = a_daemon_with_a_claude_home(ADaemonSpec::default());
    let (session_id, _started) = daemon.start(false).await;
    daemon.stub_report(&session_id).await;
    let started = daemon.exec_plan_of(&session_id);

    // When
    let _relaunched = daemon.resume(&session_id).await;
    let report = daemon.stub_report(&session_id).await;

    // Then — the same plan, resuming the transcript rather than assigning its id
    let mut expected = started.clone();
    expected.argv[3] = "--resume".to_string();
    let relaunched = daemon.exec_plan_of(&session_id);
    assert_eq!(
        (relaunched.clone(), report.argv()),
        (expected, relaunched.argv)
    );
}

#[tokio::test]
async fn claude_in_a_jail_reaches_a_declared_host_through_the_shim_and_is_refused_an_undeclared_one(
) {
    // Given — claude declares only the first of two local stand-in hosts
    let declared = a_stub_http_endpoint_answering_ok().await;
    let undeclared = a_stub_http_endpoint_answering_ok().await;
    let declared_host = format!("127.0.0.1:{}", declared.port());
    let undeclared_host = format!("127.0.0.1:{}", undeclared.port());
    let daemon = a_daemon_with_a_claude_home(ADaemonSpec {
        probes: vec![declared_host.clone(), undeclared_host.clone()],
        claude_override: Some(claude_yaml_declaring_hosts(std::slice::from_ref(
            &declared_host,
        ))),
        ..ADaemonSpec::default()
    });

    // When
    let (session_id, _jail) = daemon.start(false).await;
    let report = daemon.stub_report(&session_id).await;

    // Then
    assert_eq!(
        (
            report.connect_status(&declared_host),
            report.connect_status(&undeclared_host),
        ),
        ("200".to_string(), "403".to_string())
    );
}

#[tokio::test]
async fn a_routed_session_reaches_its_base_url_host_and_carries_no_oauth_token_secret() {
    // Given — a gateway the route is rooted at, a host nobody declares, and an OAuth token the spec
    // delivers to unrouted sessions only
    let gateway = a_stub_http_endpoint_answering_ok().await;
    let undeclared = a_stub_http_endpoint_answering_ok().await;
    let gateway_host = format!("127.0.0.1:{}", gateway.port());
    let undeclared_host = format!("127.0.0.1:{}", undeclared.port());
    let secrets = tempfile::tempdir().unwrap();
    let token_file = secrets.path().join("oauth-token");
    std::fs::write(&token_file, "a-host-oauth-token").unwrap();
    let daemon = a_daemon_with_a_claude_home(ADaemonSpec {
        probes: vec![gateway_host.clone(), undeclared_host.clone()],
        claude_override: Some(with_the_oauth_token_from(
            claude_yaml_declaring_hosts(&[]),
            &token_file,
        )),
        route_base_url: Some(format!("http://{gateway_host}")),
        ..ADaemonSpec::default()
    });

    // When
    let (routed_id, _routed) = daemon.start(true).await;
    let routed = daemon.stub_report(&routed_id).await;
    let (unrouted_id, _unrouted) = daemon.start(false).await;
    let unrouted = daemon.stub_report(&unrouted_id).await;

    // Then — the route's host is reachable and the token absent; the unrouted session has it
    assert_eq!(
        (
            routed.connect_status(&gateway_host),
            routed.connect_status(&undeclared_host),
            routed.env_names().contains(OAUTH_TOKEN),
            unrouted.env_names().contains(OAUTH_TOKEN),
        ),
        ("200".to_string(), "403".to_string(), false, true)
    );
}

#[tokio::test]
async fn an_existing_sandbox_claude_home_is_migrated_to_homes_claude_once_with_its_credentials() {
    // Given — the home a recipe left at <tddyhome>/sandbox-claude-home, holding credentials
    let daemon = a_jailed_claude_daemon(ADaemonSpec {
        configured_claude_home: "sandbox-claude-home",
        ..ADaemonSpec::default()
    });
    let legacy = daemon.tddy_home.join("sandbox-claude-home");
    a_home_holding_credentials(&legacy, CREDENTIALS);

    // When — two sessions start, one after the other
    let (first_id, _first) = daemon.start(false).await;
    let first = daemon.stub_report(&first_id).await;
    let (second_id, _second) = daemon.start(false).await;
    let second = daemon.stub_report(&second_id).await;

    // Then
    assert_eq!(
        (
            first.env("HOME"),
            first.credentials(),
            second.env("HOME"),
            second.credentials(),
            legacy.exists(),
        ),
        (
            daemon.claude_home(),
            CREDENTIALS.to_string(),
            daemon.claude_home(),
            CREDENTIALS.to_string(),
            false,
        )
    );
}

#[tokio::test]
async fn the_runner_receives_no_claude_specific_flag_on_the_daemon_path() {
    // Given
    let daemon = a_daemon_with_a_claude_home(ADaemonSpec::default());

    // When
    let (session_id, _jail) = daemon.start(false).await;
    daemon.stub_report(&session_id).await;

    // Then
    let runner_argv = daemon.runner_argv(&session_id);
    let claude_flags: Vec<&str> = CLAUDE_RUNNER_FLAGS
        .iter()
        .copied()
        .filter(|flag| runner_argv.iter().any(|arg| arg == flag))
        .collect();
    assert_eq!(
        (
            claude_flags,
            runner_argv.iter().any(|arg| arg == "--exec-plan")
        ),
        (Vec::<&str>::new(), true),
        "runner argv: {runner_argv:?}"
    );
}
