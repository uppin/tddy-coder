//! Daemon configuration — users, tools, LiveKit, GitHub, etc.

#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;
#[cfg(test)]
use std::time::Duration;

use tddy_core::LogConfig;

pub use crate::live_users::LiveUsers;

fn default_spawn_mouse() -> bool {
    true
}

fn default_spawn_worker_request_timeout_secs() -> u64 {
    300
}

fn default_peer_forward_timeout_secs() -> u64 {
    crate::peer_forwarding::PEER_FORWARD_TIMEOUT.as_secs()
}

fn default_common_room_set_metadata_timeout_secs() -> u64 {
    60
}

/// An optional string whose empty or whitespace-only value means "not set".
fn blank_as_none<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value: Option<String> = serde::Deserialize::deserialize(deserializer)?;
    Ok(value.filter(|s| !s.trim().is_empty()))
}

/// How long a freshly spawned session process is watched for an immediate exit before it is
/// reported as started. Long enough to catch a binary that dies on its own arguments, short
/// enough not to stall the caller.
pub fn default_spawn_startup_grace_period_ms() -> u64 {
    500
}

/// How often that watch checks. Fine-grained so a healthy spawn returns promptly.
pub fn default_spawn_startup_poll_interval_ms() -> u64 {
    25
}

pub use crate::config::timings::DEFAULT_SESSION_ROOM_GIT_TIMEOUT;

fn default_codex_oauth_loopback_proxy_eligible() -> bool {
    true
}

pub use crate::config::sections::{IndexDaemonConfig, ProjectHomeConfig, RelayConfig};

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct DaemonConfig {
    #[serde(default)]
    pub listen: ListenConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_bundle_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub livekit: Option<LiveKitConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github: Option<GitHubConfig>,
    /// Directory holding server-side auth state: this daemon's Ed25519 session-token signing key
    /// (`signing_key.pem`, mode `0600`, generated on first boot) and each user's credential vault
    /// (`credentials-<hex login>.vault`, mode `0600`), sealing the GitHub access token their login
    /// granted — the credential PR-status reads act with. Unset means no GitHub token is retained, so PR status reports itself
    /// *unavailable* for a real login — and the signing key lives in the `auth` directory under
    /// `tddy_data_dir` instead. Session tokens themselves are stateless and are never stored here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_storage: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log: Option<LogConfig>,
    /// Path to a `tddy-coder` config file (e.g. `dev.config.yaml`) whose `log:` block is passed
    /// verbatim to spawned children as their `--config`, so operators own child log routing (e.g.
    /// routing `libwebrtc*` / `livekit*` chatter to a separate file at INFO). Relative to the
    /// daemon's cwd. When unset, the spawner synthesizes a minimal log config from the daemon `log:`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coder_config_path: Option<PathBuf>,
    /// The GitHub-login → OS-user map every token-gated RPC authorizes by. A shared holder rather
    /// than a `Vec`: cloning this config shares these rows, so a row enrolled at run time is seen
    /// by every service built from it — see [`LiveUsers`].
    #[serde(default, skip_serializing_if = "LiveUsers::is_empty")]
    pub users: LiveUsers,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_tools: Vec<AllowedTool>,
    /// Directory holding this deployment's tddy binaries (`tddy-tools`, `tddy-sandbox-runner`,
    /// `tddy-index-daemon`, …). Absent — the ordinary case — means "beside the running daemon",
    /// which is how `./install` ships them and where a dev `target/debug` tree has them.
    ///
    /// Stated as its own key because the answer used to be *derived* from `allowed_tools[0].path`,
    /// a UI menu entry naming which `tddy-coder` build to offer an operator. That made the
    /// toolchain's location a side effect of a dropdown, and — since those paths are relative in
    /// every dev config — produced commands that resolved against each consumer's own cwd. See
    /// [`crate::toolchain`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toolchain_dir: Option<String>,
    /// Allowed coding backends / agents (`tddy-coder --agent` values), with optional UI labels.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_agents: Vec<AllowedAgent>,
    /// Relative to each OS user's home directory (e.g. `repos` → `~/repos/`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repos_base_path: Option<String>,
    /// When true (default), spawned `tddy-*` processes receive `--mouse` (browser / touch terminals).
    #[serde(default = "default_spawn_mouse")]
    pub spawn_mouse: bool,
    /// Max seconds for clone/spawn work (`SpawnClient` round-trip or direct `spawn_as_user` /
    /// `clone_as_user`). Minimum effective value is 1.
    #[serde(default = "default_spawn_worker_request_timeout_secs")]
    pub spawn_worker_request_timeout_secs: u64,
    /// Max seconds a forwarded unary RPC to a peer daemon, or the opening of a forwarded server
    /// stream, may wait for the peer to answer before failing with `DeadlineExceeded`. Minimum
    /// effective value is 1. The per-frame idle timeout of a forwarded stream is not configurable.
    #[serde(default = "default_peer_forward_timeout_secs")]
    pub peer_forward_timeout_secs: u64,
    /// How long a freshly spawned session process is watched for an immediate exit before the
    /// spawn is reported as successful. Minimum effective value is 1ms.
    #[serde(default = "default_spawn_startup_grace_period_ms")]
    pub spawn_startup_grace_period_ms: u64,
    /// How often that watch polls. Minimum effective value is 1ms.
    #[serde(default = "default_spawn_startup_poll_interval_ms")]
    pub spawn_startup_poll_interval_ms: u64,
    /// Timing budget for the specialized-subagent warm-up gate (see [`AgentWarmupConfig`]).
    #[serde(default)]
    pub agent_warmup: AgentWarmupConfig,
    /// Cadence and patience of the session-room poll loop (see [`SessionRoomConfig`]).
    #[serde(default)]
    pub session_room: SessionRoomConfig,
    /// Stable id for this daemon in a shared LiveKit room (multi-host). When set, spawned tools
    /// and ConnectSession use `daemon-{instance_id}-{session_id}` as LiveKit server identity.
    /// Overridable at startup via the `TDDY_DAEMON_INSTANCE_ID` env var (see `apply_env_overrides`
    /// in `runtime.rs`); falls back to the machine short hostname when neither is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daemon_instance_id: Option<String>,
    /// When true, append `-<unix_ms>` once per process to the resolved instance id (YAML override
    /// or hostname default) so multiple local daemons avoid LiveKit `DuplicateIdentity` in
    /// `common_room`. Intended for desktop dev / overlapping CLI embed + standalone runs.
    #[serde(default)]
    pub daemon_instance_id_append_startup_timestamp: bool,
    /// When true (default), this daemon may bind the Codex OAuth loopback TCP port (e.g. 127.0.0.1:1455)
    /// and relay browser callbacks to session hosts via LiveKit. Set false when another process
    /// already uses that port or this instance must not act as the operator-side OAuth proxy.
    #[serde(default = "default_codex_oauth_loopback_proxy_eligible")]
    pub codex_oauth_loopback_proxy_eligible: bool,
    /// Optional Telegram bot notifications (see `telegram_notifier` module).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub telegram: Option<TelegramConfig>,
    /// Claude Code CLI session configuration (spawning interactive `claude` processes in PTYs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_cli: Option<agent_cli::ClaudeCliConfig>,
    /// Cursor Agent CLI session configuration (spawning interactive `agent` processes in PTYs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor_cli: Option<agent_cli::CursorCliConfig>,
    /// Further CLI harnesses (OpenCode, Muse Code), keyed by session type — `opencode-cli:`,
    /// `muse-cli:`. Absent entries run the harness's default binary.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub cli_harnesses: std::collections::BTreeMap<String, agent_cli::CliHarnessConfig>,
    /// When set, this daemon runs in relay mode: no web bundle, idle-timeout auto-shutdown,
    /// forwards RPCs to a remote peer via LiveKit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relay: Option<RelayConfig>,
    /// Override the tddy home data directory. When absent, the profile default is used
    /// (`tmp/.tddy` in debug builds, `$HOME/.tddy` in release builds).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tddy_data_dir: Option<PathBuf>,
    /// Screen-sharing bridge binary configuration (VNC + RDP paths).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen_sharing: Option<ScreenSharingConfig>,
    /// Git behavior for daemon-side remote operations (see `GitConfig`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<GitConfig>,
    /// Credential propagation between daemons (see [`KeyringConfig`]). Absent = this daemon
    /// propagates credentials to no peer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyring: Option<KeyringConfig>,
    /// Linux rootless cgroups sandbox delegation (see `SandboxCgroupConfig`). None = runtime defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox_cgroup: Option<SandboxCgroupConfig>,
    /// Local Unix-domain socket transport (see `LocalConfig`). Absent = defaults (socket path
    /// resolved at bind time).
    #[serde(default)]
    pub local: LocalConfig,
    /// Privileged broker this daemon delegates spawning and cgroup work to (see
    /// `SupervisorClientConfig`). Absent = no supervisor on this host: the daemon keeps its own
    /// forked spawn worker and whatever privilege its own unit grants it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supervisor: Option<SupervisorClientConfig>,
    /// The warm code-intelligence index daemon this daemon starts, supervises and stops (see
    /// [`IndexDaemonConfig`]). Absent = [`IndexDaemonConfig::default`]; the section only overrides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index_daemon: Option<IndexDaemonConfig>,
    /// Project-home sessions (`#project-home` 6/7, see [`ProjectHomeConfig`]). Absent =
    /// [`ProjectHomeConfig::default`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_home: Option<ProjectHomeConfig>,

    /// Browser DEBUG mask exposed to tddy-web via `GET /api/config` (`debug` field). A `debug`-package
    /// namespace mask (e.g. `tddy:term:*`, or `tddy:term:write,tddy:term:resize`) that enables scoped
    /// `[tddy]` diagnostics in the browser. Mainly for `./web-dev` to debug terminal garbling /
    /// misalignment. The browser invalidates any local override when this value changes. None = off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debug: Option<String>,

    /// Largest single session attachment this host will serve or materialize, in bytes. Bounds
    /// `StreamReadHostDocument` and is advertised to clients so a picker can refuse an oversized
    /// document before uploading it. A host setting rather than a constant: hosts differ in disk
    /// and link capacity, and a browser cannot guess. Note this is a *policy* cap — the unary
    /// `ReadHostDocument` keeps its own, smaller `MAX_HOST_DOCUMENT_BYTES` ceiling, which is a
    /// transport message-size limit.
    #[serde(default = "default_max_attachment_bytes")]
    pub max_attachment_bytes: u64,
}

