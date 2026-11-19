//! Acceptance: a driven session's lifecycle hooks reach the driver the way a daemon start's reach the
//! daemon, and are read back over MCP.
//!
//! The agent is `acme-hooked`, whose spec declares the `hooks` capability and Claude Code's hooks
//! adapter. Each test fires a hook as Claude Code does — the command the launch installed in the
//! worktree's `.claude/settings.local.json`, under `sh -c`, payload on stdin — and reads what the
//! driver made of it with `read_hooks` / `wait_for_hook`. A Coder target is reached through the
//! stand-in `coder` the suite writes; no deployment is reached.
//!
//! Docs: packages/tddy-session-driver/docs/architecture.md.

mod support;

use std::time::Duration;

use pretty_assertions::assert_eq;
use serde_json::{json, Value};

use support::{ADriver, AMachine, IO_TIMEOUT};

/// An agent that announces itself, then reads its terminal until it is stopped.
const AN_AGENT_THAT_WAITS: &str = "#!/bin/sh\n\
     printf 'ACME READY in %s\\n' \"$(/bin/pwd -P)\"\n\
     while IFS= read -r line; do :; done\n";

/// What a hook's report says, without its session, user and token: the part a test compares.
fn report_summary(report: &Value) -> Value {
    match report["kind"].as_str() {
        Some("session_status") => json!({
            "kind": "session_status",
            "status": report["status"],
            "hook_event_name": report["hook_event_name"],
        }),
        Some("agent_activity") => json!({
            "kind": "agent_activity",
            "event": report["event"],
            "tool_name": report["tool_name"],
        }),
        _ => report.clone(),
    }
}

