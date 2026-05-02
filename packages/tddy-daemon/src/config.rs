//! Daemon configuration — users, tools, LiveKit, GitHub, etc.

use std::path::PathBuf;
use std::time::Duration;

use tddy_core::LogConfig;

fn default_spawn_mouse() -> bool {
    true
}

fn default_spawn_worker_request_timeout_secs() -> u64 {
    300
}

fn default_common_room_set_metadata_timeout_secs() -> u64 {
    60
}

fn default_codex_oauth_loopback_proxy_eligible() -> bool {
    true
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DaemonConfig {
    #[serde(default)]
    pub listen: ListenConfig,
    #[serde(default)]
    pub web_bundle_path: Option<PathBuf>,
    #[serde(default)]
    pub livekit: Option<LiveKitConfig>,
    #[serde(default)]
    pub github: Option<GitHubConfig>,
    #[serde(default)]
    pub auth_storage: Option<PathBuf>,
    #[serde(default)]
    pub log: Option<LogConfig>,
    #[serde(default)]
    pub users: Vec<UserMapping>,
    #[serde(default)]
    pub allowed_tools: Vec<AllowedTool>,
    /// Allowed coding backends / agents (`tddy-coder --agent` values), with optional UI labels.
    #[serde(default)]
    pub allowed_agents: Vec<AllowedAgent>,
    /// Relative to each OS user's home directory (e.g. `repos` → `~/repos/`).
    #[serde(default)]
    pub repos_base_path: Option<String>,
    /// When true (default), spawned `tddy-*` processes receive `--mouse` (browser / touch terminals).
    #[serde(default = "default_spawn_mouse")]
    pub spawn_mouse: bool,
    /// Max seconds for clone/spawn work (`SpawnClient` round-trip or direct `spawn_as_user` /
    /// `clone_as_user`). Minimum effective value is 1.
    #[serde(default = "default_spawn_worker_request_timeout_secs")]
    pub spawn_worker_request_timeout_secs: u64,
    /// Stable id for this daemon in a shared LiveKit room (multi-host). When set, spawned tools
    /// and ConnectSession use `daemon-{instance_id}-{session_id}` as LiveKit server identity.
    #[serde(default)]
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
    #[serde(default)]
    pub telegram: Option<TelegramConfig>,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            listen: ListenConfig::default(),
            web_bundle_path: None,
            livekit: None,
            github: None,
            auth_storage: None,
            log: None,
            users: Vec::new(),
            allowed_tools: Vec::new(),
            allowed_agents: Vec::new(),
            repos_base_path: None,
            spawn_mouse: true,
            spawn_worker_request_timeout_secs: default_spawn_worker_request_timeout_secs(),
            daemon_instance_id: None,
            daemon_instance_id_append_startup_timestamp: false,
            codex_oauth_loopback_proxy_eligible: default_codex_oauth_loopback_proxy_eligible(),
            telegram: None,
        }
    }
}

/// Telegram Bot API integration (teloxide). Loaded from daemon YAML under `telegram:`.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TelegramConfig {
    #[serde(default)]
    pub enabled: bool,
    pub bot_token: String,
    #[serde(default)]
    pub chat_ids: Vec<i64>,
}

#[derive(Debug, Default, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListenConfig {
    #[serde(default)]
    pub web_port: Option<u16>,
    #[serde(default)]
    pub web_host: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiveKitConfig {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub api_secret: Option<String>,
    #[serde(default)]
    pub public_url: Option<String>,
    /// Shared LiveKit room for presence (browser + tddy-* tools). Exposed to web as `common_room` in `/api/config`.
    #[serde(default)]
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
            url: None,
            api_key: None,
            api_secret: None,
            public_url: None,
            common_room: None,
            common_room_set_metadata_timeout_secs: default_common_room_set_metadata_timeout_secs(),
        }
    }
}

