use crate::config::DaemonConfig;
use crate::config::TelegramConfig;
use crate::config::MIN_WORKTREE_ROOM_INTERVAL_MS;

/// The pure half of [`DaemonConfig::apply_timing_env_overrides`]. `lookup` stands in for the
/// process environment so the precedence rules are testable without mutating it — which no test
/// running alongside others can do safely.
pub(super) fn merge_timing_env(config: &mut DaemonConfig, lookup: impl Fn(&str) -> Option<String>) {
    if let Some(ms) = env_u64(&lookup, "TDDY_SPAWN_STARTUP_GRACE_PERIOD_MS") {
        config.spawn_startup_grace_period_ms = ms;
    }
    if let Some(ms) = env_u64(&lookup, "TDDY_SPAWN_STARTUP_POLL_INTERVAL_MS") {
        config.spawn_startup_poll_interval_ms = ms;
    }
    if let Some(secs) = env_u64(&lookup, "TDDY_AGENT_WARMUP_TIMEOUT_SECS") {
        config.agent_warmup.timeout_secs = secs;
    }
    if let Some(ms) = env_u64(&lookup, "TDDY_AGENT_WARMUP_RETRY_INTERVAL_MS") {
        config.agent_warmup.retry_interval_ms = ms;
    }
    if let Some(secs) = env_u64(&lookup, "TDDY_AGENT_WARMUP_REQUEST_TIMEOUT_SECS") {
        config.agent_warmup.request_timeout_secs = secs;
    }
    if let Some(ms) = env_session_room_ms(&lookup, "TDDY_SESSION_ROOM_POLL_INTERVAL_MS") {
        config.session_room.poll_interval_ms = ms;
    }
    if let Some(ms) = env_session_room_ms(&lookup, "TDDY_SESSION_ROOM_GIT_TIMEOUT_MS") {
        config.session_room.git_timeout_ms = ms;
    }
}

/// A session-room millisecond override, or `None` (with a warning) when it is not a whole number
/// at or above [`MIN_WORKTREE_ROOM_INTERVAL_MS`]. Keeping the file/default value on a bad input is
/// how every other override in [`merge_timing_env`] behaves.
fn env_session_room_ms(lookup: &impl Fn(&str) -> Option<String>, name: &str) -> Option<u64> {
    let ms = env_u64(lookup, name)?;
    if ms < MIN_WORKTREE_ROOM_INTERVAL_MS {
        log::warn!(
            target: "tddy_daemon::config",
            "{name}: {ms} is below the {MIN_WORKTREE_ROOM_INTERVAL_MS}ms floor (each poll runs four git subprocesses per hosted worktree), ignoring"
        );
        return None;
    }
    Some(ms)
}

/// A whole-number environment override, or `None` (with a warning) when it is not one.
fn env_u64(lookup: &impl Fn(&str) -> Option<String>, name: &str) -> Option<u64> {
    let raw = lookup(name)?;
    match raw.trim().parse() {
        Ok(parsed) => Some(parsed),
        Err(_) => {
            log::warn!(
                target: "tddy_daemon::config",
                "{name}: expected a whole number, ignoring {raw:?}"
            );
            None
        }
    }
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

pub(super) fn parse_env_bool(s: &str) -> Option<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

pub(super) fn merge_telegram_env(
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