/// Where to reach `tddy-supervisor` (`supervisor:` YAML section).
///
/// Presence of this section is the switch: with it, session spawning, repo cloning and sandbox
/// jailing all go to the supervisor, and a supervisor that cannot be reached fails the operation.
/// There is deliberately no fallback — quietly spawning a session as the daemon user instead of
/// the requested user would turn an outage into a silent loss of isolation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct SupervisorClientConfig {
    /// Absolute path of the supervisor's unix socket, e.g. `/run/tddy-supervisor.sock`.
    pub socket_path: PathBuf,
}

/// 64 MiB — comfortably above a screenshot, a PDF spec or a log excerpt, well below anything that
/// would strain a host's disk or a LiveKit data channel.
fn default_max_attachment_bytes() -> u64 {
    64 * 1024 * 1024
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            listen: ListenConfig::default(),
            web_bundle_path: None,
            toolchain_dir: None,
            livekit: None,
            github: None,
            auth_storage: None,
            log: None,
            coder_config_path: None,
            users: LiveUsers::default(),
            allowed_tools: Vec::new(),
            allowed_agents: Vec::new(),
            repos_base_path: None,
            spawn_mouse: true,
            spawn_worker_request_timeout_secs: default_spawn_worker_request_timeout_secs(),
            peer_forward_timeout_secs: default_peer_forward_timeout_secs(),
            spawn_startup_grace_period_ms: default_spawn_startup_grace_period_ms(),
            spawn_startup_poll_interval_ms: default_spawn_startup_poll_interval_ms(),
            agent_warmup: AgentWarmupConfig::default(),
            session_room: SessionRoomConfig::default(),
            daemon_instance_id: None,
            daemon_instance_id_append_startup_timestamp: false,
            codex_oauth_loopback_proxy_eligible: default_codex_oauth_loopback_proxy_eligible(),
            telegram: None,
            claude_cli: None,
            cursor_cli: None,
            cli_harnesses: std::collections::BTreeMap::new(),
            relay: None,
            tddy_data_dir: None,
            screen_sharing: None,
            git: None,
            keyring: None,
            sandbox_cgroup: None,
            local: LocalConfig::default(),
            supervisor: None,
            index_daemon: None,
            project_home: None,
            debug: None,
            max_attachment_bytes: default_max_attachment_bytes(),
        }
    }
}

impl DaemonConfig {
    /// Map the optional `sandbox_cgroup:` section onto the cgroups backend's [`CgroupConfig`]. An
    /// absent section yields defaults (empty `CgroupConfig`), so the backend derives everything at
    /// runtime. Only the delegation fields map here; the default-limit overrides feed the plan's
    /// `ResourceLimits` fallback separately.
    pub fn sandbox_cgroup_config(&self) -> tddy_sandbox::CgroupConfig {
        match &self.sandbox_cgroup {
            Some(sc) => tddy_sandbox::CgroupConfig {
                base_override: sc.base_path.clone(),
                mount_root: sc.mount_root.clone(),
                controllers: sc.controllers.clone(),
                supervisor_leaf: sc.supervisor_leaf.clone(),
            },
            None => tddy_sandbox::CgroupConfig::default(),
        }
    }
}

/// Telegram Bot API integration (teloxide). Loaded from daemon YAML under `telegram:`.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct TelegramConfig {
    #[serde(default)]
    pub enabled: bool,
    pub bot_token: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chat_ids: Vec<i64>,
}

pub use crate::config::screen_sharing::{
    resolve_rdp_binary_path, resolve_vnc_binary_path, resolve_x11vnc_binary_path,
    ScreenSharingConfig,
};

pub(crate) mod access;
mod agent_cli;
mod daemon_methods;
mod env_overrides;
mod sandbox_paths;
mod screen_sharing;
mod sections;
mod timings;
#[cfg(test)]
use agent_cli::{
    choose_claude_binary, choose_cursor_binary, is_wrapper_shim_dir, ClaudeBinaryChoice,
    CursorBinaryChoice,
};
pub use agent_cli::{
    resolve_claude_binary_path, resolve_claude_home_dir, resolve_cursor_binary_path,
    resolve_cursor_cli_daemon_url, resolve_cursor_cli_tddy_tools_path, resolve_cursor_home_dir,
    ClaudeCliConfig, CursorCliConfig, CLAUDE_BINARY_ENV, CLAUDE_HOME_ENV, CURSOR_BINARY_ENV,
    CURSOR_HOME_ENV,
};

