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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tddy_remote_shell::{ExecRequest, FileMode, RemoteShell, RemoteShellError};
use tokio::io::AsyncReadExt as _;

/// Quote `arg` as one single-quoted POSIX shell word (`'\''` for an embedded quote).
fn shell_quote(arg: &str) -> String {
    format!("'{}'", arg.replace('\'', r"'\''"))
}

/// Whether `event` is installed by any hooks adapter (Claude Code, Muse or Cursor).
fn is_installed_event(event: &str) -> bool {
    tddy_core::claude_hooks::CLAUDE_INSTALLED_EVENTS.contains(&event)
        || tddy_core::muse_hooks::MUSE_INSTALLED_EVENTS.contains(&event)
        || tddy_core::cursor_hooks::CURSOR_HOOK_EVENTS.contains(&event)
}

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
        let mut fields = line.splitn(3, '\t');
        let (Some(event), Some(token), Some(payload)) =
            (fields.next(), fields.next(), fields.next())
        else {
            return Err(HookLineError::Malformed);
        };
        if token != expected_token {
            return Err(HookLineError::WrongToken);
        }
        if !is_installed_event(event) {
            return Err(HookLineError::UnknownEvent(event.to_string()));
        }
        let payload: serde_json::Value =
            serde_json::from_str(payload).map_err(|_| HookLineError::BrokenPayload)?;
        Ok(HookLine {
            event: event.to_string(),
            payload,
        })
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
    format!("tail -c +{} -f {}", offset + 1, shell_quote(log_path))
}

/// Create the session's hook log in `worktree` on `shell`'s target — empty, readable and writable
/// by its owner alone — and keep it out of the checkout's git status (`info/exclude`, as the
/// session pointer is). Returns the log's path on the target: what
/// `tddy_core::HookCommandParams::remote_log` carries and [`RemoteHookFollower::start`] follows.
pub async fn create_remote_hook_log(
    shell: &dyn RemoteShell,
    worktree: &Path,
) -> Result<String, RemoteShellError> {
    let log_path = worktree.join(tddy_core::REMOTE_HOOK_LOG);
    let log = log_path.to_string_lossy().into_owned();
    let exclude_command = format!(
        "if exclude=$(git -C {worktree} rev-parse --path-format=absolute --git-path info/exclude 2>/dev/null); then \
         mkdir -p \"$(dirname \"$exclude\")\"; \
         grep -qxF '/.tddy/hooks.log' \"$exclude\" 2>/dev/null || echo '/.tddy/hooks.log' >> \"$exclude\"; fi",
        worktree = shell_quote(&worktree.to_string_lossy()),
    );
    shell
        .exec(ExecRequest::new(exclude_command))
        .await?
        .success_or_exited()?;
    shell.write_file(&log, b"", FileMode::OwnerOnly).await?;
    Ok(log)
}

/// Remove the session's hook log from `worktree` on `shell`'s target. A log that is already gone
/// is not an error.
pub async fn remove_remote_hook_log(
    shell: &dyn RemoteShell,
    worktree: &Path,
) -> Result<(), RemoteShellError> {
    let log = worktree
        .join(tddy_core::REMOTE_HOOK_LOG)
        .to_string_lossy()
        .into_owned();
    shell
        .exec(ExecRequest::new(format!("rm -f {}", shell_quote(&log))))
        .await?
        .success_or_exited()?;
    Ok(())
}

/// The reader of one target session's hook log, running until it is stopped.
pub struct RemoteHookFollower {
    task: Option<tokio::task::JoinHandle<()>>,
    dropped: Arc<AtomicU64>,
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
        let dropped = Arc::new(AtomicU64::new(0));
        let task_dropped = Arc::clone(&dropped);
        let log_path = log_path.to_string();
        let hook_token = hook_token.to_string();
        let task = tokio::spawn(async move {
            let mut offset: u64 = 0;
            let mut pending: Vec<u8> = Vec::new();
            loop {
                if let Ok(mut streamed) = shell
                    .exec_streamed(&follow_command(&log_path, offset))
                    .await
                {
                    let mut chunk = [0u8; 4096];
                    loop {
                        match streamed.stdout.read(&mut chunk).await {
                            Ok(0) | Err(_) => break,
                            Ok(read) => {
                                offset += read as u64;
                                pending.extend_from_slice(&chunk[..read]);
                                while let Some(end) = pending.iter().position(|byte| *byte == b'\n')
                                {
                                    let mut line: Vec<u8> = pending.drain(..=end).collect();
                                    line.pop();
                                    if line.last() == Some(&b'\r') {
                                        line.pop();
                                    }
                                    let text = String::from_utf8_lossy(&line);
                                    match HookLine::parse(&text, &hook_token) {
                                        Ok(hook) => sink.deliver(hook).await,
                                        Err(_) => {
                                            task_dropped.fetch_add(1, Ordering::SeqCst);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        });
        Self {
            task: Some(task),
            dropped,
        }
    }

    /// How many lines this follower has dropped: wrong token, unknown event, broken payload or
    /// torn line.
    pub fn dropped_lines(&self) -> u64 {
        self.dropped.load(Ordering::SeqCst)
    }

    /// Stop following: the remote command is ended and nothing more is delivered. Returns once the
    /// reader is gone.
    pub async fn stop(mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
            let _ = task.await;
        }
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
