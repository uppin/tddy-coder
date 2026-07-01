//! Claude CLI sandbox recipe — reads, copies, policy, MCP argv, env overlays.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tddy_sandbox::builder::{CopySpec, MachPolicy, PolicySpec};
use tddy_sandbox::{process_exec_reads, workspace_exec_tool_names};

/// Read grants for a sandboxed Claude binary (baseline + toolchain + binary deps).
pub fn process_claude_exec_reads(claude_binary: &Path) -> Vec<tddy_sandbox::ReadSpec> {
    process_exec_reads(claude_binary)
}

/// Credentials copied into the jail HOME for Claude — `.credentials.json` only.
pub fn claude_credentials_copies(host_home: &Path, scratch_home: &Path) -> Vec<CopySpec> {
    vec![CopySpec {
        src: host_home.join(".claude").join(".credentials.json"),
        dest: scratch_home.join(".claude").join(".credentials.json"),
        optional: true,
        mode: Some(0o600),
    }]
}

/// Policy for an interactive Node/V8 CLI with PTY (Claude Code).
pub fn claude_interactive_policy() -> PolicySpec {
    PolicySpec {
        allow_dynamic_code_generation: true,
        allow_process_fork: true,
        mach_lookup: MachPolicy::All,
        sysctl_read: true,
        pseudo_tty: true,
        exec_paths: [
            "/usr/bin",
            "/usr/libexec",
            "/bin",
            "/sbin",
            "/System",
            "/Library",
            "/Applications",
            "/opt/homebrew",
            "/opt/local",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect(),
    }
}

/// Claude-specific env vars layered on [`tddy_sandbox::scratch_runner_env`].
pub fn claude_runner_env_overlay(scratch_tmp: &Path) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    let tmp = scratch_tmp.to_string_lossy().to_string();
    env.insert("CLAUDE_CODE_TMPDIR".into(), tmp.clone());
    env.insert("CLAUDE_TMPDIR".into(), tmp);
    env
}

const PERMISSION_PROMPT_TOOL: &str = "mcp__tddy-tools__approval_prompt";
const MCP_CONFIG_FILENAME: &str = "claude-mcp-config.json";

/// `--allowedTools` entries for sandbox claude: `mcp__tddy-tools__*` exec tools + `AskUserQuestion`.
pub fn build_claude_allowlist() -> Vec<String> {
    tddy_workflow_recipes::permissions::build_remote_allowlist(workspace_exec_tool_names())
}

/// Write MCP config registering `tddy-tools --mcp` under a writable scratch directory.
pub fn write_claude_mcp_config(scratch_dir: &Path, tddy_tools_path: &Path) -> Result<PathBuf> {
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
pub fn append_claude_mcp_args(
    argv: &mut Vec<String>,
    scratch_dir: &Path,
    tddy_tools_path: &Path,
) -> Result<()> {
    let mcp_path = write_claude_mcp_config(scratch_dir, tddy_tools_path)?;
    for tool in build_claude_allowlist() {
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
pub fn claude_scratch_mcp_dir(fallback: &Path) -> PathBuf {
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
    use tddy_sandbox::builder::{ReadKind, ReadReason};

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
    fn claude_allowlist_uses_mcp_prefix_and_excludes_native_tools() {
        let allowlist = build_claude_allowlist();
        let allowset: HashSet<_> = allowlist.iter().cloned().collect();

        for name in workspace_exec_tool_names() {
            let prefixed = format!("mcp__tddy-tools__{name}");
            assert!(
                allowset.contains(&prefixed),
                "allowlist must contain {prefixed}; got: {allowlist:?}"
            );
        }
        assert!(allowset.contains("AskUserQuestion"));
        for native in NATIVE_TOOLS_EXCLUDED {
            assert!(
                !allowset.contains(*native),
                "native tool {native} must not appear un-prefixed in sandbox allowlist"
            );
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn claude_exec_reads_include_the_dyld_root_literal() {
        let reads = process_claude_exec_reads(Path::new("/usr/bin/true"));
        assert!(
            reads.iter().any(|r| {
                r.kind == ReadKind::Literal
                    && r.host == Path::new("/")
                    && r.reason == ReadReason::DyldRoot
            }),
            "claude reads must include the dyld root literal: {reads:?}"
        );
    }

    #[test]
    fn claude_credentials_copies_seed_only_the_credentials_file() {
        let copies = claude_credentials_copies(Path::new("/home/user"), Path::new("/jail/home"));
        assert_eq!(
            copies.len(),
            1,
            "expected only the credentials copy: {copies:?}"
        );
        assert!(
            copies[0].src.ends_with(".claude/.credentials.json"),
            "copy src must be the credentials file: {:?}",
            copies[0].src
        );
        assert!(
            copies
                .iter()
                .all(|c| !c.src.to_string_lossy().contains("settings")),
            "settings.json must not be seeded into the jail: {copies:?}"
        );
    }

    #[test]
    fn write_claude_mcp_config_registers_tddy_tools_mcp_server() {
        let dir = tempfile::tempdir().unwrap();
        let tools = dir.path().join("tddy-tools");
        let path =
            write_claude_mcp_config(dir.path(), &tools).expect("write MCP config must succeed");
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let server = &json["mcpServers"]["tddy-tools"];
        assert_eq!(server["command"].as_str().unwrap(), tools.to_string_lossy());
        assert_eq!(server["args"], serde_json::json!(["--mcp"]));
    }
}
