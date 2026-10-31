//! The fixture `harness_clis_from_spec_acceptance` builds its daemon from: a real git repo with
//! `origin/main` registered as a project, a stub harness binary (`opencode`, `muse` or `warp`) that
//! reports what it was started with on its terminal, an owner whose unlocked vault holds a Meta and
//! an OpenAI-compatible account, and a real `DaemonSessionHost` given its coding agents as the
//! daemon runtime gives them — the agent registry from `<tddyhome>/tools`, resolved against a
//! fixture host whose `PATH` is the stub and the real `tddy-tools`, and whose `$HOME` is a directory
//! of the test's. A sandboxed start runs the real `tddy-sandbox-runner` in a real jail.
//!
//! The stub touches a spawn marker, then reports on its terminal — inside a jail it cannot write
//! anywhere the test reads — one `STUB <key>=<value>` line per observation, then `STUB-END`. Argv
//! entries are joined with `|`. A stub `warp` then prints Warp's exit banner naming
//! [`STUB_RESUME_TOKEN`] and exits, as Warp does; the others wait on stdin so the session stays up.
//!
//! The shape follows `cursor_cli_support` (`#agent-tools 7/12`); the stub, the vault and the
//! session types are the three harnesses'. Every jail a test starts is terminated when its guard
//! drops.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use tddy_accounts::{endpoints_for, preset, ProviderAccount};
use tddy_actions::tool::ToolHost;
use tddy_agent_launch::spec_launch::HostCodingAgents;
use tddy_coding_agent_specs::load_agent_registry;
use tddy_core::session_metadata::{read_session_metadata, write_session_metadata, SessionMetadata};
use tddy_credentials::{AccountId, CredentialStore, SecretString, SessionVaults};
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
pub const TEST_PROJECT_ID: &str = "harness-clis-from-spec-project";
pub const RESUMED_SESSION_ID: &str = "01900000-0000-7000-8000-00000000c1a5";
/// The model a start chooses.
pub const TEST_MODEL: &str = "muse-wonder-1.0";
/// The Meta account the owner's vault holds, and its key.
pub const META_ACCOUNT: &str = "meta-hackathon";
pub const META_KEY: &str = "a-meta-key-for-the-jail";
/// The OpenAI-compatible account the owner's vault holds when a test gives it a base URL, and its
/// key.
pub const COMPATIBLE_ACCOUNT: &str = "local-compatible";
pub const COMPATIBLE_KEY: &str = "a-compatible-key-for-the-jail";
/// The token a stub `warp` prints in its exit banner.
pub const STUB_RESUME_TOKEN: &str = "01JB7Q2N5WSTUBTOKEN";
const PASSPHRASE: &str = "correct horse battery staple";

/// How long a stub has to report on its terminal.
const TERMINAL_OBSERVE: Duration = Duration::from_secs(90);
/// How long a recorded resume token has to reach the session's metadata.
const TOKEN_OBSERVE: Duration = Duration::from_secs(20);

/// One of the three harnesses this node launches from specs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AHarness {
    OpenCode,
    Muse,
    Warp,
}

impl AHarness {
    pub fn session_type(self) -> &'static str {
        match self {
            AHarness::OpenCode => "opencode-cli",
            AHarness::Muse => "muse-cli",
            AHarness::Warp => "warp-cli",
        }
    }

    /// The binary its spec names, which the stub is installed as.
    pub fn binary(self) -> &'static str {
        match self {
            AHarness::OpenCode => "opencode",
            AHarness::Muse => "muse",
            AHarness::Warp => "warp",
        }
    }
}

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