use crate::config::access::AllowedTool;
#[cfg(test)]
use crate::config::sandbox_paths::resolve_sandbox_config_path_with;
pub use crate::config::sandbox_paths::{
    resolve_sandbox_config_path, sandbox_config_os_token, SANDBOX_CONFIG_BASENAME,
    SANDBOX_CONFIG_ENV,
};
use crate::config::sections::GitConfig;
use crate::config::sections::KeyringConfig;
use crate::config::sections::LocalConfig;
use crate::config::sections::SandboxCgroupConfig;
use crate::config::timings::AgentWarmupConfig;
use crate::config::timings::SessionRoomConfig;

#[derive(Debug, Default, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct ListenConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_host: Option<String>,
    /// Externally-reachable HTTP base URL peer daemons use to reach this daemon's Connect-HTTP
    /// surface (today: `auth.LiveKitTokenService/MintLiveKitToken`, used by `tddy-remote-git-repo`
    /// to mint the common-room LiveKit token before driving `remote_git.RemoteGitService/Serve`).
    ///
    /// When unset, falls back to `http://127.0.0.1:{web_port}` — correct for single-host deployments
    /// and tests where every daemon is in-process on loopback, but unreachable from a peer on
    /// another host. Operators with peers on other hosts set this to the daemon's external URL
    /// (e.g. `http://ws-01.internal:9000`). The facilitating daemon publishes this in
    /// `AgentClonePlacement.facilitating_daemon_url` so an owning daemon that has never seen the
    /// project can clone it (PRD AC37).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub advertise_url: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct LiveKitConfig {
    /// Whether this daemon joins its common room at all.
    ///
    /// Defaults to **`false`**, so the common room is opt-in: a block carrying url, key, secret and
    /// room names a room the operator *could* join, not one this daemon does. Turning it off is
    /// therefore a one-key edit rather than the deletion of a working block — every other field
    /// survives, and turning it back on restores exactly what was there.
    ///
    /// It governs the **common room only**. Session tokens are signed with the daemon's own key and
    /// not with anything in this block, so a disabled daemon — or one with no `livekit:` block at
    /// all — still authenticates its own gated RPCs, including the one an operator re-enables it
    /// from. Per-session rooms, screen sharing and the rooms panel read the same block for their
    /// own purposes and are not governed by it.
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_secret: Option<String>,
    /// URL handed to clients and session terminal bridges in place of `url`. A blank value reads
    /// as absent: the Settings form saves an empty field as `''`, and kept as `Some("")` it would
    /// shadow `url` for every reader that prefers the public URL.
    #[serde(
        default,
        deserialize_with = "blank_as_none",
        skip_serializing_if = "Option::is_none"
    )]
    pub public_url: Option<String>,
    /// Shared LiveKit room for presence (browser + tddy-* tools). Exposed to web as `common_room` in `/api/config`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub common_room: Option<String>,
    /// Total wall-clock time the daemon spends retrying `set_metadata` for the common-room advertisement
    /// in one publish round. The LiveKit Rust SDK uses a fixed **5 s** timeout **per attempt**; this value
    /// caps how long we keep trying before treating the round as failed (minimum effective **1** second).
    #[serde(default = "default_common_room_set_metadata_timeout_secs")]
    pub common_room_set_metadata_timeout_secs: u64,
}

impl Default for LiveKitConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            url: None,
            api_key: None,
            api_secret: None,
            public_url: None,
            common_room: None,
            common_room_set_metadata_timeout_secs: default_common_room_set_metadata_timeout_secs(),
        }
    }
}

impl LiveKitConfig {
    /// Whether `livekit` names a common room this daemon may join.
    ///
    /// The **one** predicate every join path asks, so "is the common room on?" is decided in a
    /// single place rather than re-derived per call site. It answers only the operator's switch —
    /// whether the block is *complete* enough to join is [`CommonRoomTarget::from_livekit`]'s
    /// question, and the two are deliberately separate: a disabled daemon and a misconfigured one
    /// are different operator problems and must be reported differently.
    pub fn common_room_enabled(livekit: Option<&Self>) -> bool {
        livekit.is_some_and(|livekit| livekit.enabled)
    }
}

pub use crate::config::access::{AllowedAgent, GitHubConfig, UserMapping};

/// The single source of truth for "what tool should a session run when nothing more specific was
/// requested" — used by `StartSession`/`ResumeSession`/Telegram workflow spawn alike, so they
/// can never independently drift on the default (see `resume_session`'s prior bug: it
/// hardcoded a bare `"tddy-coder"` fallback that only resolves when the binary happens to be on
/// `PATH`, unlike this method's `allowed_tools`-driven default, e.g. `"target/debug/tddy-coder"`
/// in dev — a session resumed after its metadata never recorded an explicit `tool` value failed
/// to spawn with "No such file or directory").
#[cfg(test)]
mod default_tool_path_tests {
    use super::*;
    use crate::config::access::AllowedTool;

    #[test]
    fn default_tool_path_uses_the_first_configured_allowed_tool() {
        // Given — dev.daemon.yaml's real shape: a debug-build tool path configured first
        let config = DaemonConfig {
            allowed_tools: vec![AllowedTool {
                path: "target/debug/tddy-coder".to_string(),
                label: Some("tddy-coder (debug)".to_string()),
            }],
            ..Default::default()
        };

        // When
        let default_path = config.default_tool_path();

        // Then
        assert_eq!(default_path, "target/debug/tddy-coder");
    }

    #[test]
    fn default_tool_path_falls_back_to_a_bare_binary_name_when_no_tools_are_configured() {
        // Given — no allowed_tools configured at all
        let config = DaemonConfig {
            allowed_tools: vec![],
            ..Default::default()
        };

        // When
        let default_path = config.default_tool_path();

        // Then
        assert_eq!(default_path, "tddy-coder");
    }
}

#[cfg(test)]
mod telegram_env_tests {
    use super::*;
    use crate::config::env_overrides::merge_telegram_env;

    #[test]
    fn token_from_env_creates_telegram_and_enables_by_default() {
        let mut c = DaemonConfig::default();
        merge_telegram_env(&mut c, Some("tok"), None, None);
        let tg = c.telegram.as_ref().expect("telegram");
        assert_eq!(tg.bot_token, "tok");
        assert!(tg.enabled);
        assert!(tg.chat_ids.is_empty());
    }

    #[test]
    fn token_override_on_existing_yaml_does_not_auto_enable() {
        let mut c = DaemonConfig {
            telegram: Some(TelegramConfig {
                enabled: false,
                bot_token: "old".to_string(),
                chat_ids: vec![1],
            }),
            ..Default::default()
        };
        merge_telegram_env(&mut c, Some("new"), None, None);
        let tg = c.telegram.as_ref().expect("telegram");
        assert_eq!(tg.bot_token, "new");
        assert!(!tg.enabled);
        assert_eq!(tg.chat_ids, vec![1]);
    }

    #[test]
    fn chat_ids_from_env_merge_into_existing_block() {
        let mut c = DaemonConfig {
            telegram: Some(TelegramConfig {
                enabled: true,
                bot_token: "t".to_string(),
                chat_ids: vec![1],
            }),
            ..Default::default()
        };
        merge_telegram_env(&mut c, None, Some("-100, 42"), None);
        let tg = c.telegram.as_ref().expect("telegram");
        assert_eq!(tg.chat_ids, vec![-100, 42]);
    }

    #[test]
    fn enabled_false_disables_even_when_created_from_env_token() {
        let mut c = DaemonConfig::default();
        merge_telegram_env(&mut c, Some("tok"), None, Some("false"));
        let tg = c.telegram.as_ref().expect("telegram");
        assert_eq!(tg.bot_token, "tok");
        assert!(!tg.enabled);
    }
}

