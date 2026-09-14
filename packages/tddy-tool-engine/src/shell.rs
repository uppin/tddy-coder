//! Local vs remote execution backends for the exec catalog.
//!
//! The coding agent never opens SSH from its jail. Dispatch stays IPC/HTTP/LiveKit to the
//! code-managing daemon; that daemon picks [`LocalShell`] (empty `ssh_config_host`) or
//! [`RemoteShell`] (`ssh -o BatchMode=yes <alias>`). A BatchMode failure is the result — never a
//! fallback to the host filesystem.

use std::path::{Path, PathBuf};

use async_trait::async_trait;

use crate::{contain_path, execute_tool, ToolOutcome};
use tddy_task::TaskRegistry;

/// Why a shell operation could not complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellError {
    /// Behaviour that is published but not yet wired (`TODO(exec)`).
    Unimplemented(String),
    /// The host filesystem or the OpenSSH child failed.
    Io(String),
}

impl std::fmt::Display for ShellError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unimplemented(m) | Self::Io(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for ShellError {}

/// Which backend a session is executing through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellKind {
    Local,
    Remote,
}

/// File and command operations contained against a worktree root — local disk or an SSH target.
#[async_trait]
pub trait Shell: Send + Sync {
    fn kind(&self) -> ShellKind;
    /// Worktree root tools are contained against (a local path, or an absolute path on the target).
    fn root(&self) -> &str;
    async fn read_to_string(&self, path: &str) -> Result<String, ShellError>;
    async fn write_string(&self, path: &str, contents: &str) -> Result<(), ShellError>;
    async fn run(&self, command: &str) -> Result<std::process::Output, ShellError>;
}

/// Today's exec path: the code-managing host's own filesystem.
pub struct LocalShell {
    root: PathBuf,
}

impl LocalShell {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

#[async_trait]
impl Shell for LocalShell {
    fn kind(&self) -> ShellKind {
        ShellKind::Local
    }

    fn root(&self) -> &str {
        self.root.to_str().unwrap_or("")
    }

    async fn read_to_string(&self, path: &str) -> Result<String, ShellError> {
        let resolved = contain_path(&self.root, path).map_err(ShellError::Io)?;
        std::fs::read_to_string(&resolved).map_err(|e| ShellError::Io(e.to_string()))
    }

    async fn write_string(&self, path: &str, contents: &str) -> Result<(), ShellError> {
        let resolved = contain_path(&self.root, path).map_err(ShellError::Io)?;
        if let Some(parent) = resolved.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ShellError::Io(e.to_string()))?;
        }
        std::fs::write(&resolved, contents).map_err(|e| ShellError::Io(e.to_string()))
    }

    async fn run(&self, command: &str) -> Result<std::process::Output, ShellError> {
        tokio::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(&self.root)
            .output()
            .await
            .map_err(|e| ShellError::Io(e.to_string()))
    }
}

/// Exec catalog on an SSH target that has no tddy-daemon. OpenSSH CLI so `~/.ssh/config` and
/// ssh-agent apply; `BatchMode=yes` so an unloaded key is a hard failure.
pub struct RemoteShell {
    host_alias: String,
    remote_root: String,
}

impl RemoteShell {
    pub fn new(host_alias: impl Into<String>, remote_root: impl Into<String>) -> Self {
        Self {
            host_alias: host_alias.into(),
            remote_root: remote_root.into(),
        }
    }

    /// `ssh -o BatchMode=yes -- <alias> …` — the argv green will spawn. Encrypted keys must already
    /// be loaded (Hosts add-key); this never prompts.
    pub fn ssh_argv(&self, remote_command: &str) -> Vec<String> {
        vec![
            "ssh".to_string(),
            "-o".to_string(),
            "BatchMode=yes".to_string(),
            "--".to_string(),
            self.host_alias.clone(),
            remote_command.to_string(),
        ]
    }

    pub fn host_alias(&self) -> &str {
        &self.host_alias
    }
}

#[async_trait]
impl Shell for RemoteShell {
    fn kind(&self) -> ShellKind {
        ShellKind::Remote
    }

    fn root(&self) -> &str {
        &self.remote_root
    }

    async fn read_to_string(&self, _path: &str) -> Result<String, ShellError> {
        // TODO(exec): implement — `ssh -o BatchMode=yes -- <alias> -- cat …` contained against remote_root
        Err(ShellError::Unimplemented(
            "TODO(exec): implement RemoteShell".to_string(),
        ))
    }

    async fn write_string(&self, _path: &str, _contents: &str) -> Result<(), ShellError> {
        // TODO(exec): implement
        Err(ShellError::Unimplemented(
            "TODO(exec): implement RemoteShell".to_string(),
        ))
    }

    async fn run(&self, _command: &str) -> Result<std::process::Output, ShellError> {
        // TODO(exec): implement
        Err(ShellError::Unimplemented(
            "TODO(exec): implement RemoteShell".to_string(),
        ))
    }
}

/// Pick the backend a session's exec catalog uses. Empty alias is LocalShell; a set alias is
/// RemoteShell and never falls back.
pub fn session_shell(local_root: PathBuf, ssh_config_host: &str) -> Box<dyn Shell> {
    if ssh_config_host.is_empty() {
        Box::new(LocalShell::new(local_root))
    } else {
        Box::new(RemoteShell::new(
            ssh_config_host,
            local_root.to_string_lossy().into_owned(),
        ))
    }
}

/// Dispatch one exec-catalog tool through `shell`. LocalShell is today's [`execute_tool`].
pub async fn execute_tool_on_shell(
    shell: &dyn Shell,
    tool_name: &str,
    args_json: &str,
    registry: &TaskRegistry,
    session_id: &str,
) -> ToolOutcome {
    match shell.kind() {
        ShellKind::Local => {
            execute_tool(
                Path::new(shell.root()),
                tool_name,
                args_json,
                registry,
                session_id,
            )
            .await
        }
        ShellKind::Remote => {
            // TODO(exec): implement — every exec-catalog tool, contained against shell.root()
            let _ = (tool_name, args_json, registry, session_id);
            ToolOutcome::err("TODO(exec): execute_tool via RemoteShell")
        }
    }
}
