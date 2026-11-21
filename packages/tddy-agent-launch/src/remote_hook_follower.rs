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
//! is not JSON is dropped and counted, never delivered. A hook that waits for an answer (a
//! permission request) is answered through the same shell: [`ActivityHookSink::answering_through`].
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
pub(crate) fn shell_quote(arg: &str) -> String {
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
    /// The nonce of a hook that **waits for an answer**
    /// (`tddy_core::remote_answering_hook_command`), written as `<event>@<nonce>` in the line's
    /// first field: the hook on the target waits for `<REMOTE_ANSWERS_DIR>/<nonce>`. `None` for a
    /// hook that only reports.
    pub answer_nonce: Option<String>,
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
        let (event, nonce) = match event.split_once('@') {
            Some((event, nonce)) => (event, Some(nonce)),
            None => (event, None),
        };
        if !is_installed_event(event) {
            return Err(HookLineError::UnknownEvent(event.to_string()));
        }
        let payload: serde_json::Value =
            serde_json::from_str(payload).map_err(|_| HookLineError::BrokenPayload)?;
        Ok(HookLine {
            event: event.to_string(),
            payload,
            answer_nonce: nonce.map(str::to_string),
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

/// The daemon end of a delivered hook: the two hook reports a local `tddy-tools session-hook`
/// makes, served in-process. Implemented on the session host, which owns the activity service;
/// a report the handling refuses is the delivery's error, never retried by the follower.
#[async_trait]
pub trait HookReportDelivery: Send + Sync {
    /// Serve one `ReportSessionStatus` request, as the activity service's RPC does, and return its
    /// response: a permission or a question is answered with the `request_id` it was recorded under.
    async fn report_session_status(
        &self,
        request: tddy_service::proto::activity::ReportSessionStatusRequest,
    ) -> Result<tddy_service::proto::activity::ReportSessionStatusResponse, String>;
    /// Hold for the orchestrator's answer to a recorded request, as the activity service's
    /// `AwaitQuestionAnswer` does for a local hook: the same presence check, the same recorded
    /// wait, the same outcomes.
    async fn await_question_answer(
        &self,
        request: tddy_service::proto::activity::AwaitQuestionAnswerRequest,
    ) -> Result<tddy_service::proto::activity::AwaitQuestionAnswerResponse, String>;
    /// Serve one `ReportAgentActivity` request, as the activity service's RPC does.
    async fn report_agent_activity(
        &self,
        request: tddy_service::proto::activity::ReportAgentActivityRequest,
    ) -> Result<(), String>;
}

/// The production [`HookSink`]: each followed hook is mapped to the daemon's hook reports by
/// the one payload-to-request mapping (`tddy_session_activity::hook_request`, which the local
/// `tddy-tools session-hook` also reports with) and served through `delivery`. A refused report
/// is logged and the next hook still arrives: a hook must never block on the daemon's answer.
pub struct ActivityHookSink {
    delivery: Arc<dyn HookReportDelivery>,
    session_id: String,
    hook_token: String,
    os_user: String,
    /// Where a waiting hook's answer is written: the session's shell and its worktree on the
    /// target. `None` delivers report-only.
    answers: Option<(Arc<dyn RemoteShell>, std::path::PathBuf)>,
}

impl ActivityHookSink {
    /// The sink of `session_id`'s hooks, authenticated with its `hook_token` as `os_user`.
    pub fn new(
        delivery: Arc<dyn HookReportDelivery>,
        session_id: &str,
        hook_token: &str,
        os_user: &str,
    ) -> Self {
        Self {
            delivery,
            session_id: session_id.to_string(),
            hook_token: hook_token.to_string(),
            os_user: os_user.to_string(),
            answers: None,
        }
    }

    /// This sink, answering the waiting hooks it delivers: once the orchestrator answers, Claude's
    /// decision is written to `<worktree>/<REMOTE_ANSWERS_DIR>/<nonce>` through `shell`, where the
    /// hook on the target waits for it (`#target-parity` 1/7).
    #[must_use]
    pub fn answering_through(mut self, shell: Arc<dyn RemoteShell>, worktree: &Path) -> Self {
        self.answers = Some((shell, worktree.to_path_buf()));
        self
    }

    /// Hold for the answer of a waiting hook and write it, in a task of its own: the follower's
    /// reader must not stall on an orchestrator that takes minutes. A hook that only reports, or a
    /// sink that does not answer, holds nothing.
    fn answer_in_background(&self, line: &HookLine, request_id: Option<String>) {
        let (Some(nonce), Some((shell, worktree))) = (&line.answer_nonce, &self.answers) else {
            return;
        };
        tokio::spawn(crate::remote_hook_answer::hold_and_answer(
            Arc::clone(&self.delivery),
            Arc::clone(shell),
            worktree.clone(),
            nonce.clone(),
            crate::remote_hook_answer::HeldHook {
                session_id: self.session_id.clone(),
                hook_token: self.hook_token.clone(),
                os_user: self.os_user.clone(),
                request_id,
                permission_suggestions: line.payload.get("permission_suggestions").cloned(),
            },
        ));
    }
}

#[async_trait]
impl HookSink for ActivityHookSink {
    async fn deliver(&self, line: HookLine) {
        let payload = line.payload.to_string();
        if let Some(request) = tddy_session_activity::hook_request::session_status_request(
            &self.session_id,
            &self.hook_token,
            &self.os_user,
            &line.event,
            &payload,
        ) {
            let request_id = match self.delivery.report_session_status(request).await {
                Ok(response) => Some(response.request_id),
                Err(error) => {
                    log::warn!(
                        "remote hook {} for session {} refused: {error}",
                        line.event,
                        self.session_id
                    );
                    None
                }
            };
            self.answer_in_background(&line, request_id);
        }
        if let Some(request) = tddy_session_activity::hook_request::agent_activity_request(
            &line.event,
            &payload,
            &self.session_id,
            &self.hook_token,
            &self.os_user,
        ) {
            if let Err(error) = self.delivery.report_agent_activity(request).await {
                log::warn!(
                    "remote agent activity {} for session {} refused: {error}",
                    line.event,
                    self.session_id
                );
            }
        }
    }
}

/// The followers of this daemon's target sessions, by session id, with the shell and worktree
/// each one's log lives on — so a session's exit, stop or delete can stop its follower and
/// remove its log without re-deriving where either is.
#[derive(Clone, Default)]
pub struct RemoteHookFollowers {
    entries: Arc<std::sync::Mutex<std::collections::HashMap<String, RemoteHookEntry>>>,
}

struct RemoteHookEntry {
    follower: Option<RemoteHookFollower>,
    shell: Arc<dyn RemoteShell>,
    worktree: std::path::PathBuf,
}

impl RemoteHookFollowers {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `session_id`'s follower, replacing any registered before it (a resume).
    pub fn insert(
        &self,
        session_id: &str,
        follower: RemoteHookFollower,
        shell: Arc<dyn RemoteShell>,
        worktree: std::path::PathBuf,
    ) {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(
                session_id.to_string(),
                RemoteHookEntry {
                    follower: Some(follower),
                    shell,
                    worktree,
                },
            );
    }

    /// Stop `session_id`'s follower and remove its hook log on its target. A session with no
    /// follower registered has nothing to stop.
    pub async fn stop_and_remove_log(&self, session_id: &str) {
        let entry = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(session_id);
        if let Some(mut entry) = entry {
            if let Some(follower) = entry.follower.take() {
                follower.stop().await;
            }
            if let Err(error) = remove_remote_hook_log(entry.shell.as_ref(), &entry.worktree).await
            {
                log::warn!("removing session {session_id}'s hook log on its target: {error}");
            }
        }
    }
}

/// The command a follower runs on the target to read `log_path` from the byte `offset` on, and
/// everything appended after it, ending once the log is removed:
///
/// ```text
/// tail -c +<offset + 1> -f '<log_path>' & t=$!; (while [ -e '<log_path>' ]; do sleep 1; done; kill "$t") >/dev/null 2>&1 & wait "$t"
/// ```
///
/// `tail -c +N` starts at the Nth byte, counted from one. Ending the follower's client does not end
/// a remote command — ssh without a terminal sends it no hang-up, and `tail -f` on a log nobody
/// writes to never learns its reader is gone — so a watch of its own ends the `tail` within a
/// second of the log's removal, which every session end performs. The watch runs in a background
/// subshell, not in the shell that waits on `tail`, so it outlives that shell being killed with its
/// client. The path is single-quoted for a POSIX shell.
#[must_use]
pub fn follow_command(log_path: &str, offset: u64) -> String {
    let log = shell_quote(log_path);
    format!(
        "tail -c +{} -f {log} & t=$!; (while [ -e {log} ]; do sleep 1; done; kill \"$t\") >/dev/null 2>&1 & wait \"$t\"",
        offset + 1
    )
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
         for entry in '/.tddy/hooks.log' '/.tddy/answers/'; do \
         grep -qxF \"$entry\" \"$exclude\" 2>/dev/null || echo \"$entry\" >> \"$exclude\"; done; fi",
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
    let answers = worktree
        .join(tddy_core::REMOTE_ANSWERS_DIR)
        .to_string_lossy()
        .into_owned();
    shell
        .exec(ExecRequest::new(format!(
            "rm -f {}; rm -rf {}",
            shell_quote(&log),
            shell_quote(&answers)
        )))
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
        let task = tokio::spawn(follow_log(
            shell,
            log_path.to_string(),
            hook_token.to_string(),
            sink,
            Arc::clone(&dropped),
        ));
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

/// Follow `log_path` until aborted: open its stream, deliver its lines, and when the stream
/// ends open another from the byte reached, after a back-off.
async fn follow_log(
    shell: Arc<dyn RemoteShell>,
    log_path: String,
    hook_token: String,
    sink: Arc<dyn HookSink>,
    dropped: Arc<AtomicU64>,
) {
    let mut offset: u64 = 0;
    let mut pending: Vec<u8> = Vec::new();
    loop {
        if let Ok(mut streamed) = shell
            .exec_streamed(&follow_command(&log_path, offset))
            .await
        {
            read_stream(
                &mut streamed,
                &mut offset,
                &mut pending,
                &hook_token,
                &sink,
                &dropped,
            )
            .await;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Read one opened stream to its end, delivering every whole line it carries.
async fn read_stream(
    streamed: &mut tddy_remote_shell::StreamedExec,
    offset: &mut u64,
    pending: &mut Vec<u8>,
    hook_token: &str,
    sink: &Arc<dyn HookSink>,
    dropped: &AtomicU64,
) {
    let mut chunk = [0u8; 4096];
    loop {
        match streamed.stdout.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                *offset += read as u64;
                pending.extend_from_slice(&chunk[..read]);
                deliver_whole_lines(pending, hook_token, sink, dropped).await;
            }
        }
    }
}

/// Deliver every newline-terminated line in `pending`, keeping a trailing partial line there.
async fn deliver_whole_lines(
    pending: &mut Vec<u8>,
    hook_token: &str,
    sink: &Arc<dyn HookSink>,
    dropped: &AtomicU64,
) {
    while let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
        let mut line: Vec<u8> = pending.drain(..=end).collect();
        line.pop();
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        let text = String::from_utf8_lossy(&line);
        match HookLine::parse(&text, hook_token) {
            Ok(hook) => sink.deliver(hook).await,
            Err(_) => {
                dropped.fetch_add(1, Ordering::SeqCst);
            }
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
                answer_nonce: None,
            })
        );
    }

    #[test]
    fn a_waiting_hooks_line_carries_its_nonce() {
        // Given the line a waiting permission hook appended
        let line = format!(
            "PermissionRequest@4711\t{TOKEN}\t{}",
            r#"{"hook_event_name":"PermissionRequest","tool_name":"Read"}"#
        );

        // When
        let hook = HookLine::parse(&line, TOKEN);

        // Then the event is the event, and the nonce is what the hook waits for
        assert_eq!(
            hook,
            Ok(HookLine {
                event: "PermissionRequest".to_string(),
                payload: serde_json::json!({"hook_event_name": "PermissionRequest", "tool_name": "Read"}),
                answer_nonce: Some("4711".to_string()),
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
            "tail -c +1 -f '/home/dev/repo/.worktrees/s1/.tddy/hooks.log' & t=$!; \
             (while [ -e '/home/dev/repo/.worktrees/s1/.tddy/hooks.log' ]; do sleep 1; done; \
             kill \"$t\") >/dev/null 2>&1 & wait \"$t\""
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
            "tail -c +4097 -f '/home/dev/repo/.worktrees/s1/.tddy/hooks.log' & t=$!; \
             (while [ -e '/home/dev/repo/.worktrees/s1/.tddy/hooks.log' ]; do sleep 1; done; \
             kill \"$t\") >/dev/null 2>&1 & wait \"$t\""
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
            r#"tail -c +1 -f '/home/dev/o'\''brien'\''s repo/.tddy/hooks.log' & t=$!; (while [ -e '/home/dev/o'\''brien'\''s repo/.tddy/hooks.log' ]; do sleep 1; done; kill "$t") >/dev/null 2>&1 & wait "$t""#
        );
    }

    /// The daemon's end, recording what each delivered hook reported.
    #[derive(Default)]
    struct ARecordingDaemon {
        statuses: std::sync::Mutex<Vec<String>>,
        activities: std::sync::Mutex<Vec<(String, String)>>,
    }

    #[async_trait]
    impl HookReportDelivery for ARecordingDaemon {
        async fn report_session_status(
            &self,
            request: tddy_service::proto::activity::ReportSessionStatusRequest,
        ) -> Result<tddy_service::proto::activity::ReportSessionStatusResponse, String> {
            self.statuses
                .lock()
                .unwrap()
                .push(format!("{} {}", request.hook_event_name, request.tool_name));
            Ok(Default::default())
        }

        async fn report_agent_activity(
            &self,
            request: tddy_service::proto::activity::ReportAgentActivityRequest,
        ) -> Result<(), String> {
            self.activities
                .lock()
                .unwrap()
                .push((request.event, request.tool_name));
            Ok(())
        }

        async fn await_question_answer(
            &self,
            _request: tddy_service::proto::activity::AwaitQuestionAnswerRequest,
        ) -> Result<tddy_service::proto::activity::AwaitQuestionAnswerResponse, String> {
            Ok(Default::default())
        }
    }

    /// The catch-all `PreToolUse` hook reports the tool call and never a status: the question tools
    /// are reported by the hook that waits for their answer (`#target-parity` 4/7).
    #[tokio::test]
    async fn a_reporting_pre_tool_use_line_reports_the_call_and_no_status() {
        // Given
        let daemon = Arc::new(ARecordingDaemon::default());
        let sink = ActivityHookSink::new(daemon.clone(), "s1", TOKEN, "dev");

        // When the catch-all hook reports a shell command and a question
        for payload in [
            serde_json::json!({"hook_event_name": "PreToolUse", "tool_name": "Bash", "tool_use_id": "toolu_01", "tool_input": {"command": "ls"}}),
            serde_json::json!({"hook_event_name": "PreToolUse", "tool_name": "AskUserQuestion", "tool_input": {"questions": []}}),
        ] {
            sink.deliver(HookLine {
                event: "PreToolUse".to_string(),
                payload,
                answer_nonce: None,
            })
            .await;
        }

        // Then the shell command is a running call, the question nothing, and no status was sent
        assert_eq!(
            (
                daemon.statuses.lock().unwrap().clone(),
                daemon.activities.lock().unwrap().clone()
            ),
            (
                Vec::<String>::new(),
                vec![("PreToolUse".to_string(), "Bash".to_string())]
            )
        );
    }
}
