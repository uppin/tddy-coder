//! Acceptance (real jail): a sandboxed session's shell tab opened "In sandbox" runs in a real jail —
//! a second `tddy-sandbox-runner` over the session's worktree — that works in the worktree and
//! cannot write outside the session's grants.
//!
//! PRD: docs/ft/daemon/session-liveness.md (A sandboxed session's tabs) (`#session-liveness` 5/8)
//!
//! **Linux only**: the test runs where the jail is a user-namespace jail, and self-skips where
//! unprivileged user namespaces are unavailable, as every Linux jail suite does. CI runs none of
//! them until `docs/dev/todo/2026-11-02-ci-runs-no-linux-jail.md` is closed. The fixtures compile on
//! every host, so a macOS build still type-checks them; only the test is Linux's.
//!
//! The session is a sandboxed `cursor-cli` session, whose agent is a stub (the fixture of
//! `sandboxed_cursor_cli_acceptance`): it is the session's jail grants a tab jail is lowered from,
//! not the agent in its main.
#![cfg_attr(not(target_os = "linux"), allow(dead_code, unused_imports))]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use tddy_core::session_metadata::read_session_metadata;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::Request;
use tddy_service::proto::session::{SessionService as SessionServiceTrait, StartSessionRequest};
use tddy_session_lifecycle::claude_cli_session::CliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_terminal_rpc::bridge::DEFAULT_INITIAL_FRAME_BYTES;
use tddy_terminal_rpc::proto::terminal_session::{
    LaunchNode, StartTerminalSessionRequest, StreamReplayMode, StreamTerminalOutputRequest,
    TerminalSessionService as TerminalSessionServiceTrait,
};
use tddy_terminal_rpc::serve_stream_terminal_output_with;
use tddy_terminal_rpc::session::TerminalSessionStore;

const VALID_TOKEN: &str = "valid-token";
const TEST_MODEL: &str = "composer-2.5";
const TEST_PROJECT_ID: &str = "sandbox-shell-tab-project";

/// A safety net for a real jail to start and its shell to answer, not a prediction.
const JAIL_OBSERVE: Duration = Duration::from_secs(60);

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

#[cfg(target_os = "linux")]
#[tokio::test]
async fn a_jailed_shell_tab_works_in_the_worktree_and_cannot_write_outside_its_grants() {
    if !tddy_sandbox_cgroups::unprivileged_userns_available() {
        return;
    }

    // Given a sandboxed session, and a directory outside everything its jail is granted
    let daemon = a_daemon_with_a_sandboxed_session().await;
    let outside = tempfile::tempdir().unwrap();
    let forbidden = outside.path().join("written-from-the-jail.txt");

    // When a shell tab is opened "In sandbox" and asked to write inside its worktree and outside
    let tab = daemon.opens_a_jailed_shell().await;
    daemon
        .types_into(
            &tab,
            &format!(
                "pwd; echo in > in-worktree.txt && echo WROTE-$((1+1)); \
                 echo out > {} 2>/dev/null || echo REFUSED-$((2+2))\n",
                forbidden.display()
            ),
        )
        .await;

    // Then its shell runs in the session's worktree and may write there, and the write outside is
    // refused — the file is not on the host
    let streamed = daemon
        .until_streamed_output_contains(&tab, &["WROTE-2", "REFUSED-4"])
        .await;
    assert!(
        streamed.contains(&daemon.worktree.display().to_string()),
        "the shell's working directory is the worktree: {streamed}"
    );
    assert_eq!(
        std::fs::read_to_string(daemon.worktree.join("in-worktree.txt")).unwrap(),
        "in\n"
    );
    assert!(!forbidden.exists(), "the jail wrote outside its grants");
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

struct ADaemonWithASandboxedSession {
    host: Arc<DaemonSessionHost>,
    session_id: String,
    worktree: PathBuf,
    _dirs: Vec<tempfile::TempDir>,
}

/// A daemon over a started sandboxed `cursor-cli` session, its main jail running.
async fn a_daemon_with_a_sandboxed_session() -> ADaemonWithASandboxedSession {
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_dir = tempfile::tempdir().unwrap();
    register_project(&sessions_dir.path().join("projects"), repo_dir.path());
    let stub = a_stub_cursor_agent(repo_dir.path());
    let (config_dir, config) = a_daemon_config();
    let host = a_session_host(config, &stub, sessions_dir.path().to_path_buf());
    let session_id = host
        .start_session(Request::direct(a_sandboxed_cursor_start()))
        .await
        .expect("a sandboxed cursor-cli session starts")
        .into_inner()
        .session_id;
    let metadata = read_session_metadata(&sessions_dir.path().join("sessions").join(&session_id))
        .expect("the session is recorded");
    let worktree = PathBuf::from(
        metadata
            .repo_path
            .expect("the session records its worktree"),
    );
    ADaemonWithASandboxedSession {
        host,
        session_id,
        worktree,
        _dirs: vec![repo_dir, sessions_dir, config_dir],
    }
}

impl ADaemonWithASandboxedSession {
    async fn opens_a_jailed_shell(&self) -> String {
        self.host
            .terminal_session_service()
            .start_terminal_session(Request::direct(StartTerminalSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: self.session_id.clone(),
                control_token: String::new(),
                kind: "bash".to_string(),
                tmux_session: String::new(),
                wrappers: vec![LaunchNode {
                    kind: "sandbox".to_string(),
                    properties: Default::default(),
                }],
            }))
            .await
            .expect("a jailed shell tab starts")
            .into_inner()
            .terminal_id
    }

    async fn types_into(&self, terminal_id: &str, text: &str) {
        self.host
            .terminal_sessions()
            .get_terminal(&self.session_id, terminal_id)
            .await
            .unwrap_or_else(|| panic!("terminal {terminal_id} is not served"))
            .send_input(Bytes::from(text.to_string()), 0);
    }

    /// Stream terminal `terminal_id` from its oldest retained byte until it showed all of `wanted`;
    /// what it streamed by then.
    async fn until_streamed_output_contains(&self, terminal_id: &str, wanted: &[&str]) -> String {
        let mut frames = serve_stream_terminal_output_with(
            &self.host.terminal_sessions(),
            StreamTerminalOutputRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: self.session_id.clone(),
                terminal_id: terminal_id.to_string(),
                initial_cols: 200,
                initial_rows: 40,
                mode: StreamReplayMode::FromOffset as i32,
                from_offset: 0,
                control_token: String::new(),
            },
            DEFAULT_INITIAL_FRAME_BYTES,
        )
        .await
        .unwrap_or_else(|status| panic!("terminal {terminal_id} is not streamed: {status:?}"));
        let mut streamed = String::new();
        let read = tokio::time::timeout(JAIL_OBSERVE, async {
            while !wanted.iter().all(|text| streamed.contains(text)) {
                match frames.recv().await {
                    Some(Ok(frame)) => streamed.push_str(&String::from_utf8_lossy(&frame.data)),
                    Some(Err(status)) => panic!("the stream errored: {status:?}"),
                    None => panic!("the stream ended having streamed {streamed:?}"),
                }
            }
        })
        .await;
        assert!(
            read.is_ok(),
            "never streamed {wanted:?}; streamed {streamed:?}"
        );
        streamed
    }
}

