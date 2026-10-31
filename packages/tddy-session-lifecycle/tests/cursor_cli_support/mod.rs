//! The fixture `cursor_cli_from_spec_acceptance` builds its daemon from: a real git repo with
//! `origin/main` registered as a project, a stub `agent` that reports what it was started with on
//! its terminal, and a real `DaemonSessionHost` given its coding agents as the daemon runtime gives
//! them — the agent registry from `<tddyhome>/tools`, resolved against a fixture host whose `PATH`
//! is the stub `agent` and the real `tddy-tools`, and whose `$HOME` is a directory of the test's.
//! A sandboxed start runs the real `tddy-sandbox-runner` in a real Seatbelt jail.
//!
//! The stub reports on its terminal — inside a jail it cannot write anywhere the test reads — one
//! `STUB <key>=<value>` line per observation, then `STUB-END`, then it waits on stdin so the session
//! stays up. Argv entries are joined with `|`. Before it reports, it runs the `beforeSubmitPrompt`
//! command of the `.cursor/hooks.json` in its working directory, as Cursor does when a prompt is
//! submitted, and reports that command's exit status.
//!
//! The shape follows `claude_jail_support` (`#agent-tools 5/12`); the stub, the config and the
//! session type are Cursor's. Every jail a test starts is terminated when its guard drops.
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use tddy_actions::tool::ToolHost;
use tddy_agent_launch::spec_launch::HostCodingAgents;
use tddy_coding_agent_specs::{load_agent_registry, CURSOR_SPEC_YAML};
use tddy_core::session_metadata::{read_session_metadata, write_session_metadata, SessionMetadata};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::Request;
use tddy_service::proto::session::{
    ResumeSessionRequest, SessionService as SessionServiceTrait, StartSessionRequest,
};
use tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_terminal_rpc::proto::terminal_session::{
    StreamReplayMode, StreamTerminalOutputRequest,
    TerminalSessionService as TerminalSessionServiceTrait,
};

pub const VALID_TOKEN: &str = "valid-token";
pub const TEST_MODEL: &str = "composer-2.5";
pub const TEST_PROJECT_ID: &str = "cursor-from-spec-project";
/// The chat the stub `agent` mints for `create-chat`.
pub const STUB_CHAT_ID: &str = "f8db82db-e154-41d0-ae72-312bdf6d4d80";
/// The chat an exited session recorded, which its resume reattaches to.
pub const RECORDED_CHAT_ID: &str = "0b1c2d3e-4f50-4617-8899-aabbccddeeff";
pub const RESUMED_SESSION_ID: &str = "01900000-0000-7000-8000-00000000c0c5";

/// How long a stub has to report on its terminal.
const TERMINAL_OBSERVE: Duration = Duration::from_secs(90);
/// How long a hook report has to reach the session's metadata.
const STATUS_OBSERVE: Duration = Duration::from_secs(20);

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

/// A stub `agent`: mints [`STUB_CHAT_ID`] for `create-chat`; otherwise runs the worktree's
/// `beforeSubmitPrompt` hook, then reports its argv, cwd, the environment it was given, its
/// credentials and MCP registration under `$HOME/.cursor`, and — for each `host:port` it was told
/// to probe — the status of a `CONNECT` through `$HTTPS_PROXY`. Every external command is named by
/// its absolute path, so the report does not depend on `PATH`.
pub fn a_stub_agent_script(probes: &[String]) -> String {
    let probe_lines: String = probes
        .iter()
        .map(|target| {
            format!(
                "printf 'STUB connect:{target}=%s\\n' \"$(/usr/bin/curl -s -o /dev/null --max-time 10 --noproxy '' --proxytunnel --proxy \"$HTTPS_PROXY\" -w '%{{http_connect}}' http://{target}/)\"\n"
            )
        })
        .collect();
    format!(
        r#"#!/bin/sh
if [ "$1" = "create-chat" ]; then echo "{STUB_CHAT_ID}"; exit 0; fi
hook=$(/usr/bin/sed -n '/"beforeSubmitPrompt"/,/"command"/p' .cursor/hooks.json 2>/dev/null | /usr/bin/sed -n 's/.*"command": "\(.*\)"$/\1/p')
if [ -n "$hook" ]; then
  printf '{{"hook_event_name":"beforeSubmitPrompt","prompt":"go"}}' | /bin/sh -c "$hook" >/dev/null 2>&1
  printf 'STUB hook-exit=%s\n' "$?"
else
  printf 'STUB hook-exit=NO-HOOK\n'
fi
printf 'STUB argv=%s' "$0"; for a in "$@"; do printf '|%s' "$a"; done; printf '\n'
printf 'STUB cwd=%s\n' "$(/bin/pwd -P)"
for name in HOME TMPDIR CURSOR_TMPDIR CURSOR_SKIP_KEYCHAIN CI CLAUDE_CODE_TMPDIR CLAUDE_TMPDIR HTTPS_PROXY; do
  eval "value=\${{$name-UNSET}}"
  printf 'STUB env:%s=%s\n' "$name" "$value"
done
printf 'STUB path-bin=%s\n' "$(/bin/ls "${{PATH%%:*}}" | /usr/bin/tr '\n' ' ')"
printf 'STUB credentials=%s\n' "$(/bin/cat "$HOME/.cursor/auth.json" 2>/dev/null)"
printf 'STUB mcp=%s\n' "$(/bin/cat "$HOME/.cursor/mcp.json" 2>/dev/null | /usr/bin/tr -d '\n')"
{probe_lines}printf 'STUB-END\n'
exec /bin/cat
"#
    )
}

