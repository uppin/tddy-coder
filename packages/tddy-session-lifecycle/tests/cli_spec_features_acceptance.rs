//! Acceptance: the generic CLI spec launch serves everything Claude Code's and Cursor's sessions
//! need (`#agent-tools 14/19`). Each test drops a copy of the built-in `claude.yaml` or
//! `cursor.yaml` into `<tddyhome>/tools` under another id (`acme`) — an agent no Rust names, so its
//! session (`acme-cli`) launches through the generic path (`start_cli_harness_session`), never the
//! bespoke claude/cursor paths the built-in ids still dispatch to until `#agent-tools 17/19`.
//!
//! Feature docs: `docs/ft/daemon/claude-cli-session.md`, `docs/ft/daemon/cursor-cli-session.md`,
//! `docs/ft/daemon/session-agent-roster.md`.
//!
//! The daemon is `agent_registry_support`'s: the real `DaemonSessionHost` given the coding-agent
//! registry over its `<tddyhome>/tools`, the copy's binary a stub that reports its argv. The ssh
//! target is a stub `ssh` first on this process's `PATH` that records what it is asked to write and
//! runs nothing; tmux is a private server (`support::PrivateTmux`). The jailed start runs a real
//! Seatbelt jail, so it is macOS-only: CI (ubuntu) never runs the `jailed` module.

mod agent_registry_support;
mod support;

use std::path::{Path, PathBuf};
use std::time::Duration;

use agent_registry_support::{
    a_registry_daemon, builtin_yaml_as, cursor_copy_minting_within, ARegistryDaemon, ACME_MODEL,
    MINTED_CHAT_ID,
};
use pretty_assertions::assert_eq;
use support::PrivateTmux;
use tddy_rpc::Code;
use tddy_service::proto::session::StartSessionRequest;

/// The id every copy is dropped in under.
const COPY_ID: &str = "acme";
/// The copy's session type.
const COPY_SESSION_TYPE: &str = "acme-cli";
/// The ssh `Host` alias an ssh start names.
const SSH_ALIAS: &str = "buildbox";

/// A daemon holding `claude.yaml` under [`COPY_ID`].
fn a_daemon_with_a_claude_copy() -> ARegistryDaemon {
    a_registry_daemon()
        .with_tool("acme.yaml", &builtin_yaml_as("claude", COPY_ID))
        .build()
}

/// A daemon holding `cursor.yaml` under [`COPY_ID`].
fn a_daemon_with_a_cursor_copy() -> ARegistryDaemon {
    a_registry_daemon()
        .with_tool("acme.yaml", &builtin_yaml_as("cursor", COPY_ID))
        .build()
}

/// The branch a claude copy's start names: claude.yaml declares `branch_name_required`.
const BRANCH: &str = "acme-work";

/// `start` naming [`BRANCH`] for its new branch.
fn on_a_named_branch(start: StartSessionRequest) -> StartSessionRequest {
    StartSessionRequest {
        new_branch_name: BRANCH.to_string(),
        ..start
    }
}

/// The argv the `claude` spec renders for a host start of `session_id` on [`ACME_MODEL`] with no
/// prompt and no permission mode chosen: what the bespoke claude-cli start launches today.
fn claude_start_argv(stub: &str, session_id: &str) -> Vec<String> {
    [
        stub,
        "--model",
        ACME_MODEL,
        "--session-id",
        session_id,
        "--permission-mode",
        "auto",
    ]
    .map(str::to_string)
    .to_vec()
}

/// The argv the `claude` spec renders for a resume of `session_id`: the recorded model, `--resume`
/// with the same id, and the default permission mode.
fn claude_resume_argv(stub: &str, session_id: &str) -> Vec<String> {
    [
        stub,
        "--model",
        ACME_MODEL,
        "--resume",
        session_id,
        "--permission-mode",
        "auto",
    ]
    .map(str::to_string)
    .to_vec()
}

