//! YAML config and LiveKit token resolution (aligned with `tddy-coder` LiveKit fields).

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::Deserialize;

/// Default frames per second when not set in file or CLI.
pub const DEFAULT_FPS: u32 = 30;

/// Token TTL when generating from API key/secret (long sessions).
const TOKEN_TTL: Duration = Duration::from_secs(3600);

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileConfig {
    #[serde(default)]
    pub livekit: Option<LiveKitYaml>,
    /// Default FPS when CLI `--fps` is not set.
    #[serde(default)]
    pub fps: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiveKitYaml {
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
}

/// CLI values that override YAML when set.
#[derive(Debug, Default, Clone)]
pub struct CliOverrides {
    pub room: Option<String>,
    pub identity: Option<String>,
    pub fps: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct ResolvedStreamConfig {
    pub url: String,
    pub token: String,
    pub room: String,
    pub identity: String,
    pub fps: u32,
}

pub fn load_config_file(path: &Path) -> Result<FileConfig> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read config file {}", path.display()))?;
    let cfg: FileConfig = serde_yaml::from_str(&raw).context("invalid YAML config")?;
    Ok(cfg)
}

/// Merge file config with CLI overrides and produce connection parameters.
pub fn resolve_stream_config(
    file: Option<&FileConfig>,
    overrides: &CliOverrides,
) -> Result<ResolvedStreamConfig> {
    let livekit = file
        .and_then(|f| f.livekit.as_ref())
        .context("config: missing `livekit` section")?;

    let url = livekit.url.clone().context("livekit.url is required")?;

    let room = overrides
        .room
        .clone()
        .or_else(|| livekit.room.clone())
        .context("livekit.room is required (YAML or --room)")?;

    let identity = overrides
        .identity
        .clone()
        .or_else(|| livekit.identity.clone())
        .context("livekit.identity is required (YAML or --identity)")?;

    let token = if let Some(t) = &livekit.token {
        t.clone()
    } else {
        let api_key = livekit
            .api_key
            .as_ref()
            .context("livekit.api_key and api_secret are required when livekit.token is not set")?;
        let api_secret = livekit
            .api_secret
            .as_ref()
            .context("livekit.api_secret is required when livekit.token is not set")?;

        let gen = tddy_livekit::TokenGenerator::new(
            api_key.clone(),
            api_secret.clone(),
            room.clone(),
            identity.clone(),
            TOKEN_TTL,
        );
        gen.generate()
            .map_err(|e| anyhow::anyhow!("failed to generate LiveKit token: {}", e))?
    };

    let fps = overrides
        .fps
        .or_else(|| file.and_then(|f| f.fps))
        .unwrap_or(DEFAULT_FPS);

    if fps == 0 {
        anyhow::bail!("fps must be greater than zero");
    }

    Ok(ResolvedStreamConfig {
        url,
        token,
        room,
        identity,
        fps,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_uses_token_when_present() {
        let file = FileConfig {
            livekit: Some(LiveKitYaml {
                url: Some("ws://localhost:7880".to_string()),
                token: Some("jwt-here".to_string()),
                room: Some("r".to_string()),
                identity: Some("i".to_string()),
                api_key: None,
                api_secret: None,
            }),
            fps: Some(15),
        };
        let r = resolve_stream_config(Some(&file), &CliOverrides::default()).unwrap();
        assert_eq!(r.token, "jwt-here");
        assert_eq!(r.fps, 15);
    }

    #[test]
    fn resolve_cli_overrides_room_identity_fps() {
        let file = FileConfig {
            livekit: Some(LiveKitYaml {
                url: Some("ws://localhost:7880".to_string()),
                token: Some("t".to_string()),
                room: Some("yaml-room".to_string()),
                identity: Some("yaml-id".to_string()),
                api_key: None,
                api_secret: None,
            }),
            fps: Some(10),
        };
        let r = resolve_stream_config(
            Some(&file),
            &CliOverrides {
                room: Some("cli-room".to_string()),
                identity: Some("cli-id".to_string()),
                fps: Some(60),
            },
        )
        .unwrap();
        assert_eq!(r.room, "cli-room");
        assert_eq!(r.identity, "cli-id");
        assert_eq!(r.fps, 60);
    }

    #[test]
    fn resolve_errors_when_livekit_missing() {
        let file = FileConfig::default();
        assert!(resolve_stream_config(Some(&file), &CliOverrides::default()).is_err());
    }
}