/// Each recorded hook as `(event, summarised reports)`.
fn hooks_and_reports(read: &Value) -> Vec<(String, Vec<Value>)> {
    read["hooks"]
        .as_array()
        .map(|hooks| {
            hooks
                .iter()
                .map(|hook| {
                    (
                        hook["event"].as_str().unwrap_or_default().to_string(),
                        hook["reports"]
                            .as_array()
                            .map(|reports| reports.iter().map(report_summary).collect())
                            .unwrap_or_default(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Start the session and wait for the agent's banner and the hook log the launch creates.
async fn started(driver: &mut ADriver, machine: &AMachine) {
    let started = driver.answer("start_session", json!({})).await;
    assert_eq!(started["state"], json!("running"), "{started}");
    let ready = driver
        .answer(
            "wait_for_text",
            json!({ "text": "ACME READY", "timeout_secs": 20 }),
        )
        .await;
    assert_eq!(ready["matched"], json!(true), "{ready}");
    assert!(
        machine.hook_log().exists(),
        "the launch created the session's hook log in the worktree"
    );
}

/// Fire the three hooks of one turn — the start, one tool call, the end — as Claude Code does.
fn a_turn_fires_its_hooks(machine: &AMachine) {
    machine.fire_hook(
        "SessionStart",
        &json!({ "hook_event_name": "SessionStart", "session_id": "conv-1", "source": "startup" }),
    );
    machine.fire_hook(
        "PostToolUse",
        &json!({
            "hook_event_name": "PostToolUse",
            "session_id": "conv-1",
            "tool_name": "Bash",
            "tool_input": { "command": "ls" },
            "tool_response": { "stdout": "README.md" },
        }),
    );
    machine.fire_hook(
        "Stop",
        &json!({ "hook_event_name": "Stop", "session_id": "conv-1" }),
    );
}

/// The three hooks of [`a_turn_fires_its_hooks`], as the daemon's activity handling would be served
/// them.
fn the_turn_s_reports() -> Vec<(String, Vec<Value>)> {
    vec![
        (
            "SessionStart".to_string(),
            vec![
                json!({ "kind": "session_status", "status": "Started", "hook_event_name": "SessionStart" }),
            ],
        ),
        (
            "PostToolUse".to_string(),
            vec![
                json!({ "kind": "session_status", "status": "ExecutingTool", "hook_event_name": "PostToolUse" }),
                json!({ "kind": "agent_activity", "event": "PostToolUse", "tool_name": "Bash" }),
            ],
        ),
        (
            "Stop".to_string(),
            vec![json!({ "kind": "session_status", "status": "Done", "hook_event_name": "Stop" })],
        ),
    ]
}

#[tokio::test]
async fn a_coder_session_s_hooks_reach_the_driver_as_the_daemon_is_served_them() {
    // Given a hooked agent started on a Coder workspace
    let machine = AMachine::with_agent(AN_AGENT_THAT_WAITS);
    let mut driver =
        ADriver::started(machine.driver_of("acme-hooked", "coder:ada/devbox", &[])).await;
    started(&mut driver, &machine).await;
    // The hooks the launch installed are the target's: an append to the log, no `tddy-tools`
    let installed = machine.installed_hook("Stop").unwrap_or_default();
    assert!(
        installed.contains(".tddy/hooks.log") && !installed.contains("tddy-tools"),
        "the installed hook appends to the session's log: {installed}"
    );

    // When a turn fires its hooks, and a line forged with another token is appended to the log
    a_turn_fires_its_hooks(&machine);
    std::fs::OpenOptions::new()
        .append(true)
        .open(machine.hook_log())
        .and_then(|mut log| {
            use std::io::Write as _;
            log.write_all(b"Stop\tnot-the-session-s-token\t{\"hook_event_name\":\"Stop\"}\n")
        })
        .expect("the forged line is appended");
    let waited = driver
        .answer(
            "wait_for_hook",
            json!({ "event": "Stop", "timeout_secs": 20 }),
        )
        .await;
    let read = driver.answer("read_hooks", json!({})).await;

    // Then every hook arrived, in order, with the reports the daemon's activity handling is served,
    // and the forged line was dropped
    assert_eq!(waited["matched"], json!(true), "{waited}");
    assert_eq!(read["followed"], json!(true), "{read}");
    assert_eq!(hooks_and_reports(&read), the_turn_s_reports());
    assert_eq!(read["next"], json!(3), "{read}");
    // And a hook keeps its payload as the agent wrote it
    assert_eq!(
        read["hooks"][1]["payload"]["tool_input"],
        json!({ "command": "ls" })
    );
    // And the log was read through the workspace's CLI, as the daemon reads it
    let follows: Vec<String> = machine
        .ssh_calls()
        .into_iter()
        .filter(|call| call.contains("tail -c +"))
        .collect();
    assert!(
        !follows.is_empty() && follows.iter().all(|call| call.starts_with("yes ")),
        "the follower read the log through `coder ssh --stdio`: {follows:?}"
    );

    // When the session is stopped, its hook log goes with it, and nothing on the workspace is left
    // following it — though killing the CLI leaves a workspace's command running
    let _ = driver.answer("stop_session", json!({})).await;
    let left = machine
        .hook_log_followers_end_within(Duration::from_secs(10))
        .await;
    assert!(
        !machine.hook_log().exists(),
        "stopping the session removed its hook log"
    );
    assert_eq!(
        left,
        Vec::<String>::new(),
        "the log's followers outlived the session"
    );
}

#[tokio::test]
async fn a_local_session_s_hooks_reach_the_driver_the_same_way() {
    // Given a hooked agent started on this machine
    let machine = AMachine::with_agent(AN_AGENT_THAT_WAITS);
    let mut driver = ADriver::started(machine.driver_of("acme-hooked", "local", &[])).await;
    started(&mut driver, &machine).await;

    // When a turn fires its hooks
    a_turn_fires_its_hooks(&machine);
    let waited = driver
        .answer(
            "wait_for_hook",
            json!({ "event": "Stop", "timeout_secs": 20 }),
        )
        .await;
    let read = driver.answer("read_hooks", json!({})).await;

    // Then they arrived exactly as a target's do
    assert_eq!(waited["matched"], json!(true), "{waited}");
    assert_eq!(hooks_and_reports(&read), the_turn_s_reports());

    // And stopping the session removes its hook log and ends what followed it
    let _ = driver.answer("stop_session", json!({})).await;
    let left = machine
        .hook_log_followers_end_within(Duration::from_secs(10))
        .await;
    assert!(!machine.hook_log().exists(), "the hook log is gone");
    assert_eq!(
        left,
        Vec::<String>::new(),
        "the log's followers outlived the session"
    );
}

#[tokio::test]
async fn read_hooks_since_answers_only_the_hooks_after_it() {
    // Given a session whose turn fired three hooks
    let machine = AMachine::with_agent(AN_AGENT_THAT_WAITS);
    let mut driver = ADriver::started(machine.driver_of("acme-hooked", "local", &[])).await;
    started(&mut driver, &machine).await;
    a_turn_fires_its_hooks(&machine);
    let _ = driver
        .answer(
            "wait_for_hook",
            json!({ "event": "Stop", "timeout_secs": 20 }),
        )
        .await;

    // When the hooks from the second on are read
    let read = driver.answer("read_hooks", json!({ "since": 1 })).await;

    // Then only those are answered, with their own indexes
    let events: Vec<(u64, String)> = read["hooks"]
        .as_array()
        .expect("the hooks")
        .iter()
        .map(|hook| {
            (
                hook["index"].as_u64().unwrap_or(u64::MAX),
                hook["event"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    let _ = driver.answer("stop_session", json!({})).await;
    assert_eq!(
        events,
        vec![(1, "PostToolUse".to_string()), (2, "Stop".to_string())]
    );
}

#[tokio::test]
async fn waiting_for_a_hook_that_never_fires_ends_unmatched() {
    // Given a hooked session no hook has fired in
    let machine = AMachine::with_agent(AN_AGENT_THAT_WAITS);
    let mut driver = ADriver::started(machine.driver_of("acme-hooked", "local", &[])).await;
    started(&mut driver, &machine).await;

    // When a Stop hook is waited for, briefly
    let began = std::time::Instant::now();
    let waited = driver
        .answer(
            "wait_for_hook",
            json!({ "event": "Stop", "timeout_secs": 1 }),
        )
        .await;

    // Then the wait ends unmatched once its time is up
    let _ = driver.answer("stop_session", json!({})).await;
    assert_eq!(waited["matched"], json!(false), "{waited}");
    assert!(
        began.elapsed() < IO_TIMEOUT && began.elapsed() >= Duration::from_secs(1),
        "the wait took {:?}",
        began.elapsed()
    );
}

#[tokio::test]
async fn an_agent_without_hooks_has_none_followed() {
    // Given an agent whose spec declares no `hooks` capability, started on a Coder workspace
    let machine = AMachine::with_agent(AN_AGENT_THAT_WAITS);
    let mut driver = ADriver::started(machine.driver_of("acme", "coder:ada/devbox", &[])).await;
    let started = driver.answer("start_session", json!({})).await;
    assert_eq!(started["state"], json!("running"), "{started}");

    // When its hooks are read
    let read = driver.answer("read_hooks", json!({})).await;

    // Then none are followed, no hooks file was installed and no log created
    let _ = driver.answer("stop_session", json!({})).await;
    assert_eq!(
        (
            read["followed"].clone(),
            read["hooks"].clone(),
            machine.installed_hook("Stop"),
            machine.hook_log().exists(),
        ),
        (json!(false), json!([]), None, false)
    );
}