/// What a stub `agent` reported on its terminal.
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

    fn line(&self, key: &str) -> &str {
        self.lines
            .get(key)
            .unwrap_or_else(|| panic!("the stub reported {key}; it reported {:?}", self.lines))
    }

    pub fn argv(&self) -> Vec<String> {
        self.line("argv").split('|').map(str::to_string).collect()
    }

    pub fn cwd(&self) -> PathBuf {
        PathBuf::from(self.line("cwd"))
    }

    /// `name` as the stub saw it; `UNSET` when it was not set.
    pub fn env(&self, name: &str) -> String {
        self.line(&format!("env:{name}")).to_string()
    }

    /// What the first directory of `PATH` holds, sorted.
    pub fn path_bin(&self) -> Vec<String> {
        self.line("path-bin")
            .split_whitespace()
            .map(str::to_string)
            .collect()
    }

    pub fn credentials(&self) -> String {
        self.line("credentials").to_string()
    }

    /// `$HOME/.cursor/mcp.json`, parsed.
    pub fn mcp_config(&self) -> serde_json::Value {
        serde_json::from_str(self.line("mcp"))
            .unwrap_or_else(|e| panic!("the MCP registration is JSON ({e}): {}", self.line("mcp")))
    }

    /// The exit status of the `beforeSubmitPrompt` hook, or `NO-HOOK` when the worktree had none.
    pub fn hook_exit(&self) -> String {
        self.line("hook-exit").to_string()
    }

    /// The `CONNECT` status a probe of `target` got through the proxy (`200`, `403`, `000`).
    pub fn connect_status(&self, target: &str) -> String {
        self.line(&format!("connect:{target}")).to_string()
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

/// How an [`ACursorDaemon`] is built.
#[derive(Default)]
pub struct ADaemonSpec {
    /// `host:port`s the stub probes through its proxy.
    pub probes: Vec<String>,
    /// Write this `cursor.yaml` override into `<tddyhome>/tools` before the registry loads.
    pub cursor_override: Option<serde_yaml::Value>,
}

/// `specs/cursor.yaml`, parsed, for a test to edit into an override.
pub fn the_builtin_cursor_yaml() -> serde_yaml::Value {
    serde_yaml::from_str(CURSOR_SPEC_YAML).expect("cursor.yaml is YAML")
}

/// A daemon serving one project whose `cursor-cli` sessions run the stub `agent`.
pub struct ACursorDaemon {
    pub service: Arc<DaemonSessionHost>,
    /// `<tddyhome>` — the daemon's data dir and sessions base — canonical.
    pub tddy_home: PathBuf,
    /// The fixture host's `$HOME`.
    pub host_home: PathBuf,
    pub stub_bin: PathBuf,
    _repo_dir: tempfile::TempDir,
    _tddy_home_dir: tempfile::TempDir,
    _host_home_dir: tempfile::TempDir,
    _stub_dir: tempfile::TempDir,
    _config_dir: tempfile::TempDir,
}

pub fn a_cursor_daemon(spec: ADaemonSpec) -> ACursorDaemon {
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let tddy_home_dir = tempfile::tempdir().unwrap();
    let tddy_home = std::fs::canonicalize(tddy_home_dir.path()).unwrap();
    register_project(&tddy_home.join("projects"), repo_dir.path());
    std::fs::create_dir_all(tddy_home.join("tools")).unwrap();
    if let Some(override_yaml) = &spec.cursor_override {
        std::fs::write(
            tddy_home.join("tools/cursor.yaml"),
            serde_yaml::to_string(override_yaml).unwrap(),
        )
        .unwrap();
    }
    let host_home_dir = tempfile::tempdir().unwrap();
    let host_home = std::fs::canonicalize(host_home_dir.path()).unwrap();

    let stub_dir = tempfile::tempdir().unwrap();
    let stub_bin = std::fs::canonicalize(stub_dir.path()).unwrap();
    let stub_agent = stub_bin.join("agent");
    std::fs::write(&stub_agent, a_stub_agent_script(&spec.probes)).unwrap();
    std::fs::set_permissions(&stub_agent, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::os::unix::fs::symlink(tddy_tools_binary(), stub_bin.join("tddy-tools")).unwrap();

    let (config_dir, config) = a_daemon_config(&stub_agent);
    let registry = load_agent_registry(&tddy_home.join("tools")).expect("the agent registry loads");
    let host = a_session_host(config, tddy_home.clone()).with_coding_agents(Arc::new(
        HostCodingAgents::new(
            registry,
            Arc::new(AFixtureHost {
                path: vec![stub_bin.clone()],
                home: host_home.clone(),
            }),
        ),
    ));
    let service = Arc::new(host);
    service.install_sandbox_rpc_bridge();
    ACursorDaemon {
        service,
        tddy_home,
        host_home,
        stub_bin,
        _repo_dir: repo_dir,
        _tddy_home_dir: tddy_home_dir,
        _host_home_dir: host_home_dir,
        _stub_dir: stub_dir,
        _config_dir: config_dir,
    }
}

impl ACursorDaemon {
    pub fn a_cursor_cli_start(&self, sandbox: bool) -> StartSessionRequest {
        StartSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            project_id: TEST_PROJECT_ID.to_string(),
            session_type: "cursor-cli".to_string(),
            model: TEST_MODEL.to_string(),
            sandbox,
            ..Default::default()
        }
    }

    /// Start a `cursor-cli` session — in a jail when `sandbox` — and guard the jail it started.
    pub async fn start(&self, sandbox: bool) -> (String, AJailGuard) {
        let session_id = self
            .service
            .start_session(Request::direct(self.a_cursor_cli_start(sandbox)))
            .await
            .expect("a cursor-cli start succeeds")
            .into_inner()
            .session_id;
        let guard = AJailGuard(if sandbox {
            self.pid_of(&session_id)
        } else {
            None
        });
        (session_id, guard)
    }

    /// An inactive host `cursor-cli` session whose process has exited, recorded with
    /// [`RECORDED_CHAT_ID`] and running in `worktree`.
    pub fn an_exited_host_session(&self, worktree: &Path) {
        let session_dir = self.session_dir(RESUMED_SESSION_ID);
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
            session_type: Some("cursor-cli".to_string()),
            model: Some(TEST_MODEL.to_string()),
            cursor_chat_id: Some(RECORDED_CHAT_ID.to_string()),
            harness_resume_token: None,
            activity_status: None,
            hook_token: Some("a-recorded-hook-token".to_string()),
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

    pub async fn resume(&self, session_id: &str) {
        self.service
            .resume_session(Request::direct(ResumeSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: session_id.to_string(),
            }))
            .await
            .expect("an exited cursor-cli session resumes");
    }

    pub fn session_dir(&self, session_id: &str) -> PathBuf {
        self.tddy_home.join("sessions").join(session_id)
    }

    pub fn metadata_of(&self, session_id: &str) -> SessionMetadata {
        read_session_metadata(&self.session_dir(session_id)).expect("the session records metadata")
    }

    pub fn pid_of(&self, session_id: &str) -> Option<u32> {
        read_session_metadata(&self.session_dir(session_id))
            .ok()
            .and_then(|meta| meta.pid)
    }

    /// The checkout a started session runs in, as `.session.yaml` records it, canonical.
    pub fn worktree_of(&self, session_id: &str) -> PathBuf {
        std::fs::canonicalize(
            self.metadata_of(session_id)
                .repo_path
                .expect("a cursor-cli session records its checkout"),
        )
        .unwrap()
    }

    pub fn cursor_home(&self) -> PathBuf {
        self.tddy_home.join("homes/cursor")
    }

    pub fn jail_tmp_of(&self, session_id: &str) -> String {
        self.session_dir(session_id)
            .join("sandbox/.work/tmp")
            .to_string_lossy()
            .into_owned()
    }

    /// The argv of `session_id`'s runner process, as the OS reports it.
    pub fn runner_argv(&self, session_id: &str) -> Vec<String> {
        let pid = self
            .pid_of(session_id)
            .expect("the session records its pid");
        let out = std::process::Command::new("/bin/ps")
            .args(["-o", "args=", "-p", &pid.to_string()])
            .output()
            .expect("ps runs");
        String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .map(str::to_string)
            .collect()
    }

    /// The activity status `session_id`'s metadata records once it is `expected`, or the last one
    /// it recorded within [`STATUS_OBSERVE`].
    pub async fn activity_status_reaching(
        &self,
        session_id: &str,
        expected: &str,
    ) -> Option<String> {
        let deadline = tokio::time::Instant::now() + STATUS_OBSERVE;
        loop {
            let status = self.metadata_of(session_id).activity_status;
            if status.as_deref() == Some(expected) || tokio::time::Instant::now() >= deadline {
                return status;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// What the stub `agent` of `session_id` reported, once it has reported all of it.
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

/// An embedded daemon's agent-tool socket — bound where `tddy-daemon` binds it for this data dir
/// (`tddy_daemon_kernel::agent_tool_socket_path`), serving the host's own
/// `activity.ActivityService` entry one endpoint per connection, as `tddy-daemon`'s
/// `serve_agent_tool_socket` does. Dropping it stops serving and removes the socket.
pub struct AnAgentToolSocket {
    pub path: PathBuf,
    serving: tokio::task::JoinHandle<()>,
}

impl AnAgentToolSocket {
    pub fn serving_the_activity_of(daemon: &ACursorDaemon) -> Self {
        let path = tddy_daemon_kernel::agent_tool_socket_path(&daemon.tddy_home);
        let _ = std::fs::remove_file(&path);
        let listener = tokio::net::UnixListener::bind(&path).expect("bind the agent-tool socket");
        let service = Arc::clone(&daemon.service);
        let serving = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let entry = service.activity_entry();
                tokio::spawn(async move {
                    let (read_half, write_half) = tokio::io::split(stream);
                    let (_client, endpoint) = tddy_stdio::StdioEndpoint::from_duplex(
                        read_half,
                        write_half,
                        tddy_rpc::MultiRpcService::new(vec![entry]),
                        tddy_rpc::RequestTransport::UnixSocket,
                    );
                    endpoint.run().await;
                });
            }
        });
        Self { path, serving }
    }
}

impl Drop for AnAgentToolSocket {
    fn drop(&mut self) {
        self.serving.abort();
        let _ = std::fs::remove_file(&self.path);
    }
}

/// `TDDY_*` names in `names`.
pub fn tddy_names<'a>(names: impl IntoIterator<Item = &'a String>) -> BTreeSet<String> {
    names
        .into_iter()
        .filter(|name| name.starts_with("TDDY_"))
        .cloned()
        .collect()
}

fn a_daemon_config(stub_agent: &Path) -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let user = current_os_user();
    let yaml = format!(
        r#"
users:
  - github_user: "{user}"
    os_user: "{user}"
allowed_tools:
  - path: /bin/true
    label: true
cursor_cli:
  binary_path: {stub}
  tddy_tools_path: {tddy_tools}
"#,
        stub = stub_agent.display(),
        tddy_tools = tddy_tools_binary().display(),
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
        "projects:\n  - project_id: {TEST_PROJECT_ID}\n    name: cursor-from-spec\n    git_url: \"\"\n    main_repo_path: {}\n",
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
