//! Coverage-first baseline: what a sandboxed `claude-cli` start and relaunch hand the jailed
//! `claude` **today** — its argv, the environment it relies on, the read-write mount of its home and
//! the files it reads.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-claude-cli-jail-from-spec.md` (`#agent-tools 5/12`).
//! Todo: `docs/dev/todo/2026-09-24-lifecycle-shared-sandboxed-jail-launch-needs-coverage-first.md`.
//!
//! These tests pass on the hand-built launch and must stay green when the shared jail launcher
//! (`tddy_agent_launch::jail_launch`) replaces it. The jail home is configured at
//! `<tddyhome>/homes/claude` — where the launcher puts Claude's persistent home — so the same
//! assertions hold on both sides of the replacement.
//!
//! The jail is a real Seatbelt jail, so this suite is macOS-only; CI (ubuntu) never runs it.
#![cfg(target_os = "macos")]

mod claude_jail_support;

use claude_jail_support::{a_jailed_claude_daemon, ADaemonSpec, AStubReport, TEST_MODEL};
use pretty_assertions::assert_eq;

const CREDENTIALS: &str = r#"{"claudeAiOauth":{"accessToken":"baseline"}}"#;

/// A daemon whose persistent Claude home is `<tddyhome>/homes/claude`, already holding
/// [`CREDENTIALS`].
fn a_daemon_with_a_home_holding_credentials() -> claude_jail_support::AJailedClaudeDaemon {
    let daemon = a_jailed_claude_daemon(ADaemonSpec {
        configured_claude_home: "homes/claude",
        coding_agents: false,
        ..ADaemonSpec::default()
    });
    let credentials = daemon
        .tddy_home
        .join("homes/claude/.claude/.credentials.json");
    std::fs::create_dir_all(credentials.parent().unwrap()).unwrap();
    std::fs::write(&credentials, CREDENTIALS).unwrap();
    daemon
}

/// The facts of one launch a replacement must keep: argv, home, temp dirs, proxy, credentials and
/// the registered MCP server.
fn the_launch_facts(report: &AStubReport) -> (Vec<String>, [String; 4], bool, String, String) {
    let proxies = [
        report.env("HTTPS_PROXY"),
        report.env("https_proxy"),
        report.env("HTTP_PROXY"),
        report.env("http_proxy"),
    ];
    let one_loopback_proxy = proxies.iter().all(|p| p == &proxies[0])
        && proxies[0]
            .strip_prefix("http://127.0.0.1:")
            .is_some_and(|port| port.parse::<u16>().is_ok());
    let mcp = report.mcp_config();
    (
        report.argv(),
        [
            report.env("HOME"),
            report.env("TMPDIR"),
            report.env("CLAUDE_CODE_TMPDIR"),
            report.env("CLAUDE_TMPDIR"),
        ],
        one_loopback_proxy,
        report.credentials(),
        mcp["mcpServers"]["tddy-tools"]["command"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    )
}

/// The `(subpath …)` entries of a Seatbelt profile's first `(allow file-write* …)` block — the
/// directories the jail may write.
fn the_writable_subpaths(profile: &str) -> Vec<String> {
    let block = profile
        .split("(allow file-write*\n")
        .nth(1)
        .expect("the profile allows some writes");
    block[..block.find("\n)").expect("the block closes")]
        .lines()
        .filter_map(|line| line.trim().strip_prefix("(subpath \""))
        .filter_map(|rest| rest.strip_suffix("\")"))
        .map(str::to_string)
        .collect()
}

#[tokio::test]
async fn todays_sandboxed_claude_start_records_argv_env_mounts_and_files() {
    // Given
    let daemon = a_daemon_with_a_home_holding_credentials();

    // When
    let (session_id, _jail) = daemon.start(false).await;
    let report = daemon.stub_report(&session_id).await;

    // Then
    let home = daemon.tddy_home.join("homes/claude");
    let tmp = daemon.session_dir(&session_id).join("sandbox/.work/tmp");
    let tmp = tmp.to_string_lossy().into_owned();
    let tddy_tools = std::fs::canonicalize(claude_jail_support::tddy_tools_binary()).unwrap();
    let mcp_config = format!("{tmp}/claude-mcp-config.json");
    let facts = the_launch_facts(&report);
    assert_eq!(
        (&facts.1, facts.2, facts.3.as_str(), facts.4.as_str()),
        (
            &[
                home.to_string_lossy().into_owned(),
                tmp.clone(),
                tmp.clone(),
                tmp.clone()
            ],
            true,
            CREDENTIALS,
            tddy_tools.to_str().unwrap(),
        )
    );
    assert_eq!(
        facts.0[..7].to_vec(),
        vec![
            daemon.stub_claude.to_string_lossy().into_owned(),
            "--model".to_string(),
            TEST_MODEL.to_string(),
            "--session-id".to_string(),
            session_id.clone(),
            "--permission-mode".to_string(),
            "auto".to_string(),
        ]
    );
    assert_eq!(
        facts.0[facts.0.len() - 2..].to_vec(),
        vec!["--mcp-config".to_string(), mcp_config]
    );
    let profile =
        std::fs::read_to_string(daemon.session_dir(&session_id).join("sandbox/sandbox.sb"))
            .expect("the jail's profile is written beside it");
    assert!(
        the_writable_subpaths(&profile).contains(&home.to_string_lossy().into_owned()),
        "the persistent home is mounted read-write:\n{profile}"
    );
}

#[tokio::test]
async fn todays_sandboxed_relaunch_records_the_same() {
    // Given — a sandboxed session, as its start launched it
    let daemon = a_daemon_with_a_home_holding_credentials();
    let (session_id, _started) = daemon.start(false).await;
    let started = the_launch_facts(&daemon.stub_report(&session_id).await);

    // When
    let _relaunched = daemon.resume(&session_id).await;
    let relaunched = the_launch_facts(&daemon.stub_report(&session_id).await);

    // Then — the same launch, resuming the transcript rather than assigning its id
    let mut resumed_argv = started.0.clone();
    resumed_argv[3] = "--resume".to_string();
    assert_eq!(
        relaunched,
        (resumed_argv, started.1, started.2, started.3, started.4)
    );
}
