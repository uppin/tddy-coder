//! Following a target session's hook log: how the hooks of an agent that runs on a **target** — a
//! machine with no tddy software on it — reach the daemon.
//!
//! On a target every hook is `tddy_core::remote_hook_command`: a shell one-liner that appends
//! `<event>\t<hook token>\t<payload on one line>\n` to a log in the session's worktree
//! ([`tddy_core::REMOTE_HOOK_LOG`]). The daemon creates that log before the agent starts
//! ([`create_remote_hook_log`]), reads it through the session's remote shell for as long as the
//! agent runs ([`RemoteHookFollower`]), and removes it when the session goes
//! ([`remove_remote_hook_log`]).
//!
//! The follower runs [`follow_command`] through [`RemoteShell::exec_streamed`], counts the bytes
//! it has read, and hands every whole, well-formed line ([`HookLine::parse`]) to a [`HookSink`], in
//! the order the log holds them. When the stream ends — a dropped connection — it opens another
//! from the byte it had reached, so no hook is lost and none is delivered twice.
//!
//! The log is writable by anything running as the session's user on the target: a line is
//! untrusted input. One with the wrong token, an event no hooks adapter installs or a payload that
//! is not JSON is dropped and counted, never delivered. Nothing travels the other way: a hook on a
//! target reports, and is never answered.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-15-remote-shell-remote-hooks.md.

use std::fmt;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use tddy_remote_shell::{RemoteShell, RemoteShellError};

/// One hook an agent on a target reported: a line of its hook log, checked.
#[derive(Debug, Clone, PartialEq)]
pub struct HookLine {
    /// The hook event the command was installed for (`SessionStart`, `Stop`, Cursor's `stop`, …).
    pub event: String,
    /// The event's payload as the agent wrote it to the hook's stdin.
    pub payload: serde_json::Value,
}

/// Why a line of a hook log is not a hook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookLineError {
    /// Not `<event>\t<token>\t<payload>`: a torn or foreign line.
    Malformed,
    /// The line's token is not the session's.
    WrongToken,
    /// The event is one no hooks adapter installs.
    UnknownEvent(String),
    /// The payload is not JSON.
    BrokenPayload,
}

impl fmt::Display for HookLineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed => write!(f, "the line is not an event, a token and a payload"),
            Self::WrongToken => write!(f, "the line's token is not the session's"),
            Self::UnknownEvent(event) => write!(f, "no hooks adapter installs the event {event}"),
            Self::BrokenPayload => write!(f, "the line's payload is not JSON"),
        }
    }
}

impl std::error::Error for HookLineError {}

impl HookLine {
    /// The hook `line` reports — one line of a hook log, without its newline — when it carries
    /// `expected_token`, names an event a hooks adapter installs (Claude Code's, Muse's or
    /// Cursor's) and its payload is JSON. The payload is everything after the second tab, so a tab
    /// inside it is kept. The token is checked before anything else of the line is looked at.
    pub fn parse(line: &str, expected_token: &str) -> Result<HookLine, HookLineError> {
        // TODO(remote-hooks): implement
        let _ = (line, expected_token);
        Err(HookLineError::Malformed)
    }
}

/// Where a target session's hooks go: the session-hook handling a local hook reaches.
#[async_trait]
pub trait HookSink: Send + Sync {
    /// One hook the session's agent reported. Called once per hook, in the log's order; the next
    /// is not delivered until this one returns.
    async fn deliver(&self, line: HookLine);
}

/// The command a follower runs on the target to read `log_path` from the byte `offset` on, and
/// everything appended after it:
///
/// ```text
/// tail -c +<offset + 1> -f '<log_path>'
/// ```
///
/// `tail -c +N` starts at the Nth byte, counted from one. The path is single-quoted for a POSIX
/// shell.
#[must_use]
pub fn follow_command(log_path: &str, offset: u64) -> String {
    // TODO(remote-hooks): implement
    let _ = (log_path, offset);
    String::new()
}

/// Create the session's hook log in `worktree` on `shell`'s target — empty, readable and writable
/// by its owner alone — and keep it out of the checkout's git status (`info/exclude`, as the
/// session pointer is). Returns the log's path on the target: what
/// `tddy_core::HookCommandParams::remote_log` carries and [`RemoteHookFollower::start`] follows.
pub async fn create_remote_hook_log(
    shell: &dyn RemoteShell,
    worktree: &Path,
) -> Result<String, RemoteShellError> {
    // TODO(remote-hooks): implement
    let _ = (shell, worktree);
    Err(RemoteShellError::Spawn(
        "creating a target session's hook log is not implemented".to_string(),
    ))
}

/// Remove the session's hook log from `worktree` on `shell`'s target. A log that is already gone
/// is not an error.
pub async fn remove_remote_hook_log(
    shell: &dyn RemoteShell,
    worktree: &Path,
) -> Result<(), RemoteShellError> {
    // TODO(remote-hooks): implement
    let _ = (shell, worktree);
    Err(RemoteShellError::Spawn(
        "removing a target session's hook log is not implemented".to_string(),
    ))
}

/// The reader of one target session's hook log, running until it is stopped.
pub struct RemoteHookFollower {
    // TODO(remote-hooks): implement — the reading task and the dropped-line count.
    _private: (),
}

