//! The fixture the sandboxed-Claude suites (`sandboxed_claude_launch_baseline`,
//! `claude_cli_jail_from_spec_acceptance`) build their daemon from: a real git repo with
//! `origin/main` registered as a project, a stub `claude` that reports what it was started with on
//! its terminal, and a real `DaemonSessionHost` with its sandbox RPC bridge installed, running the
//! real `tddy-sandbox-runner` in a real Seatbelt jail.
//!
//! The stub cannot write outside the jail, so it reports on its terminal: one `STUB <key>=<value>`
//! line per observation, then `STUB-END`, then it waits on stdin so the session stays up. Argv
//! entries are joined with `|`.
//!
//! Every jail a test starts is terminated when its guard drops, pass or fail.
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use tddy_accounts::{endpoints_for, preset, ProviderAccount};
use tddy_actions::tool::ToolHost;
use tddy_agent_launch::spec_launch::HostCodingAgents;
use tddy_coding_agent_specs::{builtin_specs, load_agent_registry};
use tddy_core::session_metadata::read_session_metadata;
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
pub const OWNER: &str = "testuser";
pub const TEST_MODEL: &str = "claude-opus-4-8";
pub const TEST_PROJECT_ID: &str = "claude-jail-project";
pub const ROUTE_ACCOUNT: &str = "anthropic-gateway";
const PASSPHRASE: &str = "correct horse battery staple";
const ROUTE_KEY: &str = "a-gateway-key-for-the-jail";

/// How long a jailed stub has to report on its terminal.
const TERMINAL_OBSERVE: Duration = Duration::from_secs(90);

/// The real `tddy-tools`, built beside this test binary.
pub fn tddy_tools_binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_tddy-tools")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/tddy-tools")
        })
}

/// A stub `claude` that reports its argv, the environment it was given, what its `PATH`'s first
/// directory holds, its credentials file and its MCP config, then — for each `host:port` it was
/// told to probe — the status of a `CONNECT` through `$HTTPS_PROXY`. Every external command is
/// named by its absolute path, so the report does not depend on `PATH`.
pub fn a_stub_claude_script(probes: &[String]) -> String {
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
printf 'STUB argv=%s' "$0"; for a in "$@"; do printf '|%s' "$a"; done; printf '\n'
for name in HOME TMPDIR CLAUDE_CODE_TMPDIR CLAUDE_TMPDIR HTTPS_PROXY https_proxy HTTP_PROXY http_proxy; do
  eval "value=\${{$name-UNSET}}"
  printf 'STUB env:%s=%s\n' "$name" "$value"
done
printf 'STUB env-names=%s\n' "$(/usr/bin/env | /usr/bin/sed 's/=.*//' | /usr/bin/sort | /usr/bin/tr '\n' ' ')"
printf 'STUB path-bin=%s\n' "$(/bin/ls "${{PATH%%:*}}" | /usr/bin/tr '\n' ' ')"
printf 'STUB credentials=%s\n' "$(/bin/cat "$HOME/.claude/.credentials.json" 2>/dev/null)"
mcp=""; prev=""
for a in "$@"; do [ "$prev" = "--mcp-config" ] && mcp="$a"; prev="$a"; done
printf 'STUB mcp=%s\n' "$(/bin/cat "$mcp" 2>/dev/null | /usr/bin/tr -d '\n')"
{probe_lines}printf 'STUB-END\n'
exec /bin/cat
"#
    )
}