fn a_sandboxed_cursor_start() -> StartSessionRequest {
    StartSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        session_type: "cursor-cli".to_string(),
        model: TEST_MODEL.to_string(),
        sandbox: true,
        ..Default::default()
    }
}

/// A stub `agent` that mints a chat when asked and otherwise waits on stdin.
fn a_stub_cursor_agent(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("stub_agent.sh");
    std::fs::write(
        &path,
        "#!/bin/sh\nif [ \"$1\" = \"create-chat\" ]; then \
         echo f8db82db-e154-41d0-ae72-312bdf6d4d80; exit 0; fi\ncat\n",
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn a_daemon_config() -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("daemon.yaml");
    std::fs::write(
        &config_path,
        "\nallowed_tools:\n  - path: /bin/true\n    label: true\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("users.yaml"),
        "- github_user: \"testuser\"\n  os_user: \"testuser\"\n",
    )
    .unwrap();
    let config = DaemonConfig::load(&config_path).expect("config must parse");
    (dir, config)
}

/// The `ToolHost` the daemon's coding agents resolve against: a `PATH` holding the real
/// `tddy-tools` and the stub `agent`, and a `$HOME` of the test's.
struct AFixtureHost {
    path: Vec<PathBuf>,
    home: PathBuf,
}

impl tddy_actions::tool::ToolHost for AFixtureHost {
    fn env_var(&self, name: &str) -> Option<String> {
        (name == "HOME").then(|| self.home.to_string_lossy().into_owned())
    }

    fn path_dirs(&self) -> Vec<PathBuf> {
        self.path.clone()
    }

    fn is_executable_file(&self, path: &Path) -> bool {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }

    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }
}

fn tddy_tools_binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_tddy-tools")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/tddy-tools")
        })
}

/// A daemon whose coding agents are bound as the daemon runtime binds them, with its sandbox RPC
/// bridge installed — and its tab jails started by its own launch, not a double.
fn a_session_host(
    config: DaemonConfig,
    stub: &Path,
    sessions_base: PathBuf,
) -> Arc<DaemonSessionHost> {
    let tools_dir = sessions_base.join("tools");
    std::fs::create_dir_all(&tools_dir).unwrap();
    let registry =
        tddy_coding_agent_specs::load_agent_registry(&tools_dir).expect("the agent registry loads");
    let tool_bin = sessions_base.join("tool-bin");
    std::fs::create_dir_all(&tool_bin).unwrap();
    std::os::unix::fs::symlink(tddy_tools_binary(), tool_bin.join("tddy-tools")).unwrap();
    std::os::unix::fs::symlink(stub, tool_bin.join("agent")).unwrap();
    let agents = Arc::new(tddy_agent_launch::spec_launch::HostCodingAgents::new(
        registry,
        Arc::new(AFixtureHost {
            path: vec![tool_bin],
            home: sessions_base.join("home"),
        }),
    ));
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver =
        Arc::new(|token| (token == VALID_TOKEN).then(|| "testuser".to_string()));
    let host = Arc::new(DaemonSessionHost::new(
        config.clone(),
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        tddy_session_lifecycle::peer_routing::PeerRouting::local_only(&config),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        Arc::new(CliSessionManager::new()),
        agents,
    ));
    host.install_sandbox_rpc_bridge();
    host
}

fn create_test_repo_with_origin(dir: &Path) {
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "t@t.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "t@t.com")
            .output()
            .expect("git runs");
    };
    run(&["init", "-b", "main"]);
    run(&["commit", "--allow-empty", "-m", "init"]);
    run(&["remote", "add", "origin", dir.to_str().unwrap()]);
    run(&["push", "-u", "origin", "main"]);
}

fn register_project(projects_dir: &Path, repo_path: &Path) {
    std::fs::create_dir_all(projects_dir).unwrap();
    std::fs::write(
        projects_dir.join("projects.yaml"),
        format!(
            "projects:\n  - project_id: {TEST_PROJECT_ID}\n    name: sandbox-shell-tab\n    \
             git_url: \"\"\n    main_repo_path: {}\n",
            repo_path.to_str().unwrap()
        ),
    )
    .unwrap();
}
