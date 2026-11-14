//! The fixture `yaml_only_agent_acceptance` builds its daemon from (`#agent-tools 12/12`): a real
//! git repo with `origin/main` registered as a project, a `<tddyhome>/tools` holding the tool files
//! a test drops in, a stub `acme` binary that reports what it was started with on its terminal, and
//! a real `DaemonSessionHost` given its coding agents as the daemon runtime gives them — the
//! coding-agent registry over `<tddyhome>/tools`, resolved against a fixture host whose `PATH` is
//! the stub and the real `tddy-tools`. The daemon.yaml carries no agent section at all.
//!
//! The stub touches a spawn marker, then reports one `STUB <key>=<value>` line per observation
//! (argv joined with `|`, then cwd, then the route environment a provider account gives it), then
//! `STUB-END`, and waits on stdin so the session stays up.
//!
//! The shape follows `harness_clis_support` (`#agent-tools 8/12`), cut to what a YAML-only agent
//! needs: no resume. A daemon built [`ARegistryDaemonBuilder::with_a_meta_account`] has an owner
//! whose unlocked vault holds [`META_ACCOUNT`] (`#agent-tools 13/19`), and one built
//! [`ARegistryDaemonBuilder::with_profile`] has harness profiles in `<tddyhome>/agents`.
//!
//! `#agent-tools 19/19` adds the answer wait: [`builtin_yaml_run_as_the_stub_waiting`] overrides
//! claude.yaml with an `answer_wait_secs`, [`ARegistryDaemon::an_orchestrator_of`] makes a started
//! session orchestrated by a running acme session, and [`ARegistryDaemon::the_permission_hook_of`]
//! holds its hook in the daemon's real `AwaitQuestionAnswer`. [`ARegistryDaemon::restart_with_tool`]
//! is a daemon restarted over the same `<tddyhome>` after an operator edited a tool file.
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
use tddy_coding_agent_specs::CodingAgentRegistry;
use tddy_core::pending_question::{write_pending_question, PendingQuestion};
use tddy_core::session_metadata::{read_session_metadata, write_session_metadata, SessionMetadata};
use tddy_credentials::{AccountId, CredentialStore, SecretString, SessionVaults};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::Request;
use tddy_service::proto::activity::{
    ActivityService as _, AwaitQuestionAnswerRequest, AwaitQuestionOutcome,
    ReportSessionStatusRequest,
};
use tddy_service::proto::session::{
    EnsureProjectHomeSessionRequest, ResumeSessionRequest, SessionService as SessionServiceTrait,
    StartSessionRequest,
};
use tddy_session_lifecycle::claude_cli_session::CliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_terminal_rpc::proto::terminal_session::{
    StreamReplayMode, StreamTerminalOutputRequest,
    TerminalSessionService as TerminalSessionServiceTrait,
};
use tddy_testing_commons::wait::eventually;

pub const VALID_TOKEN: &str = "valid-token";
pub const TEST_PROJECT_ID: &str = "yaml-only-agent-project";
pub const HOOK_TOKEN: &str = "a-per-session-hook-token";
/// The model a start chooses.
pub const ACME_MODEL: &str = "acme-1";
/// The Meta account the owner's vault holds in a daemon built with one, and its key. Meta serves
/// Anthropic Messages at [`META_ANTHROPIC_ROOT`].
pub const META_ACCOUNT: &str = "meta-hackathon";
pub const META_KEY: &str = "a-meta-key-for-acme";
pub const META_ANTHROPIC_ROOT: &str = "https://api.meta.ai";
const PASSPHRASE: &str = "correct horse battery staple";
/// The variables the stub reports, `UNSET` when it was not given one.
const REPORTED_ENV: &str = "ANTHROPIC_BASE_URL ANTHROPIC_AUTH_TOKEN ANTHROPIC_MODEL";

/// How long a stub has to report on its terminal.
const TERMINAL_OBSERVE: Duration = Duration::from_secs(90);

/// `acme`: an agent no Rust names, declared in this one file. Its session type is `acme-cli`, as its
/// `session` extension declares; it
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
  session: { session_type: acme-cli, launch: agent_cli, label: Acme Code }
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

