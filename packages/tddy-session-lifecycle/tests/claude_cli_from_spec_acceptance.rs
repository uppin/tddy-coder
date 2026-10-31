//! Acceptance: an unsandboxed `claude-cli` session — a start and a Resume — is launched from the
//! `claude` tool definition (`tddy-coding-agent-specs/specs/claude.yaml`) through the spec-driven
//! host PTY launcher, and what reaches the process is what the hand-built launch produced.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-claude-cli-from-spec.md` (`#agent-tools 2/12`).
//!
//! The host is given its coding agents as the daemon runtime gives them: the agent registry loaded
//! from a `<tddyhome>/tools` directory, resolved against a fixture `ToolHost` whose `PATH` holds a
//! stub `claude` and a stub `tddy-tools` and nothing of the real machine. The stub `claude` records
//! its argv, environment and working directory into a directory beside it, prints a marker, and
//! stays alive on stdin.
//!
//! The golden files under `tests/fixtures/claude_cli_from_spec/` were captured from the hand-built
//! launch on `master` for the same session, with every per-run value (session id, paths, hook
//! token, OS user) replaced by a `<NAME>` token.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_actions::tool::ToolHost;
use tddy_agent_launch::spec_launch::HostCodingAgents;
use tddy_coding_agent_specs::{builtin_specs, load_agent_registry};
use tddy_core::session_metadata::{read_session_metadata, write_session_metadata, SessionMetadata};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::Request;
use tddy_service::proto::session::{
    ResumeSessionRequest, SessionService as SessionServiceTrait, StartSessionRequest,
};
use tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_testing_commons::wait::{a_capture_showing, PTY_STUB_OUTPUT};

const VALID_TOKEN: &str = "valid-token";
const TEST_MODEL: &str = "claude-opus-4-8";
const TEST_PROJECT_ID: &str = "claude-from-spec-project";
const RESUMED_SESSION_ID: &str = "01900000-0000-7000-8000-00000000c1a5";
/// What a stub agent prints once it has recorded its launch.
const LAUNCHED: &str = "STUB LAUNCHED";
/// The environment a golden pins: what the launch sets, never what the daemon happens to inherit.
const PINNED_ENV: &[&str] = &[
    "CLAUDE_CODE_NO_FLICKER",
    "TDDY_DAEMON_URL",
    "TDDY_HOOK_TOKEN",
    "TDDY_OS_USER",
    "TDDY_SESSION_ID",
];

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// The OS user the test process runs as — a real, resolvable user, so the spawn needs no
/// privilege drop.
fn current_os_user() -> String {
    let pw = unsafe { libc::getpwuid(libc::getuid()) };
    assert!(!pw.is_null(), "current uid must resolve to a passwd entry");
    unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) }
        .to_string_lossy()
        .into_owned()
}

fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A directory of stub binaries — the fixture host's whole `PATH` — and where the stub agents
/// record their launches.
struct AStubBin {
    dir: tempfile::TempDir,
}

fn a_stub_bin() -> AStubBin {
    let stubs = AStubBin {
        dir: tempfile::tempdir().unwrap(),
    };
    std::fs::create_dir(stubs.records()).unwrap();
    stubs.with_recording_agent("claude").with_tool("tddy-tools")
}

impl AStubBin {
    fn path(&self) -> PathBuf {
        std::fs::canonicalize(self.dir.path()).unwrap()
    }

    fn records(&self) -> PathBuf {
        self.dir.path().join("records")
    }

    fn binary(&self, name: &str) -> PathBuf {
        self.path().join(name)
    }

