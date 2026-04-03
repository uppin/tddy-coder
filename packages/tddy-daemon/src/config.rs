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

#[derive(Debug, Default, Clone, serde::Deserialize)]
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
}
