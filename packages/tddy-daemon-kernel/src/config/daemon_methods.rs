use crate::config::access::AllowedTool;
use crate::config::access::{AllowedAgent, UserMapping};
use crate::config::default_common_room_set_metadata_timeout_secs;
use crate::config::env_overrides::merge_telegram_env;
use crate::config::env_overrides::merge_timing_env;
use crate::config::env_overrides::parse_env_bool;
use crate::config::DaemonConfig;
use std::path::PathBuf;
use std::time::Duration;

/// Refuse a `supervisor:` block, which a `daemon.yaml` written before `tddy-supervisor` told the
/// daemon its socket still carries.
///
/// Checked on the untyped document, before typed deserialization, so the operator is told the
/// block is obsolete and what to do about it, rather than getting serde's bare `unknown field`.
/// Refusing it, rather than ignoring it, is what keeps a leftover block from quietly looking like
/// a setting that still does something.
fn refuse_obsolete_supervisor_block(contents: &str, path: &std::path::Path) -> anyhow::Result<()> {
    let document: serde_yaml::Value = serde_yaml::from_str(contents)
        .map_err(|e| anyhow::anyhow!("failed to parse config {}: {}", path.display(), e))?;
    let declares_supervisor = document
        .as_mapping()
        .is_some_and(|top_level| top_level.contains_key("supervisor"));
    if declares_supervisor {
        anyhow::bail!(
            "{}: the `supervisor:` block is obsolete — tddy-supervisor now tells the daemon its socket. Delete the block.",
            path.display()
        );
    }
    Ok(())
}

impl DaemonConfig {
    /// Default subdirectory under home for cloned project repos when `repos_base_path` is unset.
    pub fn repos_base_path_or_default(&self) -> &str {
        self.repos_base_path.as_deref().unwrap_or("repos")
    }

