//! Acceptance: `cursor-cli` sessions — a host start and resume, and a jailed start — launch from
//! the `cursor` tool definition (`tddy-coding-agent-specs/specs/cursor.yaml`) through the shared
//! launchers, and Cursor's hooks reach the daemon on the host and from inside the jail.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-cursor-cli-from-spec.md` (`#agent-tools 7/12`).
//!
//! The daemon is the real `DaemonSessionHost` given its coding agents as the daemon runtime gives
//! them (see `cursor_cli_support`); the stub `agent` reports on its terminal what it was started
//! with. The jailed tests run a real Seatbelt jail with the real `tddy-sandbox-runner`, so they are
//! macOS-only: CI (ubuntu) never runs the `jailed` module.

mod cursor_cli_support;

use cursor_cli_support::{
    a_cursor_daemon, ADaemonSpec, AnAgentToolSocket, RECORDED_CHAT_ID, RESUMED_SESSION_ID,
    STUB_CHAT_ID, TEST_MODEL,
};
use pretty_assertions::assert_eq;

#[tokio::test]
#[serial_test::serial]
async fn a_host_cursor_cli_start_and_resume_spawn_a_stub_agent_whose_argv_and_cwd_match_the_spec_and_register_no_mcp_server_in_the_users_home(
) {
    // Given
    let daemon = a_cursor_daemon(ADaemonSpec::default());
    let exited_worktree = tempfile::tempdir().unwrap();
    let exited_worktree_path = std::fs::canonicalize(exited_worktree.path()).unwrap();
    daemon.an_exited_host_session(&exited_worktree_path);

    // When
    let (session_id, _host) = daemon.start(false).await;
    let started = daemon.stub_report(&session_id).await;
    daemon.resume(RESUMED_SESSION_ID).await;
    let resumed = daemon.stub_report(RESUMED_SESSION_ID).await;

    // Then — the hand-built argv, rendered from cursor.yaml, in the session's checkout
    let agent = daemon.stub_bin.join("agent").to_string_lossy().into_owned();
    let argv = |chat: &str| {
        vec![
            agent.clone(),
            "--resume".to_string(),
            chat.to_string(),
            "--model".to_string(),
            TEST_MODEL.to_string(),
        ]
    };
    assert_eq!(
        (started.argv(), started.cwd(), resumed.argv(), resumed.cwd()),
        (
            argv(STUB_CHAT_ID),
            daemon.worktree_of(&session_id),
            argv(RECORDED_CHAT_ID),
            exited_worktree_path,
        )
    );
    // And — the hooks slot wrote its file into the checkout, while the user's own
    // `~/.cursor/mcp.json` was left alone: a host session registers no MCP server, as Claude's does
    assert_eq!(
        (
            daemon
                .worktree_of(&session_id)
                .join(".cursor/hooks.json")
                .is_file(),
            daemon.host_home.join(".cursor/mcp.json").exists(),
        ),
        (true, false)
    );
}

#[tokio::test]
#[serial_test::serial]
async fn on_an_embedded_daemon_a_host_cursor_sessions_hook_updates_its_activity_and_status() {
    // Given — a daemon serving its agent-tool socket, as Tddy Desktop's embedded daemon does
    let daemon = a_cursor_daemon(ADaemonSpec::default());
    let _socket = AnAgentToolSocket::serving_the_activity_of(&daemon);

    // When — the stub runs its `beforeSubmitPrompt` hook as Cursor does
    let (session_id, _host) = daemon.start(false).await;
    let report = daemon.stub_report(&session_id).await;

    // Then
    assert_eq!(
        (
            report.hook_exit(),
            daemon
                .activity_status_reaching(&session_id, "Running")
                .await,
        ),
        ("0".to_string(), Some("Running".to_string()))
    );
}

#[cfg(target_os = "macos")]
mod jailed {
    use std::collections::BTreeSet;
    use std::path::Path;

    use pretty_assertions::assert_eq;
    use tddy_agent_launch::jail_launch::EXEC_PLAN_FILE;
    use tddy_sandbox_runner::ExecPlan;
    use tddy_testing_commons::a_stub_http_endpoint_answering_ok;

    use super::cursor_cli_support::{
        a_cursor_daemon, the_builtin_cursor_yaml, ACursorDaemon, ADaemonSpec,
    };

    const CREDENTIALS: &str = r#"{"accessToken":"from-the-persistent-cursor-home"}"#;

    /// The runner flags only its Cursor agent mode reads.
    const CURSOR_RUNNER_FLAGS: &[&str] = &[
        "--agent-kind",
        "--agent-binary",
        "--agent-arg",
        "--cursor-home-dir",
    ];

    /// A daemon whose `<tddyhome>/homes/cursor` already holds [`CREDENTIALS`] as Cursor's login.
    fn a_daemon_with_a_cursor_home(spec: ADaemonSpec) -> ACursorDaemon {
        let daemon = a_cursor_daemon(spec);
        let auth = daemon.cursor_home().join(".cursor/auth.json");
        std::fs::create_dir_all(auth.parent().unwrap()).unwrap();
        std::fs::write(auth, CREDENTIALS).unwrap();
        daemon
    }

