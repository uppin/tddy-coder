//! Unit tests: what a claude-cli start launches its agent with, beyond the managed workflow's own.

use super::*;

fn the_accounts_git_pairs() -> Vec<(String, String)> {
    vec![
        ("GIT_AUTHOR_NAME".to_string(), "ada".to_string()),
        ("GIT_COMMITTER_NAME".to_string(), "ada".to_string()),
    ]
}

/// A start with no recipe and no index, so the only environment is the one under test.
async fn the_launch_environment_with(
    git_environment: Vec<(String, String)>,
) -> Vec<(String, String)> {
    let progress = tddy_session_files::attachment_progress::AttachmentProgressSink::discarding();
    let registry = TaskRegistry::new();
    let dir = tempfile::tempdir().expect("a temporary directory");
    let (_, _, env_extra) = managed_claude_cli_launch(ManagedClaudeCliLaunch {
        tddy_data_dir: dir.path(),
        session_id: "s1",
        managed_recipe: &None,
        child_spawn_handler: None,
        conversation_spawn_handler: None,
        semantic_index: false,
        progress: &progress,
        task_registry: &registry,
        session_dir: dir.path(),
        worktree_path: dir.path(),
        tddy_tools_path: String::new(),
        git_environment,
        github_credential_handler: None,
    })
    .await
    .expect("a launch with nothing to prepare succeeds");
    env_extra
}

#[tokio::test]
async fn the_resolved_accounts_pairs_are_in_the_agents_environment() {
    // Given a start whose project resolved to an account
    // When the launch environment is built
    let env = the_launch_environment_with(the_accounts_git_pairs()).await;

    // Then the agent is launched with exactly those pairs
    assert_eq!(env, the_accounts_git_pairs());
}

#[tokio::test]
async fn a_start_that_resolved_nothing_adds_no_git_variables() {
    // Given a start whose project resolved to no account
    // When the launch environment is built
    let env = the_launch_environment_with(Vec::new()).await;

    // Then the agent inherits the checkout's identity: nothing is added
    assert!(env.is_empty());
}

/// The loopback port a desktop daemon's GitHub sign-in returns on — and the only HTTP it answers.
const SIGN_IN_CALLBACK_PORT: u16 = 8899;

/// A daemon embedded in Tddy Desktop: it serves no HTTP RPC, and its agent-tool socket is bound.
///
/// The socket is bound for real and held, because "is this host embedded" is answered by whether
/// something is listening on that socket.
struct AnEmbeddedHost {
    config: DaemonConfig,
    socket_path: PathBuf,
    _listener: std::os::unix::net::UnixListener,
    data_dir: tempfile::TempDir,
}

impl AnEmbeddedHost {
    fn with_its_agent_tool_socket_bound() -> Self {
        let data_dir = tempfile::tempdir().expect("a temporary data dir");
        let socket_path = tddy_daemon_kernel::agent_tool_socket_path(data_dir.path());
        let listener = std::os::unix::net::UnixListener::bind(&socket_path)
            .expect("the agent-tool socket binds");
        let mut config = DaemonConfig::default();
        config.listen.web_port = Some(SIGN_IN_CALLBACK_PORT);
        Self {
            config,
            socket_path,
            _listener: listener,
            data_dir,
        }
    }

    fn data_dir(&self) -> &Path {
        self.data_dir.path()
    }

    fn agent_tool_socket(&self) -> String {
        self.socket_path.to_string_lossy().into_owned()
    }
}

impl Drop for AnEmbeddedHost {
    fn drop(&mut self) {
        // Binding leaves the path behind; it lives under $TMPDIR, not the temporary data dir.
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

/// A binary daemon (`tddy-daemon`): it serves HTTP RPC and never binds an agent-tool socket.
struct ARegularHost {
    config: DaemonConfig,
    data_dir: tempfile::TempDir,
}

impl ARegularHost {
    fn without_an_agent_tool_socket() -> Self {
        Self {
            config: DaemonConfig::default(),
            data_dir: tempfile::tempdir().expect("a temporary data dir"),
        }
    }

    fn data_dir(&self) -> &Path {
        self.data_dir.path()
    }
}

/// The `--daemon-socket` argument a hook command carries for `socket`: the flag, then the path
/// shell-quoted — the command is a shell string.
fn the_daemon_socket_argument_for(socket: &str) -> String {
    format!(
        " --daemon-socket {} ",
        tddy_core::claude_hooks::shell_quote(socket)
    )
}

/// The shell command Claude Code runs for `event`, as installed in `worktree`.
fn the_installed_hook_command(worktree: &Path, event: &str) -> String {
    let settings = std::fs::read_to_string(worktree.join(".claude/settings.local.json"))
        .expect("the hooks settings were written into the worktree");
    let settings: serde_json::Value =
        serde_json::from_str(&settings).expect("the hooks settings are JSON");
    settings["hooks"][event][0]["hooks"][0]["command"]
        .as_str()
        .expect("the event has a hook command")
        .to_string()
}

/// On Tddy Desktop the claude-cli hooks are pointed at `http://127.0.0.1:{listen.web_port}`, which
/// there is only the GitHub sign-in callback: every `ReportAgentActivity` and `ReportSessionStatus`
/// the hook POSTs is answered with an empty 404 that `tddy-tools session-hook` swallows. The session
/// never records an `agent-activity.jsonl`, so its Activity pane never appears and its status never
/// moves. The embedded daemon's agent-tool socket is the channel a co-located process has to it.
#[test]
fn on_an_embedded_host_the_tool_use_hook_reports_over_the_agent_tool_socket() {
    // Given a desktop daemon, whose agent-tool socket is bound
    let host = AnEmbeddedHost::with_its_agent_tool_socket_bound();
    let worktree = tempfile::tempdir().expect("a temporary worktree");

    // When a claude-cli session's hooks are installed into its worktree
    install_claude_cli_hooks(
        &host.config,
        host.data_dir(),
        "ada",
        "01a14a71-f790-7083-8ad8-e4df3feed64e",
        worktree.path(),
    );

    // Then the tool-use hook reaches the daemon over that socket
    let command = the_installed_hook_command(worktree.path(), "PostToolUse");
    let expected = the_daemon_socket_argument_for(&host.agent_tool_socket());
    assert!(
        command.contains(&expected),
        "the PostToolUse hook must report over the agent-tool socket ({expected:?}), got: {command}"
    );
}

/// A regular `tddy-daemon` serves HTTP RPC on `daemon_url` and binds no agent-tool socket, so its
/// hooks must not be told of one: a `--daemon-socket` there would send every report to a path
/// nobody serves.
#[test]
fn on_a_regular_host_the_tool_use_hook_names_no_daemon_socket() {
    // Given a binary daemon, whose agent-tool socket is not bound
    let host = ARegularHost::without_an_agent_tool_socket();
    let worktree = tempfile::tempdir().expect("a temporary worktree");

    // When a claude-cli session's hooks are installed into its worktree
    install_claude_cli_hooks(
        &host.config,
        host.data_dir(),
        "ada",
        "01a14a71-f790-7083-8ad8-e4df3feed64e",
        worktree.path(),
    );

    // Then the tool-use hook reports over HTTP alone
    let command = the_installed_hook_command(worktree.path(), "PostToolUse");
    assert!(
        !command.contains("--daemon-socket"),
        "a regular host's PostToolUse hook must name no daemon socket, got: {command}"
    );
}