    /// A stub agent `name` that records its argv (one entry per line, `$0` first), its environment
    /// and its working directory, then prints [`LAUNCHED`] and stays alive on stdin.
    fn with_recording_agent(self, name: &str) -> Self {
        let script = self.dir.path().join(name);
        let records = self.records();
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$0\" \"$@\" > '{records}/argv'\nenv > '{records}/env'\npwd -P > '{records}/cwd'\necho '{LAUNCHED}'\ncat\n",
                records = records.display(),
            ),
        )
        .unwrap();
        make_executable(&script);
        self
    }

    /// A stub tool `name` that exits at once.
    fn with_tool(self, name: &str) -> Self {
        let script = self.dir.path().join(name);
        std::fs::write(&script, "#!/bin/sh\nexit 0\n").unwrap();
        make_executable(&script);
        self
    }

    /// What the last stub agent launched recorded.
    fn last_launch(&self) -> RecordedLaunch {
        let read = |file: &str| std::fs::read_to_string(self.records().join(file)).unwrap();
        RecordedLaunch {
            argv: read("argv").lines().map(str::to_string).collect(),
            env: read("env")
                .lines()
                .filter_map(|line| line.split_once('='))
                .map(|(name, value)| (name.to_string(), value.to_string()))
                .collect(),
            cwd: PathBuf::from(read("cwd").trim_end()),
        }
    }
}

/// What a stub agent was launched with.
#[derive(Debug)]
struct RecordedLaunch {
    argv: Vec<String>,
    env: BTreeMap<String, String>,
    cwd: PathBuf,
}

impl RecordedLaunch {
    fn pinned_env(&self) -> BTreeMap<String, String> {
        PINNED_ENV
            .iter()
            .filter_map(|name| self.env.get(*name).map(|v| (name.to_string(), v.clone())))
            .collect()
    }
}

/// A host with a declared environment and `PATH`, over the real filesystem — nothing of the real
/// machine's `PATH` or `TDDY_*_BINARY` variables reaches resolution.
struct AFixtureHost {
    path: Vec<PathBuf>,
    env: BTreeMap<String, String>,
}

impl ToolHost for AFixtureHost {
    fn env_var(&self, name: &str) -> Option<String> {
        self.env.get(name).cloned()
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

/// A `<tddyhome>` whose `tools/` directory a test may write overrides into.
struct ATddyHome {
    dir: tempfile::TempDir,
}

fn a_tddy_home() -> ATddyHome {
    let home = ATddyHome {
        dir: tempfile::tempdir().unwrap(),
    };
    std::fs::create_dir(home.tools()).unwrap();
    home
}

impl ATddyHome {
    fn tools(&self) -> PathBuf {
        self.dir.path().join("tools")
    }

    /// `<tddyhome>/tools/claude.yaml`: the built-in `claude.yaml` with `token` added to its argv
    /// right after `{binary}`.
    fn with_claude_override_adding(self, token: &str) -> Self {
        let builtin = builtin_specs()
            .into_iter()
            .find(|(path, _)| path.ends_with("claude.yaml"))
            .expect("claude.yaml is built in")
            .1;
        let binary_item = "    - \"{binary}\"\n";
        assert!(
            builtin.contains(binary_item),
            "the built-in argv starts with {{binary}}"
        );
        let overridden = builtin.replacen(binary_item, &format!("{binary_item}    - {token}\n"), 1);
        std::fs::write(self.tools().join("claude.yaml"), overridden).unwrap();
        self
    }

    /// The coding agents the daemon would install from this home, resolving against `stubs`.
    fn coding_agents(&self, stubs: &AStubBin) -> Arc<HostCodingAgents> {
        let registry = load_agent_registry(&self.tools()).expect("the agent registry loads");
        Arc::new(HostCodingAgents::new(
            registry,
            Arc::new(AFixtureHost {
                path: vec![stubs.path()],
                env: BTreeMap::new(),
            }),
        ))
    }
}

/// A daemon config naming the stub `tddy-tools`, and `claude_cli.binary_path` when given.
fn a_daemon_config(
    stubs: &AStubBin,
    binary_path: Option<&Path>,
) -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let user = current_os_user();
    let binary_line = binary_path
        .map(|path| format!("  binary_path: {}\n", path.display()))
        .unwrap_or_default();
    let yaml = format!(
        "users:\n  - github_user: \"{user}\"\n    os_user: \"{user}\"\nallowed_tools:\n  - path: /bin/true\n    label: true\nclaude_cli:\n  tddy_tools_path: {}\n{binary_line}",
        stubs.binary("tddy-tools").display()
    );
    let config_path = dir.path().join("daemon.yaml");
    std::fs::write(&config_path, yaml).unwrap();
    let config = DaemonConfig::load(&config_path).expect("config must parse");
    (dir, config)
}

/// A session host over `sessions_base`, given `coding_agents` when `Some`.
fn a_session_host(
    config: DaemonConfig,
    sessions_base: PathBuf,
    manager: Arc<ClaudeCliSessionManager>,
    coding_agents: Option<Arc<HostCodingAgents>>,
) -> Arc<DaemonSessionHost> {
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let resolved_user = current_os_user();
    let user_resolver: UserResolver = Arc::new(move |token| {
        if token == VALID_TOKEN {
            Some(resolved_user.clone())
        } else {
            None
        }
    });
    let host = DaemonSessionHost::new(
        config.clone(),
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        tddy_session_lifecycle::peer_routing::PeerRouting::local_only(&config),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        manager,
    );
    let host = match coding_agents {
        Some(agents) => host.with_coding_agents(agents),
        None => host,
    };
    let host = Arc::new(host);
    host.install_sandbox_rpc_bridge();
    host
}

/// A git repo with an `origin`, registered as the test project under `sessions_base/projects`.
fn a_registered_project(sessions_base: &Path) -> tempfile::TempDir {
    let repo_dir = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(repo_dir.path())
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "t@t.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "t@t.com")
            .output()
            .expect("git command failed");
    };
    run(&["init", "-b", "main"]);
    run(&["commit", "--allow-empty", "-m", "init"]);
    run(&["remote", "add", "origin", repo_dir.path().to_str().unwrap()]);
    run(&["push", "-u", "origin", "main"]);

