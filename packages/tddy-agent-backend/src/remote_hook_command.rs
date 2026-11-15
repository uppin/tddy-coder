//! The hook command of a session whose agent runs on a **target**: a machine the daemon reaches
//! through a remote shell, with no tddy software on it.
//!
//! A local hook runs `tddy-tools session-hook`, which needs that binary and a route to the daemon.
//! A target has neither. There the hook is a shell one-liner that appends one line to a log file
//! in the session's worktree, and the daemon follows that file through the remote shell
//! (`tddy_agent_launch::remote_hook_follower`). One line per hook, three tab-separated fields:
//!
//! ```text
//! <event>\t<hook token>\t<the payload, its newlines removed>\n
//! ```
//!
//! The carrier is a property of the command, not of the agent: every hooks builder takes its
//! command from the same [`HookCommandParams`](crate::claude_hooks::HookCommandParams), whose
//! `remote_log` selects this command.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-15-remote-shell-remote-hooks.md.

/// Where a target session's hook log lives, relative to the session's worktree on the target.
pub const REMOTE_HOOK_LOG: &str = ".tddy/hooks.log";

/// The shell command an agent on a target runs for the hook `event`: it reads the hook's payload
/// on its standard input and appends `<event>\t<hook_token>\t<payload>\n` to `log_path`, the
/// payload's newlines removed.
///
/// It names nothing but `printf`, `cat` and `tr`, so it runs under a bare POSIX `sh`. The line is
/// written once the whole payload has been read, in one piece: two hooks that fire together each
/// leave a whole line. `event`, `hook_token` and `log_path` are single-quoted for the shell. The
/// log's directory must exist; the command creates the file.
#[must_use]
pub fn remote_hook_command(event: &str, hook_token: &str, log_path: &str) -> String {
    // TODO(remote-hooks): implement
    let _ = (event, hook_token, log_path);
    String::new()
}

#[cfg(all(test, unix))]
mod tests {
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
}
