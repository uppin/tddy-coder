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
use tddy_agent_launch::remote_hook_follower::{HookLine, HookSink, RemoteHookFollower};
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
    format!("tail -c +{} -f '{LOG}'", offset + 1)
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