    let projects_dir = sessions_base.join("projects");
    std::fs::create_dir_all(&projects_dir).unwrap();
    std::fs::write(
        projects_dir.join("projects.yaml"),
        format!(
            "projects:\n  - project_id: {TEST_PROJECT_ID}\n    name: {TEST_PROJECT_ID}\n    git_url: \"\"\n    main_repo_path: {}\n",
            repo_dir.path().display()
        ),
    )
    .unwrap();
    repo_dir
}

fn a_claude_cli_start_request() -> StartSessionRequest {
    StartSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        session_type: "claude-cli".to_string(),
        model: TEST_MODEL.to_string(),
        ..Default::default()
    }
}

/// An inactive host `claude-cli` session whose process has exited, ready to be resumed.
fn an_exited_host_session(sessions_base: &Path, worktree: &Path) {
    let session_dir = sessions_base.join("sessions").join(RESUMED_SESSION_ID);
    std::fs::create_dir_all(&session_dir).unwrap();
    let meta = SessionMetadata {
        session_id: RESUMED_SESSION_ID.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        created_at: "2026-10-31T08:00:00Z".to_string(),
        updated_at: "2026-10-31T08:05:00Z".to_string(),
        status: "inactive".to_string(),
        repo_path: Some(worktree.to_str().unwrap().to_string()),
        pid: None,
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("claude-cli".to_string()),
        model: Some(TEST_MODEL.to_string()),
        cursor_chat_id: None,
        harness_resume_token: None,
        activity_status: None,
        hook_token: None,
        sandbox: None,
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
    };
    write_session_metadata(&session_dir, &meta).unwrap();
}

