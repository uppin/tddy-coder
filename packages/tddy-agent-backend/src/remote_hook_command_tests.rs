//! Remote-hook command tests over a real worktree on disk.
//
// Test-only: included via `mod` under `#[cfg(all(test, unix))]`, so the filesystem-port
// guard reads the whole file as test code and the real-disk fixtures below are the tests'
// fixtures, not production I/O.
#![cfg(test)]
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use pretty_assertions::assert_eq;

use super::*;

const HOOK_TOKEN: &str = "tok-7f3a9c";

/// A worktree on a target, holding the directory the hook log lives in.
struct AWorktree {
    root: PathBuf,
    _dir: tempfile::TempDir,
}

fn a_worktree() -> AWorktree {
    a_worktree_named("s1")
}

fn a_worktree_named(name: &str) -> AWorktree {
    let dir = tempfile::tempdir().expect("a directory standing for the target");
    let root = dir.path().join(name);
    std::fs::create_dir_all(root.join(".tddy")).expect("the worktree's .tddy directory");
    AWorktree { root, _dir: dir }
}

impl AWorktree {
    fn log_path(&self) -> String {
        self.root
            .join(REMOTE_HOOK_LOG)
            .to_string_lossy()
            .into_owned()
    }

    /// What the hook log holds; nothing when no hook has written it.
    fn log(&self) -> String {
        std::fs::read_to_string(self.root.join(REMOTE_HOOK_LOG)).unwrap_or_default()
    }

    /// The log's lines, in the order they were appended.
    fn log_lines(&self) -> Vec<String> {
        self.log().lines().map(str::to_string).collect()
    }
}

/// A hook an agent has fired and is still writing the payload of.
struct AFiredHook {
    shell: Child,
}

/// The agent fires a hook: `command` is started under `/bin/sh -c`, its payload not yet sent.
fn fire(command: &str) -> AFiredHook {
    fire_with(Command::new("/bin/sh").arg("-c").arg(command))
}

fn fire_with(shell: &mut Command) -> AFiredHook {
    AFiredHook {
        shell: shell
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("a shell runs on this host"),
    }
}

impl AFiredHook {
    /// The agent writes `bytes` of the payload and keeps the hook's stdin open.
    fn sends(mut self, bytes: &str) -> Self {
        let stdin = self
            .shell
            .stdin
            .as_mut()
            .expect("the hook's stdin is piped");
        stdin
            .write_all(bytes.as_bytes())
            .and_then(|()| stdin.flush())
            .expect("the hook reads its payload");
        self
    }

    /// The agent closes the payload and the hook runs to its end. Whether it exited zero.
    fn finishes(mut self) -> bool {
        drop(self.shell.stdin.take());
        self.shell.wait().expect("the hook exits").success()
    }
}

/// Run `command` as an agent runs a hook, `payload` on its stdin. Whether it exited zero.
fn run_hook(command: &str, payload: &str) -> bool {
    fire(command).sends(payload).finishes()
}

/// Where this host keeps `program`.
fn program_on_this_host(program: &str) -> PathBuf {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .chain([PathBuf::from("/bin"), PathBuf::from("/usr/bin")])
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| panic!("this host has no {program}"))
}

/// A directory holding `cat` and `tr` and nothing else: with a shell's own `printf`, all a
/// bare target is asked to have.
fn a_path_holding_only_cat_and_tr(dir: &Path) -> PathBuf {
    let bin = dir.join("bare-bin");
    std::fs::create_dir_all(&bin).expect("a directory for the two programs");
    for program in ["cat", "tr"] {
        std::os::unix::fs::symlink(program_on_this_host(program), bin.join(program))
            .expect("the program is linked");
    }
    bin
}

