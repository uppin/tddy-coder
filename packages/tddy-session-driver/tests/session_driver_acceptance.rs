//! Acceptance: `tddy-session-driver` runs one agent session on the target its arguments name and is
//! driven over MCP on its stdin/stdout.
//!
//! Each test spawns the compiled binary and speaks the real newline-delimited JSON-RPC wire. The
//! agent is `acme`, a tool file in the driver's tddy home whose binary is a stub script; a Coder
//! target is reached through a stand-in `coder` that runs the workspace's command on this machine.
//! No deployment is reached.
//!
//! Docs: packages/tddy-session-driver/docs/architecture.md.

mod support;

use std::path::Path;
use std::time::Duration;
use tddy_testing_commons::remote_target::write_executable;

use pretty_assertions::assert_eq;
use serde_json::json;
use tokio::io::AsyncReadExt;

use support::{ADriver, AMachine, DEPLOYMENT_URL, IO_TIMEOUT, SESSION_TOKEN};

/// An agent that announces itself, then answers every line it reads.
const AN_AGENT_THAT_ANSWERS: &str = "#!/bin/sh\n\
     printf 'ACME READY in %s\\n' \"$(/bin/pwd -P)\"\n\
     while IFS= read -r line; do printf 'acme heard: %s\\n' \"$line\"; done\n";

/// An agent that ignores the hang-up of a terminal that went away, as an agent on a remote
/// workspace never sees its client stop, and records it when it is told to terminate.
const AN_AGENT_THAT_OUTLIVES_ITS_CLIENT: &str = "#!/bin/sh\n\
     trap '' HUP\n\
     trap 'printf terminated > \"$(dirname \"$0\")/agent-ended\"; exit 0' TERM\n\
     printf 'ACME READY in %s\\n' \"$(/bin/pwd -P)\"\n\
     while :; do /bin/sleep 1; done\n";

/// An agent that ignores a hang-up, as [`AN_AGENT_THAT_OUTLIVES_ITS_CLIENT`] does, and records being
/// told to terminate in a file named by its own pid — the pid its launch records — so agents sharing
/// one worktree each leave their own record.
const AN_AGENT_THAT_RECORDS_ITS_END_BY_PID: &str = "#!/bin/sh\n\
     trap '' HUP\n\
     trap 'printf terminated > \"$(dirname \"$0\")/agent-ended-$$\"; exit 0' TERM\n\
     printf 'ACME READY in %s\\n' \"$(/bin/pwd -P)\"\n\
     while :; do /bin/sleep 1; done\n";

/// An agent that answers every line it reads until it reads `bye`, then exits 3.
const AN_AGENT_THAT_EXITS_3_ON_BYE: &str = "#!/bin/sh\n\
     printf 'ACME READY in %s\\n' \"$(/bin/pwd -P)\"\n\
     while IFS= read -r line; do\n\
       [ \"$line\" = bye ] && exit 3\n\
       printf 'acme heard: %s\\n' \"$line\"\n\
     done\n";

/// An agent that refuses to run and says why.
const AN_AGENT_THAT_EXITS: &str = "#!/bin/sh\nprintf 'acme: no licence for this host\\n'\nexit 3\n";