/// Per-run values in a recorded launch, replaced by `<NAME>` tokens so it compares to a golden.
struct Normalizer {
    pairs: Vec<(String, &'static str)>,
}

impl Normalizer {
    /// Longest first, so a path is replaced before a prefix of it.
    fn new(mut pairs: Vec<(String, &'static str)>) -> Self {
        pairs.retain(|(value, _)| !value.is_empty());
        pairs.sort_by_key(|(value, _)| std::cmp::Reverse(value.len()));
        Self { pairs }
    }

    fn apply(&self, text: &str) -> String {
        self.pairs
            .iter()
            .fold(text.to_string(), |text, (value, token)| {
                text.replace(value, token)
            })
    }
}

fn golden(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/claude_cli_from_spec")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("golden {}: {e}", path.display()))
}

// ---------------------------------------------------------------------------
// Drivers
// ---------------------------------------------------------------------------

/// Start a host `claude-cli` session and wait until its agent has recorded its launch; the
/// session id.
async fn a_started_session(host: &DaemonSessionHost, manager: &ClaudeCliSessionManager) -> String {
    let session_id = host
        .start_session(Request::direct(a_claude_cli_start_request()))
        .await
        .expect("StartSession must succeed")
        .into_inner()
        .session_id;
    agent_launched(manager, &session_id).await;
    session_id
}

async fn agent_launched(manager: &ClaudeCliSessionManager, session_id: &str) {
    let handle = manager
        .get(session_id)
        .await
        .expect("the session must be registered in the shared manager");
    a_capture_showing(&handle.capture, LAUNCHED, PTY_STUB_OUTPUT).await;
}

/// The checkout a started session runs in, as `.session.yaml` records it.
fn worktree_of(sessions_base: &Path, session_id: &str) -> PathBuf {
    let meta = read_session_metadata(&sessions_base.join("sessions").join(session_id))
        .expect("the session records its metadata");
    std::fs::canonicalize(
        meta.repo_path
            .expect("a claude-cli session records its checkout"),
    )
    .unwrap()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial]
async fn an_unsandboxed_claude_cli_start_runs_the_spec_binary_with_the_argv_its_template_renders() {
    // Given — no daemon.yaml binary_path: the spec's binary, found on the fixture PATH, is used
    let sessions_tmp = tempfile::tempdir().unwrap();
    let _repo = a_registered_project(sessions_tmp.path());
    let stubs = a_stub_bin();
    let home = a_tddy_home();
    let (_cfg, config) = a_daemon_config(&stubs, None);
    let manager = Arc::new(ClaudeCliSessionManager::new());
    let host = a_session_host(
        config,
        sessions_tmp.path().to_path_buf(),
        Arc::clone(&manager),
        Some(home.coding_agents(&stubs)),
    );

    // When
    let session_id = a_started_session(&host, &manager).await;

    // Then
    let launch = stubs.last_launch();
    let sessions_base = std::fs::canonicalize(sessions_tmp.path()).unwrap();
    assert_eq!(
        launch.argv,
        vec![
            stubs.binary("claude").to_string_lossy().into_owned(),
            "--model".to_string(),
            TEST_MODEL.to_string(),
            "--session-id".to_string(),
            session_id.clone(),
            "--permission-mode".to_string(),
            "auto".to_string(),
            "--append-system-prompt-file".to_string(),
            sessions_base
                .join("sessions")
                .join(&session_id)
                .join("agent-system-prompt.md")
                .to_string_lossy()
                .into_owned(),
        ]
    );
    assert_eq!(launch.cwd, worktree_of(sessions_tmp.path(), &session_id));
}

#[tokio::test]
#[serial_test::serial]
async fn a_claude_cli_resume_renders_resume_and_the_session_id_from_the_same_template() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let worktree = tempfile::tempdir().unwrap();
    an_exited_host_session(sessions_tmp.path(), worktree.path());
    let stubs = a_stub_bin();
    let home = a_tddy_home();
    let (_cfg, config) = a_daemon_config(&stubs, None);
    let manager = Arc::new(ClaudeCliSessionManager::new());
    let host = a_session_host(
        config,
        sessions_tmp.path().to_path_buf(),
        Arc::clone(&manager),
        Some(home.coding_agents(&stubs)),
    );

    // When
    host.resume_session(Request::direct(ResumeSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        session_id: RESUMED_SESSION_ID.to_string(),
    }))
    .await
    .expect("ResumeSession must succeed for an exited claude-cli session");
    agent_launched(&manager, RESUMED_SESSION_ID).await;

    // Then
    let launch = stubs.last_launch();
    assert_eq!(
        launch.argv,
        vec![
            stubs.binary("claude").to_string_lossy().into_owned(),
            "--model".to_string(),
            TEST_MODEL.to_string(),
            "--resume".to_string(),
            RESUMED_SESSION_ID.to_string(),
            "--permission-mode".to_string(),
            "auto".to_string(),
        ]
    );
    assert_eq!(launch.cwd, std::fs::canonicalize(worktree.path()).unwrap());
}