/// [`ACME_YAML`] with `lines` added to its `coding_agent` section — each indented four spaces, as
/// that section's keys are.
pub fn acme_yaml_declaring(lines: &str) -> String {
    let section = "  coding_agent:\n";
    assert!(
        ACME_YAML.contains(section),
        "acme.yaml has a coding_agent section"
    );
    ACME_YAML.replacen(section, &format!("{section}{lines}"), 1)
}

/// The built-in `builtin.yaml` (`claude`, `cursor`) as an agent of its own under `id`
/// (`#agent-tools 14/19`): its id and its binary's command are `id` (the stub), its binary's
/// environment variable `TDDY_<ID>_CLI`. Everything else — argv, slots, capabilities, jail sections
/// — is the built-in's, byte for byte, so its `<id>-cli` session launches through the generic path
/// with exactly what the built-in declares.
pub fn builtin_yaml_as(builtin: &str, id: &str) -> String {
    let file_name = format!("/{builtin}.yaml");
    let (_, yaml) = tddy_coding_agent_specs::builtin_specs()
        .into_iter()
        .find(|(path, _)| path.ends_with(&file_name))
        .unwrap_or_else(|| panic!("{builtin}.yaml is built in"));
    let def: tddy_actions::tool::ToolDef =
        serde_yaml::from_str(yaml).expect("the built-in is a tool file");
    let swaps = [
        (format!("\nid: {builtin}\n"), format!("\nid: {id}\n")),
        // The session type it declares is its own, as a dropped-in agent's must be: two files
        // declaring one session type refuse the registry.
        (
            format!("\n    session_type: {builtin}-cli\n"),
            format!("\n    session_type: {id}-cli\n"),
        ),
        (
            format!("\n  command: {}\n", def.binary.command),
            format!("\n  command: {id}\n"),
        ),
        (
            format!(
                "\n  env: {}\n",
                def.binary
                    .env
                    .clone()
                    .expect("the built-in names its binary's variable")
            ),
            format!("\n  env: TDDY_{}_CLI\n", id.to_uppercase()),
        ),
    ];
    swaps.iter().fold(yaml.to_string(), |copy, (from, to)| {
        assert!(
            copy.contains(from.as_str()),
            "{builtin}.yaml holds {from:?}"
        );
        copy.replacen(from.as_str(), to, 1)
    })
}

/// The built-in `builtin.yaml` (`claude`, `cursor`) overriding itself from `<tddyhome>/tools`
/// (`#agent-tools 17/19`): its id — so its session type — is still the built-in's, but its binary's
/// command is the stub (`acme`), so a `claude-cli` or `cursor-cli` session runs the stub through
/// whatever launch the daemon picks for the built-in. Everything else is the built-in's, byte for
/// byte.
pub fn builtin_yaml_run_as_the_stub(builtin: &str) -> String {
    let file_name = format!("/{builtin}.yaml");
    let (_, yaml) = tddy_coding_agent_specs::builtin_specs()
        .into_iter()
        .find(|(path, _)| path.ends_with(&file_name))
        .unwrap_or_else(|| panic!("{builtin}.yaml is built in"));
    let def: tddy_actions::tool::ToolDef =
        serde_yaml::from_str(yaml).expect("the built-in is a tool file");
    let command = format!("\n  command: {}\n", def.binary.command);
    assert!(yaml.contains(&command), "{builtin}.yaml holds {command:?}");
    yaml.replacen(&command, "\n  command: acme\n", 1)
}

/// [`builtin_yaml_run_as_the_stub`] of `claude.yaml`, its `coding_agent` section declaring
/// `answer_wait_secs: <secs>` — an operator's override of how long an orchestrated claude-cli
/// session's permission hook waits for its orchestrator (`#agent-tools 19/19`).
pub fn builtin_yaml_run_as_the_stub_waiting(secs: u64) -> String {
    let claude = builtin_yaml_run_as_the_stub("claude");
    let section = "  coding_agent:\n";
    assert!(
        claude.contains(section),
        "claude.yaml has a coding_agent section"
    );
    claude.replacen(
        section,
        &format!("{section}    answer_wait_secs: {secs}\n"),
        1,
    )
}