#[tokio::test]
#[serial_test::serial]
async fn a_copy_of_claude_yaml_under_another_id_starts_in_tmux_with_the_argv_the_claude_spec_renders_and_resumes_the_same_way(
) {
    // Given — claude.yaml as acme, and a private tmux server
    let _tmux = PrivateTmux::started();
    let daemon = a_daemon_with_a_claude_copy();

    // When — started in tmux, stopped, then resumed
    let (session_id, _host) = daemon
        .start(on_a_named_branch(daemon.a_tmux_start(COPY_SESSION_TYPE)))
        .await
        .expect("a tmux acme-cli start succeeds");
    let started = daemon.stub_report(&session_id).await;
    daemon.stop(&session_id).await;
    daemon
        .resume(&session_id)
        .await
        .expect("the stopped acme-cli session resumes");
    let resumed = daemon.stub_report(&session_id).await;

    // Then — the claude spec's argv in tmux on both, the conversation being the session id, and the
    // operator-notify guidance (the copy declares `operator_notify`) appended to the system prompt
    let notify_prompt = [
        "--append-system-prompt-file".to_string(),
        daemon
            .tddy_home
            .join("sessions")
            .join(&session_id)
            .join("agent-system-prompt.md")
            .to_string_lossy()
            .into_owned(),
    ];
    assert_eq!(
        (
            started.argv(),
            resumed.argv(),
            daemon.metadata_of(&session_id).tmux_session.is_some(),
        ),
        (
            [
                claude_start_argv(&daemon.stub_path(), &session_id),
                notify_prompt.to_vec()
            ]
            .concat(),
            [
                claude_resume_argv(&daemon.stub_path(), &session_id),
                notify_prompt.to_vec()
            ]
            .concat(),
            true,
        )
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_copy_of_claude_yaml_under_another_id_as_a_project_home_main_records_its_role_and_restricts_its_tools(
) {
    // Given — claude.yaml as acme, which declares `project_home_main`
    let daemon = a_daemon_with_a_claude_copy();

    // When — the project's main is set up from an acme-cli start
    let session_id = daemon
        .ensure_project_home(daemon.a_start(COPY_SESSION_TYPE, false))
        .await
        .expect("an acme-cli main is set up");
    let started = daemon.stub_report(&session_id).await;

    // Then — the role is recorded, and the agent is pre-approved exactly the project-home tools
    assert_eq!(
        (
            daemon.metadata_of(&session_id).session_role,
            started.values_of("--allowedTools"),
        ),
        (
            Some(tddy_core::session_metadata::PROJECT_HOME_ROLE.to_string()),
            tddy_tool_engine::read_only::project_home_allowed_tools(),
        )
    );
}

/// A stub `ssh` first on this process's `PATH` for as long as it lives: every call appends its
/// remote command and its stdin to `<log>/calls`, and runs nothing. With `slot_writes_hang`, a call
/// that writes a file (`cat > …`) never returns.
struct AnSshTarget {
    log: tempfile::TempDir,
    _bin: tempfile::TempDir,
    previous_path: Option<std::ffi::OsString>,
}

impl AnSshTarget {
    fn installed(slot_writes_hang: bool) -> Self {
        let log = tempfile::tempdir().unwrap();
        let bin = tempfile::tempdir().unwrap();
        let hang = if slot_writes_hang {
            r#"case "$remote" in *"cat > "*) exec /bin/sleep 3600 ;; esac"#
        } else {
            ""
        };
        let script = format!(
            r#"#!/bin/sh
while [ "$#" -gt 0 ] && [ "$1" != "--" ]; do shift; done
shift
alias="$1"; shift
remote="$*"
{hang}
printf 'CALL %s %s\n' "$alias" "$remote" >> "{log}/calls"
/bin/cat >> "{log}/calls"
printf '\nEND\n' >> "{log}/calls"
exit 0
"#,
            log = log.path().display(),
        );
        let ssh = bin.path().join("ssh");
        std::fs::write(&ssh, script).unwrap();
        std::fs::set_permissions(&ssh, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        let previous_path = std::env::var_os("PATH");
        let mut path = std::ffi::OsString::from(bin.path());
        if let Some(previous) = &previous_path {
            path.push(":");
            path.push(previous);
        }
        std::env::set_var("PATH", path);
        Self {
            log,
            _bin: bin,
            previous_path,
        }
    }

    /// Whether the target was asked to write `path` on `alias`.
    fn wrote(&self, alias: &str, path: &Path) -> bool {
        let calls = std::fs::read_to_string(self.log.path().join("calls")).unwrap_or_default();
        calls.lines().any(|line| {
            line.starts_with(&format!("CALL {alias} "))
                && line.contains(&format!("cat > '{}'", path.display()))
        })
    }
}

impl Drop for AnSshTarget {
    fn drop(&mut self) {
        match &self.previous_path {
            Some(path) => std::env::set_var("PATH", path),
            None => std::env::remove_var("PATH"),
        }
    }
}

#[tokio::test]
#[serial_test::serial]
async fn a_copy_of_claude_yaml_under_another_id_on_an_ssh_host_writes_its_slot_files_on_the_target_only(
) {
    // Given — claude.yaml as acme, and an ssh target that answers
    let target = AnSshTarget::installed(false);
    let daemon = a_daemon_with_a_claude_copy();

    // When
    let (session_id, _host) = daemon
        .start(on_a_named_branch(
            daemon.an_ssh_start(COPY_SESSION_TYPE, SSH_ALIAS),
        ))
        .await
        .expect("an acme-cli start on an ssh target succeeds");

    // Then — the hooks settings were written on the target, at the remote worktree, and never at
    // that path on this host
    let meta = daemon.metadata_of(&session_id);
    let hooks_file = PathBuf::from(
        meta.repo_path
            .clone()
            .expect("a remote worktree is recorded"),
    )
    .join(".claude/settings.local.json");
    assert_eq!(
        (
            meta.ssh_config_host,
            target.wrote(SSH_ALIAS, &hooks_file),
            hooks_file.exists(),
        ),
        (Some(SSH_ALIAS.to_string()), true, false)
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_copy_of_claude_yaml_under_another_id_on_an_ssh_host_runs_without_the_daemons_tddy_variables(
) {
    // Given — a TDDY_ variable in the daemon's own environment, claude.yaml as acme, and an ssh
    // target that answers
    let _leak = agent_registry_support::ATddyVariableInTheDaemon::set("TDDY_LEAK_PROBE", "1");
    let _target = AnSshTarget::installed(false);
    let daemon = a_daemon_with_a_claude_copy();

    // When
    let (session_id, _host) = daemon
        .start(on_a_named_branch(
            daemon.an_ssh_start(COPY_SESSION_TYPE, SSH_ALIAS),
        ))
        .await
        .expect("an acme-cli start on an ssh target succeeds");
    let started = daemon.stub_report(&session_id).await;

    // Then — no TDDY_ variable reaches the agent, the operator-notify ones included
    assert_eq!(started.tddy_variables(), Vec::<String>::new());
}

#[tokio::test]
#[serial_test::serial]
async fn a_managed_claude_copys_shell_run_tddy_tools_submit_reaches_its_session_with_no_tddy_variable_in_the_agent(
) {
    // Given — claude.yaml as acme, whose stub runs `tddy-tools submit` from its own shell
    let daemon = a_registry_daemon()
        .with_tool("acme.yaml", &builtin_yaml_as("claude", COPY_ID))
        .with_a_stub_that_submits_from_its_shell()
        .build();

    // When — a managed `tdd` start on the host
    let (session_id, _host) = daemon
        .start(on_a_named_branch(
            daemon.a_managed_start(COPY_SESSION_TYPE, "tdd"),
        ))
        .await
        .expect("a managed acme-cli start succeeds");
    let started = daemon.stub_report(&session_id).await;

    // Then — the agent saw no TDDY_ variable; its submission was answered by the session (an
    // answer carrying its goal), never "no session to report to"; and the pointer it was answered
    // through also carries the session `tddy-tools notify` reports for
    let answer = started.submit_answer();
    let pointer =
        tddy_core::session_pointer::find_session_pointer(&daemon.worktree_of(&session_id))
            .expect("the pointer parses")
            .map(|(_, pointer)| pointer.notify.get("TDDY_SESSION_ID").cloned());
    assert_eq!(
        (started.tddy_variables(), answer["goal"].clone(), pointer),
        (
            Vec::<String>::new(),
            serde_json::json!("plan"),
            Some(Some(session_id.clone()))
        ),
        "the stub's submit answered {answer}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_copy_of_cursor_yaml_under_another_id_starts_with_its_hooks_and_a_minted_chat_id() {
    // Given — cursor.yaml as acme; the stub's `create-chat` prints MINTED_CHAT_ID
    let daemon = a_daemon_with_a_cursor_copy();

    // When
    let (session_id, _host) = daemon
        .start(daemon.a_start(COPY_SESSION_TYPE, false))
        .await
        .expect("a host acme-cli start succeeds");
    let started = daemon.stub_report(&session_id).await;

    // Then — the chat is minted, recorded and resumed by the first launch; cursor's hooks file is
    // written in the worktree
    assert_eq!(
        (
            started.argv(),
            daemon.metadata_of(&session_id).cursor_chat_id,
            daemon
                .worktree_of(&session_id)
                .join(".cursor/hooks.json")
                .exists(),
        ),
        (
            [
                daemon.stub_path().as_str(),
                "--resume",
                MINTED_CHAT_ID,
                "--model",
                ACME_MODEL,
            ]
            .map(str::to_string)
            .to_vec(),
            Some(MINTED_CHAT_ID.to_string()),
            true,
        )
    );
}

/// How long a start whose mint has a one-second deadline may take before the test calls it hung.
/// Generous on purpose: it bounds a hang, it is not the deadline under test.
const A_HUNG_START: Duration = Duration::from_secs(30);

#[tokio::test]
#[serial_test::serial]
async fn a_cursor_chat_mint_that_never_returns_refuses_the_start_after_its_deadline_naming_the_agent(
) {
    // Given — cursor.yaml as acme, its mint bounded at one second, and a `create-chat` that never
    // returns
    let daemon = a_registry_daemon()
        .with_tool("acme.yaml", &cursor_copy_minting_within(COPY_ID, 1))
        .with_a_create_chat_that_never_returns()
        .build();

    // When
    let refusal = tokio::time::timeout(
        A_HUNG_START,
        daemon.start(daemon.a_start(COPY_SESSION_TYPE, false)),
    )
    .await
    .expect("the start returns: the mint has a deadline")
    .map(|(session_id, _host)| session_id)
    .expect_err("a start whose chat is never minted is refused");

    // Then — refused naming the agent, its adapter and the deadline; the agent never spawned
    assert_eq!(
        (refusal.code, refusal.message, daemon.spawn_marker.exists()),
        (
            Code::DeadlineExceeded,
            "acme could not mint its conversation with cursor-create-chat: no conversation id \
             within 1s; the mint was killed"
                .to_string(),
            false,
        )
    );
}

/// How long the ssh slot write may take before the test calls the start hung: the production
/// deadline (`SLOT_FILE_OVER_SSH_DEADLINE`, 60 s, `#agent-tools 12/19`) plus a margin. This test is
/// over the integration budget on purpose — the deadline is the production constant, and a
/// test-only shorter one would be a test-only branch.
const A_HUNG_SSH_START: Duration = Duration::from_secs(90);

#[tokio::test]
#[serial_test::serial]
async fn an_ssh_slot_write_that_never_returns_refuses_the_start_after_its_deadline_naming_the_host()
{
    // Given — claude.yaml as acme, and an ssh target that never answers a file write
    let _target = AnSshTarget::installed(true);
    let daemon = a_daemon_with_a_claude_copy();

    // When
    let refusal = tokio::time::timeout(
        A_HUNG_SSH_START,
        daemon.start(on_a_named_branch(
            daemon.an_ssh_start(COPY_SESSION_TYPE, SSH_ALIAS),
        )),
    )
    .await
    .expect("the start returns: the ssh write has a deadline")
    .map(|(session_id, _host)| session_id)
    .expect_err("a start whose slot files never reach the target is refused");

    // Then — refused as a deadline naming the host and the deadline; the agent never spawned. The
    // file's path on the target is the target's layout, so the message is matched on what it
    // names rather than in full.
    assert_eq!(
        (
            refusal.code,
            refusal
                .message
                .contains("on ssh target buildbox: no answer within 60s"),
            daemon.spawn_marker.exists(),
        ),
        (Code::DeadlineExceeded, true, false)
    );
}

/// The jailed half: macOS only, through a real Seatbelt jail.
#[cfg(target_os = "macos")]
mod jailed {
    use pretty_assertions::assert_eq;

    use super::{
        a_daemon_with_a_claude_copy, claude_start_argv, on_a_named_branch, COPY_SESSION_TYPE,
    };

    #[tokio::test]
    #[serial_test::serial]
    async fn a_copy_of_claude_yaml_under_another_id_starts_in_a_jail_with_its_closure_tools() {
        // Given
        let daemon = a_daemon_with_a_claude_copy();

        // When
        let (session_id, _jail) = daemon
            .start(on_a_named_branch(daemon.a_start(COPY_SESSION_TYPE, true)))
            .await
            .expect("a jailed acme-cli start succeeds");
        let started = daemon.stub_report(&session_id).await;

        // Then — the claude spec's argv inside the jail (the binary is the jail's own path), followed
        // by the slots a jailed agent is given: its tool lists, its permission prompt and the
        // registration of `tddy-tools`, its only way to the code; and the closure's tddy-tools on
        // the jail's PATH
        let meta = daemon.metadata_of(&session_id);
        let spec_argv = claude_start_argv("", &session_id)
            .into_iter()
            .skip(1)
            .collect::<Vec<_>>();
        let argv = started.argv().into_iter().skip(1).collect::<Vec<_>>();
        let slot_flags: Vec<&str> = argv[spec_argv.len().min(argv.len())..]
            .iter()
            .map(String::as_str)
            .filter(|arg| arg.starts_with("--"))
            .collect();
        assert_eq!(
            (
                argv[..spec_argv.len()].to_vec(),
                slot_flags.contains(&"--mcp-config"),
                slot_flags.contains(&"--permission-prompt-tool"),
                started.tddy_tools_on_path(),
                meta.sandbox,
            ),
            (spec_argv, true, true, true, Some(true))
        );
    }
}
