//! Unit tests: the remote command of a shell tab on a target ([`target_shell_tab_command`]) enters
//! the worktree and becomes the target's login shell, writing nothing there first.

use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_testing_commons::remote_target::write_executable;

use super::target_shell_tab_command;

/// A directory standing for the target, holding `worktree_name`, and a login shell of the target's
/// own that records the directory it was started in and the arguments it was given, then exits.
struct ATarget {
    worktree: PathBuf,
    login_shell: PathBuf,
    record: PathBuf,
    _dir: tempfile::TempDir,
}

fn a_target_holding(worktree_name: &str) -> ATarget {
    let dir = tempfile::tempdir().expect("a directory standing for the target");
    let root = std::fs::canonicalize(dir.path()).expect("the target's directory resolves");
    let worktree = root.join("home/dev/repo/.worktrees").join(worktree_name);
    std::fs::create_dir_all(&worktree).expect("the worktree on the target");
    let record = root.join("login-shell-run");
    let login_shell = root.join("login-shell");
    write_executable(
        &login_shell,
        &format!(
            "#!/bin/sh\n{{ /bin/pwd -P; printf '%s\\n' \"$@\"; }} > '{}'\n",
            record.display()
        ),
    );
    ATarget {
        worktree,
        login_shell,
        record,
        _dir: dir,
    }
}

impl ATarget {
    /// Run `command` as the target's remote shell runs a terminal's command: `/bin/sh -c`, under a
    /// login whose `$SHELL` is the target's own.
    fn runs(&self, command: &str) {
        let status = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(command)
            .env_clear()
            .env("SHELL", &self.login_shell)
            .env("PATH", "/usr/bin:/bin")
            .status()
            .expect("a shell runs on this host");
        assert!(status.success(), "the tab's command exited {status}");
    }

    /// The directory the login shell started in, and the arguments it was given.
    fn login_shell_run(&self) -> (PathBuf, Vec<String>) {
        let recorded = std::fs::read_to_string(&self.record).expect("the target's login shell ran");
        let mut lines = recorded.lines();
        let cwd = PathBuf::from(
            lines
                .next()
                .expect("the login shell recorded its directory"),
        );
        (cwd, lines.map(str::to_string).collect())
    }

    /// Everything inside the worktree on the target.
    fn worktree_entries(&self) -> Vec<PathBuf> {
        std::fs::read_dir(&self.worktree)
            .expect("the worktree reads")
            .map(|entry| entry.expect("a worktree entry").path())
            .collect()
    }
}

#[test]
fn the_tab_command_enters_the_worktree_and_execs_the_login_shell() {
    // Given
    let worktree = Path::new("/home/dev/repo/.worktrees/s1");

    // When
    let command = target_shell_tab_command(worktree);

    // Then
    assert_eq!(
        command,
        r#"cd '/home/dev/repo/.worktrees/s1' && exec "${SHELL:-/bin/sh}" -l"#
    );
}

#[test]
fn the_tab_runs_the_targets_login_shell_as_a_login_in_the_worktree() {
    // Given
    let target = a_target_holding("s1");

    // When the tab's command is run on the target
    target.runs(&target_shell_tab_command(&target.worktree));

    // Then the target's own shell ran, as a login shell, in the worktree
    assert_eq!(
        target.login_shell_run(),
        (target.worktree.clone(), vec!["-l".to_string()])
    );
}

#[test]
fn the_tab_command_quotes_a_worktree_path_holding_a_space_and_a_quote() {
    // Given a worktree whose name holds a space and a single quote
    let target = a_target_holding("ada's repo");

    // When the tab's command is run on the target
    target.runs(&target_shell_tab_command(&target.worktree));

    // Then the login shell started in that worktree
    assert_eq!(target.login_shell_run().0, target.worktree);
}

#[test]
fn the_tab_command_writes_nothing_in_the_worktree() {
    // Given
    let target = a_target_holding("s1");

    // When the tab's command is run on the target
    target.runs(&target_shell_tab_command(&target.worktree));

    // Then no pid file, pointer or directory of the session's main was written there
    assert_eq!(target.worktree_entries(), Vec::<PathBuf>::new());
}