#[tokio::test]
#[serial_test::serial]
async fn the_mcp_config_allow_deny_flags_permission_prompt_and_preamble_equal_todays_for_the_same_session(
) {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let _repo = a_registered_project(sessions_tmp.path());
    let stubs = a_stub_bin();
    let home = a_tddy_home();
    let (_cfg, config) = a_daemon_config(&stubs, None);
    let manager = Arc::new(ClaudeCliSessionManager::new());
    let host = a_session_host(
        config,
        sessions_tmp.path().to_path_buf(),
        Arc::clone(&manager),
        Some(home.coding_agents(&stubs)),
    );

    // When
    let session_id = a_started_session(&host, &manager).await;

    // Then — argv, the launch's own environment, and the hooks settings, as `master` produced them
    let launch = stubs.last_launch();
    let worktree = worktree_of(sessions_tmp.path(), &session_id);
    let meta = read_session_metadata(&sessions_tmp.path().join("sessions").join(&session_id))
        .expect("the session records its metadata");
    let normalize = Normalizer::new(vec![
        (session_id.clone(), "<SESSION_ID>"),
        (worktree.to_string_lossy().into_owned(), "<WORKTREE>"),
        (
            std::fs::canonicalize(sessions_tmp.path())
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            "<SESSIONS>",
        ),
        (
            sessions_tmp.path().to_string_lossy().into_owned(),
            "<SESSIONS>",
        ),
        (stubs.path().to_string_lossy().into_owned(), "<STUBS>"),
        (meta.hook_token.unwrap_or_default(), "<HOOK_TOKEN>"),
        (current_os_user(), "<OS_USER>"),
    ]);
    let settings = std::fs::read_to_string(worktree.join(".claude").join("settings.local.json"))
        .expect("the hooks settings are written into the worktree");
    let pinned_env = launch
        .pinned_env()
        .into_iter()
        .map(|(name, value)| format!("{name}={value}\n"))
        .collect::<String>();
    assert_eq!(
        (
            normalize.apply(&(launch.argv.join("\n") + "\n")),
            normalize.apply(&pinned_env),
            normalize.apply(&settings),
        ),
        (
            golden("host_start_argv.txt"),
            golden("host_start_env.txt"),
            golden("host_start_settings_local.json"),
        )
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_tddyhome_override_of_claude_yaml_adds_an_argv_token_that_reaches_the_process() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let _repo = a_registered_project(sessions_tmp.path());
    let stubs = a_stub_bin();
    let home = a_tddy_home().with_claude_override_adding("--verbose");
    let (_cfg, config) = a_daemon_config(&stubs, None);
    let manager = Arc::new(ClaudeCliSessionManager::new());
    let host = a_session_host(
        config,
        sessions_tmp.path().to_path_buf(),
        Arc::clone(&manager),
        Some(home.coding_agents(&stubs)),
    );

    // When
    a_started_session(&host, &manager).await;

    // Then
    let launch = stubs.last_launch();
    assert_eq!(
        launch.argv[..4].to_vec(),
        vec![
            stubs.binary("claude").to_string_lossy().into_owned(),
            "--verbose".to_string(),
            "--model".to_string(),
            TEST_MODEL.to_string(),
        ]
    );
}

#[tokio::test]
#[serial_test::serial]
async fn the_daemon_yaml_claude_binary_path_still_overrides_the_spec_binary() {
    // Given — a second stub agent, named only by daemon.yaml; the fixture PATH still has `claude`
    let sessions_tmp = tempfile::tempdir().unwrap();
    let _repo = a_registered_project(sessions_tmp.path());
    let stubs = a_stub_bin().with_recording_agent("claude-pinned");
    let home = a_tddy_home();
    let (_cfg, config) = a_daemon_config(&stubs, Some(&stubs.binary("claude-pinned")));
    let manager = Arc::new(ClaudeCliSessionManager::new());
    let host = a_session_host(
        config,
        sessions_tmp.path().to_path_buf(),
        Arc::clone(&manager),
        Some(home.coding_agents(&stubs)),
    );

    // When
    a_started_session(&host, &manager).await;

    // Then
    let launch = stubs.last_launch();
    assert_eq!(
        launch.argv[..3].to_vec(),
        vec![
            stubs.binary("claude-pinned").to_string_lossy().into_owned(),
            "--model".to_string(),
            TEST_MODEL.to_string(),
        ]
    );
}
