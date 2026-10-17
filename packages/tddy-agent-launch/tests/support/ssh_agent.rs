//! A real `ssh-agent` on a private socket, so "the key landed in the agent" is `ssh-add -l`'s word.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use tddy_testing_commons::wait::eventually_blocking;

pub struct AgentFixture {
    _dir: tempfile::TempDir,
    socket: PathBuf,
    child: Child,
}

pub fn a_running_ssh_agent() -> AgentFixture {
    let dir = tempfile::tempdir().expect("a directory for the agent socket");
    let socket = dir.path().join("agent.sock");
    let child = Command::new("ssh-agent")
        .args(["-D", "-a"])
        .arg(&socket)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("ssh-agent starts");
    eventually_blocking(
        "the ssh-agent socket to appear",
        Duration::from_secs(10),
        || {
            socket
                .exists()
                .then_some(())
                .ok_or_else(|| format!("{} not there yet", socket.display()))
        },
    );
    AgentFixture {
        _dir: dir,
        socket,
        child,
    }
}

impl AgentFixture {
    pub fn socket(&self) -> &Path {
        &self.socket
    }

    /// The fingerprints `ssh-add -l` lists. Empty for an agent holding nothing.
    pub fn fingerprints(&self) -> Vec<String> {
        let out = Command::new("ssh-add")
            .arg("-l")
            .env("SSH_AUTH_SOCK", &self.socket)
            .output()
            .expect("ssh-add runs");
        String::from_utf8(out.stdout)
            .expect("UTF-8")
            .lines()
            .filter_map(|line| line.split_whitespace().nth(1).map(str::to_string))
            .filter(|column| column.starts_with("SHA256:"))
            .collect()
    }
}

impl Drop for AgentFixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
