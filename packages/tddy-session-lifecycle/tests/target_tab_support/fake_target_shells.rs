//! An in-memory [`TabTargetShell`] and the machine it reaches: a directory on this host standing for
//! the target, holding the session's worktree there, whose shell runs commands with this machine's
//! `/bin/sh` under a login shell of the target's own. Run as `-l` — as a shell tab runs it — that
//! login shell records the directory it started in, prints [`ON_TARGET_BANNER`] and stays up. Run as
//! `-lc <command>` — as an agent tab's launch runs it — it runs the command with the target's own
//! agents directory first on its `PATH`, where [`AStandInTarget::installs_the_agent`] puts an agent
//! (`#session-liveness` 8/8).
//!
//! What the daemon owns of a target tab is which target it asks for, the command it runs there and
//! the tab's lifetime and record — which a real local process shows — not the transport, which the
//! `tddy-agent-launch` suite covers over the stand-in `ssh` and `coder`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};

use async_trait::async_trait;
use tddy_agent_launch::tab_target_shell::{TabTarget, TabTargetShell};
use tddy_remote_shell::{
    ExecOutput, ExecRequest, InteractiveCommand, InteractiveExit, LocalShell, RemoteShell,
    RemoteShellError, ShellAddress, StreamedExec,
};
use tddy_rpc::Status;
use tddy_testing_commons::remote_target::write_executable;

/// What the target's login shell prints once it has started.
pub const ON_TARGET_BANNER: &str = "on-target shell $ ";

/// The name the stand-in target goes by.
pub const TARGET_NAME: &str = "buildbox";

/// The machine a session's target shell reaches.
pub struct AStandInTarget {
    root: PathBuf,
    reachable: bool,
    interactive: StdMutex<Vec<String>>,
    _dir: tempfile::TempDir,
}

impl AStandInTarget {
    fn new(reachable: bool) -> Arc<Self> {
        let dir = tempfile::tempdir().expect("a directory standing for the target");
        let root = std::fs::canonicalize(dir.path()).expect("the target's directory resolves");
        std::fs::create_dir_all(root.join("home/dev/repo/.worktrees/target-tab"))
            .expect("the session's worktree on the target");
        std::fs::create_dir_all(root.join("opt/agents")).expect("the target's agents directory");
        write_executable(
            &root.join("login-shell"),
            &format!(
                "#!/bin/sh\n\
                 if [ \"$1\" = -lc ]; then PATH='{agents}':\"$PATH\"; export PATH; exec /bin/sh -c \"$2\"; fi\n\
                 [ \"$1\" = -l ] || exit 64\n/bin/pwd -P > '{started_in}.part' && mv '{started_in}.part' '{started_in}'\n\
                 printf '{ON_TARGET_BANNER}'\nexec /bin/sh\n",
                started_in = root.join("login-shell-started-in").display(),
                agents = root.join("opt/agents").display()
            ),
        );
        Arc::new(Self {
            root,
            reachable,
            interactive: StdMutex::new(Vec::new()),
            _dir: dir,
        })
    }

    /// The session's worktree on the target.
    pub fn worktree(&self) -> PathBuf {
        self.root.join("home/dev/repo/.worktrees/target-tab")
    }

    /// Where the target's login shell started, once it has.
    pub fn login_shell_started_in(&self) -> Option<PathBuf> {
        std::fs::read_to_string(self.root.join("login-shell-started-in"))
            .ok()
            .map(|cwd| PathBuf::from(cwd.trim_end()))
    }

    /// Install the agent `command` on the target, first on its login shell's `PATH`: run, it records
    /// the directory it started in and its arguments, then stays up on its terminal.
    pub fn installs_the_agent(&self, command: &str) {
        write_executable(
            &self.root.join("opt/agents").join(command),
            &format!(
                "#!/bin/sh\n{{ /bin/pwd -P; printf '%s\\n' \"$@\"; }} > '{record}.part'\n\
                 /bin/mv '{record}.part' '{record}'\nexec /bin/sh\n",
                record = self.agent_record(command).display()
            ),
        );
    }

    /// The directory the target's agent `command` started in and the arguments it was given, once it
    /// has run.
    pub fn agent_run(&self, command: &str) -> Option<(PathBuf, Vec<String>)> {
        let recorded = std::fs::read_to_string(self.agent_record(command)).ok()?;
        let mut lines = recorded.lines();
        let cwd = PathBuf::from(lines.next()?);
        Some((cwd, lines.map(str::to_string).collect()))
    }