/// [`builtin_yaml_as`] of `cursor.yaml` under `id`, its conversation minted by
/// `cursor-create-chat` within `secs` — replacing any `conversation:` line the built-in declares.
pub fn cursor_copy_minting_within(id: &str, secs: u64) -> String {
    let copy: String = builtin_yaml_as("cursor", id)
        .lines()
        .filter(|line| !line.trim_start().starts_with("conversation:"))
        .map(|line| format!("{line}\n"))
        .collect();
    let section = "  coding_agent:\n";
    assert!(
        copy.contains(section),
        "cursor.yaml has a coding_agent section"
    );
    copy.replacen(
        section,
        &format!(
            "{section}    conversation: {{ kind: minted, adapter: cursor-create-chat, mint_deadline_secs: {secs} }}\n"
        ),
        1,
    )
}

/// A `TDDY_*` variable in this process's — the daemon's — environment for as long as it lives.
pub struct ATddyVariableInTheDaemon {
    name: &'static str,
}

impl ATddyVariableInTheDaemon {
    pub fn set(name: &'static str, value: &str) -> Self {
        assert!(name.starts_with("TDDY_"), "{name} is a TDDY_ variable");
        std::env::set_var(name, value);
        Self { name }
    }
}

impl Drop for ATddyVariableInTheDaemon {
    fn drop(&mut self) {
        std::env::remove_var(self.name);
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

/// A stub agent: touches `marker`, reports its argv, cwd and the route variables
/// ([`REPORTED_ENV`]), then waits on stdin. Every external command is named by its absolute path,
/// so the report does not depend on `PATH`.
pub fn a_stub_agent_script(marker: &Path) -> String {
    a_stub_agent_script_minting(marker, &StubCreateChat::Prints, false)
}

/// What the stub runs from its own shell, in a directory below its cwd, when built to submit
/// ([`ARegistryDaemonBuilder::with_a_stub_that_submits_from_its_shell`]): the command every workflow
/// prompt names, with a `plan` payload. It reports the command's stdout as `submit`.
pub const STUB_SUBMIT: &str = r##"tddy-tools submit --goal plan --data '{"goal":"plan","prd":"# PRD\n\n## Summary\nFeature X","todo":"- [ ] Task 1"}'"##;

/// What the stub does when it is run as `<binary> create-chat` — the `cursor-create-chat` mint
/// (`#agent-tools 14/19`). It never touches the spawn marker: a mint is not the agent's spawn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StubCreateChat {
    /// Prints [`MINTED_CHAT_ID`] and exits 0.
    #[default]
    Prints,
    /// Never returns.
    NeverReturns,
}

/// The chat the stub's `create-chat` mints.
pub const MINTED_CHAT_ID: &str = "acme-chat-0001";

/// [`a_stub_agent_script`] whose `create-chat` behaves as `create_chat`. Besides its argv, cwd and
/// route environment it reports the names of every `TDDY_*` variable it was given
/// (`tddy-env`) and whether `tddy-tools` is on its `PATH` (`tddy-tools`). With `submit`, it runs
/// [`STUB_SUBMIT`] from its shell and reports the answer (`submit`) before `STUB-END`.
pub fn a_stub_agent_script_minting(
    marker: &Path,
    create_chat: &StubCreateChat,
    submit: bool,
) -> String {
    let create_chat = match create_chat {
        StubCreateChat::Prints => format!("printf '%s\\n' '{MINTED_CHAT_ID}'; exit 0"),
        StubCreateChat::NeverReturns => "exec /bin/sleep 3600".to_string(),
    };
    let submit = if submit {
        format!(
            "/bin/mkdir -p src && printf 'STUB submit=%s\\n' \"$(cd src && {STUB_SUBMIT} 2>/dev/null | /usr/bin/tr '\\n' ' ')\"\n"
        )
    } else {
        String::new()
    };
    format!(
        r#"#!/bin/sh
if [ "$1" = "create-chat" ]; then {create_chat}; fi
/usr/bin/touch "{marker}" 2>/dev/null
printf 'STUB argv=%s' "$0"; for a in "$@"; do printf '|%s' "$a"; done; printf '\n'
printf 'STUB cwd=%s\n' "$(/bin/pwd -P)"
for name in {reported_env}; do
  eval "value=\${{$name-UNSET}}"
  printf 'STUB env:%s=%s\n' "$name" "$value"
done
printf 'STUB tddy-env=%s\n' "$(/usr/bin/env | /usr/bin/sed -n 's/^\(TDDY_[A-Za-z0-9_]*\)=.*/\1/p' | /usr/bin/tr '\n' ' ')"
if command -v tddy-tools >/dev/null 2>&1; then printf 'STUB tddy-tools=on-path\n'; else printf 'STUB tddy-tools=absent\n'; fi
{submit}printf 'STUB-END\n'
exec /bin/cat
"#,
        marker = marker.display(),
        reported_env = REPORTED_ENV,
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
            // A terminal repaint (a tmux client re-attaching) puts cursor escapes in front of the
            // first line: the report starts where its marker does.
            .filter_map(|line| line.find("STUB ").map(|at| &line[at + "STUB ".len()..]))
            .filter_map(|line| line.split_once('='))
            // A value ends where a repaint's next escape starts.
            .map(|(key, value)| {
                let value = value.split('\u{1b}').next().unwrap_or_default();
                (key.to_string(), value.trim_end().to_string())
            })
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

    /// The values every `flag` in the stub's argv is followed by, in order.
    pub fn values_of(&self, flag: &str) -> Vec<String> {
        self.argv()
            .windows(2)
            .filter(|pair| pair[0] == flag)
            .map(|pair| pair[1].clone())
            .collect()
    }

    /// The names of the `TDDY_*` variables the stub was given, in its environment's order.
    pub fn tddy_variables(&self) -> Vec<String> {
        self.lines
            .get("tddy-env")
            .unwrap_or_else(|| {
                panic!(
                    "the stub reported its TDDY_ names; it reported {:?}",
                    self.lines
                )
            })
            .split_whitespace()
            .map(str::to_string)
            .collect()
    }

    /// The answer [`STUB_SUBMIT`] printed, parsed as the JSON `tddy-tools` answers with; `Null`
    /// when it printed none.
    pub fn submit_answer(&self) -> serde_json::Value {
        let printed = self.lines.get("submit").unwrap_or_else(|| {
            panic!("the stub reported its submit; it reported {:?}", self.lines)
        });
        serde_json::from_str(printed.trim()).unwrap_or(serde_json::Value::Null)
    }

    /// Whether `tddy-tools` was on the stub's `PATH`.
    pub fn tddy_tools_on_path(&self) -> bool {
        self.lines.get("tddy-tools").map(String::as_str) == Some("on-path")
    }

    /// `name` (one of [`REPORTED_ENV`]) as the stub saw it; `UNSET` when it was not set.
    pub fn env(&self, name: &str) -> String {
        self.lines
            .get(&format!("env:{name}"))
            .unwrap_or_else(|| panic!("the stub reported {name}; it reported {:?}", self.lines))
            .clone()
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
    /// The daemon.yaml it was started with — no agent section.
    config: DaemonConfig,
    /// The fixture host's `$HOME`.
    host_home: PathBuf,
    /// The owner's unlocked vaults, when built with a Meta account.
    vaults: Option<Arc<SessionVaults>>,
    _repo_dir: tempfile::TempDir,
    _tddy_home_dir: tempfile::TempDir,
    _host_home_dir: tempfile::TempDir,
    _stub_dir: tempfile::TempDir,
    _marker_dir: tempfile::TempDir,
    _config_dir: tempfile::TempDir,
    _vault_dir: Option<tempfile::TempDir>,
}

/// A daemon whose `<tddyhome>/tools` holds `tools` (`(file name, YAML)`), with the stub installed
/// as `acme`.
pub fn a_daemon_with_tools(tools: &[(&str, &str)]) -> ARegistryDaemon {
    tools
        .iter()
        .fold(a_registry_daemon(), |daemon, (name, yaml)| {
            daemon.with_tool(name, yaml)
        })
        .build()
}

/// How an [`ARegistryDaemon`] is built: no tools, no profiles and no vault until a test adds them.
#[derive(Default)]
pub struct ARegistryDaemonBuilder {
    tools: Vec<(String, String)>,
    profiles: Vec<(String, String)>,
    meta_account: bool,
    create_chat: StubCreateChat,
    submits: bool,
}

pub fn a_registry_daemon() -> ARegistryDaemonBuilder {
    ARegistryDaemonBuilder::default()
}

impl ARegistryDaemonBuilder {
    /// `<tddyhome>/tools/<name>` holds `yaml`.
    pub fn with_tool(mut self, name: &str, yaml: &str) -> Self {
        self.tools.push((name.to_string(), yaml.to_string()));
        self
    }

    /// `<tddyhome>/agents/<name>.yaml` holds the harness profile `yaml`.
    pub fn with_profile(mut self, name: &str, yaml: &str) -> Self {
        self.profiles.push((name.to_string(), yaml.to_string()));
        self
    }

    /// The owner has unlocked a vault holding [`META_ACCOUNT`].
    pub fn with_a_meta_account(mut self) -> Self {
        self.meta_account = true;
        self
    }

    /// The stub's `create-chat` never returns (`#agent-tools 14/19`).
    pub fn with_a_create_chat_that_never_returns(mut self) -> Self {
        self.create_chat = StubCreateChat::NeverReturns;
        self
    }

    /// The stub runs [`STUB_SUBMIT`] from its own shell once started (`#agent-tools 17/19`).
    pub fn with_a_stub_that_submits_from_its_shell(mut self) -> Self {
        self.submits = true;
        self
    }

    pub fn build(self) -> ARegistryDaemon {
        a_daemon_built_from(self)
    }
}

fn a_daemon_built_from(spec: ARegistryDaemonBuilder) -> ARegistryDaemon {
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let tddy_home_dir = tempfile::tempdir().unwrap();
    let tddy_home = std::fs::canonicalize(tddy_home_dir.path()).unwrap();
    register_project(&tddy_home.join("projects"), repo_dir.path());
    let tools_dir = tddy_home.join("tools");
    std::fs::create_dir_all(&tools_dir).unwrap();
    for (name, yaml) in &spec.tools {
        std::fs::write(tools_dir.join(name), yaml).unwrap();
    }
    let agents_dir = tddy_home.join("agents");
    std::fs::create_dir_all(&agents_dir).unwrap();
    for (name, yaml) in &spec.profiles {
        std::fs::write(agents_dir.join(format!("{name}.yaml")), yaml).unwrap();
    }
    let host_home_dir = tempfile::tempdir().unwrap();
    let host_home = std::fs::canonicalize(host_home_dir.path()).unwrap();

    let marker_dir = tempfile::tempdir().unwrap();
    let spawn_marker = marker_dir.path().join("spawned");
    let stub_dir = tempfile::tempdir().unwrap();
    let stub_bin = std::fs::canonicalize(stub_dir.path()).unwrap();
    let stub = stub_bin.join("acme");
    std::fs::write(
        &stub,
        a_stub_agent_script_minting(&spawn_marker, &spec.create_chat, spec.submits),
    )
    .unwrap();
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::os::unix::fs::symlink(tddy_tools_binary(), stub_bin.join("tddy-tools")).unwrap();

    let (config_dir, config) = a_daemon_config_without_agent_sections();
    let registry = CodingAgentRegistry::load(&tools_dir).expect("the coding-agent registry loads");
    let vault_dir = spec.meta_account.then(|| tempfile::tempdir().unwrap());
    let vaults = vault_dir
        .as_ref()
        .map(|dir| vaults_holding_the_meta_account(dir.path()));
    let service = a_running_daemon(
        &config,
        &tddy_home,
        &registry,
        &stub_bin,
        &host_home,
        vaults.clone(),
    );
    ARegistryDaemon {
        service,
        registry,
        tddy_home,
        stub_bin,
        spawn_marker,
        config,
        host_home,
        vaults,
        _repo_dir: repo_dir,
        _tddy_home_dir: tddy_home_dir,
        _host_home_dir: host_home_dir,
        _stub_dir: stub_dir,
        _marker_dir: marker_dir,
        _config_dir: config_dir,
        _vault_dir: vault_dir,
    }
}

/// The session host the daemon runtime builds over `tddy_home`: given `registry` as its coding
/// agents, resolved against a fixture host whose `PATH` is the stub, with the sandbox RPC bridge.
fn a_running_daemon(
    config: &DaemonConfig,
    tddy_home: &Path,
    registry: &CodingAgentRegistry,
    stub_bin: &Path,
    host_home: &Path,
    vaults: Option<Arc<SessionVaults>>,
) -> Arc<DaemonSessionHost> {
    let host = a_session_host(config.clone(), tddy_home.to_path_buf());
    let host = match vaults {
        Some(vaults) => host.with_credential_vaults(vaults),
        None => host,
    };
    let host = host.with_coding_agents(Arc::new(HostCodingAgents::new(
        registry.tools().clone(),
        Arc::new(AFixtureHost {
            path: vec![stub_bin.to_path_buf()],
            home: host_home.to_path_buf(),
        }),
    )));
    let service = Arc::new(host);
    service.install_sandbox_rpc_bridge();
    service
}

/// The vaults of a daemon on which the owner has unlocked a vault holding [`META_ACCOUNT`].
fn vaults_holding_the_meta_account(dir: &Path) -> Arc<SessionVaults> {
    let owner = current_os_user();
    let passphrase = SecretString::new(PASSPHRASE);
    let store =
        CredentialStore::create(&CredentialStore::path_in(dir, &owner), &passphrase, &owner)
            .expect("the owner's vault is created");
    let meta = preset("meta").expect("the meta preset");
    let account = ProviderAccount {
        account: AccountId::new(META_ACCOUNT),
        preset: meta,
        label: "Meta hackathon".to_string(),
        endpoints: endpoints_for(meta, None, None).expect("meta endpoints"),
        api_key: SecretString::new(META_KEY),
    };
    store
        .put(account.to_record(1_760_000_000))
        .expect("the record is retained");
    let vaults = Arc::new(SessionVaults::new(dir));
    vaults
        .unlock(&owner, &passphrase)
        .expect("the passphrase opens the vault");
    vaults
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

    /// [`Self::a_start`] on the host, its agent run in tmux (`#agent-tools 14/19`).
    pub fn a_tmux_start(&self, session_type: &str) -> StartSessionRequest {
        StartSessionRequest {
            run_in_tmux: true,
            ..self.a_start(session_type, false)
        }
    }

    /// [`Self::a_start`] on the host over a managed codebase driven by `recipe`.
    pub fn a_managed_start(&self, session_type: &str, recipe: &str) -> StartSessionRequest {
        StartSessionRequest {
            managed_codebase: true,
            recipe: recipe.to_string(),
            ..self.a_start(session_type, false)
        }
    }

    /// [`Self::a_start`] whose checkout lives on the ssh target `alias`.
    pub fn an_ssh_start(&self, session_type: &str, alias: &str) -> StartSessionRequest {
        StartSessionRequest {
            ssh_config_host: alias.to_string(),
            ..self.a_start(session_type, false)
        }
    }

    /// Set the project's main up from `start` (`EnsureProjectHomeSession`), answering its session.
    pub async fn ensure_project_home(
        &self,
        start: StartSessionRequest,
    ) -> Result<String, tddy_rpc::Status> {
        let ensured = self
            .service
            .ensure_project_home_session(Request::direct(EnsureProjectHomeSessionRequest {
                session_token: a_caller_token().to_string(),
                project_id: TEST_PROJECT_ID.to_string(),
                daemon_instance_id: String::new(),
                start: Some(StartSessionRequest {
                    branch_worktree_intent: String::new(),
                    ..start
                }),
            }))
            .await?
            .into_inner();
        Ok(ensured
            .session
            .expect("an ensured main answers its session")
            .session_id)
    }

    /// Stop `session_id`'s agent — kill the pid it records — and wait until it is gone.
    pub async fn stop(&self, session_id: &str) {
        let pid = self
            .metadata_of(session_id)
            .pid
            .unwrap_or_else(|| panic!("{session_id} records its agent's pid"));
        // SAFETY: a plain kill(2) of the stub this test started.
        unsafe { libc::kill(pid as i32, libc::SIGKILL) };
        eventually("the stopped agent is gone", TERMINAL_OBSERVE, || {
            // SAFETY: signal 0 only probes whether the pid still exists.
            match unsafe { libc::kill(pid as i32, 0) } {
                0 => Err(format!("pid {pid} is still alive")),
                _ => Ok(()),
            }
        })
        .await;
    }

    /// Resume `session_id`.
    pub async fn resume(&self, session_id: &str) -> Result<(), tddy_rpc::Status> {
        self.service
            .resume_session(Request::direct(ResumeSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: session_id.to_string(),
            }))
            .await
            .map(|_| ())
    }

    /// The checkout a started session runs in, as `.session.yaml` records it.
    pub fn worktree_of(&self, session_id: &str) -> PathBuf {
        PathBuf::from(
            self.metadata_of(session_id)
                .repo_path
                .unwrap_or_else(|| panic!("{session_id} records its worktree")),
        )
    }

    /// [`Self::a_start`] on the host, on the provider account `account`.
    pub fn a_start_on_account(&self, session_type: &str, account: &str) -> StartSessionRequest {
        StartSessionRequest {
            provider_account_id: account.to_string(),
            ..self.a_start(session_type, false)
        }
    }

    /// A host start through the harness profile `profile`: no session type and no model of its own.
    pub fn a_start_through_profile(&self, profile: &str) -> StartSessionRequest {
        StartSessionRequest {
            session_type: String::new(),
            model: String::new(),
            agent: profile.to_string(),
            ..self.a_start("", false)
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
            effort: None,
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
            tmux_session: None,
            orchestration_mark: None,
            agent_conversations: Vec::new(),
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

    /// The daemon restarted over the same `<tddyhome>` after an operator wrote `yaml` to
    /// `<tddyhome>/tools/<name>`: the registry is loaded again, as a daemon start loads it. The
    /// sessions it started are on disk; their agents belonged to the stopped daemon.
    pub fn restart_with_tool(mut self, name: &str, yaml: &str) -> Self {
        let tools_dir = self.tddy_home.join("tools");
        std::fs::write(tools_dir.join(name), yaml).unwrap();
        self.registry =
            CodingAgentRegistry::load(&tools_dir).expect("the coding-agent registry loads");
        self.service = a_running_daemon(
            &self.config,
            &self.tddy_home,
            &self.registry,
            &self.stub_bin,
            &self.host_home,
            self.vaults.clone(),
        );
        self
    }

    /// Make `child` orchestrated by a running session: an acme-cli session is started on the host
    /// (its tool file must be installed), its terminal is served once its stub has reported, and
    /// `child`'s changeset names it as `#start-parity` 2/9's start records an orchestrator.
    /// Answers the orchestrator's session id.
    pub async fn an_orchestrator_of(&self, child: &str) -> String {
        let (orchestrator, _host) = self
            .start(self.a_start("acme-cli", false))
            .await
            .expect("the orchestrating acme-cli session starts on the host");
        self.stub_report(&orchestrator).await;
        let child_dir = self.session_dir(child);
        let mut changeset = tddy_core::read_changeset(&child_dir).unwrap_or_default();
        changeset.orchestrator_session_id = Some(orchestrator.clone());
        changeset.orchestrator_daemon_instance_id = None;
        tddy_core::write_changeset(&child_dir, &changeset).unwrap();
        orchestrator
    }

    /// `session_id`'s agent asks for permission to run `cargo publish` as [`PERMISSION_REQUEST_ID`],
    /// as `#home-inbox` 3/8 records it.
    pub fn asks_for_a_permission(&self, session_id: &str) {
        write_pending_question(
            &self.session_dir(session_id),
            &PendingQuestion {
                kind: "permission".to_string(),
                question: "Run Bash: cargo publish".to_string(),
                request_id: PERMISSION_REQUEST_ID.to_string(),
                tool_name: "Bash".to_string(),
                tool_input_json: r#"{"command":"cargo publish"}"#.to_string(),
                ..Default::default()
            },
        )
        .unwrap();
    }

    /// `session_id`'s `PermissionRequest` hook held in this daemon's `AwaitQuestionAnswer` — with
    /// the hook token its start recorded — for at most `budget`.
    pub async fn the_permission_hook_of(&self, session_id: &str, budget: Duration) -> AHeldHook {
        let request = AwaitQuestionAnswerRequest {
            session_id: session_id.to_string(),
            hook_token: self
                .metadata_of(session_id)
                .hook_token
                .unwrap_or_else(|| panic!("{session_id} records its hook token")),
            os_user: current_os_user(),
            request_id: PERMISSION_REQUEST_ID.to_string(),
            permission_suggestions_json: String::new(),
        };
        let activity = self.service.activity_service();
        let started = tokio::time::Instant::now();
        let answered = tokio::time::timeout(
            budget,
            activity.await_question_answer(Request::direct(request)),
        )
        .await;
        let held = started.elapsed();
        AHeldHook {
            outcome: answered.ok().map(|answer| {
                answer
                    .map(|response| response.into_inner().outcome)
                    .map_err(|status| status.message)
            }),
            held,
        }
    }

    /// The `timeout` the hooks file written into `session_id`'s worktree gives its
    /// `PermissionRequest` hook — how long Claude lets that hook run.
    pub fn permission_hook_timeout_of(&self, session_id: &str) -> u64 {
        let settings = self
            .worktree_of(session_id)
            .join(".claude/settings.local.json");
        let json: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&settings)
                .unwrap_or_else(|e| panic!("{} is written: {e}", settings.display())),
        )
        .expect("the hooks file is JSON");
        json["hooks"]["PermissionRequest"][0]["hooks"][0]["timeout"]
            .as_u64()
            .unwrap_or_else(|| panic!("the PermissionRequest hook has a timeout: {json:#}"))
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

/// The request id a held permission hook waits on.
pub const PERMISSION_REQUEST_ID: &str = "req-cargo-publish";

/// A permission hook held in `AwaitQuestionAnswer`.
#[derive(Debug)]
pub struct AHeldHook {
    /// The outcome it was released with, or `None` when it was still held at the budget.
    pub outcome: Option<Result<i32, String>>,
    /// How long it was held.
    pub held: Duration,
}

impl AHeldHook {
    /// The hold in whole seconds — a hold of `n` seconds and a few milliseconds of RPC is `n`.
    pub fn whole_seconds(&self) -> u64 {
        self.held.as_secs()
    }

    /// The `TIMED_OUT` outcome, as the hook reads it.
    pub fn timed_out() -> Option<Result<i32, String>> {
        Some(Ok(AwaitQuestionOutcome::TimedOut as i32))
    }
}

fn a_daemon_config_without_agent_sections() -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let user = current_os_user();
    let yaml = "allowed_tools:\n  - path: /bin/true\n    label: true\n";
    let config_path = dir.path().join("daemon.yaml");
    std::fs::write(&config_path, yaml).unwrap();
    std::fs::write(
        dir.path().join("users.yaml"),
        format!("- github_user: \"{user}\"\n  os_user: \"{user}\"\n"),
    )
    .unwrap();
    let config = DaemonConfig::load(&config_path).expect("config must parse");
    (dir, config)
}

/// This daemon's signing identity (`#agent-tools 14/19`): a project's main is given a credential of
/// its own, minted under the verified caller, so the daemon must be able to verify the caller's token
/// and sign the agent's.
fn this_daemons_session_tokens() -> &'static tddy_daemon_auth::SessionTokens {
    static TOKENS: std::sync::OnceLock<tddy_daemon_auth::SessionTokens> =
        std::sync::OnceLock::new();
    TOKENS.get_or_init(|| {
        let home = tempfile::tempdir().expect("a directory for the daemon's key");
        let key = tddy_daemon_auth::DaemonSigningKey::load_or_generate(
            &home.path().join("signing_key.pem"),
        )
        .expect("the daemon generates its keypair");
        tddy_daemon_auth::SessionTokens::new(
            &key,
            Arc::new(tddy_daemon_auth::StandaloneKeyDirectory),
        )
    })
}

/// The credential a caller presents when it sets a project's main up, signed with
/// [`this_daemons_session_tokens`]'s key.
fn a_caller_token() -> &'static str {
    static TOKEN: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    TOKEN.get_or_init(|| {
        this_daemons_session_tokens()
            .signer()
            .mint_access(&tddy_github::GitHubUser {
                id: 4242,
                login: current_os_user(),
                avatar_url: "https://avatars.githubusercontent.com/u/4242?v=4".to_string(),
                name: "Test User".to_string(),
            })
    })
}

fn a_session_host(config: DaemonConfig, tddy_home: PathBuf) -> DaemonSessionHost {
    let sessions_base = tddy_home.clone();
    let user = current_os_user();
    DaemonSessionHost::new(
        config.clone(),
        Arc::new(move |_| Some(sessions_base.clone())),
        tddy_home,
        Arc::new(move |token| {
            (token == VALID_TOKEN || token == a_caller_token()).then(|| user.clone())
        }),
        None,
        tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        tddy_session_lifecycle::peer_routing::PeerRouting::local_only(&config),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        Arc::new(CliSessionManager::new()),
        tddy_agent_launch::host_coding_agents::no_coding_agents(),
    )
    .with_session_tokens(this_daemons_session_tokens().clone())
}

fn register_project(projects_dir: &Path, repo_path: &Path) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let yaml = format!(
        "projects:\n  - project_id: {TEST_PROJECT_ID}\n    name: yaml-only-agent\n    git_url: \"\"\n    main_repo_path: {}\n    main_branch_ref: origin/main\n",
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
