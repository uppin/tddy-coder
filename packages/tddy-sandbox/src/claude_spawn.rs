//! Claude CLI argv + MCP config for sandboxed sessions (remote-codebase tool model).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::workspace_exec_tool_names;

const PERMISSION_PROMPT_TOOL: &str = "mcp__tddy-tools__approval_prompt";
const MCP_CONFIG_FILENAME: &str = "claude-mcp-config.json";

/// `--allowedTools` entries for sandbox claude: `mcp__tddy-tools__*` exec tools + `AskUserQuestion`.
pub fn build_sandbox_claude_allowlist() -> Vec<String> {
    tddy_workflow_recipes::permissions::build_remote_allowlist(workspace_exec_tool_names())
}

/// Write MCP config registering `tddy-tools --mcp` under a writable scratch directory.
pub fn write_sandbox_mcp_config(scratch_dir: &Path, tddy_tools_path: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(scratch_dir).with_context(|| {
        format!(
            "create scratch dir for MCP config: {}",
            scratch_dir.display()
        )
    })?;
    let path = scratch_dir.join(MCP_CONFIG_FILENAME);
    let config = serde_json::json!({
        "mcpServers": {
            "tddy-tools": {
                "command": tddy_tools_path.to_string_lossy(),
                "args": ["--mcp"]
            }
        }
    });
    std::fs::write(&path, config.to_string())
        .with_context(|| format!("write MCP config: {}", path.display()))?;
    Ok(path)
}

/// Append `--allowedTools`, `--permission-prompt-tool`, and `--mcp-config` for sandbox spawn.
pub fn append_sandbox_claude_mcp_args(
    argv: &mut Vec<String>,
    scratch_dir: &Path,
    tddy_tools_path: &Path,
) -> Result<()> {
    let mcp_path = write_sandbox_mcp_config(scratch_dir, tddy_tools_path)?;
    for tool in build_sandbox_claude_allowlist() {
        argv.push("--allowedTools".into());
        argv.push(tool);
    }
    argv.push("--permission-prompt-tool".into());
    argv.push(PERMISSION_PROMPT_TOOL.into());
    argv.push("--mcp-config".into());
    argv.push(mcp_path.to_string_lossy().into_owned());
    Ok(())
}

/// Writable directory for sandbox MCP config (context dir is read-only).
pub fn sandbox_claude_scratch_dir(fallback: &Path) -> PathBuf {
    std::env::var("TMPDIR")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| fallback.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const NATIVE_TOOLS_EXCLUDED: &[&str] = &[
        "Read",
        "Write",
        "Edit",
        "Glob",
        "Grep",
        "Bash",
        "Shell",
        "SemanticSearch",
    ];

    #[test]
    fn sandbox_claude_allowlist_uses_mcp_prefix_and_excludes_native_tools() {
        let allowlist = build_sandbox_claude_allowlist();
        let allowset: HashSet<_> = allowlist.iter().cloned().collect();

        for name in workspace_exec_tool_names() {
            let prefixed = format!("mcp__tddy-tools__{name}");
            assert!(
                allowset.contains(&prefixed),
                "allowlist must contain {prefixed}; got: {allowlist:?}"
            );
        }
        assert!(allowset.contains(&"AskUserQuestion".to_string()));
        for native in NATIVE_TOOLS_EXCLUDED {
            assert!(
                !allowset.contains(&(*native).to_string()),
                "native tool {native} must not appear un-prefixed in sandbox allowlist"
            );
        }
    }

    #[test]
    fn write_sandbox_mcp_config_registers_tddy_tools_mcp_server() {
        let dir = tempfile::tempdir().unwrap();
        let tools = dir.path().join("tddy-tools");
        let path =
            write_sandbox_mcp_config(dir.path(), &tools).expect("write MCP config must succeed");
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let server = &json["mcpServers"]["tddy-tools"];
        assert_eq!(server["command"].as_str().unwrap(), tools.to_string_lossy());
        assert_eq!(server["args"], serde_json::json!(["--mcp"]));
    }
}
