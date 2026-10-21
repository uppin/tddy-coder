use crate::config::DaemonConfig;
use std::path::PathBuf;

/// Apply environment variable overrides to config (e.g. from .env loaded by web-dev).
///
/// Also sets `codex_oauth_loopback_proxy_eligible` from `TDDY_CODEX_OAUTH_LOOPBACK_PROXY_ELIGIBLE`
/// when present.
pub fn apply_env_overrides(config: &mut DaemonConfig) {
    if let Some(v) = env_var("LIVEKIT_PUBLIC_URL") {
        if let Some(ref mut lk) = config.livekit {
            lk.public_url = Some(v.clone());
            lk.url = Some(v);
        }
    }
    if let Some(v) = env_var("LIVEKIT_URL") {
        if let Some(ref mut lk) = config.livekit {
            lk.url = Some(v);
        }
    }
    if let Some(v) = env_var("LIVEKIT_API_KEY") {
        if let Some(ref mut lk) = config.livekit {
            lk.api_key = Some(v);
        }
    }
    if let Some(v) = env_var("LIVEKIT_API_SECRET") {
        if let Some(ref mut lk) = config.livekit {
            lk.api_secret = Some(v);
        }
    }
    if let Some(v) = env_var("WEB_HOST") {
        config.listen.web_host = Some(v);
    }
    if let Some(v) = env_var("WEB_PUBLIC_URL") {
        let base = v.trim_end_matches('/');
        if let Some(ref mut g) = config.github {
            g.redirect_uri = Some(format!("{}/auth/callback", base));
        }
    }
    if let Some(v) = env_var("GITHUB_CLIENT_ID") {
        if let Some(ref mut g) = config.github {
            g.client_id = Some(v);
        }
    }
    if let Some(v) = env_var("GITHUB_CLIENT_SECRET") {
        if let Some(ref mut g) = config.github {
            g.client_secret = Some(v);
        }
    }
    if let Some(v) = env_var("GITHUB_REDIRECT_URI") {
        if let Some(ref mut g) = config.github {
            g.redirect_uri = Some(v);
        }
    }
    if let Some(v) = env_var("TDDY_DAEMON_INSTANCE_ID") {
        config.daemon_instance_id = Some(v);
    }
    config.apply_oauth_loopback_proxy_env_override();
    config.apply_telegram_env_overrides();
    config.apply_timing_env_overrides();
}

fn env_var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.is_empty())
}

/// The tddy home data directory: config is the single source of truth.
/// Where this daemon keeps sessions and state, given what the configuration says and what the
/// environment overrides it with.
///
/// `TDDY_DATA_DIR` wins: it is how a development run is kept out of the `~/.tddy` an installed app
/// owns, and how one machine can host two daemons that share a configuration file without sharing
/// their sessions. Taken as a parameter rather than read here so the rule is testable without
/// process-wide state.
pub(super) fn data_dir_from(
    config: &DaemonConfig,
    env_override: Option<String>,
) -> Option<PathBuf> {
    env_override
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| config.tddy_data_dir.clone())
}

pub(super) fn tddy_data_dir_for(config: &DaemonConfig) -> PathBuf {
    data_dir_from(config, env_var("TDDY_DATA_DIR"))
        .or_else(tddy_core::output::default_tddy_data_dir)
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
            PathBuf::from(home).join(".tddy")
        })
}