/// A stub harness: touches `marker`, then reports its argv, cwd, the environment it was given, the
/// configuration file `OPENCODE_CONFIG` names (its content and mode), and — for each `host:port` it
/// was told to probe — the status of a `CONNECT` through `$HTTPS_PROXY`. Every external command is
/// named by its absolute path, so the report does not depend on `PATH`.
pub fn a_stub_harness_script(harness: AHarness, marker: &Path, probes: &[String]) -> String {
    let probe_lines: String = probes
        .iter()
        .map(|target| {
            format!(
                "printf 'STUB connect:{target}=%s\\n' \"$(/usr/bin/curl -s -o /dev/null --max-time 10 --noproxy '' --proxytunnel --proxy \"$HTTPS_PROXY\" -w '%{{http_connect}}' http://{target}/)\"\n"
            )
        })
        .collect();
    let ending = match harness {
        AHarness::Warp => format!(
            "printf '\\033[2mResume this conversation with:\\033[0m\\r\\n  \\033[1mwarp --resume {STUB_RESUME_TOKEN}\\033[0m\\r\\n'\nexit 0\n"
        ),
        AHarness::OpenCode | AHarness::Muse => "exec /bin/cat\n".to_string(),
    };
    format!(
        r#"#!/bin/sh
/usr/bin/touch "{marker}" 2>/dev/null
printf 'STUB argv=%s' "$0"; for a in "$@"; do printf '|%s' "$a"; done; printf '\n'
printf 'STUB cwd=%s\n' "$(/bin/pwd -P)"
for name in HOME OPENCODE_CONFIG TDDY_OPENCODE_PROVIDER_KEY META_API_KEY HTTPS_PROXY; do
  eval "value=\${{$name-UNSET}}"
  printf 'STUB env:%s=%s\n' "$name" "$value"
done
if [ -n "$OPENCODE_CONFIG" ]; then
  printf 'STUB config=%s\n' "$(/bin/cat "$OPENCODE_CONFIG" | /usr/bin/tr -d '\n')"
  printf 'STUB config-mode=%s\n' "$(/usr/bin/stat -f %Lp "$OPENCODE_CONFIG" 2>/dev/null || /usr/bin/stat -c %a "$OPENCODE_CONFIG")"
else
  printf 'STUB config=NONE\n'
fi
{probe_lines}printf 'STUB-END\n'
{ending}"#,
        marker = marker.display(),
    )
}

