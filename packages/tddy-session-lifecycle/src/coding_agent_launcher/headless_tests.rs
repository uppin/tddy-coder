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
async fn cursor_runs_its_resolved_cli() {
    // Given
    let data_dir = tempfile::tempdir().expect("data dir");
    let worktree = a_git_worktree();
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
    assert!(
        command.permission_prompt_tool.is_none(),
        "cursor has no permission-prompt tool to route approvals to"
    );
}

/// A git checkout to run a conversation's CLI in, as a clone-placed conversation's worktree is.
fn a_git_worktree() -> tempfile::TempDir {
    let worktree = tempfile::tempdir().expect("worktree");
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(worktree.path())
        .args(["init", "-q", "-b", "main"])
        .status()
        .expect("git init");
    assert!(status.success(), "git init");
    worktree
}

/// Whether git ignores `path` (relative to `worktree`).
fn git_ignores(worktree: &Path, path: &str) -> bool {
    std::process::Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["check-ignore", "-q", path])
        .status()
        .expect("git check-ignore")
        .success()
}

/// `path`'s JSON, and its permission bits.
fn written_json(path: &Path) -> (serde_json::Value, u32) {
    let mode = std::fs::metadata(path)
        .unwrap_or_else(|e| panic!("{} was written: {e}", path.display()))
        .permissions()
        .mode();
    let json = serde_json::from_str(&std::fs::read_to_string(path).expect("readable"))
        .expect("the config is JSON");
    (json, mode & 0o777)
}

/// The server entry is the launch's tool server: `tddy-tools --mcp`, its credential and
/// conversation in its environment.
fn assert_is_the_tool_server(server: &serde_json::Value) {
    assert_eq!(server["command"], TDDY_TOOLS_PATH, "{server}");
    assert_eq!(
        server["args"],
        serde_json::json!([TDDY_TOOLS_MCP_ARG]),
        "{server}"
    );
    assert_eq!(
        server["env"][SESSION_TOKEN_ENV_NAME], MINTED_TOKEN,
        "{server}"
    );
    assert_eq!(server["env"][CONVERSATION_ID_ENV], CONVERSATION, "{server}");
}

#[tokio::test]
async fn a_headless_cursor_is_handed_tddy_tools_through_its_worktrees_cursor_mcp_config() {
    // Given
    let data_dir = tempfile::tempdir().expect("data dir");
    let worktree = a_git_worktree();
    let launch = a_headless_launch("cursor", worktree.path());

    // When
    a_headless_launcher(data_dir.path(), None)
        .headless_command_for(&launch)
        .await
        .expect("the cursor CLI is settled");

    // Then cursor reads it from the project it runs in, in cursor's own shape, owner only
    let (written, mode) = written_json(&worktree.path().join(".cursor/mcp.json"));
    assert_is_the_tool_server(&written["mcpServers"][TDDY_TOOLS_SERVER]);
    assert_eq!(mode, 0o600, "owner only");
}

#[tokio::test]
async fn a_headless_cursors_mcp_config_is_never_committed_from_its_worktree() {
    // Given
    let data_dir = tempfile::tempdir().expect("data dir");
    let worktree = a_git_worktree();
    let launch = a_headless_launch("cursor", worktree.path());

    // When
    a_headless_launcher(data_dir.path(), None)
        .headless_command_for(&launch)
        .await
        .expect("the cursor CLI is settled");

    // Then the credential it holds is ignored by git, so the agent cannot commit it
    assert!(
        git_ignores(worktree.path(), ".cursor/mcp.json"),
        "the worktree's .cursor/mcp.json is ignored by git"
    );
}

