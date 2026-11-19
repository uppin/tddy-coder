//! OpenSSH as a remote-shell transport: the `ssh` client on this host, so `~/.ssh/config` and
//! ssh-agent apply. `BatchMode=yes` always — an unloaded key is a hard failure, never a prompt.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use async_trait::async_trait;
use tokio::process::Command;

use crate::{
    ExecOutput, ExecRequest, InteractiveCommand, InteractiveExit, RemoteShell, RemoteShellError,
    ShellAddress, StreamedExec,
};

/// A shell on the ssh `Host` alias `alias`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshShell {
    ssh: PathBuf,
    alias: String,
}

impl SshShell {
    /// The tag of this transport in a [`ShellAddress`].
    pub const TRANSPORT: &'static str = "ssh";

    /// The shell on `alias`, reached through the `ssh` on `PATH`.
    pub fn new(alias: impl Into<String>) -> Self {
        Self::through("ssh", alias)
    }

    /// [`Self::new`], reaching the target through the ssh client `ssh_binary`.
    pub fn through(ssh_binary: impl Into<PathBuf>, alias: impl Into<String>) -> Self {
        Self {
            ssh: ssh_binary.into(),
            alias: alias.into(),
        }
    }

    /// The ssh client this shell runs.
    pub fn ssh_binary(&self) -> &Path {
        &self.ssh
    }

    /// The `Host` alias this shell reaches.
    pub fn alias(&self) -> &str {
        &self.alias
    }

    /// The connection options every ssh this crate starts carries, as `-o <option>` pairs: no
    /// prompt (`BatchMode=yes`), and a connection that cannot hang (`ConnectTimeout=15`,
    /// `ServerAliveInterval=5`, `ServerAliveCountMax=3`).
    pub fn ssh_options() -> Vec<String> {
        [
            "BatchMode=yes",
            "ConnectTimeout=15",
            "ServerAliveInterval=5",
            "ServerAliveCountMax=3",
        ]
        .into_iter()
        .flat_map(|option| ["-o".to_string(), option.to_string()])
        .collect()
    }

    fn command(&self, remote_command: &str) -> Command {
        let mut command = Command::new(&self.ssh);
        command
            .args(Self::ssh_options())
            .arg("--")
            .arg(&self.alias)
            .arg(remote_command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        command
    }
}

#[async_trait]
impl RemoteShell for SshShell {
    fn address(&self) -> ShellAddress {
        ShellAddress::new(Self::TRANSPORT, self.alias.clone())
    }

    /// `ssh <options> -- <alias> <command>`. OpenSSH's own exit status 255 is
    /// [`RemoteShellError::Unreachable`] with what it wrote to stderr.
    async fn exec(&self, request: ExecRequest) -> Result<ExecOutput, RemoteShellError> {
        let output = crate::child::run_to_end(
            self.command(&request.command),
            &self.ssh,
            request.stdin,
            request.deadline,
            self.address(),
            &request.command,
        )
        .await?;
        if output.code == 255 {
            return Err(RemoteShellError::Unreachable {
                address: self.address(),
                detail: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }
        Ok(ExecOutput {
            status: output.code,
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }

    async fn exec_streamed(&self, command: &str) -> Result<StreamedExec, RemoteShellError> {
        crate::child::stream(self.command(command), &self.ssh)
    }

    /// `ssh -tt <options> -- <alias> <command>`.
    fn interactive(&self, command: &str) -> InteractiveCommand {
        let mut argv: Vec<OsString> = vec![self.ssh.as_os_str().to_os_string(), "-tt".into()];
        argv.extend(Self::ssh_options().into_iter().map(OsString::from));
        argv.push("--".into());
        argv.push(self.alias.clone().into());
        argv.push(command.into());
        InteractiveCommand {
            argv,
            env: Vec::new(),
            exit: InteractiveExit::ClientStatus,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn the_connection_options_never_prompt_and_cannot_hang() {
        // Given / When
        let options = SshShell::ssh_options();

        // Then
        assert_eq!(
            options,
            [
                "-o",
                "BatchMode=yes",
                "-o",
                "ConnectTimeout=15",
                "-o",
                "ServerAliveInterval=5",
                "-o",
                "ServerAliveCountMax=3",
            ]
        );
    }

    #[test]
    fn an_ssh_shell_is_addressed_by_its_alias() {
        // Given
        let shell = SshShell::through("/opt/bin/ssh", "buildbox");

        // When
        let address = shell.address();

        // Then
        assert_eq!(address, ShellAddress::new("ssh", "buildbox"));
        assert_eq!(shell.ssh_binary(), Path::new("/opt/bin/ssh"));
        assert_eq!(shell.alias(), "buildbox");
    }
}
