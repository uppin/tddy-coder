//! This machine as a remote-shell target: `/bin/sh -c <command>` as a local child, for a caller
//! that runs a local session through the same port as a remote one — the session driver follows a
//! local session's hook log with it, exactly as it follows a target's.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Stdio;

use async_trait::async_trait;
use tokio::process::Command;

use crate::{
    ExecOutput, ExecRequest, InteractiveCommand, InteractiveExit, RemoteShell, RemoteShellError,
    ShellAddress, StreamedExec,
};

/// The shell of the machine this process runs on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalShell {
    sh: PathBuf,
}

impl Default for LocalShell {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalShell {
    /// The tag of this transport in a [`ShellAddress`].
    pub const TRANSPORT: &'static str = "local";

    /// This machine's `/bin/sh`.
    pub fn new() -> Self {
        Self {
            sh: PathBuf::from("/bin/sh"),
        }
    }

    fn command(&self, command: &str) -> Command {
        let mut child = Command::new(&self.sh);
        child
            .arg("-c")
            .arg(command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        child
    }
}

#[async_trait]
impl RemoteShell for LocalShell {
    fn address(&self) -> ShellAddress {
        ShellAddress::new(Self::TRANSPORT, "this machine")
    }

    /// `/bin/sh -c <command>`; its exit status is the command's.
    async fn exec(&self, request: ExecRequest) -> Result<ExecOutput, RemoteShellError> {
        let output = crate::child::run_to_end(
            self.command(&request.command),
            &self.sh,
            request.stdin,
            request.deadline,
            self.address(),
            &request.command,
        )
        .await?;
        Ok(ExecOutput {
            status: output.code,
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }

    async fn exec_streamed(&self, command: &str) -> Result<StreamedExec, RemoteShellError> {
        crate::child::stream(self.command(command), &self.sh)
    }

    /// `/bin/sh -c <command>` for a local PTY.
    fn interactive(&self, command: &str) -> InteractiveCommand {
        InteractiveCommand {
            argv: vec![
                self.sh.as_os_str().to_os_string(),
                OsString::from("-c"),
                OsString::from(command),
            ],
            env: Vec::new(),
            exit: InteractiveExit::ClientStatus,
        }
    }
}