#[tokio::test]
async fn a_headless_cursor_refuses_a_worktree_that_already_has_a_cursor_mcp_config() {
    // Given a worktree with its own .cursor/mcp.json
    let data_dir = tempfile::tempdir().expect("data dir");
    let worktree = a_git_worktree();
    std::fs::create_dir_all(worktree.path().join(".cursor")).expect(".cursor");
    std::fs::write(
        worktree.path().join(".cursor/mcp.json"),
        "{\"mcpServers\":{\"theirs\":{}}}",
    )
    .expect("the worktree's own config");
    let launch = a_headless_launch("cursor", worktree.path());

    // When
    let error = a_headless_launcher(data_dir.path(), None)
        .headless_command_for(&launch)
        .await
        .expect_err("the worktree's own MCP config is not overwritten");

    // Then
    assert!(error.to_string().contains(".cursor/mcp.json"), "{error}");
    assert_eq!(
        std::fs::read_to_string(worktree.path().join(".cursor/mcp.json")).expect("still there"),
        "{\"mcpServers\":{\"theirs\":{}}}"
    );
}

#[tokio::test]
async fn a_headless_oz_is_handed_tddy_tools_through_an_owner_only_mcp_file_in_its_own_shape() {
    // Given
    let data_dir = tempfile::tempdir().expect("data dir");
    let worktree = tempfile::tempdir().expect("worktree");
    let launch = a_headless_launch("oz", worktree.path());

    // When
    let command = a_headless_launcher(data_dir.path(), None)
        .headless_command_for(&launch)
        .await
        .expect("the oz CLI is settled");

    // Then oz is handed the file (`--mcp <path>`), servers keyed by name at the top level
    let path = command
        .mcp_config
        .clone()
        .expect("oz is handed an MCP file");
    assert_eq!(
        path,
        data_dir.path().join(CONVERSATION).join(MCP_CONFIG_FILE)
    );
    let (written, mode) = written_json(&path);
    assert!(written.get("mcpServers").is_none(), "no wrapper: {written}");
    assert_is_the_tool_server(&written[TDDY_TOOLS_SERVER]);
    assert_eq!(mode, 0o600, "owner only");
}

#[tokio::test]
async fn a_headless_codex_carries_tddy_tools_inline_with_its_environment_in_the_clis() {
    // Given
    let data_dir = tempfile::tempdir().expect("data dir");
    let worktree = tempfile::tempdir().expect("worktree");
    let launch = a_headless_launch("codex", worktree.path());

    // When
    let command = a_headless_launcher(data_dir.path(), None)
        .headless_command_for(&launch)
        .await
        .expect("the codex CLI is settled");

    // Then the server reaches codex inline, and its variables reach the CLI's process
    let server = command
        .mcp_server
        .clone()
        .expect("codex is handed its MCP server inline");
    assert_eq!(server.name, TDDY_TOOLS_SERVER);
    assert_eq!(server.command, PathBuf::from(TDDY_TOOLS_PATH));
    assert_eq!(server.args, vec![TDDY_TOOLS_MCP_ARG.to_string()]);
    assert!(
        command
            .env
            .contains(&(SESSION_TOKEN_ENV_NAME.to_string(), MINTED_TOKEN.to_string())),
        "{:?}",
        command.env
    );
    assert!(
        command
            .env
            .contains(&(CONVERSATION_ID_ENV.to_string(), CONVERSATION.to_string())),
        "{:?}",
        command.env
    );
}

#[tokio::test]
async fn a_headless_plan_whose_mcp_config_cannot_be_written_leaves_no_approval_route() {
    // Given a data directory where the conversation's files cannot go: a file sits at its path
    let data_dir = tempfile::tempdir().expect("data dir");
    std::fs::write(data_dir.path().join(CONVERSATION), "in the way").expect("a file in the way");
    let worktree = tempfile::tempdir().expect("worktree");
    let launch = a_headless_launch("claude", worktree.path());
    let routes = Arc::new(ApprovalRoutes::default());

    // When
    a_headless_launcher(data_dir.path(), None)
        .with_approval_routes(Arc::clone(&routes))
        .headless_command_for(&launch)
        .await
        .expect_err("an MCP config that cannot be written refuses the launch");

    // Then no route was left for the conversation
    let refusal = routes
        .request(
            CONVERSATION,
            SESSION_ID,
            MINTED_TOKEN,
            "Bash",
            &serde_json::json!({ "command": "cargo test" }),
        )
        .await
        .expect_err("no approval is answered for a launch that failed");
    assert!(
        refusal.contains("is not open on this daemon"),
        "the route was never left behind: {refusal}"
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
