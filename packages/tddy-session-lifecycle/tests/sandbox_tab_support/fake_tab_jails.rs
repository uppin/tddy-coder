//! An in-memory [`TabJailSpawn`]: each "jail" is a `sleep` standing in for its runner, whose
//! terminal has already printed [`jailed_shell_banner`]. It records every launch it is asked for,
//! and can be told to refuse every one.
//!
//! What the daemon owns of a tab jail is its lifetime and its terminal — which a real process and a
//! real capture show — not the kernel sandbox, which `sandbox_shell_tab_jail_linux` covers.

use std::collections::{BTreeMap, BTreeSet};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex as StdMutex};

use async_trait::async_trait;
use bytes::Bytes;
use tddy_daemon_sandbox::sandbox_session::{SandboxSessionState, SandboxSessionStateInit};
use tddy_daemon_sandbox::tab_jails::{TabJailLaunch, TabJailSpawn};
use tddy_rpc::Status;
use tddy_task::TerminalCapture;
use tokio::sync::{broadcast, mpsc};

/// What a fake jail's shell has printed by the time it is handed back: names its tab, so a stream
/// served from the wrong jail shows it.
pub fn jailed_shell_banner(terminal_id: &str) -> String {
    format!("jailed shell of {terminal_id} $ ")
}

/// The double: jails started, by terminal id, and every launch asked for, in order.
pub struct FakeTabJailSpawn {
    launches: StdMutex<Vec<TabJailLaunch>>,
    jails: StdMutex<BTreeMap<String, Arc<SandboxSessionState>>>,
    refusal: Option<String>,
    dir: tempfile::TempDir,
}

/// A spawn that starts every jail it is asked for.
pub fn a_tab_jail_spawn() -> Arc<FakeTabJailSpawn> {
    Arc::new(FakeTabJailSpawn {
        launches: StdMutex::new(Vec::new()),
        jails: StdMutex::new(BTreeMap::new()),
        refusal: None,
        dir: tempfile::tempdir().unwrap(),
    })
}

/// A spawn that refuses every jail with `FAILED_PRECONDITION` and `reason`, as a session whose
/// grants cannot be lowered is refused.
pub fn a_tab_jail_spawn_refusing(reason: &str) -> Arc<FakeTabJailSpawn> {
    Arc::new(FakeTabJailSpawn {
        launches: StdMutex::new(Vec::new()),
        jails: StdMutex::new(BTreeMap::new()),
        refusal: Some(reason.to_string()),
        dir: tempfile::tempdir().unwrap(),
    })
}

impl FakeTabJailSpawn {
    /// Every launch asked for, oldest first — refused ones included.
    pub fn launches(&self) -> Vec<TabJailLaunch> {
        self.launches.lock().unwrap().clone()
    }

    /// The terminal ids every launch was asked for, oldest first.
    pub fn launched_terminals(&self) -> Vec<String> {
        self.launches()
            .into_iter()
            .map(|launch| launch.terminal_id)
            .collect()
    }

    /// The launch asked for `terminal_id`'s jail.
    pub fn launch_of(&self, terminal_id: &str) -> TabJailLaunch {
        self.launches()
            .into_iter()
            .find(|launch| launch.terminal_id == terminal_id)
            .unwrap_or_else(|| panic!("no jail was asked for terminal {terminal_id}"))
    }

    /// Whether the jail started for `terminal_id` still runs.
    pub fn jail_runs(&self, terminal_id: &str) -> bool {
        self.jail_of(terminal_id).is_running()
    }

    /// The terminal ids whose jail still runs.
    pub fn running_jails(&self) -> BTreeSet<String> {
        self.jails
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, jail)| jail.is_running())
            .map(|(terminal_id, _)| terminal_id.clone())
            .collect()
    }

    /// The runner of `terminal_id`'s jail exits on its own, as an `exit`ed or crashed jail does.
    pub fn the_jail_exits(&self, terminal_id: &str) {
        let pid = self
            .jail_of(terminal_id)
            .runner_pid()
            .unwrap_or_else(|| panic!("the jail of {terminal_id} is not running"))
            as libc::pid_t;
        // SAFETY: `pid` is the stand-in runner this double spawned, still running and unreaped.
        unsafe { libc::kill(pid, libc::SIGKILL) };
    }

    fn jail_of(&self, terminal_id: &str) -> Arc<SandboxSessionState> {
        self.jails
            .lock()
            .unwrap()
            .get(terminal_id)
            .cloned()
            .unwrap_or_else(|| panic!("no jail was started for terminal {terminal_id}"))
    }

    fn a_stand_in_jail(&self, launch: &TabJailLaunch) -> SandboxSessionState {
        let child = Command::new("sleep")
            .arg("300")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the stand-in runner starts");
        let pid = child.id();
        let handle = tddy_sandbox::SandboxHandle::new(
            child,
            self.dir.path().join("profile.sb"),
            self.dir.path().join("unused.sock"),
            self.dir.path().join("sandbox.ready"),
        );
        let banner = jailed_shell_banner(&launch.terminal_id);
        let capture = Arc::new(StdMutex::new(TerminalCapture::new()));
        capture.lock().unwrap().append(banner.as_bytes());
        let (stdout_tx, _) = broadcast::channel(64);
        let _ = stdout_tx.send(Bytes::from(banner));
        let (stdin_tx, _) = mpsc::unbounded_channel();
        SandboxSessionState::new(SandboxSessionStateInit {
            pid,
            worktree_path: launch.worktree.clone(),
            stdout_tx,
            capture,
            stdin_tx,
            ready_marker: launch.paths.ready_marker.clone(),
            handle,
            managed_workflow: None,
        })
    }
}

#[async_trait]
impl TabJailSpawn for FakeTabJailSpawn {
    async fn spawn_shell(
        &self,
        launch: &TabJailLaunch,
    ) -> Result<Arc<SandboxSessionState>, Status> {
        self.launches.lock().unwrap().push(launch.clone());
        if let Some(reason) = &self.refusal {
            return Err(Status::failed_precondition(reason.clone()));
        }
        let jail = Arc::new(self.a_stand_in_jail(launch));
        self.jails
            .lock()
            .unwrap()
            .insert(launch.terminal_id.clone(), Arc::clone(&jail));
        Ok(jail)
    }
}