    /// Load config from a YAML file.
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("failed to read config {}: {}", path.display(), e))?;
        refuse_obsolete_supervisor_block(&contents, path)?;
        let config: Self = serde_yaml::from_str(&contents)
            .map_err(|e| anyhow::anyhow!("failed to parse config {}: {}", path.display(), e))?;
        config
            .session_room
            .validate()
            .map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
        Ok(config)
    }

    /// Resolve OS user for a GitHub user. Returns None if not mapped.
    pub fn os_user_for_github(&self, github_user: &str) -> Option<String> {
        self.users.os_user_for_github(github_user)
    }

    /// Resolve the local caller's identity from its peer uid (SO_PEERCRED), for the local peer-trust
    /// auth path used by same-host clients (e.g. tddy-sandbox-app). `uid_to_username` maps a uid to
    /// an OS username — injected so this is host-independent and unit-testable — and the result is
    /// matched against a configured `users[]` entry by `os_user`. Returns None when the uid has no
    /// username or no matching user mapping.
    pub fn local_identity_for_uid(
        &self,
        uid: u32,
        uid_to_username: impl Fn(u32) -> Option<String>,
    ) -> Option<UserMapping> {
        let username = uid_to_username(uid)?;
        self.users.mapping_for_os_user(&username)
    }

    /// The GitHub login for a local caller's peer uid (SO_PEERCRED), used to mint its access token
    /// in the local peer-trust auth path. Thin wrapper over [`Self::local_identity_for_uid`].
    pub fn local_token_login_for_uid(
        &self,
        uid: u32,
        uid_to_username: impl Fn(u32) -> Option<String>,
    ) -> Option<String> {
        self.local_identity_for_uid(uid, uid_to_username)
            .map(|m| m.github_user)
    }

    /// Resolve the path the local Unix-domain socket binds at. Uses the explicit `local.socket_path`
    /// when configured; otherwise `${XDG_RUNTIME_DIR}/tddy-daemon.sock`, falling back to
    /// `/run/tddy-daemon.sock` when `XDG_RUNTIME_DIR` is unset.
    pub fn local_socket_path(&self) -> PathBuf {
        if let Some(path) = &self.local.socket_path {
            return path.clone();
        }
        let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/run"));
        runtime_dir.join("tddy-daemon.sock")
    }

    /// List allowed tools with path and label.
    pub fn allowed_tools(&self) -> &[AllowedTool] {
        &self.allowed_tools
    }

    /// The tool path to spawn when nothing more specific was requested — the first configured
    /// `allowed_tools` entry (e.g. `"target/debug/tddy-coder"` in dev), falling back to a bare
    /// `"tddy-coder"` only when no tools are configured at all. Single source of truth: callers
    /// (`StartSession`'s Telegram workflow spawn path, `ResumeSession`'s no-recorded-tool
    /// fallback) must not each hardcode their own default, or they can silently drift apart.
    pub fn default_tool_path(&self) -> String {
        self.allowed_tools
            .first()
            .map(|t| t.path.clone())
            .unwrap_or_else(|| "tddy-coder".to_string())
    }

    /// This deployment's tddy toolchain — where its binaries are, resolved absolutely.
    ///
    /// Never derived from `allowed_tools`: that list is the operator's menu of `tddy-coder`
    /// builds, not a statement about where the toolchain lives.
    pub fn toolchain(&self) -> crate::toolchain::TddyToolchain {
        crate::toolchain::TddyToolchain::resolve(self.toolchain_dir.as_deref())
    }

    /// Allowed agent ids (`StartSession.agent` / `tddy-coder --agent`) and display labels.
    pub fn allowed_agents(&self) -> &[AllowedAgent] {
        &self.allowed_agents
    }

    /// Wall-clock limit for blocking clone/spawn operations (spawn worker or in-process).
    pub fn spawn_worker_request_timeout(&self) -> Duration {
        let secs = self.spawn_worker_request_timeout_secs.max(1);
        Duration::from_secs(secs)
    }

    /// How long a forward to a peer daemon waits for its answer (see `peer_forward_timeout_secs`).
    pub fn peer_forward_timeout(&self) -> Duration {
        Duration::from_secs(self.peer_forward_timeout_secs.max(1))
    }

    /// How long to watch a freshly spawned session process for an immediate exit.
    pub fn spawn_startup_grace_period(&self) -> Duration {
        Duration::from_millis(self.spawn_startup_grace_period_ms.max(1))
    }

    /// How often that watch polls.
    pub fn spawn_startup_poll_interval(&self) -> Duration {
        Duration::from_millis(self.spawn_startup_poll_interval_ms.max(1))
    }

    /// The warm-up gate's timing budget, in the form `tddy_discovery` takes it.
    ///
    /// On a default config this is exactly `WarmupOptions::default()` — the config path is a way
    /// to say something different, not a change of production behaviour.
    pub fn agent_warmup_options(&self) -> tddy_discovery::warmup::WarmupOptions {
        tddy_discovery::warmup::WarmupOptions {
            timeout: Duration::from_secs(self.agent_warmup.timeout_secs.max(1)),
            retry_interval: Duration::from_millis(self.agent_warmup.retry_interval_ms.max(1)),
            request_timeout: Duration::from_secs(self.agent_warmup.request_timeout_secs.max(1)),
        }
    }

    /// How often a hosted worktree is measured.
    ///
    /// Not clamped: a value below [`MIN_WORKTREE_ROOM_INTERVAL_MS`] is refused when the config is
    /// loaded, so what an operator wrote is what runs.
    pub fn session_room_poll_interval(&self) -> Duration {
        Duration::from_millis(self.session_room.poll_interval_ms)
    }

    /// Wall-clock budget for one poll's git work. See [`Self::session_room_poll_interval`].
    pub fn session_room_git_timeout(&self) -> Duration {
        Duration::from_millis(self.session_room.git_timeout_ms)
    }

    /// Wall-clock budget for one common-room daemon-advertisement `set_metadata` round (the LiveKit SDK
    /// still uses **5 s per attempt**; we retry until this budget elapses or the publish succeeds).
    pub fn common_room_set_metadata_attempt_budget(&self) -> Duration {
        let secs = self
            .livekit
            .as_ref()
            .map(|l| l.common_room_set_metadata_timeout_secs)
            .unwrap_or_else(default_common_room_set_metadata_timeout_secs)
            .max(1);
        Duration::from_secs(secs)
    }

    /// Merge Telegram settings from process environment (after YAML load).
    ///
    /// Variables:
    /// - `TDDY_TELEGRAM_BOT_TOKEN` — Bot API token; when set, assigns the token. If there was no
    ///   `telegram:` block in YAML, a new block is created.
    /// - `TDDY_TELEGRAM_CHAT_IDS` — Comma-separated chat ids (e.g. `-1001234567890,123456`).
    /// - `TDDY_TELEGRAM_ENABLED` — `true`/`false`/`1`/`0`/`yes`/`no`/`on`/`off` (case-insensitive).
    ///
    /// When a new `telegram` block is created solely because `TDDY_TELEGRAM_BOT_TOKEN` is set,
    /// `enabled` defaults to `true` unless `TDDY_TELEGRAM_ENABLED` is set. When merging into an
    /// existing YAML `telegram` block, `enabled` is not changed by the token alone (set
    /// `TDDY_TELEGRAM_ENABLED` explicitly).
    pub fn apply_telegram_env_overrides(&mut self) {
        let bot_token = tddy_core::spawn_env::env_non_empty("TDDY_TELEGRAM_BOT_TOKEN");
        let chat_ids_csv = tddy_core::spawn_env::env_non_empty("TDDY_TELEGRAM_CHAT_IDS");
        let enabled = tddy_core::spawn_env::env_non_empty("TDDY_TELEGRAM_ENABLED");
        merge_telegram_env(
            self,
            bot_token.as_deref(),
            chat_ids_csv.as_deref(),
            enabled.as_deref(),
        );
    }

    /// Validate that this config is suitable for relay mode.
    ///
    /// Relay mode requires the `relay:` section to be present. It does not require
    /// `web_bundle_path` — relay daemons do not serve static files.
    pub fn validate_for_relay(&self) -> anyhow::Result<()> {
        if self.relay.is_some() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("relay section is required in relay mode"))
        }
    }

    /// Override [`Self::codex_oauth_loopback_proxy_eligible`] from `TDDY_CODEX_OAUTH_LOOPBACK_PROXY_ELIGIBLE`
    /// (`true`/`false`/`1`/`0`/`yes`/`no`/`on`/`off`, case-insensitive). Call after YAML load.
    pub fn apply_oauth_loopback_proxy_env_override(&mut self) {
        if let Some(s) =
            tddy_core::spawn_env::env_non_empty("TDDY_CODEX_OAUTH_LOOPBACK_PROXY_ELIGIBLE")
        {
            if let Some(b) = parse_env_bool(&s) {
                self.codex_oauth_loopback_proxy_eligible = b;
            } else {
                log::warn!(
                    target: "tddy_daemon::config",
                    "TDDY_CODEX_OAUTH_LOOPBACK_PROXY_ELIGIBLE: expected true/false/1/0/yes/no/on/off, ignoring"
                );
            }
        }
    }

    /// Override the spawn-startup watch and the warm-up budget from the environment — the last
    /// layer of `defaults ← daemon.yaml ← TDDY_*`. Call after YAML load.
    ///
    /// All seven take a whole number; an unparseable value warns and keeps the file/default value:
    /// - `TDDY_SPAWN_STARTUP_GRACE_PERIOD_MS`
    /// - `TDDY_SPAWN_STARTUP_POLL_INTERVAL_MS`
    /// - `TDDY_AGENT_WARMUP_TIMEOUT_SECS`
    /// - `TDDY_AGENT_WARMUP_RETRY_INTERVAL_MS`
    /// - `TDDY_AGENT_WARMUP_REQUEST_TIMEOUT_SECS`
    /// - `TDDY_SESSION_ROOM_POLL_INTERVAL_MS`
    /// - `TDDY_SESSION_ROOM_GIT_TIMEOUT_MS`
    ///
    /// The two session-room values are additionally refused below
    /// [`MIN_WORKTREE_ROOM_INTERVAL_MS`], the same floor the YAML section is loaded under — a floor
    /// an environment variable could step under would not be one.
    pub fn apply_timing_env_overrides(&mut self) {
        merge_timing_env(self, tddy_core::spawn_env::env_non_empty);
    }
}
