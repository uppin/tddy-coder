//! The headless command the daemon's launcher settles for a headless coding agent: the CLI it runs,
//! the route it runs on, the MCP config a claude conversation reads, and the refusals that keep a
//! launch from running weaker than asked.

use std::os::unix::fs::PermissionsExt;

use super::*;
use tddy_coding_subagent::CodingAgentDef;
use tddy_core::provider_route::{RouteFile, RouteSecret};

/// The model every headless launch in these tests runs.
const MODEL: &str = "sonnet";
/// Where the launches' tool server resolves `tddy-tools` to.
const TDDY_TOOLS_PATH: &str = "/opt/tddy/bin/tddy-tools";
/// The credential the tool server mints for the agent. It must reach the MCP config, and nowhere else
/// these tests look.
const MINTED_TOKEN: &str = "minted-agent-token";
/// The environment variable the minted credential is handed to `tddy-tools` in.
const SESSION_TOKEN_ENV_NAME: &str = "TDDY_REMOTE_SESSION_TOKEN";
/// The session every launch serves.
const SESSION_ID: &str = "1780828020298-coding";
/// The conversation every launch serves.
const CONVERSATION: &str = "conv-1";

fn a_daemon_config() -> DaemonConfig {
    serde_yaml::from_str(
        "users:\n  - github_user: u\n    os_user: u\nclaude_cli:\n  binary_path: /opt/tddy/bin/claude\n",
    )
    .expect("daemon config should parse")
}

fn this_user() -> String {
    tddy_session_activity::user_sessions_path::username_for_uid(unsafe { libc::geteuid() })
        .expect("the test user has a passwd entry")
}

fn the_daemons_own_identity() -> DropContext {
    let (uid, gid) = unsafe { (libc::geteuid(), libc::getegid()) };
    DropContext {
        uid,
        gid,
        privileged: false,
        setpriv: None,
    }
}

/// A headless launch of `harness` on the model every test runs, for a conversation of its own.
fn a_headless_launch(harness: &str, worktree: &Path) -> CodingAgentLaunch {
    CodingAgentLaunch {
        def: CodingAgentDef {
            name: "plain-agent".to_string(),
            label: None,
            harness: harness.to_string(),
            model: MODEL.to_string(),
            provider_account: None,
        },
        session_id: SESSION_ID.to_string(),
        session_os_user: this_user(),
        worktree: worktree.to_path_buf(),
        managed: false,
        session_token: "caller-session-token".to_string(),
        conversation_id: Some(CONVERSATION.to_string()),
    }
}

/// A launcher wired for headless conversations, keeping their MCP configs under `data_dir`, whose
/// provider route resolves to `route`.
fn a_headless_launcher(data_dir: &Path, route: Option<HarnessRoute>) -> DaemonCodingAgentLauncher {
    DaemonCodingAgentLauncher::with_identity(
        the_daemons_own_identity(),
        Arc::new(move |_| Ok(route.clone())),
        Arc::new(|launch| {
            Ok((
                PathBuf::from(TDDY_TOOLS_PATH),
                vec![
                    (SESSION_TOKEN_ENV_NAME.to_string(), MINTED_TOKEN.to_string()),
                    (
                        "TDDY_REMOTE_SESSION_ID".to_string(),
                        launch.session_id.clone(),
                    ),
                ],
            ))
        }),
    )
    .with_headless_wiring(a_daemon_config(), data_dir.to_path_buf())
}

