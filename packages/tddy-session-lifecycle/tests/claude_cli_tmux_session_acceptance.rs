//! Acceptance tests: a Claude CLI session started with `run_in_tmux` runs its agent inside a tmux
//! session on the host, as the session's OS user (`#tmux-agent` 1/3,
//! docs/ft/daemon/claude-cli-session.md § tmux mode).
//!
//! Real tmux, privately: `TMUX_TMPDIR` points at a fresh socket directory (see `support`), the
//! identity maps to the OS user running the test, and the agent is a stub `claude` that records how
//! it was launched. Serial — the tmux socket directory is process-wide environment.

mod agent_registry_support;
mod support;
mod tmux_agent_support;

use tddy_rpc::{Code, Request};
use tddy_service::proto::session::{
    DeleteSessionRequest, ListSessionsRequest, ResumeSessionRequest,
    SessionService as SessionServiceTrait, StartSessionRequest,
};
use tddy_testing_commons::stub_scripts::{read_last_appended_argv, read_recorded_env};
use tddy_testing_commons::wait::{eventually, eventually_awaiting};

use support::{this_os_user, until_attached, PrivateTmux, OBSERVE, VALID_TOKEN};
use tmux_agent_support::*;

// ---------------------------------------------------------------------------
// Acceptance
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial]
async fn a_claude_session_started_in_tmux_runs_claude_in_a_named_tmux_session_in_the_worktree() {
    // Given
    let daemon = a_daemon_with_a_private_tmux().await;

    // When
    let session_id = daemon.starts_a_session_in_tmux().await;

    // Then claude runs in the tmux session the daemon named for this session, in its worktree
    let name = tddy_cli_sessions::tmux::agent_tmux_session_name(&session_id);
    assert_eq!(daemon.tmux_session_of(&session_id), name);
    daemon
        .until_the_agent_ran_in_tmux_session(&daemon.tmux, &name)
        .await;
    assert_eq!(
        daemon.tmux.pane_directory(&name),
        daemon.worktree_of(&session_id)
    );
    let argv = read_last_appended_argv(&daemon.launches).unwrap();
    assert!(
        argv.windows(2).any(|w| w == ["--session-id", &session_id]),
        "claude is launched with the session id: {argv:?}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn the_main_terminal_of_a_tmux_run_session_is_attached_to_its_tmux_session() {
    // Given
    let daemon = a_daemon_with_a_private_tmux().await;

    // When
    let session_id = daemon.starts_a_session_in_tmux().await;

    // Then its main terminal is a tmux client of that session
    let name = daemon.tmux_session_of(&session_id);
    until_attached(&daemon.tmux, &name, 1).await;
    let main = daemon
        .manager
        .get(&session_id)
        .await
        .expect("the main terminal is registered");
    assert_eq!(main.tmux_session.as_deref(), Some(name.as_str()));
}

#[tokio::test]
#[serial_test::serial]
async fn a_tmux_run_session_records_the_pane_pid_and_reads_active() {
    // Given
    let daemon = a_daemon_with_a_private_tmux().await;

    // When
    let session_id = daemon.starts_a_session_in_tmux().await;

    // Then the recorded pid is the agent's pane process, not the tmux client
    let name = daemon.tmux_session_of(&session_id);
    daemon
        .until_the_agent_ran_in_tmux_session(&daemon.tmux, &name)
        .await;
    let pane_pid: u32 = String::from_utf8_lossy(
        &daemon
            .tmux
            .tmux(&["display-message", "-p", "-t", &name, "#{pane_pid}"])
            .stdout,
    )
    .trim()
    .parse()
    .expect("tmux reports the pane pid");
    let recorded = daemon.metadata_of(&session_id).pid;
    assert_eq!(recorded, Some(pane_pid));
    let client_pid = daemon.manager.get(&session_id).await.unwrap().pid;
    assert_ne!(recorded, Some(client_pid), "pid is not the tmux client's");
    // and it reads active
    let listed = daemon
        .host
        .list_sessions(Request::direct(ListSessionsRequest {
            session_token: VALID_TOKEN.to_string(),
        }))
        .await
        .expect("ListSessions")
        .into_inner()
        .sessions;
    let entry = listed
        .iter()
        .find(|s| s.session_id == session_id)
        .expect("the session is listed");
    assert!(entry.is_active, "a tmux-run session reads active");
}

#[tokio::test]
#[serial_test::serial]
async fn claude_in_tmux_sees_the_session_environment() {
    // Given
    let daemon = a_daemon_with_a_private_tmux().await;

    // When
    let session_id = daemon.starts_a_session_in_tmux().await;

    // Then the pane's environment carries what the daemon launches an agent with — tmux panes
    // inherit the server's environment, not the client's, so it has to be passed to the pane
    let name = daemon.tmux_session_of(&session_id);
    daemon
        .until_the_agent_ran_in_tmux_session(&daemon.tmux, &name)
        .await;
    let seen = eventually("the stub to record its environment", OBSERVE, || {
        read_recorded_env(&daemon.environment)
    })
    .await;
    assert_eq!(
        seen.get("CLAUDE_CODE_NO_FLICKER").map(String::as_str),
        Some("1")
    );
    assert!(
        daemon.working_directory.exists(),
        "the stub ran (and recorded its directory)"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn the_pane_environment_file_is_gone_once_the_agent_runs_and_no_value_was_on_tmuxs_argv() {
    // Given
    let daemon = a_daemon_with_a_private_tmux().await;

    // When
    let session_id = daemon.starts_a_session_in_tmux().await;
    let name = daemon.tmux_session_of(&session_id);
    daemon
        .until_the_agent_ran_in_tmux_session(&daemon.tmux, &name)
        .await;

    // Then the pane sourced and deleted the file the environment was handed over in
    let env_dir = daemon
        .session_dir(&session_id)
        .join(tddy_cli_sessions::tmux::pane_env::PANE_ENV_DIR);
    eventually("the pane environment file to be deleted", OBSERVE, || {
        let left: Vec<_> = std::fs::read_dir(&env_dir)
            .map_err(|e| format!("{}: {e}", env_dir.display()))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect();
        left.is_empty()
            .then_some(())
            .ok_or_else(|| format!("still there: {left:?}"))
    })
    .await;
    // and the pane's command names a file and the variables it keeps, never a value
    // (`#agent-tools 28/28`: names only reach the pane's argv, never values)
    let start_command = daemon.tmux.tmux(&[
        "display-message",
        "-p",
        "-t",
        &format!("={name}:"),
        "#{pane_start_command}",
    ]);
    let start_command = String::from_utf8_lossy(&start_command.stdout);
    assert!(
        !start_command.contains("CLAUDE_CODE_NO_FLICKER="),
        "{start_command}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn deleting_a_tmux_run_session_kills_the_agent_and_the_tmux_session() {
    // Given a running tmux-run session
    let daemon = a_daemon_with_a_private_tmux().await;
    let session_id = daemon.starts_a_session_in_tmux().await;
    let name = daemon.tmux_session_of(&session_id);
    daemon
        .until_the_agent_ran_in_tmux_session(&daemon.tmux, &name)
        .await;
    let agent_pid = daemon.metadata_of(&session_id).pid.expect("a recorded pid");

    // When it is deleted
    daemon
        .host
        .delete_session(Request::direct(DeleteSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: session_id.clone(),
        }))
        .await
        .expect("DeleteSession");

    // Then the agent and its tmux session are gone
    eventually(
        "the agent and its tmux session to be gone",
        OBSERVE,
        || match (
            a_process_is_alive(agent_pid),
            daemon.tmux.has_session(&name),
        ) {
            (false, false) => Ok(()),
            (agent, tmux) => Err(format!("agent alive: {agent}, tmux session alive: {tmux}")),
        },
    )
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn resuming_a_terminated_tmux_run_session_starts_a_fresh_tmux_session_with_the_resume_argv() {
    // Given a tmux-run session whose agent has been terminated
    let daemon = a_daemon_with_a_private_tmux().await;
    let session_id = daemon.starts_a_session_in_tmux().await;
    let name = daemon.tmux_session_of(&session_id);
    daemon
        .until_the_agent_ran_in_tmux_session(&daemon.tmux, &name)
        .await;
    assert!(daemon
        .tmux
        .tmux(&["kill-session", "-t", &name])
        .status
        .success());
    eventually_awaiting(
        "the main terminal to leave the registry",
        OBSERVE,
        || async {
            match daemon.manager.get(&session_id).await {
                None => Ok(()),
                Some(_) => Err("the main terminal is still registered".to_string()),
            }
        },
    )
    .await;

    // When it is resumed
    daemon
        .host
        .resume_session(Request::direct(ResumeSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: session_id.clone(),
        }))
        .await
        .expect("ResumeSession");

    // Then a fresh tmux session runs claude with the resume argv
    let resumed_name = daemon.tmux_session_of(&session_id);
    until_attached(&daemon.tmux, &resumed_name, 1).await;
    let argv = eventually("claude to be relaunched with --resume", OBSERVE, || {
        let argv = read_last_appended_argv(&daemon.launches)?;
        if argv.contains(&"--resume".to_string()) {
            Ok(argv)
        } else {
            Err(format!("last launch: {argv:?}"))
        }
    })
    .await;
    assert!(
        argv.windows(2).any(|w| w == ["--resume", &session_id]),
        "{argv:?}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn run_in_tmux_with_sandbox_is_refused() {
    // Given
    let daemon = a_daemon_with_a_private_tmux().await;

    // When
    let refusal = daemon
        .host
        .start_session(Request::direct(a_claude_cli_start_request(true, true)))
        .await
        .expect_err("run_in_tmux with sandbox must be refused");

    // Then
    assert_eq!(
        refusal.code(),
        Code::InvalidArgument,
        "{}",
        refusal.message()
    );
    assert!(
        refusal.message().contains("tmux"),
        "the refusal names the option: {}",
        refusal.message()
    );
}

#[tokio::test]
#[serial_test::serial]
async fn deleting_a_session_whose_metadata_is_unreadable_still_kills_its_tmux_session() {
    // Given a running tmux-run session whose .session.yaml is then lost
    let daemon = a_daemon_with_a_private_tmux().await;
    let session_id = daemon.starts_a_session_in_tmux().await;
    let name = daemon.tmux_session_of(&session_id);
    daemon
        .until_the_agent_ran_in_tmux_session(&daemon.tmux, &name)
        .await;
    std::fs::remove_file(daemon.session_dir(&session_id).join(".session.yaml")).unwrap();

    // When it is deleted
    daemon
        .host
        .delete_session(Request::direct(DeleteSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: session_id.clone(),
        }))
        .await
        .expect("DeleteSession");

    // Then the tmux session, named by the session id, is gone
    eventually("the tmux session to be gone", OBSERVE, || {
        match daemon.tmux.has_session(&name) {
            false => Ok(()),
            true => Err(format!("tmux session {name:?} is still alive")),
        }
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn daemon_shutdown_detaches_a_tmux_run_sessions_main_and_leaves_its_agent_running() {
    // Given a running tmux-run session
    let daemon = a_daemon_with_a_private_tmux().await;
    let session_id = daemon.starts_a_session_in_tmux().await;
    let name = daemon.tmux_session_of(&session_id);
    daemon
        .until_the_agent_ran_in_tmux_session(&daemon.tmux, &name)
        .await;
    let agent_pid = daemon.metadata_of(&session_id).pid.expect("a recorded pid");

    // When the daemon shuts down
    daemon.manager.kill_all().await;

    // Then the main's client is detached, and the agent and its tmux session keep running
    eventually("the main's tmux client to detach", OBSERVE, || {
        (daemon.tmux.attached_clients(&name) == 0)
            .then_some(())
            .ok_or_else(|| "a client is still attached".to_string())
    })
    .await;
    assert!(daemon.tmux.has_session(&name), "the tmux session survives");
    assert!(a_process_is_alive(agent_pid), "the agent survives");
}

#[test]
#[serial_test::serial]
fn a_tmux_session_reads_alive_while_it_runs_and_not_alive_once_killed() {
    use tddy_cli_sessions::tmux::{kill_tmux_session, tmux_session_alive};

    // Given a running tmux session of this user — the liveness probe a re-attach decides on
    let tmux = PrivateTmux::started().with_session("alpha", 1);
    let user = this_os_user();
    assert!(tmux_session_alive(&user, "alpha"));

    // When it is killed
    kill_tmux_session(&user, "alpha").expect("kill-session");

    // Then it no longer reads alive, and killing it again is not an error
    assert!(!tmux.has_session("alpha"));
    assert!(!tmux_session_alive(&user, "alpha"));
    kill_tmux_session(&user, "alpha").expect("killing an absent session is not an error");
}

// ---------------------------------------------------------------------------
// The claude spec's argv in tmux (`#agent-tools 17/19`)
// ---------------------------------------------------------------------------

/// The argument an override of claude.yaml adds after its binary, which no hand-built claude argv
/// carries: an argv holding it was rendered from the spec.
const SPEC_MARK: &str = "--rendered-from-claude-yaml";

/// The built-in claude.yaml run as the stub, its argv marked with [`SPEC_MARK`] — an operator's
/// override, launched as `claude-cli`.
fn claude_yaml_marked_as_rendered_from_the_spec() -> String {
    let claude = agent_registry_support::builtin_yaml_run_as_the_stub("claude");
    let binary = "    - \"{binary}\"\n";
    assert!(
        claude.contains(binary),
        "claude.yaml's argv starts with its binary"
    );
    claude.replacen(binary, &format!("{binary}    - {SPEC_MARK}\n"), 1)
}

/// The argv the marked claude spec renders for a host start of `session_id` on `model`, with no
/// prompt and no permission mode chosen — and for its resume, `--resume` with the same id.
fn the_marked_claude_argv(
    stub: &str,
    model: &str,
    conversation: &str,
    session_id: &str,
) -> Vec<String> {
    [
        stub,
        SPEC_MARK,
        "--model",
        model,
        conversation,
        session_id,
        "--permission-mode",
        "auto",
    ]
    .map(str::to_string)
    .to_vec()
}

/// The flags that load `session_id`'s own hooks file, rendered again on every launch.
fn the_session_hooks_loaded(
    daemon: &agent_registry_support::ARegistryDaemon,
    session_id: &str,
) -> Vec<String> {
    vec![
        "--settings".to_string(),
        tddy_core::claude_hooks::session_file::claude_session_hooks_file(
            &daemon.worktree_of(session_id),
            session_id,
        )
        .to_string_lossy()
        .into_owned(),
    ]
}

/// `argv` without the system-prompt file the operator-notify guidance is appended with: the claude
/// spec's `append_system_prompt` flag renders it, beyond what this test pins of the argv.
fn without_the_notify_guidance(argv: Vec<String>) -> Vec<String> {
    let mut kept = Vec::with_capacity(argv.len());
    let mut rest = argv.into_iter();
    while let Some(arg) = rest.next() {
        if arg == "--append-system-prompt-file" {
            rest.next();
        } else {
            kept.push(arg);
        }
    }
    kept
}

#[tokio::test]
#[serial_test::serial]
async fn a_claude_cli_start_in_tmux_runs_the_argv_the_claude_spec_renders_and_resumes_the_same_way()
{
    // Given — a private tmux, and claude.yaml overridden with a marked argv
    let _tmux = PrivateTmux::started();
    let daemon = agent_registry_support::a_daemon_with_tools(&[(
        "claude.yaml",
        &claude_yaml_marked_as_rendered_from_the_spec(),
    )]);

    // When — started in tmux, stopped, then resumed
    let (session_id, _host) = daemon
        .start(StartSessionRequest {
            new_branch_name: "claude-in-tmux".to_string(),
            ..daemon.a_tmux_start("claude-cli")
        })
        .await
        .expect("a tmux claude-cli start succeeds");
    let started = daemon.stub_report(&session_id).await;
    daemon.stop(&session_id).await;
    daemon
        .resume(&session_id)
        .await
        .expect("the stopped claude-cli session resumes");
    let resumed = daemon.stub_report(&session_id).await;

    // Then — the spec's argv in tmux on both, the conversation being the session id
    let stub = daemon.stub_path();
    let model = agent_registry_support::ACME_MODEL;
    assert_eq!(
        (
            without_the_notify_guidance(started.argv()),
            without_the_notify_guidance(resumed.argv()),
            daemon.metadata_of(&session_id).tmux_session.is_some(),
        ),
        (
            [
                the_marked_claude_argv(&stub, model, "--session-id", &session_id),
                the_session_hooks_loaded(&daemon, &session_id),
            ]
            .concat(),
            [
                the_marked_claude_argv(&stub, model, "--resume", &session_id),
                the_session_hooks_loaded(&daemon, &session_id),
            ]
            .concat(),
            true,
        )
    );
}
