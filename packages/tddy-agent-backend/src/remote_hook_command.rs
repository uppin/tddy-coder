//! The hook command of a session whose agent runs on a **target**: a machine the daemon reaches
//! through a remote shell, with no tddy software on it.
//!
//! A local hook runs `tddy-tools session-hook`, which needs that binary and a route to the daemon.
//! A target has neither. There the hook is a shell one-liner that appends one line to a log file
//! in the session's worktree, and the daemon follows that file through the remote shell
//! (`tddy_agent_launch::remote_hook_follower`). One line per hook, three tab-separated fields:
//!
//! ```text
//! <event>\t<hook token>\t<the payload, its newlines removed>\n
//! ```
//!
//! The carrier is a property of the command, not of the agent: every hooks builder takes its
//! command from the same [`HookCommandParams`](crate::claude_hooks::HookCommandParams), whose
//! `remote_log` selects this command.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-15-remote-shell-remote-hooks.md.

/// Where a target session's hook log lives, relative to the session's worktree on the target.
pub const REMOTE_HOOK_LOG: &str = ".tddy/hooks.log";

/// The hook log of a session whose worktree on its target is `worktree`: the path every hook of
/// the session appends to, the launch creates and the daemon follows.
///
/// It is the worktree's path joined with [`REMOTE_HOOK_LOG`], never [`REMOTE_HOOK_LOG`] alone: an
/// agent runs each hook in its current directory, which follows the agent's `cd`, so a log named
/// relative to it is looked for in whatever directory the agent last entered (seen 2026-11-24 on a
/// Coder workspace: every hook failed with `cannot create .tddy/hooks.log: Directory nonexistent`
/// once the agent had entered a subdirectory, and the failing `PreToolUse` hook refused every
/// tool).
#[must_use]
pub fn remote_hook_log_in(worktree: &std::path::Path) -> String {
    worktree
        .join(REMOTE_HOOK_LOG)
        .to_string_lossy()
        .into_owned()
}

/// The shell command an agent on a target runs for the hook `event`: it reads the hook's payload
/// on its standard input and appends `<event>\t<hook_token>\t<payload>\n` to `log_path`, the
/// payload's newlines removed.
///
/// It names nothing but `printf`, `cat` and `tr`, so it runs under a bare POSIX `sh`. The line is
/// written once the whole payload has been read, in one piece: two hooks that fire together each
/// leave a whole line. `event`, `hook_token` and `log_path` are single-quoted for the shell. The
/// log's directory must exist; the command creates the file.
#[must_use]
pub fn remote_hook_command(event: &str, hook_token: &str, log_path: &str) -> String {
    use crate::claude_hooks::shell_quote;
    format!(
        "printf '%s\\t%s\\t%s\\n' {} {} \"$(cat | tr -d '\\n')\" >> {}",
        shell_quote(event),
        shell_quote(hook_token),
        shell_quote(log_path),
    )
}

/// The hook-log event a child session on a target reports to its orchestrator with
/// (`#orchestra` 2/9): the child pipes `{"text": "<message>"}` into
/// `remote_hook_command(REMOTE_MESSAGE_EVENT, <hook token>, <log>)`, and the daemon's follower posts
/// the text to the session's orchestrator as a session message. No hooks adapter installs it; a
/// target has no `tddy-tools`, so this line is its only way to send one.
pub const REMOTE_MESSAGE_EVENT: &str = "TddyMessage";

/// Where a target session's answers live, relative to the session's worktree on the target: one
/// file per waiting hook, named by the nonce the hook minted (`#target-parity` 1/7).
pub const REMOTE_ANSWERS_DIR: &str = ".tddy/answers";

/// The shell command an agent on a target runs for a hook that **waits for an answer** (a
/// `PermissionRequest`): it mints a nonce, appends `<event>@<nonce>\t<hook_token>\t<payload>\n` to
/// `log_path` (the payload's newlines removed, as [`remote_hook_command`] does), then waits up to
/// `wait_secs` for `<answers_dir>/<nonce>` and prints that file's content — Claude's decision — as
/// its only output, removing the file. An empty answer file ends the wait at once with no output;
/// no file within `wait_secs` ends it with no output too. Either way Claude then asks in its own
/// terminal.
///
/// Like [`remote_hook_command`] it names nothing but POSIX shell built-ins and `cat`, `tr`,
/// `date`, `sleep` and `rm`: the target has no tddy software. The daemon writes the answer file
/// through the session's remote shell (`tddy_agent_launch::remote_hook_follower`).
#[must_use]
pub fn remote_answering_hook_command(
    event: &str,
    hook_token: &str,
    log_path: &str,
    answers_dir: &str,
    wait_secs: u64,
) -> String {
    use crate::claude_hooks::shell_quote;
    format!(
        "n=\"$$-$(date +%s)\"; payload=\"$(cat | tr -d '\\n')\"; \
         printf '%s@%s\\t%s\\t%s\\n' {event} \"$n\" {token} \"$payload\" >> {log}; \
         answer={answers}/\"$n\"; waited=0; \
         while [ \"$waited\" -le {wait_secs} ]; do \
         if [ -e \"$answer\" ]; then cat \"$answer\"; rm -f \"$answer\"; break; fi; \
         sleep 1; waited=$((waited + 1)); done; exit 0",
        event = shell_quote(event),
        token = shell_quote(hook_token),
        log = shell_quote(log_path),
        answers = shell_quote(answers_dir),
    )
}

#[cfg(all(test, unix))]
#[path = "remote_hook_command_tests.rs"]
mod tests;
