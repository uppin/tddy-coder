//! The environment an agent process starts from (`#agent-tools 28/28`): an **allow-list**, never
//! the daemon's environment minus a deny-list.
//!
//! The daemon computes an agent child's whole environment and the spawn site applies exactly that
//! ([`apply_agent_child_env`]): the spawning process's environment is never inherited. The whole
//! environment is
//!
//! 1. the variables of the **base set** the daemon holds — data, not code: the coding-agent
//!    registry loads it from YAML (`tddy-coding-agent-specs`' built-in `specs/agent-env.yaml`, or an
//!    operator's `<tddyhome>/tools/agent-env/base.yaml`), which this crate cannot name, so it is a
//!    parameter here;
//! 2. what the launch sets on purpose — the spec's `run.env` / `headless.env` / `acp.env`, the
//!    provider route's variables, and the values the spec's `coding_agent.pass_env` names.
//!
//! An inherited `TDDY_*` variable never passes: neither a base set nor a `pass_env` may name one
//! (a load error). A `TDDY_*` variable the launch sets on purpose does pass — a documented
//! exception, today only Codex's `TDDY_CODEX_OAUTH_OUT`, that expires with the Codex OAuth
//! follow-up stack (`docs/dev/todo/2026-11-05-codex-oauth-out-env-in-headless-agent.md`).

use std::ffi::{OsStr, OsString};

/// The prefix of the variables no agent child inherits, whatever its configuration says.
pub const TDDY_ENV_PREFIX: &str = "TDDY_";

/// Whether `name` is the variable `pattern` names: an exact name, or every name starting with
/// `PREFIX_` for a pattern `PREFIX_*`. A `*` anywhere but the end matches nothing.
pub fn matches_env_pattern(pattern: &str, name: &OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    match pattern.strip_suffix('*') {
        Some(prefix) if !prefix.contains('*') => name.starts_with(prefix),
        Some(_) => false,
        None => pattern == name,
    }
}

/// Whether `name` is one of the `base` set's variables (names and `PREFIX_*` patterns).
pub fn is_base_variable(base: &[String], name: &OsStr) -> bool {
    base.iter()
        .any(|pattern| matches_env_pattern(pattern, name))
}

/// The whole environment of an agent child: the variables of `parent` (the daemon's environment)
/// that `base` names, then `launch` in order — a launch variable replaces a base one of the same
/// name. Nothing else of `parent` is kept; no inherited `TDDY_*` variable is ever kept.
pub fn agent_child_env(
    base: &[String],
    parent: impl IntoIterator<Item = (OsString, OsString)>,
    launch: &[(String, String)],
) -> Vec<(OsString, OsString)> {
    let mut env: Vec<(OsString, OsString)> = parent
        .into_iter()
        .filter(|(name, _)| {
            !name.to_string_lossy().starts_with(TDDY_ENV_PREFIX) && is_base_variable(base, name)
        })
        .collect();
    for (name, value) in launch {
        env.retain(|(kept, _)| kept.as_os_str() != OsStr::new(name));
        env.push((OsString::from(name), OsString::from(value)));
    }
    env
}

/// `command` made to start from exactly `env` — the agent's whole environment, as the daemon
/// computed it ([`agent_child_env`]): its inherited environment cleared first.
pub fn apply_agent_child_env(command: &mut std::process::Command, env: &[(String, String)]) {
    command.env_clear();
    command.envs(env.iter().map(|(name, value)| (name, value)));
}

#[cfg(test)]
mod agent_env_tests;