#[cfg(test)]
mod spawn_timeout_tests {
    use super::*;

    #[test]
    fn default_spawn_worker_request_timeout_is_300_seconds() {
        let c = DaemonConfig::default();
        assert_eq!(c.spawn_worker_request_timeout_secs, 300);
        assert_eq!(c.spawn_worker_request_timeout().as_secs(), 300);
    }

    #[test]
    fn spawn_worker_request_timeout_clamps_zero_to_one_second() {
        let c = DaemonConfig {
            spawn_worker_request_timeout_secs: 0,
            ..Default::default()
        };
        assert_eq!(c.spawn_worker_request_timeout().as_secs(), 1);
    }

    #[test]
    fn common_room_set_metadata_budget_defaults_to_60_seconds() {
        let c = DaemonConfig::default();
        assert_eq!(c.common_room_set_metadata_attempt_budget().as_secs(), 60);
        let c = DaemonConfig {
            livekit: Some(LiveKitConfig::default()),
            ..Default::default()
        };
        assert_eq!(
            c.livekit
                .as_ref()
                .unwrap()
                .common_room_set_metadata_timeout_secs,
            60
        );
        assert_eq!(c.common_room_set_metadata_attempt_budget().as_secs(), 60);
    }

    #[test]
    fn common_room_set_metadata_budget_clamps_zero_to_one_second() {
        let c = DaemonConfig {
            livekit: Some(LiveKitConfig {
                common_room_set_metadata_timeout_secs: 0,
                ..LiveKitConfig::default()
            }),
            ..Default::default()
        };
        assert_eq!(c.common_room_set_metadata_attempt_budget().as_secs(), 1);
    }

    #[test]
    fn codex_oauth_loopback_proxy_eligible_defaults_true() {
        let c = DaemonConfig::default();
        assert!(c.codex_oauth_loopback_proxy_eligible);
    }
}

#[cfg(test)]
mod startup_and_warmup_timing_tests {
    use super::*;

    use crate::config::env_overrides::merge_timing_env;
    use tddy_discovery::warmup::WarmupOptions;

    fn a_daemon_configured_by(yaml: &str) -> DaemonConfig {
        serde_yaml::from_str(yaml).expect("parse daemon config")
    }

    #[test]
    fn a_default_configuration_watches_a_spawn_for_half_a_second() {
        // Given a daemon configured with nothing at all
        let config = DaemonConfig::default();

        // Then the startup watch keeps the values the spawner used before they were configurable
        assert_eq!(
            config.spawn_startup_grace_period(),
            Duration::from_millis(500)
        );
        assert_eq!(
            config.spawn_startup_poll_interval(),
            Duration::from_millis(25)
        );
    }

    #[test]
    fn a_default_configuration_warms_agents_up_on_the_library_defaults() {
        // Given a daemon configured with nothing at all
        let config = DaemonConfig::default();

        // Then the warm-up gate gets exactly the budget it had before it was configurable —
        // making the knob a way to say something else, not a change of production behaviour
        assert_eq!(config.agent_warmup_options(), WarmupOptions::default());
    }

    #[test]
    fn a_yaml_startup_watch_overrides_the_defaults() {
        // Given a daemon YAML naming its own startup watch
        let config = a_daemon_configured_by(
            "spawn_startup_grace_period_ms: 2500\nspawn_startup_poll_interval_ms: 100\n",
        );

        // Then both values come from the file
        assert_eq!(
            config.spawn_startup_grace_period(),
            Duration::from_millis(2500)
        );
        assert_eq!(
            config.spawn_startup_poll_interval(),
            Duration::from_millis(100)
        );
    }

    #[test]
    fn a_yaml_warmup_section_overrides_the_defaults() {
        // Given a daemon YAML naming its own warm-up budget
        let config = a_daemon_configured_by(
            "agent_warmup:\n  timeout_secs: 5\n  retry_interval_ms: 50\n  request_timeout_secs: 3\n",
        );

        // Then the gate is handed that budget
        assert_eq!(
            config.agent_warmup_options(),
            WarmupOptions {
                timeout: Duration::from_secs(5),
                retry_interval: Duration::from_millis(50),
                request_timeout: Duration::from_secs(3),
            }
        );
    }

    #[test]
    fn a_partial_warmup_section_keeps_the_default_for_what_it_omits() {
        // Given a daemon YAML naming only the total budget
        let config = a_daemon_configured_by("agent_warmup:\n  timeout_secs: 5\n");

        // Then the omitted fields stay at their production values
        assert_eq!(
            config.agent_warmup_options(),
            WarmupOptions {
                timeout: Duration::from_secs(5),
                ..WarmupOptions::default()
            }
        );
    }

    #[test]
    fn a_zero_startup_watch_is_clamped_to_one_millisecond() {
        // Given a daemon YAML asking for no watch at all
        let config = a_daemon_configured_by(
            "spawn_startup_grace_period_ms: 0\nspawn_startup_poll_interval_ms: 0\n",
        );

        // Then the watch still runs once, rather than dividing by zero or spinning
        assert_eq!(
            config.spawn_startup_grace_period(),
            Duration::from_millis(1)
        );
        assert_eq!(
            config.spawn_startup_poll_interval(),
            Duration::from_millis(1)
        );
    }

