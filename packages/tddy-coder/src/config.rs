//! YAML configuration file support for tddy-coder.
//!
//! The config file mirrors CLI args in a structured YAML format.
//! CLI args take precedence over config file values.

use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::Args;
use tddy_core::LogConfig;

/// File name for persisted CLI options inside a session directory (`sessions/<id>/`).
pub const SESSION_CODER_CONFIG_FILE: &str = "coder-config.yaml";

/// Top-level config file structure.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub goal: Option<String>,
    #[serde(default)]
    pub plan_dir: Option<PathBuf>,
    #[serde(default)]
    pub conversation_output: Option<PathBuf>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(default)]
    pub log: Option<LogConfig>,
    #[serde(default)]
    pub agent: Option<String>,
    /// Path to the Cursor `agent` CLI (overrides `TDDY_CURSOR_AGENT` and default `agent` on `PATH`).
    #[serde(default)]
    pub cursor_agent_path: Option<PathBuf>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub grpc: Option<u16>,
    #[serde(default)]
    pub daemon: Option<bool>,
    #[serde(default)]
    pub mouse: Option<bool>,
    #[serde(default)]
    pub livekit: Option<LiveKitConfig>,
    #[serde(default)]
    pub web: Option<WebConfig>,
    #[serde(default)]
    pub github: Option<GitHubConfig>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiveKitConfig {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub room: Option<String>,
    #[serde(default)]
    pub identity: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub api_secret: Option<String>,
    /// Public URL for browser-facing LiveKit connections (e.g. ws://192.168.1.10:7880).
    /// When not set, the internal `url` is returned to the web client.
    #[serde(default)]
    pub public_url: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebConfig {
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub bundle_path: Option<PathBuf>,
    /// Public URL for browser-facing redirects (e.g. http://192.168.1.10:8899).
    /// When not set, derived from host + port.
    #[serde(default)]
    pub public_url: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
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

/// Load a config file from the given path.
pub fn load_config(path: &Path) -> anyhow::Result<Config> {
    let contents = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("failed to read config file {}: {}", path.display(), e))?;
    let config: Config = serde_yaml::from_str(&contents)
        .map_err(|e| anyhow::anyhow!("failed to parse config file {}: {}", path.display(), e))?;
    Ok(config)
}

/// Merge config file values into Args. CLI args (already in `args`) take precedence.
pub fn merge_config_into_args(args: &mut Args, config: Config) {
    // Simple fields: only override if the CLI didn't set a value
    if args.goal.is_none() {
        args.goal = config.goal;
    }
    if args.plan_dir.is_none() {
        args.plan_dir = config.plan_dir;
    }
    if args.conversation_output.is_none() {
        args.conversation_output = config.conversation_output;
    }
    if args.model.is_none() {
        args.model = config.model;
    }
    if args.allowed_tools.is_none() {
        args.allowed_tools = config.allowed_tools;
    }
    if args.log.is_none() {
        args.log = config.log;
    }
    if args.agent.is_none() {
        if let Some(agent) = config.agent {
            args.agent = Some(agent);
        }
    }
    if args.cursor_agent_path.is_none() {
        args.cursor_agent_path = config.cursor_agent_path;
    }
    if args.prompt.is_none() {
        args.prompt = config.prompt;
    }
    if args.grpc.is_none() {
        args.grpc = config.grpc;
    }
    if !args.daemon {
        args.daemon = config.daemon.unwrap_or(false);
    }
    if !args.mouse {
        args.mouse = config.mouse.unwrap_or(false);
    }

    // LiveKit
    if let Some(lk) = config.livekit {
        if args.livekit_url.is_none() {
            args.livekit_url = lk.url;
        }
        if args.livekit_token.is_none() {
            args.livekit_token = lk.token;
        }
        if args.livekit_room.is_none() {
            args.livekit_room = lk.room;
        }
        if args.livekit_identity.is_none() {
            args.livekit_identity = lk.identity;
        }
        if args.livekit_api_key.is_none() {
            args.livekit_api_key = lk.api_key;
        }
        if args.livekit_api_secret.is_none() {
            args.livekit_api_secret = lk.api_secret;
        }
        if args.livekit_public_url.is_none() {
            args.livekit_public_url = lk.public_url;
        }
    }

    // Web
    if let Some(web) = config.web {
        if args.web_port.is_none() {
            args.web_port = web.port;
        }
        if args.web_host.is_none() {
            args.web_host = web.host;
        }
        if args.web_bundle_path.is_none() {
            args.web_bundle_path = web.bundle_path;
        }
        if args.web_public_url.is_none() {
            args.web_public_url = web.public_url;
        }
    }

    // GitHub
    if let Some(gh) = config.github {
        if args.github_client_id.is_none() {
            args.github_client_id = gh.client_id;
        }
        if args.github_client_secret.is_none() {
            args.github_client_secret = gh.client_secret;
        }
        if args.github_redirect_uri.is_none() {
            args.github_redirect_uri = gh.redirect_uri;
        }
        if !args.github_stub {
            args.github_stub = gh.stub.unwrap_or(false);
        }
        if args.github_stub_codes.is_none() {
            args.github_stub_codes = gh.stub_codes;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_config() {
        let yaml = r#"
agent: stub
daemon: true
model: sonnet

livekit:
  url: ws://127.0.0.1:7880
  api_key: devkey
  api_secret: secret
  room: my-room
  identity: server

web:
  port: 8888
  host: 0.0.0.0
  bundle_path: packages/tddy-web/dist

github:
  client_id: my-id
  client_secret: my-secret
  redirect_uri: http://localhost:8888/auth/callback
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.agent.as_deref(), Some("stub"));
        assert_eq!(config.daemon, Some(true));
        assert_eq!(config.model.as_deref(), Some("sonnet"));

        let lk = config.livekit.as_ref().unwrap();
        assert_eq!(lk.url.as_deref(), Some("ws://127.0.0.1:7880"));
        assert_eq!(lk.api_key.as_deref(), Some("devkey"));
        assert_eq!(lk.room.as_deref(), Some("my-room"));

        let web = config.web.as_ref().unwrap();
        assert_eq!(web.port, Some(8888));
        assert_eq!(web.host.as_deref(), Some("0.0.0.0"));

        let gh = config.github.as_ref().unwrap();
        assert_eq!(gh.client_id.as_deref(), Some("my-id"));
    }

    #[test]
    fn parse_minimal_config() {
        let yaml = "daemon: true\n";
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.daemon, Some(true));
        assert!(config.livekit.is_none());
    }

    #[test]
    fn parse_stub_config() {
        let yaml = r#"
daemon: true
github:
  stub: true
  stub_codes: "test-code:testuser"
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        let gh = config.github.unwrap();
        assert!(gh.stub.unwrap());
        assert_eq!(gh.stub_codes.as_deref(), Some("test-code:testuser"));
    }

    #[test]
    fn parse_log_config() {
        let yaml = r#"
log:
  loggers:
    default:
      output: stderr
      format: "{timestamp} [{level}] [{target}] {message}"
    webrtc_file:
      output: { file: "logs/webrtc.log" }
  default:
    level: debug
    logger: default
  policies:
    - selector: { target: "libwebrtc" }
      level: debug
      logger: webrtc_file
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        let log = config.log.as_ref().unwrap();
        assert_eq!(log.default.logger, "default");
        assert_eq!(log.default.level, log::LevelFilter::Debug);
        assert!(matches!(
            log.loggers.get("default").unwrap().output,
            tddy_core::LogOutput::Stderr
        ));
        assert!(matches!(
            log.loggers.get("webrtc_file").unwrap().output,
            tddy_core::LogOutput::File(_)
        ));
        assert_eq!(log.policies.len(), 1);
        assert_eq!(log.policies[0].logger.as_deref(), Some("webrtc_file"));
    }

    #[test]
    fn unknown_field_is_rejected() {
        let yaml = "bogus_field: true\n";
        let result: Result<Config, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn cli_args_take_precedence_over_config() {
        let config: Config = serde_yaml::from_str("model: sonnet\ndaemon: true\n").unwrap();
        let mut args = Args {
            goal: None,
            plan_dir: None,
            conversation_output: None,
            model: Some("opus".to_string()),
            allowed_tools: None,
            log: None,
            log_level: None,
            agent: Some("claude".to_string()),
            prompt: None,
            grpc: None,
            session_id: None,
            resume_from: None,
            daemon: false,
            livekit_url: None,
            livekit_token: None,
            livekit_room: None,
            livekit_identity: None,
            livekit_api_key: None,
            livekit_api_secret: None,
            livekit_public_url: None,
            web_port: None,
            web_bundle_path: None,
            web_host: None,
            web_public_url: None,
            github_client_id: None,
            github_client_secret: None,
            github_redirect_uri: None,
            github_stub: false,
            github_stub_codes: None,
            mouse: false,
            project_id: None,
            cursor_agent_path: None,
        };
        merge_config_into_args(&mut args, config);
        // CLI set model=opus, config has model=sonnet → opus wins
        assert_eq!(args.model.as_deref(), Some("opus"));
        // CLI didn't set daemon, config has daemon=true → true
        assert!(args.daemon);
    }
}