#[test]
fn the_command_appends_one_line_with_the_event_the_token_and_the_payload() {
    // Given
    let worktree = a_worktree();
    let payload = r#"{"hook_event_name":"Stop","session_id":"c-41"}"#;
    let command = remote_hook_command("Stop", HOOK_TOKEN, &worktree.log_path());

    // When
    let succeeded = run_hook(&command, payload);

    // Then
    assert_eq!(
        (worktree.log(), succeeded),
        (format!("Stop\t{HOOK_TOKEN}\t{payload}\n"), true)
    );
}

#[test]
fn a_second_hook_appends_after_the_first() {
    // Given a log that already holds one hook's line
    let worktree = a_worktree();
    let started = r#"{"hook_event_name":"SessionStart","source":"startup"}"#;
    let stopped = r#"{"hook_event_name":"Stop"}"#;
    run_hook(
        &remote_hook_command("SessionStart", HOOK_TOKEN, &worktree.log_path()),
        started,
    );

    // When
    run_hook(
        &remote_hook_command("Stop", HOOK_TOKEN, &worktree.log_path()),
        stopped,
    );

    // Then
    assert_eq!(
        worktree.log_lines(),
        vec![
            format!("SessionStart\t{HOOK_TOKEN}\t{started}"),
            format!("Stop\t{HOOK_TOKEN}\t{stopped}"),
        ]
    );
}

#[test]
fn a_payload_with_newlines_and_quotes_arrives_as_one_parseable_line() {
    // Given a pretty-printed payload holding both quotes, a variable reference, a command
    // substitution and an escaped newline inside a string
    let worktree = a_worktree();
    let sent = serde_json::json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "Bash",
        "tool_input": { "command": "echo \"it's $HOME\"; echo `id`\nexit 0" },
    });
    let payload = format!("{sent:#}\n");
    let command = remote_hook_command("PostToolUse", HOOK_TOKEN, &worktree.log_path());

    // When
    run_hook(&command, &payload);

    // Then the log holds one line whose third field is the payload, value for value
    let received: Vec<Option<serde_json::Value>> = worktree
        .log_lines()
        .iter()
        .map(|line| {
            line.strip_prefix(&format!("PostToolUse\t{HOOK_TOKEN}\t"))
                .and_then(|payload| serde_json::from_str(payload).ok())
        })
        .collect();
    assert_eq!(received, vec![Some(sent)]);
}

#[test]
fn the_command_names_no_tddy_binary() {
    // Given a target that has `cat`, `tr` and a shell, and nothing of tddy's
    let worktree = a_worktree();
    let bare_path = a_path_holding_only_cat_and_tr(&worktree.root);
    let payload = r#"{"hook_event_name":"SessionEnd"}"#;
    let command = remote_hook_command("SessionEnd", HOOK_TOKEN, &worktree.log_path());

    // When the hook runs there
    let succeeded = fire_with(
        Command::new("/bin/sh")
            .arg("-c")
            .arg(&command)
            .env_clear()
            .env("PATH", &bare_path),
    )
    .sends(payload)
    .finishes();

    // Then it reports, and names neither the tools binary nor its subcommand
    assert_eq!(
        (
            worktree.log(),
            succeeded,
            command.contains("tddy-tools"),
            command.contains("session-hook")
        ),
        (
            format!("SessionEnd\t{HOOK_TOKEN}\t{payload}\n"),
            true,
            false,
            false
        )
    );
}