    /// A stand-in for the process environment: only the named variables are set.
    fn an_environment_with<'a>(
        vars: &'a [(&'a str, &'a str)],
    ) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            vars.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_string())
        }
    }

    #[test]
    fn an_environment_override_wins_over_the_yaml_it_was_loaded_with() {
        // Given a daemon configured by YAML, and an environment naming other values
        let mut config = a_daemon_configured_by(
            "spawn_startup_grace_period_ms: 2500\nagent_warmup:\n  timeout_secs: 60\n",
        );

        // When the environment layer is applied
        merge_timing_env(
            &mut config,
            an_environment_with(&[
                ("TDDY_SPAWN_STARTUP_GRACE_PERIOD_MS", "7000"),
                ("TDDY_SPAWN_STARTUP_POLL_INTERVAL_MS", "5"),
                ("TDDY_AGENT_WARMUP_TIMEOUT_SECS", "9"),
                ("TDDY_AGENT_WARMUP_RETRY_INTERVAL_MS", "40"),
                ("TDDY_AGENT_WARMUP_REQUEST_TIMEOUT_SECS", "8"),
            ]),
        );

        // Then the environment values are the ones in effect
        assert_eq!(
            config.spawn_startup_grace_period(),
            Duration::from_millis(7000)
        );
        assert_eq!(
            config.spawn_startup_poll_interval(),
            Duration::from_millis(5)
        );
        assert_eq!(
            config.agent_warmup_options(),
            WarmupOptions {
                timeout: Duration::from_secs(9),
                retry_interval: Duration::from_millis(40),
                request_timeout: Duration::from_secs(8),
            }
        );
    }

    #[test]
    fn an_empty_environment_leaves_the_yaml_values_alone() {
        // Given a daemon configured by YAML, and an environment naming none of the knobs
        let mut config = a_daemon_configured_by(
            "spawn_startup_grace_period_ms: 2500\nagent_warmup:\n  timeout_secs: 60\n",
        );

        // When the environment layer is applied
        merge_timing_env(&mut config, an_environment_with(&[]));

        // Then the file's values survive
        assert_eq!(
            config.spawn_startup_grace_period(),
            Duration::from_millis(2500)
        );
        assert_eq!(
            config.agent_warmup_options().timeout,
            Duration::from_secs(60)
        );
    }

    #[test]
    fn an_unparseable_environment_override_keeps_the_yaml_value() {
        // Given a daemon configured by YAML, and an environment naming a value that is not a number
        let mut config = a_daemon_configured_by("spawn_startup_grace_period_ms: 2500\n");

        // When the environment layer is applied
        merge_timing_env(
            &mut config,
            an_environment_with(&[("TDDY_SPAWN_STARTUP_GRACE_PERIOD_MS", "soon")]),
        );

        // Then the file's value survives rather than silently collapsing to a default
        assert_eq!(
            config.spawn_startup_grace_period(),
            Duration::from_millis(2500)
        );
    }

    /// Writes `yaml` to a file and loads it the way the daemon does, so the load-time checks run.
    fn a_daemon_loaded_from(yaml: &str) -> anyhow::Result<DaemonConfig> {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("daemon.yaml");
        std::fs::write(&path, yaml).expect("write daemon.yaml");
        DaemonConfig::load(&path)
    }

    #[test]
    fn a_default_configuration_measures_a_hosted_worktree_every_two_seconds() {
        // Given a daemon configured with nothing at all
        let config = DaemonConfig::default();

        // Then a session room keeps the cadence and patience it shipped with — the section is a
        // way to say something else, not a change of production behaviour
        assert_eq!(
            config.session_room_poll_interval(),
            Duration::from_millis(2000)
        );
        assert_eq!(
            config.session_room_git_timeout(),
            Duration::from_millis(5000)
        );
    }

    #[test]
    fn a_yaml_session_room_section_overrides_the_defaults() {
        // Given a daemon YAML naming its own cadence and patience
        let config = a_daemon_loaded_from(
            "session_room:\n  poll_interval_ms: 500\n  git_timeout_ms: 1500\n",
        )
        .expect("a configuration above the floor must load");

        // Then both values come from the file
        assert_eq!(
            config.session_room_poll_interval(),
            Duration::from_millis(500)
        );
        assert_eq!(
            config.session_room_git_timeout(),
            Duration::from_millis(1500)
        );
    }

    #[test]
    fn a_session_room_poll_faster_than_the_floor_is_refused_by_name() {
        // Given a daemon YAML asking for a poll interval of zero — the typo that would otherwise
        // become a one-millisecond loop shelling out to git
        let refused = a_daemon_loaded_from("session_room:\n  poll_interval_ms: 0\n")
            .expect_err("a sub-floor poll interval must not load");

        // Then the daemon refuses to start and says which field and which floor, rather than
        // silently running something the operator did not ask for
        let message = format!("{refused:#}");
        assert!(
            message.contains("session_room.poll_interval_ms") && message.contains("100"),
            "the refusal must name the field and the floor; got {message}"
        );
    }

    #[test]
    fn a_session_room_git_timeout_below_the_floor_is_refused_too() {
        // Given a daemon YAML whose git budget is too short for git to ever answer in
        let refused =
            a_daemon_loaded_from("session_room:\n  poll_interval_ms: 2000\n  git_timeout_ms: 5\n")
                .expect_err("a sub-floor git timeout must not load");

        // Then it is refused by name, for the same reason
        assert!(
            format!("{refused:#}").contains("session_room.git_timeout_ms"),
            "the refusal must name the offending field"
        );
    }

    #[test]
    fn a_session_room_environment_override_wins_over_the_yaml_it_was_loaded_with() {
        // Given a daemon configured by YAML, and an environment naming other values
        let mut config = a_daemon_configured_by(
            "session_room:\n  poll_interval_ms: 2000\n  git_timeout_ms: 5000\n",
        );

        // When the environment layer is applied
        merge_timing_env(
            &mut config,
            an_environment_with(&[
                ("TDDY_SESSION_ROOM_POLL_INTERVAL_MS", "750"),
                ("TDDY_SESSION_ROOM_GIT_TIMEOUT_MS", "3000"),
            ]),
        );

        // Then the environment values are the ones in effect — the same defaults ← YAML ← TDDY_*
        // layering every other timing knob has
        assert_eq!(
            config.session_room_poll_interval(),
            Duration::from_millis(750)
        );
        assert_eq!(
            config.session_room_git_timeout(),
            Duration::from_millis(3000)
        );
    }

    #[test]
    fn a_session_room_environment_override_below_the_floor_keeps_the_yaml_value() {
        // Given a daemon configured by YAML, and an environment asking for a one-millisecond poll
        let mut config = a_daemon_configured_by("session_room:\n  poll_interval_ms: 2000\n");

        // When the environment layer is applied
        merge_timing_env(
            &mut config,
            an_environment_with(&[("TDDY_SESSION_ROOM_POLL_INTERVAL_MS", "1")]),
        );

        // Then the file's value survives: a floor an environment variable could step under would
        // not be a floor
        assert_eq!(
            config.session_room_poll_interval(),
            Duration::from_millis(2000)
        );
    }

    #[test]
    fn a_zero_warmup_budget_is_clamped_to_one_unit() {
        // Given a daemon YAML asking for a zero warm-up budget
        let config = a_daemon_configured_by(
            "agent_warmup:\n  timeout_secs: 0\n  retry_interval_ms: 0\n  request_timeout_secs: 0\n",
        );

        // Then each budget is clamped to its smallest usable value
        assert_eq!(
            config.agent_warmup_options(),
            WarmupOptions {
                timeout: Duration::from_secs(1),
                retry_interval: Duration::from_millis(1),
                request_timeout: Duration::from_secs(1),
            }
        );
    }
}

#[cfg(test)]
mod claude_cli_config_tests {
    use super::*;

    /// When `claude_cli:` is omitted from YAML entirely, `DaemonConfig::claude_cli` is `None`.
    #[test]
    fn claude_cli_absent_when_not_in_yaml() {
        let yaml = "users:\n  - github_user: \"u\"\n    os_user: \"u\"\n";
        let c: DaemonConfig = serde_yaml::from_str(yaml).expect("parse");
        assert!(
            c.claude_cli.is_none(),
            "claude_cli must be absent when not configured"
        );
    }

    /// `tddy_tools_path` and `daemon_url` both default to `None` when the section is present
    /// but the new fields are not specified.
    #[test]
    fn claude_cli_config_defaults_new_paths_to_none() {
        let yaml = "
users:
  - github_user: u
    os_user: u
claude_cli:
  binary_path: /usr/local/bin/claude
";
        let c: DaemonConfig = serde_yaml::from_str(yaml).expect("parse");
        let cli = c.claude_cli.as_ref().expect("claude_cli must be present");
        assert_eq!(
            cli.tddy_tools_path, None,
            "tddy_tools_path must default to None"
        );
        assert_eq!(cli.daemon_url, None, "daemon_url must default to None");
    }

    /// When both new fields are explicit, they round-trip correctly through YAML parsing.
    #[test]
    fn claude_cli_config_parses_explicit_tddy_tools_path_and_daemon_url() {
        let yaml = "
users:
  - github_user: u
    os_user: u
claude_cli:
  binary_path: claude
  tddy_tools_path: /usr/local/bin/tddy-tools
  daemon_url: http://127.0.0.1:9000
";
        let c: DaemonConfig = serde_yaml::from_str(yaml).expect("parse");
        let cli = c.claude_cli.as_ref().expect("claude_cli must be present");
        assert_eq!(
            cli.tddy_tools_path.as_deref(),
            Some("/usr/local/bin/tddy-tools")
        );
        assert_eq!(cli.daemon_url.as_deref(), Some("http://127.0.0.1:9000"));
    }