/// What a stub `claude` reported on its terminal.
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

    /// `name` as the stub saw it; `UNSET` when it was not set.
    pub fn env(&self, name: &str) -> String {
        self.line(&format!("env:{name}")).to_string()
    }

    pub fn env_names(&self) -> BTreeSet<String> {
        self.line("env-names")
            .split_whitespace()
            .map(str::to_string)
            .collect()
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

    /// The MCP config the stub was pointed at by `--mcp-config`, parsed.
    pub fn mcp_config(&self) -> serde_json::Value {
        serde_json::from_str(self.line("mcp"))
            .unwrap_or_else(|e| panic!("the MCP config is JSON ({e}): {}", self.line("mcp")))
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

/// A host with a declared `PATH` over the real filesystem — nothing of the real machine's `PATH`
/// or `TDDY_*_BINARY` variables reaches resolution.
struct AFixtureHost {
    path: Vec<PathBuf>,
}

impl ToolHost for AFixtureHost {
    fn env_var(&self, _name: &str) -> Option<String> {
        None
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

/// How a [`AJailedClaudeDaemon`] is built.
pub struct ADaemonSpec {
    /// `claude_cli.claude_home_dir`, relative to `<tddyhome>` — today's persistent jail home.
    pub configured_claude_home: &'static str,
    /// Give the host its coding agents (the agent registry from `<tddyhome>/tools`, resolved
    /// against a fixture host whose `PATH` is the stub `claude` and the real `tddy-tools`), as the
    /// daemon runtime does.
    pub coding_agents: bool,
    /// `host:port`s the stub probes through its proxy.
    pub probes: Vec<String>,
    /// Write this `claude.yaml` override into `<tddyhome>/tools` before the registry loads.
    pub claude_override: Option<serde_yaml::Value>,
    /// An `anthropic-compatible` provider account rooted at this base URL, in an unlocked vault.
    pub route_base_url: Option<String>,
}

impl Default for ADaemonSpec {
    fn default() -> Self {
        Self {
            configured_claude_home: "configured-claude-home",
            coding_agents: true,
            probes: Vec::new(),
            claude_override: None,
            route_base_url: None,
        }
    }
}

/// The built-in `claude.yaml`, parsed, for a test to edit into an override.
pub fn the_builtin_claude_yaml() -> serde_yaml::Value {
    let yaml = builtin_specs()
        .into_iter()
        .find(|(path, _)| path.ends_with("claude.yaml"))
        .expect("claude.yaml is built in")
        .1;
    serde_yaml::from_str(yaml).expect("claude.yaml is YAML")
}

/// A daemon serving one project whose sandboxed `claude-cli` sessions run the stub `claude`.
pub struct AJailedClaudeDaemon {
    pub service: Arc<DaemonSessionHost>,
    /// `<tddyhome>` — the daemon's data dir and sessions base — canonical.
    pub tddy_home: PathBuf,
    pub stub_claude: PathBuf,
    _repo_dir: tempfile::TempDir,
    _tddy_home_dir: tempfile::TempDir,
    _stub_dir: tempfile::TempDir,
    _vault_dir: tempfile::TempDir,
    _config_dir: tempfile::TempDir,
}

pub fn a_jailed_claude_daemon(spec: ADaemonSpec) -> AJailedClaudeDaemon {
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let tddy_home_dir = tempfile::tempdir().unwrap();
    let tddy_home = std::fs::canonicalize(tddy_home_dir.path()).unwrap();
    register_project(&tddy_home.join("projects"), repo_dir.path());
    std::fs::create_dir_all(tddy_home.join("tools")).unwrap();
    if let Some(override_yaml) = &spec.claude_override {
        std::fs::write(
            tddy_home.join("tools/claude.yaml"),
            serde_yaml::to_string(override_yaml).unwrap(),
        )
        .unwrap();
    }

    let stub_dir = tempfile::tempdir().unwrap();
    let stub_bin = std::fs::canonicalize(stub_dir.path()).unwrap();
    let stub_claude = stub_bin.join("claude");
    std::fs::write(&stub_claude, a_stub_claude_script(&spec.probes)).unwrap();
    std::fs::set_permissions(&stub_claude, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::os::unix::fs::symlink(tddy_tools_binary(), stub_bin.join("tddy-tools")).unwrap();

    let (config_dir, config) =
        a_daemon_config(&stub_claude, &tddy_home.join(spec.configured_claude_home));
    let vault_dir = tempfile::tempdir().unwrap();
    let mut host = a_session_host(config, tddy_home.clone());
    if let Some(base_url) = &spec.route_base_url {
        host = host
            .with_credential_vaults(vaults_holding_a_gateway_account(vault_dir.path(), base_url));
    }
    if spec.coding_agents {
        let registry =
            load_agent_registry(&tddy_home.join("tools")).expect("the agent registry loads");
        host = host.with_coding_agents(Arc::new(HostCodingAgents::new(
            registry,
            Arc::new(AFixtureHost {
                path: vec![stub_bin.clone()],
            }),
        )));
    }
    let service = Arc::new(host);
    service.install_sandbox_rpc_bridge();
    AJailedClaudeDaemon {
        service,
        tddy_home,
        stub_claude,
        _repo_dir: repo_dir,
        _tddy_home_dir: tddy_home_dir,
        _stub_dir: stub_dir,
        _vault_dir: vault_dir,
        _config_dir: config_dir,
    }
}

impl AJailedClaudeDaemon {
    /// A sandboxed `claude-cli` start, on the provider account when `routed`.
    pub fn a_sandboxed_start(&self, routed: bool) -> StartSessionRequest {
        StartSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            project_id: TEST_PROJECT_ID.to_string(),
            session_type: "claude-cli".to_string(),
            model: TEST_MODEL.to_string(),
            provider_account_id: if routed {
                ROUTE_ACCOUNT.to_string()
            } else {
                String::new()
            },
            sandbox: true,
            ..Default::default()
        }
    }

    /// Start a sandboxed session, and guard the jail it started.
    pub async fn start(&self, routed: bool) -> (String, AJailGuard) {
        let session_id = self
            .service
            .start_session(Request::direct(self.a_sandboxed_start(routed)))
            .await
            .expect("a sandboxed claude-cli start succeeds")
            .into_inner()
            .session_id;
        let guard = AJailGuard(self.pid_of(&session_id));
        (session_id, guard)
    }

    /// Resume `session_id`, and guard the jail its resume relaunched.
    pub async fn resume(&self, session_id: &str) -> AJailGuard {
        self.service
            .resume_session(Request::direct(ResumeSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: session_id.to_string(),
            }))
            .await
            .expect("a sandboxed claude-cli session resumes");
        AJailGuard(self.pid_of(session_id))
    }

    pub fn session_dir(&self, session_id: &str) -> PathBuf {
        self.tddy_home.join("sessions").join(session_id)
    }

    pub fn pid_of(&self, session_id: &str) -> Option<u32> {
        read_session_metadata(&self.session_dir(session_id))
            .ok()
            .and_then(|meta| meta.pid)
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

    /// What the stub `claude` of `session_id` reported, once it has reported all of it.
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

fn a_daemon_config(
    stub_claude: &Path,
    claude_home_dir: &Path,
) -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let yaml = format!(
        r#"
users:
  - github_user: "{OWNER}"
    os_user: "{OWNER}"
allowed_tools:
  - path: /bin/true
    label: true
claude_cli:
  binary_path: {stub}
  tddy_tools_path: {tddy_tools}
  claude_home_dir: {home}
"#,
        stub = stub_claude.display(),
        tddy_tools = tddy_tools_binary().display(),
        home = claude_home_dir.display(),
    );
    let config_path = dir.path().join("daemon.yaml");
    std::fs::write(&config_path, yaml).unwrap();
    let config = DaemonConfig::load(&config_path).expect("config must parse");
    (dir, config)
}

fn a_session_host(config: DaemonConfig, tddy_home: PathBuf) -> DaemonSessionHost {
    let sessions_base = tddy_home.clone();
    DaemonSessionHost::new(
        config.clone(),
        Arc::new(move |_| Some(sessions_base.clone())),
        tddy_home,
        Arc::new(|token| (token == VALID_TOKEN).then(|| OWNER.to_string())),
        None,
        tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        tddy_session_lifecycle::peer_routing::PeerRouting::local_only(&config),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        Arc::new(ClaudeCliSessionManager::new()),
    )
}

/// The vaults of a daemon on which the owner has unlocked a vault holding an
/// `anthropic-compatible` account rooted at `base_url`.
fn vaults_holding_a_gateway_account(dir: &Path, base_url: &str) -> Arc<SessionVaults> {
    let passphrase = SecretString::new(PASSPHRASE);
    let store = CredentialStore::create(&CredentialStore::path_in(dir, OWNER), &passphrase, OWNER)
        .expect("the owner's vault is created");
    let gateway = preset("anthropic-compatible").expect("the anthropic-compatible preset");
    let account = ProviderAccount {
        account: AccountId::new(ROUTE_ACCOUNT),
        preset: gateway,
        label: "Anthropic gateway".to_string(),
        endpoints: endpoints_for(gateway, Some(base_url), None).expect("gateway endpoints"),
        api_key: SecretString::new(ROUTE_KEY),
    };
    store
        .put(account.to_record(1_760_000_000))
        .expect("the record is retained");
    let vaults = Arc::new(SessionVaults::new(dir));
    vaults
        .unlock(OWNER, &passphrase)
        .expect("the passphrase opens the vault");
    vaults
}

fn register_project(projects_dir: &Path, repo_path: &Path) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let yaml = format!(
        "projects:\n  - project_id: {TEST_PROJECT_ID}\n    name: claude-jail\n    git_url: \"\"\n    main_repo_path: {}\n",
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