#[test]
fn two_hooks_fired_together_each_leave_a_whole_line() {
    // Given a hook that has fired and is still being sent its payload
    let worktree = a_worktree();
    let slow = fire(&remote_hook_command(
        "PostToolUse",
        HOOK_TOKEN,
        &worktree.log_path(),
    ))
    .sends(r#"{"hook_event_name":"PostToolUse","#);

    // When another hook fires and finishes meanwhile, and the first then gets the rest
    run_hook(
        &remote_hook_command("Notification", HOOK_TOKEN, &worktree.log_path()),
        r#"{"hook_event_name":"Notification","message":"Claude needs your permission"}"#,
    );
    slow.sends(r#""tool_name":"Bash"}"#).finishes();

    // Then each left its own line, whole
    assert_eq!(
        worktree.log_lines(),
        vec![
            format!(
                "Notification\t{HOOK_TOKEN}\t{}",
                r#"{"hook_event_name":"Notification","message":"Claude needs your permission"}"#
            ),
            format!(
                "PostToolUse\t{HOOK_TOKEN}\t{}",
                r#"{"hook_event_name":"PostToolUse","tool_name":"Bash"}"#
            ),
        ]
    );
}

#[test]
fn a_log_path_with_a_space_and_a_quote_is_the_file_written() {
    // Given a worktree whose path holds a space and a single quote
    let worktree = a_worktree_named("o'brien's repo");
    let payload = r#"{"hook_event_name":"Stop"}"#;
    let command = remote_hook_command("Stop", HOOK_TOKEN, &worktree.log_path());

    // When
    run_hook(&command, payload);

    // Then
    assert_eq!(worktree.log(), format!("Stop\t{HOOK_TOKEN}\t{payload}\n"));
}

#[test]
fn a_token_the_shell_would_expand_is_logged_as_it_is() {
    // Given a token holding a variable reference, a quote and a command separator
    let worktree = a_worktree();
    let token = "t$HOME'; echo pwned";
    let payload = r#"{"hook_event_name":"Stop"}"#;
    let command = remote_hook_command("Stop", token, &worktree.log_path());

    // When
    run_hook(&command, payload);

    // Then
    assert_eq!(worktree.log(), format!("Stop\t{token}\t{payload}\n"));
}

// -----------------------------------------------------------------------------------------
// The waiting command (`#target-parity` 1/7)
// -----------------------------------------------------------------------------------------

const PERMISSION_PAYLOAD: &str = r#"{"hook_event_name":"PermissionRequest","tool_name":"Read","tool_input":{"file_path":"README.md"}}"#;
const ALLOW: &str = r#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}"#;

impl AWorktree {
    fn answers_dir(&self) -> String {
        self.root
            .join(REMOTE_ANSWERS_DIR)
            .to_string_lossy()
            .into_owned()
    }

    /// The nonce the waiting hook wrote in its line (`<event>@<nonce>`), once it has written it.
    fn nonce_once_logged(&self, event: &str) -> String {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if let Some(nonce) = self.log_lines().iter().find_map(|line| {
                line.split('\t')
                    .next()
                    .and_then(|first| first.strip_prefix(&format!("{event}@")))
                    .map(str::to_string)
            }) {
                return nonce;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the waiting hook never logged its line: {:?}",
                self.log()
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    /// The daemon writes `decision` as the answer for `nonce`.
    fn answers(&self, nonce: &str, decision: &str) {
        let dir = self.root.join(REMOTE_ANSWERS_DIR);
        std::fs::create_dir_all(&dir).expect("the answers directory");
        std::fs::write(dir.join(nonce), decision).expect("the answer file");
    }
}

/// A waiting hook fired with its stdout captured.
fn fire_capturing(command: &str, path: Option<&Path>) -> Child {
    let mut shell = Command::new("/bin/sh");
    shell.arg("-c").arg(command);
    if let Some(path) = path {
        shell.env_clear().env("PATH", path);
    }
    shell
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("a shell runs on this host")
}

/// Hand `child` its payload and close it.
fn send_payload(child: &mut Child, payload: &str) {
    let mut stdin = child.stdin.take().expect("the hook's stdin is piped");
    stdin
        .write_all(payload.as_bytes())
        .expect("the hook reads its payload");
}

/// What the hook printed, and whether it exited zero, once it ends.
fn printed_once_it_ends(child: Child) -> (String, bool) {
    let output = child.wait_with_output().expect("the hook ends");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.success(),
    )
}

#[test]
fn the_waiting_command_prints_the_answer_written_for_its_nonce() {
    // Given a waiting permission hook that has logged its line
    let worktree = a_worktree();
    let command = remote_answering_hook_command(
        "PermissionRequest",
        HOOK_TOKEN,
        &worktree.log_path(),
        &worktree.answers_dir(),
        30,
    );
    let mut hook = fire_capturing(&command, None);
    send_payload(&mut hook, PERMISSION_PAYLOAD);
    let nonce = worktree.nonce_once_logged("PermissionRequest");

    // When the daemon answers it
    worktree.answers(&nonce, ALLOW);

    // Then the hook prints the decision, and the line it logged is the event, its token and
    // its payload
    assert_eq!(
        (printed_once_it_ends(hook), worktree.log_lines()),
        (
            (ALLOW.to_string(), true),
            vec![format!(
                "PermissionRequest@{nonce}\t{HOOK_TOKEN}\t{PERMISSION_PAYLOAD}"
            )]
        )
    );
}

#[test]
fn an_empty_answer_ends_the_waiting_command_at_once_with_no_output() {
    // Given a waiting hook with a long wait
    let worktree = a_worktree();
    let command = remote_answering_hook_command(
        "PermissionRequest",
        HOOK_TOKEN,
        &worktree.log_path(),
        &worktree.answers_dir(),
        300,
    );
    let mut hook = fire_capturing(&command, None);
    send_payload(&mut hook, PERMISSION_PAYLOAD);
    let nonce = worktree.nonce_once_logged("PermissionRequest");
    let started = std::time::Instant::now();

    // When the daemon has no decision to give
    worktree.answers(&nonce, "");

    // Then the hook ends with no output, long before its wait is over
    let printed = printed_once_it_ends(hook);
    assert_eq!(
        (
            printed,
            started.elapsed() < std::time::Duration::from_secs(10)
        ),
        ((String::new(), true), true)
    );
}

#[test]
fn the_waiting_command_gives_up_silently_when_no_answer_comes() {
    // Given a waiting hook that waits one second
    let worktree = a_worktree();
    let command = remote_answering_hook_command(
        "PermissionRequest",
        HOOK_TOKEN,
        &worktree.log_path(),
        &worktree.answers_dir(),
        1,
    );

    // When nobody answers
    let mut hook = fire_capturing(&command, None);
    send_payload(&mut hook, PERMISSION_PAYLOAD);

    // Then it ends with no output and status zero, and leaves no answer behind
    assert_eq!(
        (
            printed_once_it_ends(hook),
            worktree
                .root
                .join(REMOTE_ANSWERS_DIR)
                .read_dir()
                .map(|d| d.count())
                .unwrap_or(0)
        ),
        ((String::new(), true), 0)
    );
}

#[test]
fn the_waiting_command_names_no_tddy_binary() {
    // Given a target that has a shell and a few POSIX tools, and nothing of tddy's
    let worktree = a_worktree();
    let bin = worktree.root.join("bare-bin");
    std::fs::create_dir_all(&bin).expect("a directory for the programs");
    for program in ["cat", "tr", "date", "sleep", "rm"] {
        std::os::unix::fs::symlink(program_on_this_host(program), bin.join(program))
            .expect("the program is linked");
    }
    let command = remote_answering_hook_command(
        "PermissionRequest",
        HOOK_TOKEN,
        &worktree.log_path(),
        &worktree.answers_dir(),
        30,
    );

    // When the hook runs there and is answered
    let mut hook = fire_capturing(&command, Some(&bin));
    send_payload(&mut hook, PERMISSION_PAYLOAD);
    let nonce = worktree.nonce_once_logged("PermissionRequest");
    worktree.answers(&nonce, ALLOW);

    // Then it prints the answer, and names neither the tools binary nor its subcommand
    assert_eq!(
        (
            printed_once_it_ends(hook),
            command.contains("tddy-tools"),
            command.contains("session-hook")
        ),
        ((ALLOW.to_string(), true), false, false)
    );
}
