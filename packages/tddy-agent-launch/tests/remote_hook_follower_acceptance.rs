//! Acceptance: the daemon follows a target session's hook log through the remote-shell port.
//!
//! The port is `InMemoryShell`: the follower's remote command is a scripted stream the test
//! prints the log's bytes on and ends. The clock is paused, so a back-off before a reconnect costs
//! no wall-clock time and a follower that never delivers fails at once.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-15-remote-shell-remote-hooks.md.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use pretty_assertions::assert_eq;
use tddy_agent_launch::remote_hook_follower::{
    follow_command, ActivityHookSink, FollowOffset, HookDeliveryError, HookLine,
    HookReportDelivery, HookSink, RemoteHookFollower,
};
use tddy_remote_shell::{InMemoryShell, ScriptedStream, ShellCall};
use tddy_testing_commons::wait::eventually;

/// The session's hook log on the target.
const LOG: &str = "/home/dev/repo/.worktrees/019d2c7e/.tddy/hooks.log";
/// The session's hook token.
const TOKEN: &str = "tok-7f3a9c";
/// A safety net on the paused clock, not a prediction: far past any back-off.
const OBSERVE: Duration = Duration::from_secs(600);

/// The command that reads the log from the byte `offset` on, as the target is asked to run it.
fn the_follow_command_from(offset: usize) -> String {
    follow_command(LOG, offset as u64)
}

/// One hook as an agent on the target logged it.
#[derive(Debug, Clone)]
struct ALoggedHook {
    event: &'static str,
    token: &'static str,
    payload: String,
}