/// The names in `dir`, empty when nothing was written there.
fn entries_in(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .map(|entry| {
                    entry
                        .expect("entry")
                        .file_name()
                        .to_string_lossy()
                        .into_owned()
                })
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn a_headless_claude_runs_the_cli_the_daemon_resolves_on_its_worktree_and_model() {
    // Given
    let data_dir = tempfile::tempdir().expect("data dir");
    let worktree = tempfile::tempdir().expect("worktree");
    let launch = a_headless_launch("claude", worktree.path());

    // When
    let command = a_headless_launcher(data_dir.path(), None)
        .headless_command_for(&launch)
        .await
        .expect("the claude CLI is settled");

    // Then
    assert_eq!(
        command.program,
        PathBuf::from(resolve_claude_binary_path(&a_daemon_config()))
    );
    assert_eq!(command.cwd.as_deref(), Some(worktree.path()));
    assert_eq!(command.model.as_deref(), Some(MODEL));
}

#[tokio::test]
async fn a_headless_claude_is_handed_tddy_tools_through_an_owner_only_mcp_config() {
    // Given
    let data_dir = tempfile::tempdir().expect("data dir");
    let worktree = tempfile::tempdir().expect("worktree");
    let launch = a_headless_launch("claude", worktree.path());

    // When
    let command = a_headless_launcher(data_dir.path(), None)
        .headless_command_for(&launch)
        .await
        .expect("the claude CLI is settled");

    // Then
    let config_path = command
        .mcp_config
        .clone()
        .expect("claude reads an MCP config");
    assert_eq!(
        config_path,
        data_dir.path().join(CONVERSATION).join(MCP_CONFIG_FILE)
    );
    assert!(
        !config_path.starts_with(worktree.path()),
        "the credential stays out of the worktree: {config_path:?}"
    );
    let mode = std::fs::metadata(&config_path)
        .expect("the config was written")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600, "owner only");
    let written: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&config_path).expect("the config is readable"),
    )
    .expect("the config is JSON");
    let server = &written["mcpServers"][TDDY_TOOLS_SERVER];
    assert_eq!(server["command"], TDDY_TOOLS_PATH);
    assert_eq!(server["args"], serde_json::json!([TDDY_TOOLS_MCP_ARG]));
    assert_eq!(server["env"][SESSION_TOKEN_ENV_NAME], MINTED_TOKEN);
    assert_eq!(server["env"][CONVERSATION_ID_ENV], CONVERSATION);
    assert_eq!(
        command.permission_prompt_tool.as_deref(),
        Some("mcp__tddy-tools__approval_prompt")
    );
}

#[tokio::test]
async fn a_headless_conversation_without_an_id_is_refused_naming_the_harness() {
    // Given
    let data_dir = tempfile::tempdir().expect("data dir");
    let worktree = tempfile::tempdir().expect("worktree");
    let mut launch = a_headless_launch("claude", worktree.path());
    launch.conversation_id = None;

    // When
    let error = a_headless_launcher(data_dir.path(), None)
        .headless_command_for(&launch)
        .await
        .expect_err("a headless conversation names no id it was not given");

    // Then
    let message = error.to_string();
    assert!(
        message.contains("(claude)") && message.contains("conversation id"),
        "{message}"
    );
    assert!(
        entries_in(data_dir.path()).is_empty(),
        "nothing was written"
    );
}

#[tokio::test]
async fn a_managed_headless_claude_is_refused_as_it_always_was() {
    // Given
    let data_dir = tempfile::tempdir().expect("data dir");
    let worktree = tempfile::tempdir().expect("worktree");
    let mut launch = a_headless_launch("claude", worktree.path());
    launch.managed = true;

    // When
    let error = a_headless_launcher(data_dir.path(), None)
        .headless_command_for(&launch)
        .await
        .expect_err("a managed session keeps refusing a headless harness");

    // Then
    let message = error.to_string();
    assert!(message.contains("(claude)"), "{message}");
    assert!(
        entries_in(data_dir.path()).is_empty(),
        "nothing was written"
    );
}

#[tokio::test]
async fn a_headless_launch_for_another_os_user_is_refused_before_anything_is_written() {
    // Given
    let data_dir = tempfile::tempdir().expect("data dir");
    let worktree = tempfile::tempdir().expect("worktree");
    let mut launch = a_headless_launch("claude", worktree.path());
    launch.session_os_user = "no-such-user-for-headless-tests".to_string();

    // When
    let error = a_headless_launcher(data_dir.path(), None)
        .headless_command_for(&launch)
        .await
        .expect_err("a headless CLI does not run as another user");

    // Then
    assert!(error.to_string().contains("(claude)"), "{error}");
    assert!(
        entries_in(data_dir.path()).is_empty(),
        "nothing was written"
    );
}