/// What a stub harness reported on its terminal.
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

    /// The file `OPENCODE_CONFIG` names, parsed; `None` when the variable was unset.
    pub fn route_config(&self) -> Option<serde_json::Value> {
        match self.line("config") {
            "NONE" => None,
            json => Some(
                serde_json::from_str(json)
                    .unwrap_or_else(|e| panic!("the route configuration is JSON ({e}): {json}")),
            ),
        }
    }

    /// The octal mode of the file `OPENCODE_CONFIG` names (`600`).
    pub fn route_config_mode(&self) -> String {
        self.line("config-mode").to_string()
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

/// How an [`AHarnessDaemon`] is built.
pub struct ADaemonSpec {
    pub harness: AHarness,
    /// `host:port`s the stub probes through its proxy.
    pub probes: Vec<String>,
    /// The base URL of the OpenAI-compatible account the owner's vault also holds; `None` holds
    /// only the Meta account.
    pub compatible_base_url: Option<String>,
    /// Harness profiles written into `<tddyhome>/agents`, as `(name, YAML)`.
    pub profiles: Vec<(String, String)>,
}

impl ADaemonSpec {
    pub fn of(harness: AHarness) -> Self {
        Self {
            harness,
            probes: Vec::new(),
            compatible_base_url: None,
            profiles: Vec::new(),
        }
    }
}

/// A daemon serving one project whose `harness` sessions run the stub.
pub struct AHarnessDaemon {
    pub service: Arc<DaemonSessionHost>,
    pub harness: AHarness,
    /// `<tddyhome>` — the daemon's data dir and sessions base — canonical.
    pub tddy_home: PathBuf,
    pub stub_bin: PathBuf,
    /// Touched by the stub when it is spawned on the host.
    pub spawn_marker: PathBuf,
    _repo_dir: tempfile::TempDir,
    _tddy_home_dir: tempfile::TempDir,
    _host_home_dir: tempfile::TempDir,
    _stub_dir: tempfile::TempDir,
    _marker_dir: tempfile::TempDir,
    _vault_dir: tempfile::TempDir,
    _config_dir: tempfile::TempDir,
}

pub fn a_harness_daemon(spec: ADaemonSpec) -> AHarnessDaemon {
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let tddy_home_dir = tempfile::tempdir().unwrap();
    let tddy_home = std::fs::canonicalize(tddy_home_dir.path()).unwrap();
    register_project(&tddy_home.join("projects"), repo_dir.path());
    std::fs::create_dir_all(tddy_home.join("tools")).unwrap();
    std::fs::create_dir_all(tddy_home.join("agents")).unwrap();
    for (name, yaml) in &spec.profiles {
        std::fs::write(tddy_home.join("agents").join(format!("{name}.yaml")), yaml).unwrap();
    }
    let host_home_dir = tempfile::tempdir().unwrap();
    let host_home = std::fs::canonicalize(host_home_dir.path()).unwrap();

    let marker_dir = tempfile::tempdir().unwrap();
    let spawn_marker = marker_dir.path().join("spawned");
    let stub_dir = tempfile::tempdir().unwrap();
    let stub_bin = std::fs::canonicalize(stub_dir.path()).unwrap();
    let stub = stub_bin.join(spec.harness.binary());
    std::fs::write(
        &stub,
        a_stub_harness_script(spec.harness, &spawn_marker, &spec.probes),
    )
    .unwrap();
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::os::unix::fs::symlink(tddy_tools_binary(), stub_bin.join("tddy-tools")).unwrap();

    let (config_dir, config) = a_daemon_config(spec.harness, &stub);
    let vault_dir = tempfile::tempdir().unwrap();
    let vaults = vaults_holding_the_accounts(vault_dir.path(), spec.compatible_base_url.as_deref());
    let registry = load_agent_registry(&tddy_home.join("tools")).expect("the agent registry loads");
    let host = a_session_host(config, tddy_home.clone())
        .with_credential_vaults(vaults)
        .with_coding_agents(Arc::new(HostCodingAgents::new(
            registry,
            Arc::new(AFixtureHost {
                path: vec![stub_bin.clone()],
                home: host_home.clone(),
            }),
        )));
    let service = Arc::new(host);
    service.install_sandbox_rpc_bridge();
    AHarnessDaemon {
        service,
        harness: spec.harness,
        tddy_home,
        stub_bin,
        spawn_marker,
        _repo_dir: repo_dir,
        _tddy_home_dir: tddy_home_dir,
        _host_home_dir: host_home_dir,
        _stub_dir: stub_dir,
        _marker_dir: marker_dir,
        _vault_dir: vault_dir,
        _config_dir: config_dir,
    }
}

impl AHarnessDaemon {
    /// A start of the daemon's harness with [`TEST_MODEL`] and `prompt`, in a jail when `sandbox`,
    /// on `account` when given.
    pub fn a_start(
        &self,
        sandbox: bool,
        account: Option<&str>,
        prompt: Option<&str>,
    ) -> StartSessionRequest {
        StartSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            project_id: TEST_PROJECT_ID.to_string(),
            session_type: self.harness.session_type().to_string(),
            model: TEST_MODEL.to_string(),
            branch_worktree_intent: "new_branch_from_base".to_string(),
            provider_account_id: account.unwrap_or_default().to_string(),
            initial_prompt: prompt.unwrap_or_default().to_string(),
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
            self.pid_of(&session_id)
        } else {
            None
        });
        Ok((session_id, guard))
    }

    /// An inactive host session of the daemon's harness whose process has exited, running in
    /// `worktree`, with `resume_token` recorded when given.
    pub fn an_exited_host_session(&self, worktree: &Path, resume_token: Option<&str>) {
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
            session_type: Some(self.harness.session_type().to_string()),
            model: Some(TEST_MODEL.to_string()),
            cursor_chat_id: None,
            harness_resume_token: resume_token.map(str::to_string),
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

    /// Resume `session_id`, and guard the jail its resume relaunched.
    pub async fn resume(&self, session_id: &str) -> AJailGuard {
        self.service
            .resume_session(Request::direct(ResumeSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: session_id.to_string(),
            }))
            .await
            .unwrap_or_else(|e| panic!("{session_id} resumes: {e:?}"));
        AJailGuard(
            self.metadata_of(session_id)
                .sandbox
                .unwrap_or(false)
                .then(|| self.pid_of(session_id))
                .flatten(),
        )
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
                .expect("a CLI harness session records its checkout"),
        )
        .unwrap()
    }

    /// The stub's path as the host resolves it.
    pub fn stub_path(&self) -> String {
        self.stub_bin
            .join(self.harness.binary())
            .to_string_lossy()
            .into_owned()
    }

    /// The route files directory of a jailed session's scratch.
    pub fn route_files_dir_of(&self, session_id: &str) -> PathBuf {
        self.session_dir(session_id)
            .join("sandbox/.work/provider-route")
    }

    /// The resume token `session_id`'s metadata records once it is `expected`, or the last one it
    /// recorded within [`TOKEN_OBSERVE`].
    pub async fn resume_token_reaching(&self, session_id: &str, expected: &str) -> Option<String> {
        let deadline = tokio::time::Instant::now() + TOKEN_OBSERVE;
        loop {
            let token = self.metadata_of(session_id).harness_resume_token;
            if token.as_deref() == Some(expected) || tokio::time::Instant::now() >= deadline {
                return token;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
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

/// The vaults of a daemon on which the owner has unlocked a vault holding [`META_ACCOUNT`], and
/// [`COMPATIBLE_ACCOUNT`] at `compatible_base_url` when given.
fn vaults_holding_the_accounts(
    dir: &Path,
    compatible_base_url: Option<&str>,
) -> Arc<SessionVaults> {
    let owner = current_os_user();
    let passphrase = SecretString::new(PASSPHRASE);
    let store =
        CredentialStore::create(&CredentialStore::path_in(dir, &owner), &passphrase, &owner)
            .expect("the owner's vault is created");
    let meta = preset("meta").expect("the meta preset");
    let mut accounts = vec![ProviderAccount {
        account: AccountId::new(META_ACCOUNT),
        preset: meta,
        label: "Meta hackathon".to_string(),
        endpoints: endpoints_for(meta, None, None).expect("meta endpoints"),
        api_key: SecretString::new(META_KEY),
    }];
    if let Some(base_url) = compatible_base_url {
        let compatible = preset("openai-compatible").expect("the openai-compatible preset");
        accounts.push(ProviderAccount {
            account: AccountId::new(COMPATIBLE_ACCOUNT),
            preset: compatible,
            label: "Local compatible".to_string(),
            endpoints: endpoints_for(compatible, None, Some(base_url))
                .expect("openai-compatible endpoints"),
            api_key: SecretString::new(COMPATIBLE_KEY),
        });
    }
    for account in accounts {
        store
            .put(account.to_record(1_760_000_000))
            .expect("the record is retained");
    }
    let vaults = Arc::new(SessionVaults::new(dir));
    vaults
        .unlock(&owner, &passphrase)
        .expect("the passphrase opens the vault");
    vaults
}

fn a_daemon_config(harness: AHarness, stub: &Path) -> (tempfile::TempDir, DaemonConfig) {
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
cli_harnesses:
  {session_type}:
    binary_path: {stub}
"#,
        session_type = harness.session_type(),
        stub = stub.display(),
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
        "projects:\n  - project_id: {TEST_PROJECT_ID}\n    name: harness-clis-from-spec\n    git_url: \"\"\n    main_repo_path: {}\n",
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
