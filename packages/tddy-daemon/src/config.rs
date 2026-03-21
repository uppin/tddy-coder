//! Daemon configuration — users, tools, LiveKit, GitHub, etc.

use std::path::PathBuf;

use tddy_core::LogConfig;

fn default_spawn_mouse() -> bool {
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
    /// Relative to each OS user's home directory (e.g. `repos` → `~/repos/`).
    #[serde(default)]
    pub repos_base_path: Option<String>,
    /// When true (default), spawned `tddy-*` processes receive `--mouse` (browser / touch terminals).
    #[serde(default = "default_spawn_mouse")]
    pub spawn_mouse: bool,
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
            repos_base_path: None,
            spawn_mouse: true,
        }
    }
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
}