#[tokio::test]
async fn a_headless_route_passes_its_environment_and_secret_to_the_cli() {
    // Given
    let data_dir = tempfile::tempdir().expect("data dir");
    let worktree = tempfile::tempdir().expect("worktree");
    let mut launch = a_headless_launch("claude", worktree.path());
    launch.def.provider_account = Some("meta".to_string());
    let route = HarnessRoute {
        env: vec![(
            "ANTHROPIC_BASE_URL".to_string(),
            "https://llm.example.internal".to_string(),
        )],
        secret_env: vec![(
            "ANTHROPIC_AUTH_TOKEN".to_string(),
            RouteSecret::new("sk-test-route-secret"),
        )],
        files: Vec::new(),
        args: vec!["--model".to_string(), MODEL.to_string()],
    };

    // When
    let command = a_headless_launcher(data_dir.path(), Some(route))
        .headless_command_for(&launch)
        .await
        .expect("the routed claude CLI is settled");

    // Then
    assert!(command.env.contains(&(
        "ANTHROPIC_BASE_URL".to_string(),
        "https://llm.example.internal".to_string()
    )));
    assert!(command.env.contains(&(
        "ANTHROPIC_AUTH_TOKEN".to_string(),
        "sk-test-route-secret".to_string()
    )));
}

#[tokio::test]
async fn a_route_that_needs_a_configuration_file_is_refused_for_a_headless_cli() {
    // Given
    let data_dir = tempfile::tempdir().expect("data dir");
    let worktree = tempfile::tempdir().expect("worktree");
    let mut launch = a_headless_launch("claude", worktree.path());
    launch.def.provider_account = Some("meta".to_string());
    let route = HarnessRoute {
        env: Vec::new(),
        secret_env: Vec::new(),
        files: vec![RouteFile {
            name: "opencode.json".to_string(),
            contents: "{}".to_string(),
        }],
        args: Vec::new(),
    };

    // When
    let error = a_headless_launcher(data_dir.path(), Some(route))
        .headless_command_for(&launch)
        .await
        .expect_err("a headless CLI reads no route file");

    // Then
    assert!(error.to_string().contains("configuration file"), "{error}");
    assert!(
        entries_in(data_dir.path()).is_empty(),
        "nothing was written"
    );
}

#[tokio::test]
async fn a_launcher_not_wired_for_headless_conversations_refuses_one() {
    // Given
    let worktree = tempfile::tempdir().expect("worktree");
    let launch = a_headless_launch("claude", worktree.path());
    let launcher = DaemonCodingAgentLauncher::with_identity(
        the_daemons_own_identity(),
        Arc::new(|_| Ok(None)),
        Arc::new(|_| Err(SubagentError::from("the tool server is not reached"))),
    );

    // When
    let error = launcher
        .headless_command_for(&launch)
        .await
        .expect_err("no headless wiring, no headless command");

    // Then
    assert!(
        error
            .to_string()
            .contains("no headless coding-agent wiring"),
        "{error}"
    );
}

#[tokio::test]
async fn cursor_runs_its_resolved_cli_and_takes_no_mcp_config_yet() {
    // Given
    let data_dir = tempfile::tempdir().expect("data dir");
    let worktree = tempfile::tempdir().expect("worktree");
    let launch = a_headless_launch("cursor", worktree.path());

    // When
    let command = a_headless_launcher(data_dir.path(), None)
        .headless_command_for(&launch)
        .await
        .expect("the cursor CLI is settled");

    // Then
    assert_eq!(
        command.program,
        PathBuf::from(resolve_cursor_binary_path(&a_daemon_config()))
    );
    assert!(command.mcp_config.is_none());
    assert!(command.permission_prompt_tool.is_none());
    assert!(
        entries_in(data_dir.path()).is_empty(),
        "no MCP config written"
    );
}

#[tokio::test]
async fn codex_and_oz_run_by_name_from_the_path() {
    // Given
    let data_dir = tempfile::tempdir().expect("data dir");
    let worktree = tempfile::tempdir().expect("worktree");
    let launcher = a_headless_launcher(data_dir.path(), None);

    // When
    let codex = launcher
        .headless_command_for(&a_headless_launch("codex", worktree.path()))
        .await
        .expect("the codex CLI is settled");
    let oz = launcher
        .headless_command_for(&a_headless_launch("oz", worktree.path()))
        .await
        .expect("the oz CLI is settled");

    // Then
    assert_eq!(codex.program, PathBuf::from("codex"));
    assert_eq!(oz.program, PathBuf::from("oz"));
}
