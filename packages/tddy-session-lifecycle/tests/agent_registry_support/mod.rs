//! The fixture `yaml_only_agent_acceptance` builds its daemon from (`#agent-tools 12/12`): a real
//! git repo with `origin/main` registered as a project, a `<tddyhome>/tools` holding the tool files
//! a test drops in, a stub `acme` binary that reports what it was started with on its terminal, and
//! a real `DaemonSessionHost` given its coding agents as the daemon runtime gives them — the
//! coding-agent registry over `<tddyhome>/tools`, resolved against a fixture host whose `PATH` is
//! the stub and the real `tddy-tools`. The daemon.yaml carries no agent section at all.
//!
//! The stub touches a spawn marker, then reports one `STUB <key>=<value>` line per observation
//! (argv joined with `|`, then cwd), then `STUB-END`, and waits on stdin so the session stays up.
//!
//! The shape follows `harness_clis_support` (`#agent-tools 8/12`), cut to what a YAML-only agent
//! needs: no vault, no provider accounts, no resume.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use tddy_actions::tool::ToolHost;
use tddy_agent_launch::spec_launch::HostCodingAgents;
use tddy_coding_agent_specs::CodingAgentRegistry;
use tddy_core::session_metadata::{read_session_metadata, write_session_metadata, SessionMetadata};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::Request;
use tddy_service::proto::activity::{ActivityService as _, ReportSessionStatusRequest};
use tddy_service::proto::session::{SessionService as SessionServiceTrait, StartSessionRequest};
use tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_terminal_rpc::proto::terminal_session::{
    StreamReplayMode, StreamTerminalOutputRequest,
    TerminalSessionService as TerminalSessionServiceTrait,
};

pub const VALID_TOKEN: &str = "valid-token";
pub const TEST_PROJECT_ID: &str = "yaml-only-agent-project";
pub const HOOK_TOKEN: &str = "a-per-session-hook-token";
/// The model a start chooses.
pub const ACME_MODEL: &str = "acme-1";

/// How long a stub has to report on its terminal.
const TERMINAL_OBSERVE: Duration = Duration::from_secs(90);

/// `acme`: an agent no Rust names, declared in this one file. Its session type is `acme-cli`; it
/// runs on the host or in a jail, writes Claude's hook settings through the existing
/// `claude-settings-local` adapter, and declares the hooks and requires-model capabilities.
pub const ACME_YAML: &str = r#"schema_version: 1
id: acme
summary: Acme Code — an agent added as a tool file alone
binary:
  command: acme
  env: TDDY_ACME_CLI
deps: [tddy-tools]
reads: []
run:
  transport: pty
  argv:
    - "{binary}"
    - { if: model, then: ["--model", "{model}"] }
    - { if: prompt, then: ["{prompt}"] }
  env: {}
extensions:
  coding_agent:
    mcp: { kind: unsupported }
    tool_allow: unsupported
    tool_deny: unsupported
    permission_prompt: unsupported
    instructions:
      files: [AGENTS.md]
    hooks: { adapter: claude-settings-local }
    capabilities: [hooks, requires_model]
    models:
      default: acme-1
      curated: [acme-1]
    placements: [host, jail]
home: { kind: session }
policy:
  jit: false
  fork: true
  sysctl_read: true
  exec_paths: [/usr/bin, /usr/libexec, /bin, /sbin, /System, /Library]
"#;

/// The real `tddy-tools`, built beside this test binary.
pub fn tddy_tools_binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_tddy-tools")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/tddy-tools")
        })
}

/// The OS user the test process runs as — a real, resolvable user, so a host spawn needs no
/// privilege drop.
pub fn current_os_user() -> String {
    let pw = unsafe { libc::getpwuid(libc::getuid()) };
    assert!(!pw.is_null(), "current uid must resolve to a passwd entry");
    unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) }
        .to_string_lossy()
        .into_owned()
}

/// A stub agent: touches `marker`, reports its argv and cwd, then waits on stdin. Every external
/// command is named by its absolute path, so the report does not depend on `PATH`.
pub fn a_stub_agent_script(marker: &Path) -> String {
    format!(
        r#"#!/bin/sh
/usr/bin/touch "{marker}" 2>/dev/null
printf 'STUB argv=%s' "$0"; for a in "$@"; do printf '|%s' "$a"; done; printf '\n'
printf 'STUB cwd=%s\n' "$(/bin/pwd -P)"
printf 'STUB-END\n'
exec /bin/cat
"#,
        marker = marker.display(),
    )
}

/// What a stub agent reported on its terminal.
#[derive(Debug, Clone)]
pub struct AStubReport {
    lines: BTreeMap<String, String>,
}

impl AStubReport {
    fn parse(terminal: &str) -> Self {
        let lines = terminal
            .split(['\r', '\n'])
            .filter_map(|line| line.trim_start().strip_prefix("STUB "))
            .filter_map(|line| line.split_once('='))
            .map(|(key, value)| (key.to_string(), value.trim_end().to_string()))
            .collect();
        Self { lines }
    }