    /// A non-empty env override always wins, even over an explicit config path.
    #[test]
    fn env_override_wins_over_configured_path() {
        assert!(matches!(
            choose_claude_binary(Some("/opt/claude"), "/usr/local/bin/claude"),
            ClaudeBinaryChoice::Explicit(p) if p == "/opt/claude"
        ));
    }

    /// What `claude_cli:` says about `fullscreen` once parsed, read back through the serialized
    /// section so the assertion pins the YAML contract an operator writes.
    fn parsed_fullscreen(claude_cli_section: &str) -> serde_yaml::Value {
        let yaml = format!("users:\n  - github_user: u\n    os_user: u\n{claude_cli_section}");
        let c: DaemonConfig = serde_yaml::from_str(&yaml).expect("parse");
        let cli = c.claude_cli.as_ref().expect("claude_cli must be present");
        serde_yaml::to_value(cli).expect("serialize claude_cli")["fullscreen"].clone()
    }

    /// The web terminal can only scroll Claude when Claude runs its fullscreen renderer, so a
    /// `claude_cli:` section that does not mention `fullscreen` keeps it on.
    #[test]
    fn fullscreen_is_on_when_the_claude_cli_section_does_not_mention_it() {
        // Given
        let section = "claude_cli:\n  binary_path: /usr/local/bin/claude\n";

        // When
        let fullscreen = parsed_fullscreen(section);

        // Then
        assert_eq!(fullscreen, serde_yaml::Value::Bool(true));
    }

    /// An operator can turn the forced fullscreen renderer off.
    #[test]
    fn fullscreen_false_is_accepted_and_kept() {
        // Given
        let section = "claude_cli:\n  binary_path: /usr/local/bin/claude\n  fullscreen: false\n";

        // When
        let fullscreen = parsed_fullscreen(section);

        // Then
        assert_eq!(fullscreen, serde_yaml::Value::Bool(false));
    }

    /// A blank/whitespace env override is ignored (falls through to config/auto).
    #[test]
    fn blank_env_override_is_ignored() {
        assert!(matches!(
            choose_claude_binary(Some("   "), "claude"),
            ClaudeBinaryChoice::Auto
        ));
    }

    /// A configured value naming a path is explicit; a bare name auto-resolves from the host.
    #[test]
    fn configured_path_is_explicit_but_bare_name_auto_resolves() {
        assert!(matches!(
            choose_claude_binary(None, "/usr/local/bin/claude"),
            ClaudeBinaryChoice::Explicit(_)
        ));
        assert!(matches!(
            choose_claude_binary(None, "claude"),
            ClaudeBinaryChoice::Auto
        ));
    }

    /// `claude_home_dir` round-trips from YAML, and a configured value is what the resolver returns
    /// (when the `TDDY_SANDBOX_CLAUDE_HOME` env override is not set, the normal case in tests).
    #[test]
    fn claude_home_dir_parses_and_resolves_from_config() {
        let yaml = "
users:
  - github_user: u
    os_user: u
claude_cli:
  binary_path: claude
  claude_home_dir: /custom/claude-home
";
        let c: DaemonConfig = serde_yaml::from_str(yaml).expect("parse");
        assert_eq!(
            c.claude_cli.as_ref().unwrap().claude_home_dir.as_deref(),
            Some(Path::new("/custom/claude-home"))
        );
        if std::env::var_os(agent_cli::CLAUDE_HOME_ENV).is_none() {
            assert_eq!(
                resolve_claude_home_dir(&c),
                PathBuf::from("/custom/claude-home"),
                "a configured claude_home_dir must be returned when the env override is unset"
            );
        }
    }

    /// With no `claude_cli` section the resolver falls back to `$HOME/.tddy/sandbox-claude-home`.
    #[test]
    fn claude_home_dir_defaults_under_tddy_when_unconfigured() {
        if std::env::var_os(agent_cli::CLAUDE_HOME_ENV).is_some() {
            return; // env override in effect; default path not exercised
        }
        let c = DaemonConfig::default();
        let resolved = agent_cli::resolve_claude_home_dir(&c);
        assert!(
            resolved.ends_with(".tddy/sandbox-claude-home"),
            "default home must be under ~/.tddy: {}",
            resolved.display()
        );
    }

    /// The per-OS config file wins over the generic one when both are present.
    #[test]
    fn per_os_sandbox_config_takes_precedence_over_generic() {
        let base = Path::new("/cfg");
        // Both present → per-OS wins.
        let got = resolve_sandbox_config_path_with(base, "darwin", None, |p| {
            p == Path::new("/cfg/claude-sandbox.darwin.yaml")
                || p == Path::new("/cfg/claude-sandbox.yaml")
        });
        assert_eq!(got, Some(PathBuf::from("/cfg/claude-sandbox.darwin.yaml")));
        // Only generic present → generic.
        let got = resolve_sandbox_config_path_with(base, "darwin", None, |p| {
            p == Path::new("/cfg/claude-sandbox.yaml")
        });
        assert_eq!(got, Some(PathBuf::from("/cfg/claude-sandbox.yaml")));
    }

    /// The env override wins when it points at an existing file; a missing env target is skipped in
    /// favour of the per-OS/generic search. Absent everything → `None` (defaults).
    #[test]
    fn sandbox_config_env_override_and_absence() {
        let base = Path::new("/cfg");
        // Env points at an existing file → used as-is.
        let got =
            resolve_sandbox_config_path_with(base, "darwin", Some("/explicit/cfg.yaml"), |p| {
                p == Path::new("/explicit/cfg.yaml")
            });
        assert_eq!(got, Some(PathBuf::from("/explicit/cfg.yaml")));
        // Env set but missing → falls through to the per-OS file.
        let got =
            resolve_sandbox_config_path_with(base, "darwin", Some("/explicit/missing.yaml"), |p| {
                p == Path::new("/cfg/claude-sandbox.darwin.yaml")
            });
        assert_eq!(got, Some(PathBuf::from("/cfg/claude-sandbox.darwin.yaml")));
        // Nothing exists → None.
        let got = resolve_sandbox_config_path_with(base, "darwin", None, |_| false);
        assert_eq!(got, None);
    }

    /// On macOS the OS token is the repo's `"darwin"` (not Rust's `"macos"`).
    #[test]
    #[cfg(target_os = "macos")]
    fn os_token_is_darwin_on_macos() {
        assert_eq!(sandbox_config_os_token(), "darwin");
    }

    /// Wrapper-shim `bin` dirs (Superset) are recognised so PATH resolution skips them; real
    /// install dirs are not.
    #[test]
    fn recognises_wrapper_shim_dirs() {
        assert!(is_wrapper_shim_dir(Path::new("/Users/x/.superset/bin")));
        assert!(is_wrapper_shim_dir(Path::new(
            "/Users/x/.superset-worktrees/proj/bin"
        )));
        assert!(!is_wrapper_shim_dir(Path::new("/Users/x/.local/bin")));
        assert!(!is_wrapper_shim_dir(Path::new("/usr/bin")));
        // Only the `bin` leaf under a `.superset*` component counts.
        assert!(!is_wrapper_shim_dir(Path::new("/Users/x/.superset/lib")));
    }
}

#[cfg(test)]
mod cursor_cli_config_tests {
    use super::*;

