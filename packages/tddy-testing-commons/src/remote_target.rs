//! Fixtures the target-session tests (ssh and Coder targets, stand-in agents) share.
//!
//! Every test that stands a script in for `ssh`, `coder` or an agent writes it with
//! [`write_executable`], so the first-execution scan is paid once, here, and never inside a
//! window a test measures.

use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// Writes `script` to `path`, makes it executable, and runs it once, guarded, to warm it up.
///
/// A freshly written executable pays a first-execution scan on this OS (~2.4 s measured on macOS,
/// against ~20 ms afterwards): a stand-in or stub written here and then timed from its first real
/// exec would spend that scan inside whatever window the test measures. So a `#!/bin/sh` script
/// carries a warm-up guard as its first line, and is run once here with `TDDY_FIXTURE_WARMUP` set
/// -- no runs are recorded, no pid files written -- before any launch is timed. A script that
/// does not start with `#!/bin/sh` is written as given.
pub fn write_executable(path: &Path, script: &str) {
    let script = match script.strip_prefix("#!/bin/sh\n") {
        Some(body) => {
            format!("#!/bin/sh\nif [ -n \"$TDDY_FIXTURE_WARMUP\" ]; then exit 0; fi\n{body}")
        }
        None => script.to_string(),
    };
    std::fs::write(path, script).expect("the script is written");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .expect("the script is made executable");
    let warmed = std::process::Command::new(path)
        .env("TDDY_FIXTURE_WARMUP", "1")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("the freshly written script warms up");
    assert!(
        warmed.status.success(),
        "the warm-up of {} exited {}: {}",
        path.display(),
        warmed.status,
        String::from_utf8_lossy(&warmed.stderr)
    );
}

/// The login shell of a stand-in machine: `$SHELL -lc <command>` runs the command under the `PATH`
/// the stand-in set, whose first directory is the machine's own `opt/agents` -- where its agent is
/// installed, as a target start runs the agent's bare command, never this host's path to it.
///
/// `$SHELL -l` -- a person's login shell, as a shell tab on the target runs it -- records the
/// directory it started in beside itself ([`LOGIN_SHELL_STARTED_IN`]) and becomes an interactive
/// `/bin/sh` on its terminal.
const A_TARGET_LOGIN_SHELL: &str = "#!/bin/sh\n\
     if [ \"$1\" = -l ]; then here=\"$(dirname \"$0\")\"; /bin/pwd -P > \"$here/login-shell-started-in.part\" && mv \"$here/login-shell-started-in.part\" \"$here/login-shell-started-in\"; exec /bin/sh; fi\n\
     [ \"$1\" = -lc ] || exit 64\nexec /bin/sh -c \"$2\"\n";

/// The file beside a stand-in machine's login shell that its `-l` run records the directory it
/// started in.
pub const LOGIN_SHELL_STARTED_IN: &str = "login-shell-started-in";

/// The machine behind an ssh target, for as long as it lives: a stand-in `ssh` first on this
/// process's `PATH`, and the directory standing for the machine's filesystem.
///
/// Dropping it restores `PATH`. `PATH` is process-wide, so a test file holding one must keep its
/// tests from overlapping (a serial guard, or a single test).
pub struct StandInSshTarget {
    /// The directory standing for the machine's filesystem (canonical).
    pub root: PathBuf,
    previous_path: Option<OsString>,
    _dir: tempfile::TempDir,
}

impl StandInSshTarget {
    /// A machine that runs what it is asked to. The stand-in runs its last argument -- the remote
    /// command -- with `sh -c` as a child whose terminal is its own, every `/home/dev` path in it
    /// mapped under the machine's directory. It hangs the command up when it is itself hung up or
    /// terminated, and exits with the command's status. Its `$SHELL` is a login shell of the
    /// machine's own, and its `PATH` starts with [`Self::agents_dir`], where a test installs the
    /// agent a start runs by its bare command.
    pub fn behind_ssh() -> Self {
        let dir = tempfile::tempdir().expect("a directory standing for the machine");
        let root = std::fs::canonicalize(dir.path()).expect("the machine's directory resolves");
        let bin = root.join("bin");
        std::fs::create_dir_all(&bin).expect("a directory for the stand-in ssh");
        write_executable(&bin.join("login-shell"), A_TARGET_LOGIN_SHELL);
        std::fs::create_dir_all(root.join("opt/agents")).expect("the machine's agents directory");
        write_executable(
            &bin.join("ssh"),
            &format!(
                r#"#!/bin/sh
for command; do :; done
command=$(printf '%s' "$command" | /usr/bin/sed 's#/home/dev#{root}/home/dev#g')
export HOME='{root}'
SHELL='{root}/bin/login-shell'
PATH='{root}/opt/agents':"$PATH"
export SHELL PATH
exec 3<&0
/bin/sh -c "$command" <&3 3<&- &
remote=$!
trap 'kill -HUP "$remote" 2>/dev/null' HUP TERM
wait "$remote"
exit $?
"#,
                root = root.display()
            ),
        );
        let previous_path = std::env::var_os("PATH");
        let mut path = OsString::from(&bin);
        if let Some(previous) = &previous_path {
            path.push(":");
            path.push(previous);
        }
        std::env::set_var("PATH", path);
        Self {
            root,
            previous_path,
            _dir: dir,
        }
    }

    /// The directory first on the machine's `PATH`: an agent installed here is what a start's bare
    /// agent command runs.
    pub fn agents_dir(&self) -> PathBuf {
        self.root.join("opt/agents")
    }

    /// Where the machine keeps the worktree of `session_id`: under the checkout resolved from the
    /// target user's home -- the stub's `$HOME` is this directory, and the project's empty git URL
    /// names `repo` under `.tddy/repos`.
    pub fn worktree_of(&self, session_id: &str) -> PathBuf {
        self.root
            .join(".tddy/repos/repo/.worktrees")
            .join(session_id)
    }

    /// Where the pointer of `session_id` lives on the machine.
    pub fn pointer_of(&self, session_id: &str) -> PathBuf {
        self.worktree_of(session_id).join(".tddy/session.json")
    }

    /// Where the machine's login shell, run as a person's login shell (`-l`), records the directory
    /// it started in.
    pub fn login_shell_started_in(&self) -> PathBuf {
        self.root.join("bin").join(LOGIN_SHELL_STARTED_IN)
    }

    /// Where the hook log of `session_id` lives on the machine.
    pub fn hook_log_of(&self, session_id: &str) -> PathBuf {
        self.worktree_of(session_id)
            .join(tddy_core::REMOTE_HOOK_LOG)
    }
}

impl Drop for StandInSshTarget {
    fn drop(&mut self) {
        match &self.previous_path {
            Some(path) => std::env::set_var("PATH", path),
            None => std::env::remove_var("PATH"),
        }
    }
}