    fn agent_record(&self, command: &str) -> PathBuf {
        self.root.join(format!("{command}-ran-on-the-target"))
    }

    /// Every command a terminal was opened on, oldest first.
    pub fn interactive_commands(&self) -> Vec<String> {
        self.interactive.lock().unwrap().clone()
    }

    fn unreachable(&self) -> RemoteShellError {
        RemoteShellError::Unreachable {
            address: self.address(),
            detail: UNREACHABLE.to_string(),
        }
    }
}

/// What the transport of an unreachable target says.
pub const UNREACHABLE: &str = "Could not resolve hostname buildbox: Name or service not known";

#[async_trait]
impl RemoteShell for AStandInTarget {
    fn address(&self) -> ShellAddress {
        ShellAddress::new("ssh", TARGET_NAME)
    }

    async fn exec(&self, request: ExecRequest) -> Result<ExecOutput, RemoteShellError> {
        if !self.reachable {
            return Err(self.unreachable());
        }
        LocalShell::new().exec(request).await
    }

    async fn exec_streamed(&self, command: &str) -> Result<StreamedExec, RemoteShellError> {
        if !self.reachable {
            return Err(self.unreachable());
        }
        LocalShell::new().exec_streamed(command).await
    }

    /// `/bin/sh -c <command>` under the target's own login shell.
    fn interactive(&self, command: &str) -> InteractiveCommand {
        self.interactive.lock().unwrap().push(command.to_string());
        InteractiveCommand {
            argv: vec![
                OsString::from("/bin/sh"),
                OsString::from("-c"),
                OsString::from(command),
            ],
            env: vec![(
                OsString::from("SHELL"),
                self.root.join("login-shell").into_os_string(),
            )],
            exit: InteractiveExit::ClientStatus,
        }
    }
}

/// How the double answers.
enum Answer {
    Reaches(Arc<AStandInTarget>),
    Refuses(Status),
}

/// The double: answers every session's target with one fixed outcome, and records who asked for
/// which session's.
pub struct FakeTabTargetShells {
    answer: Answer,
    asked: StdMutex<Vec<(PathBuf, String)>>,
}

/// A port whose every session's target is a reachable stand-in machine.
pub fn target_shells_reaching_a_machine() -> (Arc<FakeTabTargetShells>, Arc<AStandInTarget>) {
    let machine = AStandInTarget::new(true);
    (
        FakeTabTargetShells::answering(Answer::Reaches(Arc::clone(&machine))),
        machine,
    )
}

/// A port whose every session's target is a machine its transport cannot reach.
pub fn target_shells_reaching_an_unreachable_machine(
) -> (Arc<FakeTabTargetShells>, Arc<AStandInTarget>) {
    let machine = AStandInTarget::new(false);
    (
        FakeTabTargetShells::answering(Answer::Reaches(Arc::clone(&machine))),
        machine,
    )
}

/// A port refusing every session's target with `status` — as a locked vault is refused.
pub fn target_shells_refusing(status: Status) -> Arc<FakeTabTargetShells> {
    FakeTabTargetShells::answering(Answer::Refuses(status))
}

impl FakeTabTargetShells {
    fn answering(answer: Answer) -> Arc<Self> {
        Arc::new(Self {
            answer,
            asked: StdMutex::new(Vec::new()),
        })
    }

    /// Which session directory's target was asked for, as whom (the caller's subject), oldest first.
    pub fn asked(&self) -> Vec<(PathBuf, String)> {
        self.asked.lock().unwrap().clone()
    }
}

impl TabTargetShell for FakeTabTargetShells {
    fn target_of(&self, session_dir: &Path, subject: &str) -> Result<TabTarget, Status> {
        self.asked
            .lock()
            .unwrap()
            .push((session_dir.to_path_buf(), subject.to_string()));
        match &self.answer {
            Answer::Reaches(machine) => Ok(TabTarget {
                worktree: machine.worktree(),
                shell: Arc::clone(machine) as Arc<dyn RemoteShell>,
            }),
            Answer::Refuses(status) => Err(status.clone()),
        }
    }
}
