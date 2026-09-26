//! Local vs remote execution backends for the exec catalog.
//!
//! The coding agent never opens SSH from its jail. Dispatch stays IPC/HTTP/LiveKit to the
//! code-managing daemon; that daemon picks [`LocalShell`] (empty `ssh_config_host`) or
//! [`RemoteShell`] (`ssh -o BatchMode=yes <alias>`). A BatchMode failure is the result — never a
//! fallback to the host filesystem.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tddy_core::{contain_remote_path, run_ssh_batch, shell_single_quote};

use crate::contained_shell::run_contained;
use crate::{contain_path, execute_tool_with_env, ToolOutcome};
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
        // No budget: this surface has never carried one, and the caller owns the deadline.
        run_contained(command, &self.root, &[], None)
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

    fn contain(&self, path: &str) -> Result<String, ShellError> {
        contain_remote_path(self.root(), path).map_err(ShellError::Io)
    }

    async fn ssh_run(
        &self,
        remote_shell_command: &str,
    ) -> Result<std::process::Output, ShellError> {
        let host = self.host_alias.clone();
        let cmd = remote_shell_command.to_string();
        tokio::task::spawn_blocking(move || run_ssh_batch(&host, &cmd))
            .await
            .map_err(|e| ShellError::Io(format!("ssh task join: {e}")))?
            .map_err(ShellError::Io)
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

    async fn read_to_string(&self, path: &str) -> Result<String, ShellError> {
        let abs = self.contain(path)?;
        let remote_cmd = format!("cat {}", shell_single_quote(&abs));
        let out = self.ssh_run(&remote_cmd).await?;
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    async fn write_string(&self, path: &str, contents: &str) -> Result<(), ShellError> {
        let abs = self.contain(path)?;
        let parent = abs.rsplit_once('/').map(|(p, _)| p).unwrap_or(".");
        let encoded = base64_encode(contents);
        let remote_cmd = format!(
            "set -e; mkdir -p {}; printf %s {} | base64 -d > {}",
            shell_single_quote(parent),
            shell_single_quote(&encoded),
            shell_single_quote(&abs),
        );
        self.ssh_run(&remote_cmd).await?;
        Ok(())
    }

    async fn run(&self, command: &str) -> Result<std::process::Output, ShellError> {
        let root = shell_single_quote(self.root());
        let remote_cmd = format!("cd {} && {}", root, command);
        self.ssh_run(&remote_cmd).await
    }
}

fn base64_encode(bytes: &str) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    STANDARD.encode(bytes.as_bytes())
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
            execute_tool_with_env(
                Path::new(shell.root()),
                tool_name,
                args_json,
                registry,
                session_id,
                &[],
            )
            .await
        }
        ShellKind::Remote => {
            execute_tool_on_remote_shell(shell, tool_name, args_json, registry, session_id).await
        }
    }
}

async fn execute_tool_on_remote_shell(
    shell: &dyn Shell,
    tool_name: &str,
    args_json: &str,
    registry: &TaskRegistry,
    session_id: &str,
) -> ToolOutcome {
    let args: serde_json::Value = match serde_json::from_str(args_json) {
        Ok(v) => v,
        Err(e) => return ToolOutcome::err(format!("invalid args_json: {e}")),
    };

    let kind = format!("execute_tool:{tool_name}");
    let outcome = match tool_name {
        "Read" => remote_tool_read(shell, &args).await,
        "Write" => remote_tool_write(shell, &args).await,
        "StrReplace" => remote_tool_str_replace(shell, &args).await,
        "Delete" => remote_tool_delete(shell, &args).await,
        "Grep" => remote_tool_grep(shell, &args).await,
        "Glob" => remote_tool_glob(shell, &args).await,
        "Shell" => remote_tool_shell(shell, &args, registry, session_id, &kind).await,
        "Await" => crate::tool_await(&args, registry).await,
        "ReadLints" => ToolOutcome::err("ReadLints: not available over RemoteShell"),
        "LspDiagnostics" | "LspDefinition" | "LspReferences" | "LspHover" | "LspSymbols" => {
            ToolOutcome::err(format!("{tool_name}: not available over RemoteShell"))
        }
        "SemanticSearch" => ToolOutcome::err("SemanticSearch: not available over RemoteShell"),
        other => ToolOutcome::err(format!("unknown tool: {other}")),
    };

    if tool_name == "Shell" || tool_name == "Await" {
        return outcome;
    }

    let task_id = crate::register_sync_task(registry, session_id, &kind, &outcome).await;
    ToolOutcome {
        result_json: outcome.result_json,
        is_error: outcome.is_error,
        error_message: outcome.error_message,
        job_id: task_id,
        job_running: false,
    }
}

async fn remote_tool_read(shell: &dyn Shell, args: &serde_json::Value) -> ToolOutcome {
    let path_str = match args.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolOutcome::err("Read: missing 'path' argument"),
    };
    match shell.read_to_string(path_str).await {
        Ok(content) => ToolOutcome::ok(serde_json::json!({ "content": content }).to_string()),
        Err(e) if e.to_string().contains("No such file") => ToolOutcome {
            result_json: serde_json::json!({ "error": "file not found" }).to_string(),
            is_error: true,
            error_message: "file not found".to_string(),
            job_id: String::new(),
            job_running: false,
        },
        Err(e) => ToolOutcome::err(format!("Read: {e}")),
    }
}