    pub fn argv(&self) -> Vec<String> {
        self.lines
            .get("argv")
            .unwrap_or_else(|| panic!("the stub reported its argv; it reported {:?}", self.lines))
            .split('|')
            .map(str::to_string)
            .collect()
    }
}

/// Terminates a started session's jail when dropped.
pub struct AJailGuard(pub Option<u32>);

impl Drop for AJailGuard {
    fn drop(&mut self) {
        if let Some(pid) = self.0 {
            tddy_daemon_sandbox::sandbox_session::terminate_sandbox_process(pid);
        }
    }
}

/// A host with a declared `PATH` and `$HOME` over the real filesystem — nothing of the real
/// machine's `PATH`, `$HOME` or `TDDY_*` variables reaches resolution.
struct AFixtureHost {
    path: Vec<PathBuf>,
    home: PathBuf,
}

impl ToolHost for AFixtureHost {
    fn env_var(&self, name: &str) -> Option<String> {
        (name == "HOME").then(|| self.home.to_string_lossy().into_owned())
    }

    fn path_dirs(&self) -> Vec<PathBuf> {
        self.path.clone()
    }

    fn is_executable_file(&self, path: &Path) -> bool {
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }

    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }
}

/// A daemon whose `<tddyhome>/tools` holds the files a test dropped in.
pub struct ARegistryDaemon {
    pub service: Arc<DaemonSessionHost>,
    /// The registry the daemon was given, as the daemon runtime loads it.
    pub registry: CodingAgentRegistry,
    /// `<tddyhome>` — the daemon's data dir and sessions base — canonical.
    pub tddy_home: PathBuf,
    pub stub_bin: PathBuf,
    /// Touched by the stub when it is spawned.
    pub spawn_marker: PathBuf,
    _repo_dir: tempfile::TempDir,
    _tddy_home_dir: tempfile::TempDir,
    _host_home_dir: tempfile::TempDir,
    _stub_dir: tempfile::TempDir,
    _marker_dir: tempfile::TempDir,
    _config_dir: tempfile::TempDir,
}

/// A daemon whose `<tddyhome>/tools` holds `tools` (`(file name, YAML)`), with the stub installed
/// as `acme`.
pub fn a_daemon_with_tools(tools: &[(&str, &str)]) -> ARegistryDaemon {
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let tddy_home_dir = tempfile::tempdir().unwrap();
    let tddy_home = std::fs::canonicalize(tddy_home_dir.path()).unwrap();
    register_project(&tddy_home.join("projects"), repo_dir.path());
    let tools_dir = tddy_home.join("tools");
    std::fs::create_dir_all(&tools_dir).unwrap();
    for (name, yaml) in tools {
        std::fs::write(tools_dir.join(name), yaml).unwrap();
    }
    let host_home_dir = tempfile::tempdir().unwrap();
    let host_home = std::fs::canonicalize(host_home_dir.path()).unwrap();

    let marker_dir = tempfile::tempdir().unwrap();
    let spawn_marker = marker_dir.path().join("spawned");
    let stub_dir = tempfile::tempdir().unwrap();
    let stub_bin = std::fs::canonicalize(stub_dir.path()).unwrap();
    let stub = stub_bin.join("acme");
    std::fs::write(&stub, a_stub_agent_script(&spawn_marker)).unwrap();
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::os::unix::fs::symlink(tddy_tools_binary(), stub_bin.join("tddy-tools")).unwrap();

    let (config_dir, config) = a_daemon_config_without_agent_sections();
    let registry = CodingAgentRegistry::load(&tools_dir).expect("the coding-agent registry loads");
    let host = a_session_host(config, tddy_home.clone()).with_coding_agents(Arc::new(
        HostCodingAgents::new(
            registry.tools().clone(),
            Arc::new(AFixtureHost {
                path: vec![stub_bin.clone()],
                home: host_home.clone(),
            }),
        ),
    ));
    let service = Arc::new(host);
    service.install_sandbox_rpc_bridge();
    ARegistryDaemon {
        service,
        registry,
        tddy_home,
        stub_bin,
        spawn_marker,
        _repo_dir: repo_dir,
        _tddy_home_dir: tddy_home_dir,
        _host_home_dir: host_home_dir,
        _stub_dir: stub_dir,
        _marker_dir: marker_dir,
        _config_dir: config_dir,
    }
}

