//! A target's `~/.claude.json` made ready for a Claude start: the clone root trusted, and the
//! repository's project MCP servers recorded as rejected, so the session meets neither the
//! "trust this folder" nor the "use this MCP server" dialog on a terminal nobody is watching.
//!
//! Over the remote-shell port only: the `$HOME` of the target is probed, `~/.claude.json` is read
//! (an absent file is `{}`), `projects["<clone root>"]` gets `hasTrustDialogAccepted = true` and
//! every server of the repository's `.mcp.json` in `disabledMcpjsonServers` (with
//! `enabledMcpjsonServers` empty), every other key is kept, and the file is written back with
//! [`tddy_remote_shell::FileMode::OwnerOnly`] only when it changed.

use std::fmt;
use std::path::Path;
use std::time::Duration;

use serde_json::{Map, Value};
use tddy_core::ssh_exec::shell_single_quote;
use tddy_remote_shell::{ExecRequest, FileMode, RemoteShell, RemoteShellError};

/// What [`pre_trust_clone_root`] did to the target's `~/.claude.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustOutcome {
    /// The file was written: it did not hold the trust and the MCP answer yet.
    Written,
    /// The file already held them: nothing was written.
    Unchanged,
}

/// Why the target's `~/.claude.json` could not be made ready.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetTrustError {
    /// The remote shell failed.
    Shell(RemoteShellError),
    /// The target's `~/.claude.json` or the repository's `.mcp.json` is not JSON of the shape
    /// Claude writes; `file` is the path on the target.
    Malformed { file: String, message: String },
}

impl fmt::Display for TargetTrustError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Shell(error) => write!(f, "{error}"),
            Self::Malformed { file, message } => write!(f, "{file} is malformed: {message}"),
        }
    }
}

impl std::error::Error for TargetTrustError {}

/// How long the target has to answer one of the reads or the write.
const TARGET_DEADLINE: Duration = Duration::from_secs(30);

/// Trust `clone_root` in the target's `~/.claude.json` and reject every server in the `.mcp.json`
/// of `worktree` (a worktree of that clone), keeping the file's other keys.
pub async fn pre_trust_clone_root(
    shell: &dyn RemoteShell,
    clone_root: &Path,
    worktree: &Path,
) -> Result<TrustOutcome, TargetTrustError> {
    let home = home_of_the_target(shell).await?;
    let claude_json = format!("{home}/.claude.json");
    let mut document = read_json(shell, &claude_json).await?;
    let before = document.clone();

    let mcp_json = worktree.join(".mcp.json").to_string_lossy().into_owned();
    let servers = declared_mcp_servers(&read_json(shell, &mcp_json).await?, &mcp_json)?;
    trust(
        &mut document,
        &clone_root.to_string_lossy(),
        &servers,
        &claude_json,
    )?;

    if document == before {
        return Ok(TrustOutcome::Unchanged);
    }
    let bytes =
        serde_json::to_vec_pretty(&document).map_err(|error| malformed(&claude_json, error))?;
    shell
        .write_file(&claude_json, &bytes, FileMode::OwnerOnly)
        .await
        .map_err(TargetTrustError::Shell)?;
    Ok(TrustOutcome::Written)
}

fn malformed(file: &str, message: impl fmt::Display) -> TargetTrustError {
    TargetTrustError::Malformed {
        file: file.to_string(),
        message: message.to_string(),
    }
}

/// The `$HOME` of the target's login shell: its last output line (a login shell may print a banner
/// first), which must be an absolute path.
async fn home_of_the_target(shell: &dyn RemoteShell) -> Result<String, TargetTrustError> {
    let output = shell
        .exec(
            ExecRequest::new(r#"exec "${SHELL:-/bin/sh}" -lc 'printf "\n%s\n" "$HOME"'"#)
                .within(TARGET_DEADLINE),
        )
        .await
        .map_err(TargetTrustError::Shell)?
        .success_or_exited()
        .map_err(TargetTrustError::Shell)?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let home = stdout
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .unwrap_or("");
    if !home.starts_with('/') {
        return Err(TargetTrustError::Shell(RemoteShellError::Spawn(format!(
            "the target's login shell reported {home:?} as its $HOME, which is not an absolute path"
        ))));
    }
    Ok(home.trim_end_matches('/').to_string())
}

/// The JSON object at `path` on the target; an absent file, or an empty one, is `{}`.
async fn read_json(shell: &dyn RemoteShell, path: &str) -> Result<Value, TargetTrustError> {
    let quoted = shell_single_quote(path);
    let output = shell
        .exec(
            ExecRequest::new(format!(
                "if [ -e {quoted} ]; then cat {quoted}; else printf %s '{{}}'; fi"
            ))
            .within(TARGET_DEADLINE),
        )
        .await
        .map_err(TargetTrustError::Shell)?
        .success_or_exited()
        .map_err(TargetTrustError::Shell)?;
    let text = String::from_utf8_lossy(&output.stdout);
    if text.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    let value: Value = serde_json::from_str(&text).map_err(|error| malformed(path, error))?;
    if !value.is_object() {
        return Err(malformed(path, "not a JSON object"));
    }
    Ok(value)
}

/// The server names a repository's `.mcp.json` declares.
fn declared_mcp_servers(mcp_json: &Value, file: &str) -> Result<Vec<String>, TargetTrustError> {
    match mcp_json.get("mcpServers") {
        None => Ok(Vec::new()),
        Some(Value::Object(servers)) => Ok(servers.keys().cloned().collect()),
        Some(_) => Err(malformed(file, "`mcpServers` is not an object")),
    }
}

/// `document`'s record of `clone_root` trusted, with every one of `servers` rejected.
fn trust(
    document: &mut Value,
    clone_root: &str,
    servers: &[String],
    file: &str,
) -> Result<(), TargetTrustError> {
    let projects = document
        .as_object_mut()
        .expect("read_json returns an object")
        .entry("projects")
        .or_insert_with(|| Value::Object(Map::new()));
    let projects = projects
        .as_object_mut()
        .ok_or_else(|| malformed(file, "`projects` is not an object"))?;
    let record = projects
        .entry(clone_root.to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| malformed(file, format!("`projects[{clone_root}]` is not an object")))?;
    record.insert("hasTrustDialogAccepted".to_string(), Value::Bool(true));

    if servers.is_empty() {
        return Ok(());
    }
    let names_of = |key: &str| -> Result<Vec<Value>, TargetTrustError> {
        match record.get(key) {
            None => Ok(Vec::new()),
            Some(Value::Array(names)) => Ok(names.clone()),
            Some(_) => Err(malformed(file, format!("`{key}` is not a list"))),
        }
    };
    let mut rejected = names_of("disabledMcpjsonServers")?;
    let enabled = names_of("enabledMcpjsonServers")?;
    for server in servers {
        let name = Value::String(server.clone());
        if !rejected.contains(&name) {
            rejected.push(name);
        }
    }
    record.insert("disabledMcpjsonServers".to_string(), Value::Array(rejected));
    if record.contains_key("enabledMcpjsonServers") {
        let kept: Vec<Value> = enabled
            .into_iter()
            .filter(|name| !servers.iter().any(|server| name.as_str() == Some(server)))
            .collect();
        record.insert("enabledMcpjsonServers".to_string(), Value::Array(kept));
    }
    Ok(())
}