    /// The cursor `agent` binary must get the same host auto-resolution as `claude`: a bare name
    /// (the default `"agent"`) means "auto-resolve to the real binary", while a value naming a path
    /// is used verbatim. Without this, the bare `"agent"` reaches the model probe / PTY spawn and
    /// fails "binary not found: agent" once the child does a PATH lookup that lacks the install dir.
    #[test]
    fn cursor_bare_name_auto_resolves_but_a_configured_path_is_explicit() {
        assert!(matches!(
            choose_cursor_binary(None, "agent"),
            CursorBinaryChoice::Auto
        ));
        assert!(matches!(
            choose_cursor_binary(None, "/opt/cursor/agent"),
            CursorBinaryChoice::Explicit(p) if p == "/opt/cursor/agent"
        ));
    }

    /// A non-empty env override wins over the configured value (same precedence as claude); a blank
    /// override is ignored and falls through to auto-resolution.
    #[test]
    fn cursor_env_override_wins_and_blank_is_ignored() {
        assert!(matches!(
            choose_cursor_binary(Some("/opt/agent"), "agent"),
            CursorBinaryChoice::Explicit(p) if p == "/opt/agent"
        ));
        assert!(matches!(
            choose_cursor_binary(Some("   "), "agent"),
            CursorBinaryChoice::Auto
        ));
    }