    /// `cursor.yaml` with `network.egress.hosts` set to `hosts`.
    fn cursor_yaml_declaring_hosts(hosts: &[String]) -> serde_yaml::Value {
        let mut cursor = the_builtin_cursor_yaml();
        let network: serde_yaml::Value = serde_yaml::from_str(&format!(
            "egress:\n  hosts: {}\n",
            // A JSON array is a YAML flow sequence.
            serde_json::to_string(hosts).unwrap()
        ))
        .unwrap();
        cursor
            .as_mapping_mut()
            .unwrap()
            .insert("network".into(), network);
        cursor
    }

    fn exec_plan_of(daemon: &ACursorDaemon, session_id: &str) -> ExecPlan {
        let path = daemon
            .session_dir(session_id)
            .join("sandbox")
            .join(EXEC_PLAN_FILE);
        let json = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("the jail launcher wrote {}: {e}", path.display()));
        serde_json::from_str(&json).expect("the exec plan is an ExecPlan")
    }

    fn mcp_server_names(config: &serde_json::Value) -> Option<BTreeSet<String>> {
        config["mcpServers"]
            .as_object()
            .map(|servers| servers.keys().cloned().collect())
    }

    #[tokio::test]
    async fn a_jailed_cursor_cli_start_runs_the_stub_with_the_persistent_cursor_home_credentials_and_keychain_skip_env_and_no_claude_overlay(
    ) {
        // Given
        let daemon = a_daemon_with_a_cursor_home(ADaemonSpec::default());

        // When
        let (session_id, _jail) = daemon.start(true).await;
        let report = daemon.stub_report(&session_id).await;

        // Then — Cursor's home, login, PATH and env overlay, and none of Claude's
        let jail_tmp = daemon.jail_tmp_of(&session_id);
        assert_eq!(
            (
                report.env("HOME"),
                report.credentials(),
                report.path_bin(),
                report.env("CURSOR_SKIP_KEYCHAIN"),
                report.env("CI"),
                report.env("CURSOR_TMPDIR"),
                report.env("CLAUDE_CODE_TMPDIR"),
                report.env("CLAUDE_TMPDIR"),
            ),
            (
                daemon.cursor_home().to_string_lossy().into_owned(),
                CREDENTIALS.to_string(),
                vec!["agent".to_string(), "tddy-tools".to_string()],
                "1".to_string(),
                "true".to_string(),
                jail_tmp,
                "UNSET".to_string(),
                "UNSET".to_string(),
            )
        );
        // And — the argv is the plan's, and tddy-tools is registered in the home's mcp.json
        let plan = exec_plan_of(&daemon, &session_id);
        assert_eq!(
            (
                report.argv(),
                plan.argv[1..].to_vec(),
                mcp_server_names(&report.mcp_config()),
            ),
            (
                plan.argv.clone(),
                vec![
                    "--resume".to_string(),
                    daemon
                        .metadata_of(&session_id)
                        .cursor_chat_id
                        .unwrap_or_default(),
                    "--model".to_string(),
                    super::TEST_MODEL.to_string(),
                ],
                Some(BTreeSet::from(["tddy-tools".to_string()])),
            )
        );
    }

    #[tokio::test]
    async fn cursors_jail_reaches_its_declared_hosts_and_is_refused_an_undeclared_one() {
        // Given — cursor declares only the first of two local stand-in hosts
        let declared = a_stub_http_endpoint_answering_ok().await;
        let undeclared = a_stub_http_endpoint_answering_ok().await;
        let declared_host = format!("127.0.0.1:{}", declared.port());
        let undeclared_host = format!("127.0.0.1:{}", undeclared.port());
        let daemon = a_daemon_with_a_cursor_home(ADaemonSpec {
            probes: vec![declared_host.clone(), undeclared_host.clone()],
            cursor_override: Some(cursor_yaml_declaring_hosts(std::slice::from_ref(
                &declared_host,
            ))),
        });

        // When
        let (session_id, _jail) = daemon.start(true).await;
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
    async fn the_runner_receives_no_cursor_specific_flag_on_the_daemon_path() {
        // Given
        let daemon = a_daemon_with_a_cursor_home(ADaemonSpec::default());

        // When
        let (session_id, _jail) = daemon.start(true).await;
        daemon.stub_report(&session_id).await;

        // Then
        let runner_argv = daemon.runner_argv(&session_id);
        let cursor_flags: Vec<&str> = CURSOR_RUNNER_FLAGS
            .iter()
            .copied()
            .filter(|flag| runner_argv.iter().any(|arg| arg == flag))
            .collect();
        assert_eq!(
            (
                cursor_flags,
                runner_argv.iter().any(|arg| arg == "--exec-plan")
            ),
            (Vec::<&str>::new(), true),
            "runner argv: {runner_argv:?}"
        );
    }

    #[tokio::test]
    async fn a_jailed_cursor_sessions_hook_reaches_the_daemon_through_the_bridge() {
        // Given — no agent-tool socket: the jail can reach the daemon only over its tool-IPC bridge
        let daemon = a_daemon_with_a_cursor_home(ADaemonSpec::default());

        // When — the stub runs its `beforeSubmitPrompt` hook inside the jail
        let (session_id, _jail) = daemon.start(true).await;
        let report = daemon.stub_report(&session_id).await;

        // Then
        let hooks = daemon.worktree_of(&session_id).join(".cursor/hooks.json");
        assert_eq!(
            (
                Path::new(&hooks).is_file(),
                report.hook_exit(),
                daemon
                    .activity_status_reaching(&session_id, "Running")
                    .await,
            ),
            (true, "0".to_string(), Some("Running".to_string()))
        );
    }
}