/// The hook `event` logged with the session's token, its payload naming the event and `detail`.
fn a_hook(event: &'static str, detail: &str) -> ALoggedHook {
    ALoggedHook {
        event,
        token: TOKEN,
        payload: format!(r#"{{"hook_event_name":"{event}","detail":"{detail}"}}"#),
    }
}

impl ALoggedHook {
    fn with_token(self, token: &'static str) -> Self {
        Self { token, ..self }
    }

    fn with_payload(self, payload: &str) -> Self {
        Self {
            payload: payload.to_string(),
            ..self
        }
    }

    /// The line the hook command appended to the log, newline included.
    fn line(&self) -> String {
        format!("{}\t{}\t{}\n", self.event, self.token, self.payload)
    }

    /// The hook as the daemon is told of it.
    fn delivered(&self) -> HookLine {
        HookLine {
            event: self.event.to_string(),
            payload: serde_json::from_str(&self.payload).expect("the hook's payload is JSON"),
            answer_nonce: None,
        }
    }
}

/// The lines of `hooks` as the log holds them, in order.
fn logged(hooks: &[&ALoggedHook]) -> String {
    hooks.iter().map(|hook| hook.line()).collect()
}

/// The daemon's end: every hook it was told of, in order.
#[derive(Default)]
struct TheDaemonsHooks {
    delivered: Mutex<Vec<HookLine>>,
}

#[async_trait]
impl HookSink for TheDaemonsHooks {
    async fn deliver(&self, line: HookLine) {
        self.delivered.lock().unwrap().push(line);
    }
}

impl TheDaemonsHooks {
    /// The hooks delivered once there are `count` of them.
    async fn once_it_holds(&self, count: usize) -> Vec<HookLine> {
        eventually(&format!("{count} hooks to be delivered"), OBSERVE, || {
            let delivered = self.delivered.lock().unwrap().clone();
            (delivered.len() >= count)
                .then_some(delivered.clone())
                .ok_or(format!("delivered so far: {delivered:?}"))
        })
        .await
    }
}

/// A target whose hook log a follower reads.
struct AFollowedTarget {
    shell: InMemoryShell,
    daemon: Arc<TheDaemonsHooks>,
}

fn a_target() -> AFollowedTarget {
    AFollowedTarget {
        shell: InMemoryShell::new("buildbox"),
        daemon: Arc::default(),
    }
}

impl AFollowedTarget {
    /// The target's `tail` of the log from the byte `offset` on, for the test to print on.
    fn tails_the_log_from(&self, offset: usize) -> ScriptedStream {
        self.shell.streams(the_follow_command_from(offset))
    }

    /// The daemon starts following the session's log.
    fn is_followed(&self) -> RemoteHookFollower {
        RemoteHookFollower::start(
            Arc::new(self.shell.clone()),
            LOG,
            TOKEN,
            self.daemon.clone(),
        )
    }

    /// The daemon starts following the session's log, remembering how far it read in `offset`.
    fn is_followed_remembering(&self, offset: FollowOffset) -> RemoteHookFollower {
        RemoteHookFollower::start_resuming(
            Arc::new(self.shell.clone()),
            LOG,
            TOKEN,
            self.daemon.clone(),
            offset,
        )
    }

    /// Every streamed command the target was asked to run, oldest first.
    fn streamed_commands(&self) -> Vec<String> {
        self.shell
            .calls()
            .into_iter()
            .filter_map(|call| match call {
                ShellCall::ExecStreamed(command) => Some(command),
                _ => None,
            })
            .collect()
    }

    /// Waits until the follower has opened `count` streamed commands.
    async fn has_been_asked_for(&self, count: usize) -> Vec<String> {
        eventually(
            &format!("the follower to open {count} remote commands"),
            OBSERVE,
            || {
                let commands = self.streamed_commands();
                (commands.len() >= count)
                    .then_some(commands.clone())
                    .ok_or(format!("streamed commands so far: {commands:?}"))
            },
        )
        .await
    }
}

#[tokio::test(start_paused = true)]
async fn each_logged_hook_is_delivered_once_in_order() {
    // Given a followed log
    let target = a_target();
    let tail = target.tails_the_log_from(0);
    let _follower = target.is_followed();
    let started = a_hook("SessionStart", "startup");
    let prompted = a_hook("UserPromptSubmit", "fix the login page");
    let stopped = a_hook("Stop", "end_turn");

    // When three hooks are logged, the second arriving in two pieces
    let log = logged(&[&started, &prompted, &stopped]);
    let (first_piece, second_piece) = log.split_at(started.line().len() + 9);
    tail.prints(first_piece).await;
    tail.prints(second_piece).await;
    let delivered = target.daemon.once_it_holds(3).await;

    // Then
    assert_eq!(
        delivered,
        vec![
            started.delivered(),
            prompted.delivered(),
            stopped.delivered()
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn a_line_with_the_wrong_token_is_dropped() {
    // Given a followed log
    let target = a_target();
    let tail = target.tails_the_log_from(0);
    let follower = target.is_followed();
    let started = a_hook("SessionStart", "startup");
    let forged = a_hook("SessionEnd", "logout").with_token("tok-of-another-session");
    let stopped = a_hook("Stop", "end_turn");

    // When a line with another token is logged between two of the session's
    tail.prints(logged(&[&started, &forged, &stopped])).await;
    let delivered = target.daemon.once_it_holds(2).await;

    // Then only the session's own are delivered, and the other is counted
    assert_eq!(
        (delivered, follower.dropped_lines()),
        (vec![started.delivered(), stopped.delivered()], 1)
    );
}

#[tokio::test(start_paused = true)]
async fn an_unknown_event_or_a_broken_payload_is_dropped_and_the_next_line_still_arrives() {
    // Given a followed log
    let target = a_target();
    let tail = target.tails_the_log_from(0);
    let follower = target.is_followed();
    let unknown = a_hook("Reboot", "now");
    let torn = a_hook("PostToolUse", "Bash").with_payload(r#"{"hook_event_name":"PostToolU"#);
    let stopped = a_hook("Stop", "end_turn");

    // When an event no adapter installs and a torn payload are logged before a good line
    tail.prints(logged(&[&unknown, &torn, &stopped])).await;
    let delivered = target.daemon.once_it_holds(1).await;

    // Then the good line alone is delivered, and the other two are counted
    assert_eq!(
        (delivered, follower.dropped_lines()),
        (vec![stopped.delivered()], 2)
    );
}

#[tokio::test(start_paused = true)]
async fn a_stream_that_ends_resumes_from_the_byte_it_reached() {
    // Given a followed log, and a target that will also answer a `tail` from past the first two
    // hooks' bytes
    let target = a_target();
    let started = a_hook("SessionStart", "startup");
    let prompted = a_hook("UserPromptSubmit", "fix the login page");
    let stopped = a_hook("Stop", "end_turn");
    let bytes_read = logged(&[&started, &prompted]).len();
    let tail = target.tails_the_log_from(0);
    let resumed_tail = target.tails_the_log_from(bytes_read);
    let _follower = target.is_followed();

    // When two hooks are read, the connection drops, and a third is logged afterwards
    tail.prints(logged(&[&started, &prompted])).await;
    target.daemon.once_it_holds(2).await;
    tail.ends_with(255).await;
    target.has_been_asked_for(2).await;
    resumed_tail.prints(stopped.line()).await;
    let delivered = target.daemon.once_it_holds(3).await;

    // Then every hook is delivered once, in order, the second stream opened where the first ended
    assert_eq!(
        (delivered, target.streamed_commands()),
        (
            vec![
                started.delivered(),
                prompted.delivered(),
                stopped.delivered()
            ],
            vec![
                the_follow_command_from(0),
                the_follow_command_from(bytes_read)
            ]
        )
    );
}

#[tokio::test(start_paused = true)]
async fn stopping_the_follower_ends_its_remote_command() {
    // Given a follower whose remote command is running
    let target = a_target();
    let tail = target.tails_the_log_from(0);
    let follower = target.is_followed();
    target.has_been_asked_for(1).await;

    // When
    follower.stop().await;

    // Then the command's pipes are closed from the daemon's side: its stdin reaches its end
    let ended = tokio::time::timeout(OBSERVE, tail.stdin_to_end()).await;
    assert_eq!(
        (ended, target.streamed_commands()),
        (Ok(Vec::new()), vec![the_follow_command_from(0)])
    );
}

#[tokio::test(start_paused = true)]
async fn a_stream_that_ends_mid_line_resumes_from_the_last_whole_line() {
    // Given a followed log
    let target = a_target();
    let started = a_hook("SessionStart", "startup");
    let prompted = a_hook("UserPromptSubmit", "fix the login page");
    let whole_lines = started.line().len();
    let tail = target.tails_the_log_from(0);
    let resumed_tail = target.tails_the_log_from(whole_lines);
    let _follower = target.is_followed();

    // When one whole hook and the start of the next are read before the connection drops
    let log = logged(&[&started, &prompted]);
    tail.prints(&log[..whole_lines + 7]).await;
    target.daemon.once_it_holds(1).await;
    tail.ends_with(255).await;
    target.has_been_asked_for(2).await;
    resumed_tail.prints(&log[whole_lines..]).await;
    let delivered = target.daemon.once_it_holds(2).await;

    // Then the torn line is read again whole, from the first byte nothing was delivered from
    assert_eq!(
        (delivered, target.streamed_commands()),
        (
            vec![started.delivered(), prompted.delivered()],
            vec![
                the_follow_command_from(0),
                the_follow_command_from(whole_lines)
            ]
        )
    );
}

#[tokio::test(start_paused = true)]
async fn a_new_follower_of_the_same_log_resumes_where_the_last_one_stopped() {
    // Given a follower that remembers how far it read, which has delivered two hooks and stopped
    let target = a_target();
    let remembered = tempfile::tempdir().expect("a session directory");
    let offset = FollowOffset::persisted_at(remembered.path().join("hooks.offset"));
    let started = a_hook("SessionStart", "startup");
    let prompted = a_hook("UserPromptSubmit", "fix the login page");
    let stopped = a_hook("Stop", "end_turn");
    let delivered_before = logged(&[&started, &prompted]).len();
    let tail = target.tails_the_log_from(0);
    let first = target.is_followed_remembering(offset.clone());
    tail.prints(logged(&[&started, &prompted])).await;
    target.daemon.once_it_holds(2).await;
    first.stop().await;

    // When another follower of the log starts — a resume, a re-wire, a restarted daemon
    let resumed_tail = target.tails_the_log_from(delivered_before);
    let _second = target.is_followed_remembering(offset);
    target.has_been_asked_for(2).await;
    resumed_tail.prints(stopped.line()).await;
    let delivered = target.daemon.once_it_holds(3).await;

    // Then it reads on from the last byte delivered — never from the log's end, never again from 0
    assert_eq!(
        (delivered, target.streamed_commands()),
        (
            vec![
                started.delivered(),
                prompted.delivered(),
                stopped.delivered()
            ],
            vec![
                the_follow_command_from(0),
                the_follow_command_from(delivered_before)
            ]
        )
    );
}

#[tokio::test]
async fn creating_the_hook_log_of_a_resumed_session_keeps_the_hooks_it_already_holds() {
    // Given a worktree whose hook log holds a hook its follower has not read yet
    let machine = tempfile::tempdir().expect("a directory standing for the target");
    let worktree = machine.path().join("worktree");
    std::fs::create_dir_all(&worktree).expect("the worktree");
    let shell = tddy_remote_shell::LocalShell::new();
    let log = tddy_agent_launch::remote_hook_follower::create_remote_hook_log(&shell, &worktree)
        .await
        .expect("the hook log is created");
    let unread = a_hook("UserPromptSubmit", "fix the login page").line();
    std::fs::write(&log, &unread).expect("the agent's hook appends");

    // When the session resumes and its start creates the log again
    let again = tddy_agent_launch::remote_hook_follower::create_remote_hook_log(&shell, &worktree)
        .await
        .expect("the hook log is created again");

    // Then the log is the same file, and the hook is still there to be read
    assert_eq!(
        (again, std::fs::read_to_string(&log).expect("the log")),
        (log, unread)
    );
}

#[tokio::test(start_paused = true)]
async fn behind_a_pty_that_turns_newlines_into_crlf_the_offset_still_counts_the_files_bytes() {
    // Given a target whose shell runs the follower in a pty that kept its output processing
    // (`onlcr`): every `\n` of the log arrives as `\r\n`
    let target = a_target();
    let started = a_hook("SessionStart", "startup");
    let prompted = a_hook("UserPromptSubmit", "fix the login page");
    let stopped = a_hook("Stop", "end_turn");
    let file_bytes = logged(&[&started, &prompted]).len();
    let tail = target.tails_the_log_from(0);
    let resumed_tail = target.tails_the_log_from(file_bytes);
    let _follower = target.is_followed();

    // When two hooks arrive through the pty, the connection drops, and a third is logged
    tail.prints(logged(&[&started, &prompted]).replace('\n', "\r\n"))
        .await;
    target.daemon.once_it_holds(2).await;
    tail.ends_with(255).await;
    target.has_been_asked_for(2).await;
    resumed_tail
        .prints(stopped.line().replace('\n', "\r\n"))
        .await;
    let delivered = target.daemon.once_it_holds(3).await;

    // Then the reopened tail starts at the log's own byte, not at the stream's
    assert_eq!(
        (delivered, target.streamed_commands()),
        (
            vec![
                started.delivered(),
                prompted.delivered(),
                stopped.delivered()
            ],
            vec![
                the_follow_command_from(0),
                the_follow_command_from(file_bytes)
            ]
        )
    );
}

#[tokio::test(start_paused = true)]
async fn a_line_longer_than_the_cap_is_skipped_and_the_next_hook_still_arrives() {
    // Given a followed log
    let target = a_target();
    let tail = target.tails_the_log_from(0);
    let follower = target.is_followed();
    let stopped = a_hook("Stop", "end_turn");

    // When something writes a line past the cap before the next hook
    let flood = vec![b'x'; tddy_agent_launch::log_follower::MAX_LINE_BYTES + 1];
    tail.prints(&flood).await;
    tail.prints(format!("\n{}", stopped.line())).await;
    let delivered = target.daemon.once_it_holds(1).await;

    // Then only the hook is delivered, and the flood was never handed on as a line
    assert_eq!(
        (delivered, follower.dropped_lines()),
        (vec![stopped.delivered()], 0)
    );
}

#[tokio::test(start_paused = true)]
async fn a_hook_log_that_could_not_be_removed_is_read_on_at_the_next_start_not_replayed() {
    // Given a session whose follower delivered a hook, on a target that refuses the log's removal
    let target = a_target();
    target.shell.answers_when(|request| {
        request
            .command
            .starts_with("rm -f")
            .then(|| tddy_remote_shell::ExecOutput::exited(1, "rm: Permission denied"))
    });
    let remembered = tempfile::tempdir().expect("a session directory");
    let offset = FollowOffset::persisted_at(remembered.path().join("hooks.offset"));
    let started = a_hook("SessionStart", "startup");
    let tail = target.tails_the_log_from(0);
    let followers = tddy_agent_launch::remote_hook_follower::RemoteHookFollowers::new();
    followers.insert(
        "s1",
        target.is_followed_remembering(offset.clone()),
        Arc::new(target.shell.clone()),
        "/home/dev/repo/.worktrees/019d2c7e".into(),
    );
    tail.prints(started.line()).await;
    target.daemon.once_it_holds(1).await;

    // When the session ends and its log stays behind
    followers.stop_and_remove_log("s1").await;

    // Then the offset still counts what was delivered: a start that keeps the log reads on
    assert_eq!(offset.load(LOG), started.line().len() as u64);
}

/// A session host whose session is registered after its follower starts: every report is refused
/// with "session not found" until `register()` runs, as `ActivityServiceImpl::hook_session` does
/// when the start has not written `.session.yaml` yet (n1).
struct RegisteringLate {
    registered: std::sync::atomic::AtomicBool,
    statuses: Mutex<Vec<String>>,
    activities: Mutex<Vec<(String, String)>>,
}

impl RegisteringLate {
    fn new() -> Self {
        Self {
            registered: std::sync::atomic::AtomicBool::new(false),
            statuses: Mutex::new(Vec::new()),
            activities: Mutex::new(Vec::new()),
        }
    }

    fn register(&self) {
        self.registered
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    fn is_registered(&self) -> bool {
        self.registered.load(std::sync::atomic::Ordering::SeqCst)
    }

    async fn once_it_holds(
        &self,
        statuses: usize,
        activities: usize,
    ) -> (Vec<String>, Vec<(String, String)>) {
        eventually(
            &format!("{statuses} statuses and {activities} activities to land"),
            OBSERVE,
            || {
                let held_statuses = self.statuses.lock().unwrap().clone();
                let held_activities = self.activities.lock().unwrap().clone();
                let (have_statuses, have_activities) = (held_statuses.len(), held_activities.len());
                (have_statuses >= statuses && have_activities >= activities)
                    .then_some((held_statuses, held_activities))
                    .ok_or(format!(
                        "landed so far: {have_statuses} statuses, {have_activities} activities"
                    ))
            },
        )
        .await
    }
}

#[async_trait]
impl HookReportDelivery for RegisteringLate {
    async fn report_session_status(
        &self,
        request: tddy_service::proto::activity::ReportSessionStatusRequest,
    ) -> Result<tddy_service::proto::activity::ReportSessionStatusResponse, HookDeliveryError> {
        if !self.is_registered() {
            return Err(HookDeliveryError::session_not_registered_yet());
        }
        self.statuses
            .lock()
            .unwrap()
            .push(format!("{} {}", request.hook_event_name, request.status));
        Ok(Default::default())
    }

    async fn await_question_answer(
        &self,
        _: tddy_service::proto::activity::AwaitQuestionAnswerRequest,
    ) -> Result<tddy_service::proto::activity::AwaitQuestionAnswerResponse, HookDeliveryError> {
        Ok(Default::default())
    }

    async fn report_agent_activity(
        &self,
        request: tddy_service::proto::activity::ReportAgentActivityRequest,
    ) -> Result<(), HookDeliveryError> {
        if !self.is_registered() {
            return Err(HookDeliveryError::session_not_registered_yet());
        }
        self.activities
            .lock()
            .unwrap()
            .push((request.event, request.tool_name));
        Ok(())
    }
}

#[tokio::test(start_paused = true)]
async fn hooks_logged_before_the_session_is_registered_land_once_it_is_with_no_duplicates() {
    // Given a target session whose follower starts before its `.session.yaml` is written — the
    // start writes it after the launch, and the follower starts inside the launch (n1)
    let shell = InMemoryShell::new("buildbox");
    let daemon = Arc::new(RegisteringLate::new());
    let tail = shell.streams(the_follow_command_from(0));
    let sink = ActivityHookSink::new(daemon.clone(), "s1", TOKEN, "dev");
    let _follower = RemoteHookFollower::start(Arc::new(shell.clone()), LOG, TOKEN, Arc::new(sink));
    // The probe's first turn: a prompt, one tool call, the turn's end
    let prompt = a_hook("beforeSubmitPrompt", "do it").with_payload(
        r#"{"hook_event_name":"beforeSubmitPrompt","prompt":"do it","conversation_id":"c1"}"#,
    );
    let pre = a_hook("preToolUse", "Read").with_payload(
        r#"{"hook_event_name":"preToolUse","tool_name":"Read","tool_input":{"path":"a"},"tool_use_id":"t1"}"#,
    );
    let post = a_hook("postToolUse", "Read").with_payload(
        r#"{"hook_event_name":"postToolUse","tool_name":"Read","tool_input":{"path":"a"},"tool_output":"{\"ok\":true}","tool_use_id":"t1"}"#,
    );
    let stop = a_hook("stop", "done")
        .with_payload(r#"{"hook_event_name":"stop","status":"completed","conversation_id":"c1"}"#);

    // When every hook is logged before the session registers, then it registers
    tail.prints(logged(&[&prompt, &pre, &post, &stop])).await;
    // Let the follower attempt delivery while unregistered (and, without the fix, consume them).
    tokio::time::sleep(Duration::from_millis(500)).await;
    daemon.register();
    let (statuses, activities) = daemon.once_it_holds(3, 2).await;

    // Then every hook landed once, in order: the prompt and the stop as statuses, the tool call's
    // pre and post as activity (the post also reports ExecutingTool)
    tokio::time::sleep(Duration::from_millis(300)).await;
    let (statuses_after, activities_after) = (
        daemon.statuses.lock().unwrap().clone(),
        daemon.activities.lock().unwrap().clone(),
    );
    assert_eq!(
        (
            (statuses, activities),
            (statuses_after.len(), activities_after.len())
        ),
        (
            (
                vec![
                    "beforeSubmitPrompt Running".to_string(),
                    "postToolUse ExecutingTool".to_string(),
                    "stop Done".to_string(),
                ],
                vec![
                    ("PreToolUse".to_string(), "Read".to_string()),
                    ("PostToolUse".to_string(), "Read".to_string()),
                ],
            ),
            (3, 2),
        ),
    );
}