impl ARegistryDaemon {
    /// A start of `session_type` on [`ACME_MODEL`], in a jail when `sandbox`.
    pub fn a_start(&self, session_type: &str, sandbox: bool) -> StartSessionRequest {
        StartSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            project_id: TEST_PROJECT_ID.to_string(),
            session_type: session_type.to_string(),
            model: ACME_MODEL.to_string(),
            branch_worktree_intent: "new_branch_from_base".to_string(),
            sandbox,
            ..Default::default()
        }
    }

    /// Start `req`, and guard the jail it started.
    pub async fn start(
        &self,
        req: StartSessionRequest,
    ) -> Result<(String, AJailGuard), tddy_rpc::Status> {
        let sandbox = req.sandbox;
        let session_id = self
            .service
            .start_session(Request::direct(req))
            .await?
            .into_inner()
            .session_id;
        let guard = AJailGuard(if sandbox {
            self.metadata_of(&session_id).pid
        } else {
            None
        });
        Ok((session_id, guard))
    }

    /// An active session of `session_type` with [`HOOK_TOKEN`], as a start records it — what a
    /// per-worktree hook reports against.
    pub fn an_active_session(&self, session_id: &str, session_type: &str) {
        let session_dir = self.session_dir(session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        let meta = SessionMetadata {
            session_id: session_id.to_string(),
            project_id: TEST_PROJECT_ID.to_string(),
            created_at: "2026-10-31T08:00:00Z".to_string(),
            updated_at: "2026-10-31T08:00:00Z".to_string(),
            status: "active".to_string(),
            repo_path: Some("/tmp/worktrees/yaml-only-agent".to_string()),
            pid: None,
            tool: None,
            livekit_room: None,
            pending_elicitation: false,
            previous_session_id: None,
            session_type: Some(session_type.to_string()),
            model: Some(ACME_MODEL.to_string()),
            cursor_chat_id: None,
            harness_resume_token: None,
            activity_status: None,
            hook_token: Some(HOOK_TOKEN.to_string()),
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

    /// What `ReportSessionStatus` answers when `session_id`'s hook reports `Running`.
    pub async fn a_hook_reports_running(&self, session_id: &str) -> Result<(), tddy_rpc::Status> {
        self.service
            .activity_service()
            .report_session_status(Request::direct(ReportSessionStatusRequest {
                session_id: session_id.to_string(),
                hook_token: HOOK_TOKEN.to_string(),
                os_user: current_os_user(),
                status: "Running".to_string(),
                message: String::new(),
                ..Default::default()
            }))
            .await
            .map(|_| ())
    }

    pub fn session_dir(&self, session_id: &str) -> PathBuf {
        self.tddy_home.join("sessions").join(session_id)
    }

    pub fn metadata_of(&self, session_id: &str) -> SessionMetadata {
        read_session_metadata(&self.session_dir(session_id)).expect("the session records metadata")
    }

    /// The stub's path as the host resolves it.
    pub fn stub_path(&self) -> String {
        self.stub_bin.join("acme").to_string_lossy().into_owned()
    }

    /// What the stub of `session_id` reported, once it has reported all of it.
    pub async fn stub_report(&self, session_id: &str) -> AStubReport {
        let mut stream = self
            .service
            .terminal_session_service()
            .stream_terminal_output(Request::direct(StreamTerminalOutputRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: session_id.to_string(),
                terminal_id: String::new(),
                initial_cols: 200,
                initial_rows: 24,
                mode: StreamReplayMode::Tail as i32,
                from_offset: 0,
                control_token: String::new(),
            }))
            .await
            .expect("the terminal stream opens")
            .into_inner();
        let mut seen = String::new();
        let _ = tokio::time::timeout(TERMINAL_OBSERVE, async {
            while let Some(Ok(msg)) = stream.next().await {
                seen.push_str(&String::from_utf8_lossy(&msg.data));
                if seen.contains("STUB-END") {
                    return;
                }
            }
        })
        .await;
        assert!(
            seen.contains("STUB-END"),
            "the stub finished its report within {TERMINAL_OBSERVE:?}; it showed: {seen}"
        );
        AStubReport::parse(&seen)
    }
}

fn a_daemon_config_without_agent_sections() -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let user = current_os_user();
    let yaml = format!(
        "users:\n  - github_user: \"{user}\"\n    os_user: \"{user}\"\nallowed_tools:\n  - path: /bin/true\n    label: true\n"
    );
    let config_path = dir.path().join("daemon.yaml");
    std::fs::write(&config_path, yaml).unwrap();
    let config = DaemonConfig::load(&config_path).expect("config must parse");
    (dir, config)
}

fn a_session_host(config: DaemonConfig, tddy_home: PathBuf) -> DaemonSessionHost {
    let sessions_base = tddy_home.clone();
    let user = current_os_user();
    DaemonSessionHost::new(
        config.clone(),
        Arc::new(move |_| Some(sessions_base.clone())),
        tddy_home,
        Arc::new(move |token| (token == VALID_TOKEN).then(|| user.clone())),
        None,
        tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        tddy_session_lifecycle::peer_routing::PeerRouting::local_only(&config),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        Arc::new(ClaudeCliSessionManager::new()),
    )
}

fn register_project(projects_dir: &Path, repo_path: &Path) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let yaml = format!(
        "projects:\n  - project_id: {TEST_PROJECT_ID}\n    name: yaml-only-agent\n    git_url: \"\"\n    main_repo_path: {}\n",
        repo_path.display()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}

fn create_test_repo_with_origin(dir: &Path) {
    let run = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "t@t.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "t@t.com")
            .output()
            .expect("git runs");
        assert!(out.status.success(), "git {args:?}: {out:?}");
    };
    run(&["init", "-b", "main"]);
    run(&["config", "user.email", "t@t.com"]);
    run(&["config", "user.name", "Test"]);
    run(&["commit", "--allow-empty", "-m", "init"]);
    run(&["remote", "add", "origin", dir.to_str().unwrap()]);
    run(&["push", "-u", "origin", "main"]);
}