/// Starts the session, waits for the agent's banner, has it answer a line, then stops it, checking
/// each step's answer.
async fn drive_a_conversation(driver: &mut ADriver, worktree: &Path) {
    let started = driver.answer("start_session", json!({})).await;
    assert_eq!(started["state"], json!("running"), "{started}");

    let banner = format!("ACME READY in {}", worktree.display());
    let ready = driver
        .answer(
            "wait_for_text",
            json!({ "text": banner, "timeout_secs": 20 }),
        )
        .await;
    assert_eq!(ready["matched"], json!(true), "{ready}");

    let sent = driver
        .answer(
            "send_keys",
            json!({ "text": "hello there", "keys": ["Enter"] }),
        )
        .await;
    assert_eq!(sent["sent_bytes"], json!("hello there".len() + 1));
    let heard = driver
        .answer(
            "wait_for_text",
            json!({ "text": "acme heard: hello there", "timeout_secs": 20 }),
        )
        .await;
    assert_eq!(heard["matched"], json!(true), "{heard}");

    let screen = driver.answer("read_terminal", json!({})).await;
    let lines: Vec<String> = screen["lines"]
        .as_array()
        .expect("the screen's lines")
        .iter()
        .filter_map(|line| line.as_str().map(str::to_string))
        .collect();
    assert!(
        lines.iter().any(|line| line == "acme heard: hello there"),
        "the screen shows the agent's answer: {lines:?}"
    );
    assert!(screen["cols"].as_u64().unwrap_or(0) > 0, "{screen}");

    let stopped = driver.answer("stop_session", json!({})).await;
    assert_eq!(stopped["state"], json!("exited"), "{stopped}");
    let status = driver.answer("session_status", json!({})).await;
    assert_eq!(status["state"], json!("exited"), "{status}");
}

#[tokio::test]
async fn the_driver_offers_the_session_tools() {
    // Given a driver on a local target
    let machine = AMachine::with_agent(AN_AGENT_THAT_ANSWERS);
    let mut driver = ADriver::started(machine.driver("local", &[])).await;

    // When its tools are listed
    let names = driver.tool_names().await;

    // Then they are the eight session tools
    assert_eq!(
        names,
        [
            "read_hooks",
            "read_terminal",
            "send_keys",
            "session_status",
            "start_session",
            "stop_session",
            "wait_for_hook",
            "wait_for_text",
        ]
        .map(str::to_string)
        .to_vec()
    );
    // And no session runs until one is started
    let status = driver.answer("session_status", json!({})).await;
    assert_eq!(status["state"], json!("not_started"), "{status}");
}

#[tokio::test]
async fn a_local_session_is_started_read_typed_into_and_stopped_over_mcp() {
    // Given a driver on a local target whose agent answers what it reads
    let machine = AMachine::with_agent(AN_AGENT_THAT_ANSWERS);
    let mut driver = ADriver::started(machine.driver("local", &[])).await;

    // When / Then a conversation is driven through the session's terminal
    drive_a_conversation(&mut driver, &machine.worktree()).await;
}

#[tokio::test]
async fn a_coder_session_is_driven_through_the_workspace_as_the_cli_s_own_account() {
    // Given a driver on a Coder target, given the deployment and token in the CLI's own variables
    let machine = AMachine::with_agent(AN_AGENT_THAT_ANSWERS);
    let mut driver = ADriver::started(machine.driver("coder:ada/devbox.main", &[])).await;

    // When / Then a conversation is driven through the session's terminal on the workspace
    drive_a_conversation(&mut driver, &machine.worktree()).await;
    // And every call of the CLI was given that deployment and token
    let calls = machine.coder_calls();
    assert!(!calls.is_empty(), "the stand-in coder was used");
    assert_eq!(
        calls
            .iter()
            .filter(|call| *call
                != &format!("CODER_URL={DEPLOYMENT_URL} CODER_SESSION_TOKEN={SESSION_TOKEN}"))
            .collect::<Vec<_>>(),
        Vec::<&String>::new()
    );
}

#[tokio::test]
async fn a_coder_target_without_a_session_token_is_refused_before_anything_runs() {
    // Given a driver on a Coder target with no session token in its environment
    let machine = AMachine::with_agent(AN_AGENT_THAT_ANSWERS);
    let mut command = machine.driver("coder:ada/devbox", &[]);
    command.env_remove("CODER_SESSION_TOKEN");

    // When it is started
    let mut child = command.spawn().expect("the driver starts");
    let status = tokio::time::timeout(IO_TIMEOUT, child.wait())
        .await
        .expect("the driver exits")
        .expect("its status");
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .expect("its stderr")
        .read_to_string(&mut stderr)
        .await
        .expect("its stderr is readable");

    // Then it exits non-zero naming the variable, and the CLI was never run
    assert!(!status.success(), "the driver refused the target: {status}");
    assert!(stderr.contains("CODER_SESSION_TOKEN"), "{stderr}");
    assert_eq!(machine.coder_calls(), Vec::<String>::new());
}