    /// Host auto-resolution finds `agent` under the target user's `~/.local/bin` and returns it as
    /// an **absolute** path (mirroring `find_real_claude_on_host`), so the impersonated child execs
    /// a fully-qualified path and never needs a PATH lookup. Hermetic: a tempdir stands in for the
    /// user's home — no real OS user or install path is hardcoded.
    #[test]
    fn find_real_agent_prefers_local_bin_and_returns_an_absolute_path() {
        // Given — a home directory whose ~/.local/bin holds an executable `agent`
        let home = tempfile::tempdir().unwrap();
        let bin_dir = home.path().join(".local/bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let agent = bin_dir.join("agent");
        std::fs::write(&agent, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&agent, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        // When — resolving the real host agent for that home, with no extra PATH dirs
        let resolved = agent_cli::find_real_agent_in(Some(home.path()), &[]);

        // Then — it is located there, as the absolute path under ~/.local/bin
        assert_eq!(resolved.as_deref(), Some(agent.as_path()));
    }

    /// When `~/.local/bin/agent` is absent, resolution scans the supplied PATH dirs (skipping
    /// wrapper-shim dirs) and still returns an absolute path — never a bare name.
    #[test]
    fn find_real_agent_falls_back_to_path_dirs_as_an_absolute_path() {
        // Given — no ~/.local/bin/agent, but an `agent` on a PATH dir
        let home = tempfile::tempdir().unwrap();
        let path_dir = tempfile::tempdir().unwrap();
        let agent = path_dir.path().join("agent");
        std::fs::write(&agent, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&agent, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        // When — resolving with that dir on the search path
        let resolved =
            agent_cli::find_real_agent_in(Some(home.path()), &[path_dir.path().to_path_buf()]);

        // Then — the PATH-dir binary is returned as an absolute path
        assert_eq!(resolved.as_deref(), Some(agent.as_path()));
    }
}

#[cfg(test)]
mod web_debug_mask_tests {
    use super::*;

    #[test]
    fn debug_mask_defaults_to_none() {
        let c = DaemonConfig::default();
        assert!(c.debug.is_none());
    }

    #[test]
    fn debug_mask_absent_in_yaml_is_none() {
        let yaml = "
users:
  - github_user: u
    os_user: u
";
        let c: DaemonConfig = serde_yaml::from_str(yaml).expect("parse");
        assert!(c.debug.is_none());
    }

    #[test]
    fn debug_mask_parses_from_yaml() {
        let yaml = "
debug: \"tddy:term:*\"
users:
  - github_user: u
    os_user: u
";
        let c: DaemonConfig = serde_yaml::from_str(yaml).expect("parse");
        assert_eq!(c.debug.as_deref(), Some("tddy:term:*"));
    }
}

#[cfg(test)]
mod git_config_tests {
    use super::*;

    #[test]
    fn git_ssh_command_absent_is_none() {
        let yaml = "
users:
  - github_user: u
    os_user: u
";
        let c: DaemonConfig = serde_yaml::from_str(yaml).expect("parse");
        assert!(c.git.is_none(), "git section must be absent by default");
    }

    #[test]
    fn git_ssh_command_parses_from_yaml() {
        let yaml = "
git:
  ssh_command: \"/usr/bin/ssh -o BatchMode=yes -o ConnectTimeout=10\"
users:
  - github_user: u
    os_user: u
";
        let c: DaemonConfig = serde_yaml::from_str(yaml).expect("parse");
        let git = c.git.as_ref().expect("git section must be present");
        assert_eq!(
            git.ssh_command.as_deref(),
            Some("/usr/bin/ssh -o BatchMode=yes -o ConnectTimeout=10")
        );
    }

    #[test]
    fn git_section_present_but_ssh_command_absent_is_none() {
        let yaml = "
git: {}
users:
  - github_user: u
    os_user: u
";
        let c: DaemonConfig = serde_yaml::from_str(yaml).expect("parse");
        let git = c.git.as_ref().expect("git section must be present");
        assert!(git.ssh_command.is_none());
    }

    #[test]
    fn maps_daemon_sandbox_cgroup_config_onto_the_plan_field() {
        // Given — a daemon config with an explicit sandbox_cgroup section
        let yaml = "
sandbox_cgroup:
  base_path: /custom/delegated/base
  controllers: [memory, pids]
  supervisor_leaf: worker
users:
  - github_user: u
    os_user: u
";
        let config: DaemonConfig = serde_yaml::from_str(yaml).expect("parse");

        // When
        let cgroup = config.sandbox_cgroup_config();

        // Then
        assert_eq!(
            cgroup.base_override,
            Some(std::path::PathBuf::from("/custom/delegated/base"))
        );
        assert_eq!(
            cgroup.controllers,
            vec!["memory".to_string(), "pids".to_string()]
        );
        assert_eq!(cgroup.supervisor_leaf, Some("worker".to_string()));
    }

    #[test]
    fn resolves_local_identity_from_a_mapped_peer_uid() {
        // Given — github user octocat maps to os_user dev1, and peer uid 1002 resolves to "dev1"
        let yaml = "
users:
  - github_user: octocat
    os_user: dev1
";
        let config: DaemonConfig = serde_yaml::from_str(yaml).expect("parse");

        // When
        let identity =
            config.local_identity_for_uid(1002, |uid| (uid == 1002).then(|| "dev1".to_string()));

        // Then
        let mapping = identity.expect("uid 1002 must map to a configured user");
        assert_eq!(mapping.github_user, "octocat");
        assert_eq!(mapping.os_user, "dev1");
    }

    #[test]
    fn rejects_local_identity_for_an_unmapped_peer_uid() {
        // Given — a config with one user; the peer uid resolves to a username in no mapping
        let yaml = "
users:
  - github_user: octocat
    os_user: dev1
";
        let config: DaemonConfig = serde_yaml::from_str(yaml).expect("parse");

        // When
        let identity = config.local_identity_for_uid(4242, |_| Some("stranger".to_string()));

        // Then
        assert!(identity.is_none());
    }

    #[test]
    fn mints_a_login_for_a_mapped_peer_uid() {
        // Given — octocat maps to os_user dev1; peer uid 1002 resolves to "dev1"
        let yaml = "
users:
  - github_user: octocat
    os_user: dev1
";
        let config: DaemonConfig = serde_yaml::from_str(yaml).expect("parse");

        // When
        let login =
            config.local_token_login_for_uid(1002, |uid| (uid == 1002).then(|| "dev1".to_string()));

        // Then
        assert_eq!(login.as_deref(), Some("octocat"));
    }

    #[test]
    fn no_login_for_an_unmapped_peer_uid() {
        // Given
        let yaml = "
users:
  - github_user: octocat
    os_user: dev1
";
        let config: DaemonConfig = serde_yaml::from_str(yaml).expect("parse");

        // When — the peer uid resolves to a username in no mapping
        let login = config.local_token_login_for_uid(4242, |_| Some("stranger".to_string()));

        // Then
        assert!(login.is_none());
    }
}

#[cfg(test)]
mod livekit_enabled_tests {
    use super::*;

    /// A daemon whose YAML carries `livekit_yaml` verbatim.
    fn a_daemon_configured_with(livekit_yaml: &str) -> DaemonConfig {
        serde_yaml::from_str(livekit_yaml).expect("the daemon fixture did not parse")
    }

    /// A complete LiveKit block — url, key, secret and room — with `enabled_line` spliced in.
    fn a_complete_livekit_block_with(enabled_line: &str) -> String {
        format!(
            "livekit:\n{enabled_line}  url: ws://127.0.0.1:7880\n  api_key: devkey\n  \
             api_secret: the-secret\n  common_room: tddy-lobby\n"
        )
    }

    #[test]
    fn reads_a_livekit_block_that_omits_enabled_as_disabled() {
        // Given a complete LiveKit block written before the flag existed
        let config = a_daemon_configured_with(&a_complete_livekit_block_with(""));

        // When the operator's switch is read
        let livekit = config.livekit.expect("the fixture has a livekit block");

        // Then it is off: the common room is opt-in, so a block that never said "join" does not
        assert!(
            !livekit.enabled,
            "a livekit block with no `enabled` key was read as switched on"
        );
    }

    #[test]
    fn reads_a_livekit_block_that_says_enabled_true_as_enabled() {
        // Given a complete LiveKit block the operator switched on
        let config = a_daemon_configured_with(&a_complete_livekit_block_with("  enabled: true\n"));

        // When the operator's switch is read
        let livekit = config.livekit.expect("the fixture has a livekit block");

        // Then it is on
        assert!(
            livekit.enabled,
            "`enabled: true` was not read as switched on"
        );
    }

    #[test]
    fn keeps_the_url_key_secret_and_room_when_livekit_is_switched_off() {
        // Given a complete LiveKit block the operator switched off
        let config = a_daemon_configured_with(&a_complete_livekit_block_with("  enabled: false\n"));

        // When the block is read
        let livekit = config.livekit.expect("the fixture has a livekit block");

        // Then everything needed to switch it back on is still there. "Off" is a flag, never the
        // deletion of a working block — that is the whole point of the switch.
        assert_eq!(
            (
                livekit.url.as_deref(),
                livekit.api_key.as_deref(),
                livekit.api_secret.as_deref(),
                livekit.common_room.as_deref()
            ),
            (
                Some("ws://127.0.0.1:7880"),
                Some("devkey"),
                Some("the-secret"),
                Some("tddy-lobby")
            )
        );
    }

    #[test]
    fn writes_the_enabled_flag_back_so_a_saved_config_says_what_it_was_told() {
        // Given a LiveKit block switched on
        let config = a_daemon_configured_with(&a_complete_livekit_block_with("  enabled: true\n"));

        // When it is serialised back to YAML, as an update to the daemon's own file does
        let yaml = serde_yaml::to_string(&config).expect("the config did not serialise");

        // Then the flag is in the file. `deny_unknown_fields` means the key and the field must
        // travel together, so a flag that never reached the file would be silently lost.
        assert!(
            yaml.contains("enabled: true"),
            "the enabled flag was not written back:\n{yaml}"
        );
    }

    #[test]
    fn calls_the_common_room_switched_on_when_the_operator_enabled_it() {
        // Given a LiveKit block the operator switched on
        let config = a_daemon_configured_with(&a_complete_livekit_block_with("  enabled: true\n"));

        // When the one predicate every join path asks is asked
        let enabled = LiveKitConfig::common_room_enabled(config.livekit.as_ref());

        // Then it says so
        assert!(enabled, "an enabled livekit block was called switched off");
    }

    #[test]
    fn calls_the_common_room_switched_off_when_the_operator_disabled_it() {
        // Given a complete LiveKit block the operator switched off
        let config = a_daemon_configured_with(&a_complete_livekit_block_with("  enabled: false\n"));

        // When the one predicate every join path asks is asked
        let enabled = LiveKitConfig::common_room_enabled(config.livekit.as_ref());

        // Then it says so, even though every credential is present
        assert!(
            !enabled,
            "a complete but disabled livekit block was called switched on"
        );
    }

    #[test]
    fn calls_the_common_room_switched_off_when_there_is_no_livekit_block_at_all() {
        // Given a daemon that was never configured for LiveKit
        let config = a_daemon_configured_with("listen:\n  web_port: 8899\n");

        // When the one predicate every join path asks is asked
        let enabled = LiveKitConfig::common_room_enabled(config.livekit.as_ref());

        // Then it says off — an absent block behaves exactly as it always has
        assert!(!enabled, "an absent livekit block was called switched on");
    }
}

#[cfg(test)]
mod peer_forward_timeout_tests {
    use super::*;

    #[test]
    fn the_peer_forward_deadline_defaults_to_thirty_seconds() {
        let c = DaemonConfig::default();
        assert_eq!(c.peer_forward_timeout().as_secs(), 30);
    }

    #[test]
    fn the_peer_forward_deadline_is_read_from_the_daemon_yaml() {
        let c: DaemonConfig = serde_yaml::from_str("peer_forward_timeout_secs: 2\n").expect("yaml");
        assert_eq!(c.peer_forward_timeout().as_secs(), 2);
    }

    #[test]
    fn a_zero_peer_forward_deadline_is_clamped_to_one_second() {
        let c = DaemonConfig {
            peer_forward_timeout_secs: 0,
            ..Default::default()
        };
        assert_eq!(c.peer_forward_timeout().as_secs(), 1);
    }
}

#[cfg(test)]
mod livekit_public_url_tests {
    use super::*;
    use rstest::rstest;

    /// A complete LiveKit block whose `public_url` line is `public_url_value` verbatim.
    fn a_daemon_whose_public_url_is(public_url_value: &str) -> DaemonConfig {
        serde_yaml::from_str(&format!(
            "livekit:\n  enabled: true\n  url: wss://livekit.example.com:7883\n  \
             public_url: {public_url_value}\n  api_key: devkey\n  api_secret: the-secret\n  \
             common_room: tddy-lobby\n"
        ))
        .expect("the daemon fixture did not parse")
    }

    #[rstest]
    #[case::empty("''")]
    #[case::whitespace("'   '")]
    fn reads_a_blank_public_url_as_absent(#[case] public_url_value: &str) {
        // Given a LiveKit block whose public URL was saved blank
        let config = a_daemon_whose_public_url_is(public_url_value);

        // When the block is read
        let livekit = config.livekit.expect("the fixture has a livekit block");

        // Then there is no public URL, so every reader falls back to `url`
        assert_eq!(livekit.public_url, None);
    }

    #[test]
    fn writes_no_public_url_key_back_when_the_stored_one_was_blank() {
        // Given a LiveKit block whose public URL was saved blank
        let config = a_daemon_whose_public_url_is("''");

        // When it is serialised back to YAML, as an update to the daemon's own file does
        let yaml = serde_yaml::to_string(&config).expect("the config did not serialise");

        // Then the key is gone rather than carried forward as an empty string
        assert!(
            !yaml.contains("public_url"),
            "a blank public_url was written back:\n{yaml}"
        );
    }
}