impl RemoteHookFollower {
    /// Start following `log_path` on `shell`'s target from its first byte, delivering every line
    /// that carries `hook_token` to `sink`. Must be called inside a tokio runtime.
    ///
    /// A stream that ends, or cannot be opened, is opened again after a back-off, from the byte
    /// the follower had reached.
    pub fn start(
        shell: Arc<dyn RemoteShell>,
        log_path: &str,
        hook_token: &str,
        sink: Arc<dyn HookSink>,
    ) -> Self {
        // TODO(remote-hooks): implement
        let _ = (shell, log_path, hook_token, sink);
        Self { _private: () }
    }

    /// How many lines this follower has dropped: wrong token, unknown event, broken payload or
    /// torn line.
    pub fn dropped_lines(&self) -> u64 {
        // TODO(remote-hooks): implement
        0
    }

    /// Stop following: the remote command is ended and nothing more is delivered. Returns once the
    /// reader is gone.
    pub async fn stop(self) {
        // TODO(remote-hooks): implement
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    const TOKEN: &str = "tok-7f3a9c";

    #[test]
    fn a_line_with_the_sessions_token_is_its_event_and_its_payload() {
        // Given
        let line = format!(r#"Stop	{TOKEN}	{{"hook_event_name":"Stop","session_id":"c-41"}}"#);

        // When
        let hook = HookLine::parse(&line, TOKEN);

        // Then
        assert_eq!(
            hook,
            Ok(HookLine {
                event: "Stop".to_string(),
                payload: serde_json::json!({"hook_event_name": "Stop", "session_id": "c-41"}),
            })
        );
    }

    #[test]
    fn a_cursor_event_is_a_hook_too() {
        // Given a line logged by Cursor, whose events are camel-cased
        let line =
            format!(r#"beforeSubmitPrompt	{TOKEN}	{{"hook_event_name":"beforeSubmitPrompt"}}"#);

        // When
        let event = HookLine::parse(&line, TOKEN).map(|hook| hook.event);

        // Then
        assert_eq!(event, Ok("beforeSubmitPrompt".to_string()));
    }

    #[test]
    fn a_tab_inside_the_payload_stays_in_the_payload() {
        // Given a payload holding a raw tab between two of its members
        let line = format!("PostToolUse\t{TOKEN}\t{{\"tool_name\":\"Bash\",\t\"cwd\":\"/srv\"}}");

        // When
        let payload = HookLine::parse(&line, TOKEN).map(|hook| hook.payload);

        // Then
        assert_eq!(
            payload,
            Ok(serde_json::json!({"tool_name": "Bash", "cwd": "/srv"}))
        );
    }

    #[test]
    fn a_line_with_another_token_is_refused() {
        // Given a well-formed line written with a token that is not the session's
        let line = r#"Stop	tok-of-another-session	{"hook_event_name":"Stop"}"#;

        // When
        let hook = HookLine::parse(line, TOKEN);

        // Then
        assert_eq!(hook, Err(HookLineError::WrongToken));
    }

    #[test]
    fn the_token_is_checked_before_the_event() {
        // Given a line wrong in both its token and its event
        let line = r#"Reboot	tok-of-another-session	{"hook_event_name":"Reboot"}"#;

        // When
        let hook = HookLine::parse(line, TOKEN);

        // Then
        assert_eq!(hook, Err(HookLineError::WrongToken));
    }

    #[test]
    fn an_event_no_adapter_installs_is_refused_by_name() {
        // Given
        let line = format!(r#"Reboot	{TOKEN}	{{"hook_event_name":"Reboot"}}"#);

        // When
        let hook = HookLine::parse(&line, TOKEN);

        // Then
        assert_eq!(hook, Err(HookLineError::UnknownEvent("Reboot".to_string())));
    }

    #[test]
    fn a_payload_that_is_not_json_is_refused() {
        // Given a line torn in the middle of its payload
        let line = format!(r#"Stop	{TOKEN}	{{"hook_event_name":"St"#);

        // When
        let hook = HookLine::parse(&line, TOKEN);

        // Then
        assert_eq!(hook, Err(HookLineError::BrokenPayload));
    }

    #[test]
    fn a_line_without_a_payload_field_is_malformed() {
        // Given a line holding an event and the session's token and nothing after them
        let line = format!("Stop\t{TOKEN}");

        // When
        let hook = HookLine::parse(&line, TOKEN);

        // Then
        assert_eq!(hook, Err(HookLineError::Malformed));
    }

    #[test]
    fn the_follow_command_starts_at_the_first_byte_of_a_log_not_read_yet() {
        // Given
        let log = "/home/dev/repo/.worktrees/s1/.tddy/hooks.log";

        // When
        let command = follow_command(log, 0);

        // Then
        assert_eq!(
            command,
            "tail -c +1 -f '/home/dev/repo/.worktrees/s1/.tddy/hooks.log'"
        );
    }

    #[test]
    fn the_follow_command_resumes_one_byte_past_what_was_read() {
        // Given a log of which 4096 bytes were read
        let log = "/home/dev/repo/.worktrees/s1/.tddy/hooks.log";

        // When
        let command = follow_command(log, 4096);

        // Then
        assert_eq!(
            command,
            "tail -c +4097 -f '/home/dev/repo/.worktrees/s1/.tddy/hooks.log'"
        );
    }

    #[test]
    fn the_follow_command_quotes_a_log_path_with_a_space_and_a_quote() {
        // Given
        let log = "/home/dev/o'brien's repo/.tddy/hooks.log";

        // When
        let command = follow_command(log, 0);

        // Then
        assert_eq!(
            command,
            r"tail -c +1 -f '/home/dev/o'\''brien'\''s repo/.tddy/hooks.log'"
        );
    }
}