#[tokio::test]
async fn an_agent_that_exits_as_it_starts_fails_start_session_with_its_last_words() {
    // Given a driver on a local target whose agent refuses to run
    let machine = AMachine::with_agent(AN_AGENT_THAT_EXITS);
    let mut driver = ADriver::started(machine.driver("local", &[])).await;

    // When the session is started
    let (text, is_error) = driver.call("start_session", json!({})).await;

    // Then the start fails with what the agent said
    assert!(is_error, "the start failed: {text}");
    assert!(text.contains("acme: no licence for this host"), "{text}");
    let status = driver.answer("session_status", json!({})).await;
    assert_eq!(status["state"], json!("exited"), "{status}");
}

#[tokio::test]
async fn with_start_the_session_runs_before_the_first_call() {
    // Given a driver started with --start on a local target
    let machine = AMachine::with_agent(AN_AGENT_THAT_ANSWERS);
    let mut driver = ADriver::started(machine.driver("local", &["--start"])).await;

    // When its status is asked for at once
    let status = driver.answer("session_status", json!({})).await;

    // Then the session is already running, and a second start is refused
    assert_eq!(status["state"], json!("running"), "{status}");
    let (text, is_error) = driver.call("start_session", json!({})).await;
    assert!(is_error, "a second start is refused: {text}");
    let _ = driver.answer("stop_session", json!({})).await;
    let _ = driver.child.start_kill();
}

#[tokio::test]
async fn stopping_a_coder_session_stops_its_agent_on_the_workspace() {
    // Given a driver on a Coder target whose agent outlives the CLI that carries its terminal, as an
    // agent on a real workspace does, and records how it is ended
    let machine = AMachine::with_agent_outliving_its_client(AN_AGENT_THAT_OUTLIVES_ITS_CLIENT);
    let mut driver =
        ADriver::started(machine.driver("coder:ada/devbox", &["--session-id", "stopped-session"]))
            .await;
    let _ = driver.answer("start_session", json!({})).await;
    let ready = driver
        .answer(
            "wait_for_text",
            json!({ "text": "ACME READY", "timeout_secs": 20 }),
        )
        .await;
    assert_eq!(ready["matched"], json!(true), "{ready}");

    // When the session is stopped
    let stopped = driver.answer("stop_session", json!({})).await;

    // Then the agent on the workspace was told to terminate, by the pid its launch recorded there,
    // and the session reads as exited
    let ended = machine.root.join("agent-ended");
    let deadline = std::time::Instant::now() + IO_TIMEOUT;
    while !ended.exists() && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let how = std::fs::read_to_string(&ended).unwrap_or_default();
    // An agent the stop missed keeps running; end it so the suite leaves nothing behind.
    if how.is_empty() {
        if let Ok(pid) = std::fs::read_to_string(machine.agent_pid_file("stopped-session")) {
            let _ = std::process::Command::new("kill")
                .arg("-9")
                .arg(pid.trim())
                .status();
        }
    }
    assert_eq!(how, "terminated");
    assert_eq!(stopped["state"], json!("exited"), "{stopped}");
    // And the stop removed the pid it killed by, so no later stop can kill whatever reuses it
    assert!(
        !machine.agent_pid_file("stopped-session").exists(),
        "the stopped session's pid file is gone"
    );
}

