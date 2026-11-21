//! Where a session's Claude Code hooks live, and releasing the worktree file they used to live in.
//!
//! Each session's hooks are a file of their own, `<workdir>/.tddy/hooks/<session_id>/claude-settings.json`,
//! loaded with `--settings` on every launch. They used to be `<workdir>/.claude/settings.local.json`,
//! one per worktree: a second session started in the same worktree (the project-home worktree hosts
//! the home session *and* the project's orchestrators) took the first one's hooks, and a resume —
//! which never rewrote the file — kept whatever daemon socket and session the last start had named
//! (reported 2026-11-21: `/clear` in the home session was never recorded).
//!
//! Claude Code merges hooks from every settings source, so the daemon's entries left in a worktree's
//! `settings.local.json` would keep firing beside the session's own file — reporting twice, or to
//! another session. [`release_legacy_claude_settings`] takes them out and leaves everything else
//! in that file (the permission rules Claude Code saves there) alone.

use std::path::{Path, PathBuf};

use crate::remote_hook_command::REMOTE_HOOK_LOG;

/// The directory under the agent's working directory that holds a directory of hooks per session.
pub const SESSION_HOOKS_DIR: &str = ".tddy/hooks";

/// A session's Claude Code hooks file, inside its [`SESSION_HOOKS_DIR`] directory.
pub const CLAUDE_SESSION_HOOKS_FILE: &str = "claude-settings.json";

/// The worktree-wide file the daemon wrote every session's hooks into before they were per session.
pub const LEGACY_CLAUDE_SETTINGS_FILE: &str = ".claude/settings.local.json";

/// The hooks file of `session_id` for an agent working in `workdir`.
#[must_use]
pub fn claude_session_hooks_file(workdir: &Path, session_id: &str) -> PathBuf {
    workdir
        .join(SESSION_HOOKS_DIR)
        .join(session_id)
        .join(CLAUDE_SESSION_HOOKS_FILE)
}

/// What becomes of a legacy `settings.local.json` once the daemon's entries are out of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegacyRelease {
    /// It holds none of the daemon's hooks (or is not a JSON object): it is left as it is.
    Keep,
    /// The daemon's hooks were taken out and something else remains: these are its new bytes.
    Rewrite(Vec<u8>),
    /// Nothing but the daemon's entries was in it: the file is removed.
    Remove,
}

/// Whether `command` is a hook command the daemon installs: `tddy-tools session-hook` on a host,
/// or, on a target, an append to its hook log — the plain one, or the waiting one that numbers
/// its line and then reads its answer file, which does not start with the `printf`.
fn is_daemon_hook_command(command: &str) -> bool {
    command.contains(" session-hook --session ")
        || (command.contains("printf ") && command.contains(REMOTE_HOOK_LOG))
}

/// Take the daemon's hooks out of a legacy `settings.local.json` holding `bytes`.
///
/// A hook entry is the daemon's when its command is one [`build_claude_hooks_settings`] writes;
/// a matcher group left with no hook, and an event left with no group, go with it. The daemon wrote
/// that file whole — hooks and, for a routed launch, the route's `env` pin — so once any of its
/// hooks are found the `env` it pinned goes too: left behind, a stale route pin would outlive the
/// route. Every other key (Claude Code's own `permissions` among them) is kept.
///
/// [`build_claude_hooks_settings`]: crate::claude_hooks::build_claude_hooks_settings
#[must_use]
pub fn release_legacy_claude_settings(bytes: &[u8]) -> LegacyRelease {
    let Ok(serde_json::Value::Object(mut settings)) = serde_json::from_slice(bytes) else {
        return LegacyRelease::Keep;
    };
    let Some(serde_json::Value::Object(hooks)) = settings.get_mut("hooks") else {
        return LegacyRelease::Keep;
    };
    let mut removed_any = false;
    hooks.retain(|_event, groups| {
        let Some(groups) = groups.as_array_mut() else {
            return true;
        };
        groups.retain_mut(|group| {
            let Some(entries) = group.get_mut("hooks").and_then(|h| h.as_array_mut()) else {
                return true;
            };
            let before = entries.len();
            entries.retain(|entry| {
                !entry
                    .get("command")
                    .and_then(|c| c.as_str())
                    .is_some_and(is_daemon_hook_command)
            });
            removed_any |= entries.len() != before;
            !entries.is_empty()
        });
        !groups.is_empty()
    });
    if !removed_any {
        return LegacyRelease::Keep;
    }
    if hooks.is_empty() {
        settings.remove("hooks");
    }
    settings.remove("env");
    if settings.is_empty() {
        return LegacyRelease::Remove;
    }
    LegacyRelease::Rewrite(format!("{:#}", serde_json::Value::Object(settings)).into_bytes())
}

#[cfg(test)]
#[path = "session_file_tests.rs"]
mod tests;