#[derive(Debug, Default, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitHubConfig {
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_secret: Option<String>,
    #[serde(default)]
    pub redirect_uri: Option<String>,
    #[serde(default)]
    pub stub: Option<bool>,
    #[serde(default)]
    pub stub_codes: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserMapping {
    pub github_user: String,
    pub os_user: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllowedTool {
    pub path: String,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllowedAgent {
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
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
        let config: Self = serde_yaml::from_str(&contents)
            .map_err(|e| anyhow::anyhow!("failed to parse config {}: {}", path.display(), e))?;
        Ok(config)
    }

    /// Resolve OS user for a GitHub user. Returns None if not mapped.
    pub fn os_user_for_github(&self, github_user: &str) -> Option<&str> {
        self.users
            .iter()
            .find(|u| u.github_user == github_user)
            .map(|u| u.os_user.as_str())
    }

    /// List allowed tools with path and label.
    pub fn allowed_tools(&self) -> &[AllowedTool] {
        &self.allowed_tools
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
        let bot_token = non_empty_env("TDDY_TELEGRAM_BOT_TOKEN");
        let chat_ids_csv = non_empty_env("TDDY_TELEGRAM_CHAT_IDS");
        let enabled = non_empty_env("TDDY_TELEGRAM_ENABLED");
        merge_telegram_env(
            self,
            bot_token.as_deref(),
            chat_ids_csv.as_deref(),
            enabled.as_deref(),
        );
    }

    /// Override [`Self::codex_oauth_loopback_proxy_eligible`] from `TDDY_CODEX_OAUTH_LOOPBACK_PROXY_ELIGIBLE`
    /// (`true`/`false`/`1`/`0`/`yes`/`no`/`on`/`off`, case-insensitive). Call after YAML load.
    pub fn apply_oauth_loopback_proxy_env_override(&mut self) {
        if let Some(s) = non_empty_env("TDDY_CODEX_OAUTH_LOOPBACK_PROXY_ELIGIBLE") {
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
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().and_then(|s| {
        let t = s.trim();
        if t.is_empty() {
            None
        } else {
            Some(s)
        }
    })
}

fn parse_chat_ids_csv(s: &str) -> Result<Vec<i64>, ()> {
    let mut out = Vec::new();
    for part in s.split(',') {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        let n: i64 = p.parse().map_err(|_| ())?;
        out.push(n);
    }
    Ok(out)
}

fn parse_env_bool(s: &str) -> Option<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn merge_telegram_env(
    config: &mut DaemonConfig,
    bot_token: Option<&str>,
    chat_ids_csv: Option<&str>,
    enabled: Option<&str>,
) {
    if bot_token.is_none() && chat_ids_csv.is_none() && enabled.is_none() {
        return;
    }

    let mut created_from_env_token = false;
    if config.telegram.is_none() {
        if bot_token.is_none() {
            if chat_ids_csv.is_some() || enabled.is_some() {
                log::warn!(
                    target: "tddy_daemon::config",
                    "telegram: set TDDY_TELEGRAM_BOT_TOKEN or add a `telegram:` block to the config file before using TDDY_TELEGRAM_CHAT_IDS / TDDY_TELEGRAM_ENABLED"
                );
            }
            return;
        }
        created_from_env_token = true;
        config.telegram = Some(TelegramConfig {
            enabled: false,
            bot_token: String::new(),
            chat_ids: Vec::new(),
        });
    }

    let tg = config.telegram.as_mut().expect("telegram just ensured");

    if let Some(t) = bot_token {
        tg.bot_token = t.trim().to_string();
    }

    if let Some(csv) = chat_ids_csv {
        match parse_chat_ids_csv(csv) {
            Ok(ids) if !ids.is_empty() => tg.chat_ids = ids,
            Ok(_) => {}
            Err(()) => log::warn!(
                target: "tddy_daemon::config",
                "TDDY_TELEGRAM_CHAT_IDS: expected comma-separated integers, ignoring"
            ),
        }
    }

    if let Some(e) = enabled {
        match parse_env_bool(e) {
            Some(b) => tg.enabled = b,
            None => log::warn!(
                target: "tddy_daemon::config",
                "TDDY_TELEGRAM_ENABLED: expected true/false/1/0/yes/no/on/off, ignoring"
            ),
        }
    } else if bot_token.is_some() && created_from_env_token {
        tg.enabled = true;
    }
}

#[cfg(test)]
mod telegram_env_tests {
    use super::*;

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