#[tokio::test]
async fn two_sessions_sharing_a_worktree_each_stop_only_their_own_agent() {
    // Given two driven Coder sessions in one worktree, each agent outliving its client and recording
    // its own end by its pid
    let machine = AMachine::with_agent_outliving_its_client(AN_AGENT_THAT_RECORDS_ITS_END_BY_PID);
    let mut first =
        ADriver::started(machine.driver("coder:ada/devbox", &["--session-id", "first"])).await;
    let mut second =
        ADriver::started(machine.driver("coder:ada/devbox", &["--session-id", "second"])).await;
    for driver in [&mut first, &mut second] {
        let started = driver.answer("start_session", json!({})).await;
        assert_eq!(started["state"], json!("running"), "{started}");
    }
    let first_pid = machine.recorded_agent_pid("first").await;
    let second_pid = machine.recorded_agent_pid("second").await;

    // When the first session is stopped
    let stopped = first.answer("stop_session", json!({})).await;

    // Then only the first agent was told to terminate, and only its pid file is gone
    let first_ended = machine.agent_ended(&first_pid, IO_TIMEOUT).await;
    let second_ended = machine
        .agent_ended(&second_pid, Duration::from_secs(2))
        .await;
    let first_file_left = machine.agent_pid_file("first").exists();
    let second_file_left = machine.agent_pid_file("second").exists();
    // The second agent is ended either way, so the suite leaves nothing behind.
    let _ = second.answer("stop_session", json!({})).await;
    if !machine.agent_ended(&second_pid, IO_TIMEOUT).await {
        let _ = std::process::Command::new("kill")
            .args(["-9", &second_pid])
            .status();
    }
    assert_ne!(first_pid, second_pid, "each session recorded its own agent");
    assert_eq!(stopped["state"], json!("exited"), "{stopped}");
    assert_eq!(
        (first_ended, second_ended, first_file_left, second_file_left),
        (true, false, false, true)
    );
}

#[tokio::test]
async fn a_binary_named_for_a_coder_target_is_the_one_the_workspace_runs() {
    // Given a Coder workspace whose `PATH` has the agent's bare command, and another binary on it
    let machine = AMachine::with_agent(AN_AGENT_THAT_ANSWERS);
    let other = machine.root.join("another-acme");
    write_executable(
        &other,
        "#!/bin/sh\nprintf 'ANOTHER ACME READY\\n'\nwhile IFS= read -r line; do :; done\n",
    );
    let other = other.to_string_lossy().into_owned();
    let mut driver =
        ADriver::started(machine.driver("coder:ada/devbox", &["--binary", &other])).await;

    // When the session is started
    let started = driver.answer("start_session", json!({})).await;

    // Then the binary named is the one running on the workspace
    assert_eq!(started["state"], json!("running"), "{started}");
    let ready = driver
        .answer(
            "wait_for_text",
            json!({ "text": "ANOTHER ACME READY", "timeout_secs": 20 }),
        )
        .await;
    let _ = driver.answer("stop_session", json!({})).await;
    assert_eq!(ready["matched"], json!(true), "{ready}");
}

#[tokio::test]
async fn a_coder_session_ends_with_its_agent_s_exit_status_not_the_cli_s() {
    // Given a Coder session behind a CLI that, like the real one, exits 1 whenever the workspace's
    // command fails and names the command's own status last
    let machine =
        AMachine::with_agent_behind_a_cli_that_exits_1_on_failure(AN_AGENT_THAT_EXITS_3_ON_BYE);
    let mut driver = ADriver::started(machine.driver("coder:ada/devbox", &[])).await;
    let started = driver.answer("start_session", json!({})).await;
    assert_eq!(started["state"], json!("running"), "{started}");

    // When the agent is told to exit, which it does with status 3
    let _ = driver
        .answer("send_keys", json!({ "text": "bye", "keys": ["Enter"] }))
        .await;
    let ended = driver
        .answer(
            "wait_for_text",
            json!({ "text": "text the agent never prints", "timeout_secs": 20 }),
        )
        .await;

    // Then the session ended with the agent's status, not the CLI's
    assert_eq!(ended["exited"], json!(true), "{ended}");
    let status = driver.answer("session_status", json!({})).await;
    assert_eq!(status["state"], json!("exited"), "{status}");
    assert_eq!(status["exit"], json!("exit status 3"), "{status}");
}