async fn remote_tool_write(shell: &dyn Shell, args: &serde_json::Value) -> ToolOutcome {
    let path_str = match args.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolOutcome::err("Write: missing 'path' argument"),
    };
    let contents = match args.get("contents").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => return ToolOutcome::err("Write: missing 'contents' argument"),
    };
    match shell.write_string(path_str, contents).await {
        Ok(()) => {
            let bytes = contents.len();
            ToolOutcome::ok(serde_json::json!({ "bytes_written": bytes }).to_string())
        }
        Err(e) => ToolOutcome::err(format!("Write: {e}")),
    }
}

async fn remote_tool_str_replace(shell: &dyn Shell, args: &serde_json::Value) -> ToolOutcome {
    let path_str = match args.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolOutcome::err("StrReplace: missing 'path' argument"),
    };
    let old_string = match args.get("old_string").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return ToolOutcome::err("StrReplace: missing 'old_string' argument"),
    };
    let new_string = match args.get("new_string").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return ToolOutcome::err("StrReplace: missing 'new_string' argument"),
    };
    let content = match shell.read_to_string(path_str).await {
        Ok(c) => c,
        Err(e) => return ToolOutcome::err(format!("StrReplace: read failed: {e}")),
    };
    if !content.contains(old_string) {
        return ToolOutcome::err("StrReplace: old_string not found in file");
    }
    let updated = content.replacen(old_string, new_string, 1);
    match shell.write_string(path_str, &updated).await {
        Ok(()) => ToolOutcome::ok(serde_json::json!({ "replaced": true }).to_string()),
        Err(e) => ToolOutcome::err(format!("StrReplace: write failed: {e}")),
    }
}

async fn remote_tool_delete(shell: &dyn Shell, args: &serde_json::Value) -> ToolOutcome {
    let path_str = match args.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolOutcome::err("Delete: missing 'path' argument"),
    };
    let abs = match contain_remote_path(shell.root(), path_str) {
        Ok(p) => p,
        Err(e) => return ToolOutcome::err(format!("Delete: {e}")),
    };
    let cmd = format!("rm -f {}", shell_single_quote(&abs));
    match shell.run(&cmd).await {
        Ok(_) => ToolOutcome::ok(serde_json::json!({ "deleted": true }).to_string()),
        Err(e) => ToolOutcome::err(format!("Delete: {e}")),
    }
}

async fn remote_tool_grep(shell: &dyn Shell, args: &serde_json::Value) -> ToolOutcome {
    let pattern = match args.get("pattern").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolOutcome::err("Grep: missing 'pattern' argument"),
    };
    let quoted = shell_single_quote(pattern);
    let cmd = format!("rg --json -e {} .", quoted);
    match shell.run(&cmd).await {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let mut matches = vec![];
            for line in stdout.lines() {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                    if v.get("type").and_then(|t| t.as_str()) == Some("match") {
                        matches.push(v);
                    }
                }
            }
            ToolOutcome::ok(serde_json::json!({ "matches": matches }).to_string())
        }
        Err(e) => ToolOutcome::err(format!("Grep: rg execution failed: {e}")),
    }
}

async fn remote_tool_glob(shell: &dyn Shell, args: &serde_json::Value) -> ToolOutcome {
    let pattern = match args.get("pattern").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolOutcome::err("Glob: missing 'pattern' argument"),
    };
    let quoted = shell_single_quote(pattern);
    let cmd = format!(
        "find . -path ./{} -prune -o -name {} -print | sed 's|^\\./||'",
        quoted, quoted
    );
    match shell.run(&cmd).await {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let paths: Vec<String> = stdout
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect();
            ToolOutcome::ok(serde_json::json!({ "paths": paths }).to_string())
        }
        Err(e) => ToolOutcome::err(format!("Glob: {e}")),
    }
}

async fn remote_tool_shell(
    shell: &dyn Shell,
    args: &serde_json::Value,
    registry: &TaskRegistry,
    session_id: &str,
    kind: &str,
) -> ToolOutcome {
    let command = match args.get("command").and_then(|v| v.as_str()) {
        Some(c) => c.to_string(),
        None => return ToolOutcome::err("Shell: missing 'command' argument"),
    };
    let block_until_ms = args
        .get("block_until_ms")
        .and_then(|v| v.as_i64())
        .unwrap_or(30_000);
    if block_until_ms == 0 {
        return ToolOutcome::err("Shell: background jobs are not supported over RemoteShell");
    }
    let timeout = std::time::Duration::from_millis(block_until_ms as u64);
    let fut = shell.run(&command);
    match tokio::time::timeout(timeout, fut).await {
        Ok(Ok(out)) => {
            let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
            let exit_code = out.status.code().unwrap_or(-1);
            let outcome = ToolOutcome::ok(
                serde_json::json!({
                    "stdout": stdout,
                    "stderr": stderr,
                    "exit_code": exit_code,
                })
                .to_string(),
            );
            let task_id = crate::register_sync_task(registry, session_id, kind, &outcome).await;
            ToolOutcome {
                job_id: task_id,
                ..outcome
            }
        }
        Ok(Err(e)) => ToolOutcome::err(format!("Shell: {e}")),
        Err(_) => ToolOutcome::err(format!("Shell: timed out after {}ms", block_until_ms)),
    }
}
