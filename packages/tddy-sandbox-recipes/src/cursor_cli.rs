//! Cursor Agent CLI sandbox recipe — reads, copies, policy, MCP config, argv overlays.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tddy_sandbox::binary_exec_reads;
use tddy_sandbox::builder::{CopySpec, PolicySpec, ReadReason, ReadSpec};
use tddy_sandbox::{path_traversal_reads, process_exec_reads};

/// Read grants for a sandboxed `agent` binary (baseline + toolchain + install tree + bundled node).
pub fn process_cursor_exec_reads(cursor_binary: &Path) -> Vec<ReadSpec> {
    let mut reads = process_exec_reads(cursor_binary);
    reads.extend(cursor_agent_prerequisite_reads(cursor_binary));
    reads
}

/// Extra read grants for the Cursor `agent` wrapper and its bundled Node runtime.
///
/// The published `agent` script resolves its install dir via `realpath`/`readlink`, then execs
/// `$SCRIPT_DIR/node` against `$SCRIPT_DIR/index.js`. Seatbelt must allow read-only access to that
/// entire version directory (and the `~/.local/bin` wrapper when present).
pub fn cursor_agent_prerequisite_reads(cursor_binary: &Path) -> Vec<ReadSpec> {
    let mut reads = Vec::new();
    let resolved = resolve_cursor_agent_binary(cursor_binary);

    if let Some(install_dir) = resolved.parent() {
        reads.push(
            ReadSpec::subpath(install_dir, ReadReason::BinaryDeps).executable(),
        );
        reads.extend(path_traversal_reads(install_dir));
        let node = install_dir.join("node");
        let index = install_dir.join("index.js");
        if node.is_file() {
            let node = std::fs::canonicalize(&node).unwrap_or(node);
            reads.extend(path_traversal_reads(&node));
            reads.extend(binary_exec_reads(&node));
        }
        if index.is_file() {
            let index = std::fs::canonicalize(&index).unwrap_or(index);
            reads.extend(path_traversal_reads(&index));
        }
    }

    for share_root in cursor_agent_share_roots(&resolved) {
        reads.push(ReadSpec::subpath(&share_root, ReadReason::BinaryDeps));
        reads.extend(path_traversal_reads(&share_root));
    }

    if let Some(home) = std::env::var_os("HOME") {
        let local_bin = PathBuf::from(&home).join(".local").join("bin");
        if local_bin.is_dir() {
            reads.push(ReadSpec::subpath(&local_bin, ReadReason::BinaryDeps).executable());
            reads.extend(path_traversal_reads(&local_bin));
        }
        let local_share = PathBuf::from(home).join(".local").join("share");
        if local_share.is_dir() {
            reads.extend(path_traversal_reads(&local_share));
        }
    }

    reads
}

fn resolve_cursor_agent_binary(cursor_binary: &Path) -> PathBuf {
    if cursor_binary.is_absolute() {
        std::fs::canonicalize(cursor_binary).unwrap_or_else(|_| cursor_binary.to_path_buf())
    } else {
        cursor_binary.to_path_buf()
    }
}

fn cursor_agent_share_roots(resolved: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut current = resolved.parent();
    while let Some(dir) = current {
        if dir.file_name().and_then(|n| n.to_str()) == Some("cursor-agent") {
            roots.push(dir.to_path_buf());
            break;
        }
        current = dir.parent();
    }
    roots
}

/// Mirror the host Cursor Agent install layout inside the persistent jail home so the wrapper's
/// self-checks find a consistent tree (`$HOME/.local/bin/agent` → versioned install).
#[cfg(unix)]
pub fn seed_cursor_local_install(cursor_home_dir: &Path, cursor_binary: &str) -> Result<()> {
    use std::os::unix::fs::symlink;

    let real_bin = resolve_cursor_agent_binary(Path::new(cursor_binary));
    let local_bin_dir = cursor_home_dir.join(".local").join("bin");
    std::fs::create_dir_all(&local_bin_dir)
        .with_context(|| format!("create {}", local_bin_dir.display()))?;
    let local_bin_agent = local_bin_dir.join("agent");

    let link_target = if is_cursor_versioned_install_layout(&real_bin) {
        mirror_cursor_versioned_symlink(cursor_home_dir, &real_bin)?
    } else {
        real_bin.clone()
    };

    let _ = std::fs::remove_file(&local_bin_agent);
    symlink(&link_target, &local_bin_agent).with_context(|| {
        format!(
            "symlink {} -> {}",
            local_bin_agent.display(),
            link_target.display()
        )
    })?;
    Ok(())
}

#[cfg(unix)]
fn is_cursor_versioned_install_layout(real_bin: &Path) -> bool {
    real_bin
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .is_some_and(|n| n == "versions")
        && real_bin
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .and_then(|p| p.file_name())
            .is_some_and(|n| n == "cursor-agent")
}

#[cfg(unix)]
fn mirror_cursor_versioned_symlink(cursor_home_dir: &Path, real_bin: &Path) -> Result<PathBuf> {
    use std::os::unix::fs::symlink;

    let version = real_bin
        .file_name()
        .map(|n| n.to_owned())
        .context("versioned cursor-agent binary has no file name")?;
    let versions_dir = cursor_home_dir
        .join(".local")
        .join("share")
        .join("cursor-agent")
        .join("versions");
    std::fs::create_dir_all(&versions_dir)
        .with_context(|| format!("create {}", versions_dir.display()))?;
    let versioned_link = versions_dir.join(&version);
    let _ = std::fs::remove_file(&versioned_link);
    symlink(real_bin, &versioned_link).with_context(|| {
        format!(
            "symlink {} -> {}",
            versioned_link.display(),
            real_bin.display()
        )
    })?;
    Ok(versioned_link)
}

/// Interactive Node/V8 CLI with PTY — same policy shape as Claude Code.
pub fn cursor_interactive_policy() -> PolicySpec {
    crate::claude_cli::claude_interactive_policy()
}

/// Seed persistent jail `cursor_home_dir` auth state from the host `~/.cursor` once.
/// Never overwrites existing files in the jail home.
pub fn seed_cursor_credentials(cursor_home_dir: &Path) -> Result<()> {
    let Some(host_home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Ok(());
    };
    let host_cursor = host_home.join(".cursor");
    if !host_cursor.is_dir() {
        return Ok(());
    }
    let dest_cursor = cursor_home_dir.join(".cursor");
    std::fs::create_dir_all(&dest_cursor)
        .with_context(|| format!("create persistent cursor home {}", dest_cursor.display()))?;
    for name in ["cli-config.json", "auth.json", "mcp.json"] {
        let src = host_cursor.join(name);
        let dest = dest_cursor.join(name);
        if dest.exists() || !src.is_file() {
            continue;
        }
        std::fs::copy(&src, &dest)
            .with_context(|| format!("seed cursor file {} -> {}", src.display(), dest.display()))?;
        #[cfg(unix)]
        if name == "auth.json" {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o600));
        }
    }
    Ok(())
}

/// Optional credential copies into the jail HOME for Cursor Agent CLI.
pub fn cursor_credentials_copies(host_home: &Path, scratch_home: &Path) -> Vec<CopySpec> {
    let host_cursor = host_home.join(".cursor");
    let dest_cursor = scratch_home.join(".cursor");
    ["auth.json", "cli-config.json"]
        .into_iter()
        .map(|name| CopySpec {
            src: host_cursor.join(name),
            dest: dest_cursor.join(name),
            optional: true,
            mode: if name == "auth.json" {
                Some(0o600)
            } else {
                None
            },
        })
        .collect()
}

const CURSOR_MCP_DIR: &str = ".cursor";
const CURSOR_MCP_FILENAME: &str = "mcp.json";

/// Write `.cursor/mcp.json` registering `tddy-tools --mcp` under `base_dir`.
pub fn write_cursor_mcp_config(
    base_dir: &Path,
    tddy_tools_path: &Path,
    mcp_env: &BTreeMap<String, String>,
) -> Result<PathBuf> {
    let cursor_dir = base_dir.join(CURSOR_MCP_DIR);
    std::fs::create_dir_all(&cursor_dir)
        .with_context(|| format!("create cursor dir for MCP config: {}", cursor_dir.display()))?;
    let path = cursor_dir.join(CURSOR_MCP_FILENAME);
    let mut server = serde_json::json!({
        "command": tddy_tools_path.to_string_lossy(),
        "args": ["--mcp"]
    });
    if !mcp_env.is_empty() {
        server["env"] = serde_json::json!(mcp_env);
    }
    let config = serde_json::json!({ "mcpServers": { "tddy-tools": server } });
    std::fs::write(&path, config.to_string())
        .with_context(|| format!("write cursor MCP config: {}", path.display()))?;
    Ok(path)
}

/// Write `.cursor/mcp.json` for a sandboxed Cursor session without mutating argv.
///
/// MCP approval flags (`--approve-mcps`, `--force`, `--trust`) are never injected here — callers
/// pass them explicitly via agent args when they want headless approval.
pub fn prepare_cursor_mcp_config(
    mcp_base_dir: &Path,
    tddy_tools_path: &Path,
    mcp_env: &BTreeMap<String, String>,
) -> Result<()> {
    write_cursor_mcp_config(mcp_base_dir, tddy_tools_path, mcp_env)?;
    Ok(())
}

/// Deprecated alias — only seeds `mcp.json`; does not append CLI flags to `argv`.
pub fn append_cursor_mcp_args(
    _argv: &mut Vec<String>,
    mcp_base_dir: &Path,
    tddy_tools_path: &Path,
    mcp_env: &BTreeMap<String, String>,
) -> Result<()> {
    prepare_cursor_mcp_config(mcp_base_dir, tddy_tools_path, mcp_env)
}

/// Build argv for a sandboxed Cursor Agent session.
///
/// Prefers invoking the bundled `$install_dir/node index.js` directly. The published `agent` bash
/// wrapper resolves its install dir via `realpath`/`readlink` on `$0`, which breaks under Seatbelt
/// when the PTY layer passes a basename-only argv0 — leaving `SCRIPT_DIR` as `.` (the jail cwd).
pub fn build_cursor_sandbox_argv(
    cursor_binary: &Path,
    model: &str,
    agent_args: &[String],
    mcp_base_dir: &Path,
    tddy_tools_path: &Path,
    mcp_env: &BTreeMap<String, String>,
) -> Result<Vec<String>> {
    let mut wrapper_argv = vec![cursor_binary.to_string_lossy().into_owned()];
    if !model.is_empty() {
        wrapper_argv.push("--model".into());
        wrapper_argv.push(model.to_string());
    }
    wrapper_argv.extend(agent_args.iter().cloned());
    prepare_cursor_mcp_config(mcp_base_dir, tddy_tools_path, mcp_env)?;

    let resolved = resolve_cursor_agent_binary(cursor_binary);
    let Some(install_dir) = resolved.parent() else {
        return Ok(wrapper_argv);
    };
    let node = install_dir.join("node");
    let index = install_dir.join("index.js");
    if !node.is_file() || !index.is_file() {
        return Ok(wrapper_argv);
    }

    let node = std::fs::canonicalize(&node).unwrap_or(node);
    let index = std::fs::canonicalize(&index).unwrap_or(index);
    let mut argv = vec![
        node.to_string_lossy().into_owned(),
        index.to_string_lossy().into_owned(),
    ];
    if !model.is_empty() {
        argv.push("--model".into());
        argv.push(model.to_string());
    }
    argv.extend(agent_args.iter().cloned());
    Ok(argv)
}

/// Cursor-specific env vars layered on [`tddy_sandbox::scratch_runner_env`].
pub fn cursor_runner_env_overlay(scratch_tmp: &Path) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    let tmp = scratch_tmp.to_string_lossy().to_string();
    env.insert("CURSOR_TMPDIR".into(), tmp);
    env
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_cursor_mcp_config_registers_tddy_tools_mcp_server() {
        let dir = tempfile::tempdir().unwrap();
        let tools = dir.path().join("tddy-tools");
        let path = write_cursor_mcp_config(dir.path(), &tools, &BTreeMap::new())
            .expect("write MCP config must succeed");
        assert!(path.ends_with(".cursor/mcp.json"));
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let server = &json["mcpServers"]["tddy-tools"];
        assert_eq!(server["command"].as_str().unwrap(), tools.to_string_lossy());
        assert_eq!(server["args"], serde_json::json!(["--mcp"]));
    }

    #[test]
    fn prepare_cursor_mcp_config_writes_mcp_json_without_mutating_argv() {
        let dir = tempfile::tempdir().unwrap();
        let tools = dir.path().join("tddy-tools");

        let mut argv = vec!["agent".to_string(), "-p".to_string(), "hi".to_string()];
        prepare_cursor_mcp_config(dir.path(), &tools, &BTreeMap::new())
            .expect("prepare must succeed");
        assert!(!argv.contains(&"--trust".to_string()));
        assert!(dir.path().join(".cursor/mcp.json").is_file());
    }

    #[test]
    fn append_cursor_mcp_args_only_seeds_mcp_config() {
        let dir = tempfile::tempdir().unwrap();
        let tools = dir.path().join("tddy-tools");
        let mut argv = vec!["agent".to_string()];
        append_cursor_mcp_args(&mut argv, dir.path(), &tools, &BTreeMap::new())
            .expect("append must succeed");
        assert!(!argv.contains(&"--approve-mcps".to_string()));
        assert!(!argv.contains(&"--force".to_string()));
        assert!(!argv.contains(&"--trust".to_string()));
    }

    #[test]
    fn seed_cursor_credentials_does_not_overwrite_existing_dest_file() {
        let host_home = tempfile::tempdir().unwrap();
        let host_cursor = host_home.path().join(".cursor");
        std::fs::create_dir_all(&host_cursor).unwrap();
        std::fs::write(host_cursor.join("auth.json"), "{\"token\":\"from-host\"}").unwrap();

        let cursor_home_dir = tempfile::tempdir().unwrap();
        let dest_dir = cursor_home_dir.path().join(".cursor");
        std::fs::create_dir_all(&dest_dir).unwrap();
        std::fs::write(dest_dir.join("auth.json"), "{\"token\":\"jail\"}").unwrap();

        let prev_home = std::env::var_os("HOME");
        std::env::set_var("HOME", host_home.path());
        let result = seed_cursor_credentials(cursor_home_dir.path());
        if let Some(v) = prev_home {
            std::env::set_var("HOME", v);
        } else {
            std::env::remove_var("HOME");
        }

        assert!(result.is_ok());
        let contents = std::fs::read_to_string(dest_dir.join("auth.json")).unwrap();
        assert_eq!(contents, "{\"token\":\"jail\"}");
    }

    #[test]
    fn build_cursor_sandbox_argv_prefers_bundled_node_over_bash_wrapper() {
        let install_dir = tempfile::tempdir().unwrap();
        let version_dir = install_dir
            .path()
            .join("share")
            .join("cursor-agent")
            .join("versions")
            .join("2026.07.01-test");
        std::fs::create_dir_all(&version_dir).unwrap();
        let agent = version_dir.join("cursor-agent");
        std::fs::write(&agent, "#!/bin/sh\n").unwrap();
        std::fs::write(version_dir.join("node"), b"").unwrap();
        std::fs::write(version_dir.join("index.js"), b"").unwrap();
        let tools = install_dir.path().join("tddy-tools");

        let argv = build_cursor_sandbox_argv(
            &agent,
            "composer-2.5",
            &["-p".to_string(), "hi".to_string()],
            install_dir.path(),
            &tools,
            &BTreeMap::new(),
        )
        .expect("argv build must succeed");

        assert_eq!(
            PathBuf::from(&argv[0]),
            std::fs::canonicalize(version_dir.join("node")).unwrap_or(version_dir.join("node"))
        );
        assert_eq!(
            PathBuf::from(&argv[1]),
            std::fs::canonicalize(version_dir.join("index.js")).unwrap_or(version_dir.join("index.js"))
        );
        assert!(!argv.contains(&"--trust".to_string()));
    }

    #[test]
    fn build_cursor_sandbox_argv_passes_through_explicit_headless_flags() {
        let install_dir = tempfile::tempdir().unwrap();
        let version_dir = install_dir
            .path()
            .join("share")
            .join("cursor-agent")
            .join("versions")
            .join("2026.07.01-test");
        std::fs::create_dir_all(&version_dir).unwrap();
        let agent = version_dir.join("cursor-agent");
        std::fs::write(&agent, "#!/bin/sh\n").unwrap();
        std::fs::write(version_dir.join("node"), b"").unwrap();
        std::fs::write(version_dir.join("index.js"), b"").unwrap();
        let tools = install_dir.path().join("tddy-tools");

        let argv = build_cursor_sandbox_argv(
            &agent,
            "composer-2.5",
            &[
                "-p".to_string(),
                "hi".to_string(),
                "--trust".to_string(),
                "--approve-mcps".to_string(),
                "--force".to_string(),
            ],
            install_dir.path(),
            &tools,
            &BTreeMap::new(),
        )
        .expect("argv build must succeed");

        assert!(argv.contains(&"--trust".to_string()));
        assert!(argv.contains(&"--approve-mcps".to_string()));
    }

    #[test]
    fn build_cursor_sandbox_argv_omits_headless_flags_in_interactive_mode() {
        let install_dir = tempfile::tempdir().unwrap();
        let version_dir = install_dir
            .path()
            .join("share")
            .join("cursor-agent")
            .join("versions")
            .join("2026.07.01-test");
        std::fs::create_dir_all(&version_dir).unwrap();
        let agent = version_dir.join("cursor-agent");
        std::fs::write(&agent, "#!/bin/sh\n").unwrap();
        std::fs::write(version_dir.join("node"), b"").unwrap();
        std::fs::write(version_dir.join("index.js"), b"").unwrap();
        let tools = install_dir.path().join("tddy-tools");

        let argv = build_cursor_sandbox_argv(
            &agent,
            "composer-2.5",
            &[],
            install_dir.path(),
            &tools,
            &BTreeMap::new(),
        )
        .expect("argv build must succeed");

        assert!(!argv.contains(&"--trust".to_string()));
        assert!(!argv.contains(&"--approve-mcps".to_string()));
    }

    #[test]
    fn cursor_agent_prerequisite_reads_include_install_dir_and_share_root() {
        let home = std::env::var("HOME").expect("HOME must be set for prerequisite read test");
        let share = PathBuf::from(&home)
            .join(".local")
            .join("share")
            .join("cursor-agent");
        let version_dir = share.join("versions").join("2026.07.01-test");
        std::fs::create_dir_all(&version_dir).unwrap();
        let agent = version_dir.join("cursor-agent");
        std::fs::write(&agent, "#!/bin/sh\n").unwrap();
        std::fs::write(version_dir.join("node"), b"").unwrap();

        let reads = cursor_agent_prerequisite_reads(&agent);
        let version_dir = std::fs::canonicalize(&version_dir).unwrap_or(version_dir);
        let share = std::fs::canonicalize(&share).unwrap_or(share);

        assert!(
            reads.iter().any(|r| r.host == version_dir && r.exec),
            "version install dir must be an executable subpath grant: {reads:?}"
        );
        assert!(
            reads.iter().any(|r| r.host == share),
            "cursor-agent share root must be readable: {reads:?}"
        );
        assert!(
            reads.iter().any(|r| r.host == PathBuf::from("/Users")),
            "path traversal ancestors must include /Users: {reads:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn seed_cursor_local_install_creates_agent_symlink_under_local_bin() {
        let install_dir = tempfile::tempdir().unwrap();
        let version_dir = install_dir
            .path()
            .join("share")
            .join("cursor-agent")
            .join("versions")
            .join("2026.07.01-test");
        std::fs::create_dir_all(&version_dir).unwrap();
        let real_bin = version_dir.join("cursor-agent");
        std::fs::write(&real_bin, "#!/bin/sh\n").unwrap();

        let cursor_home = tempfile::tempdir().unwrap();
        seed_cursor_local_install(cursor_home.path(), real_bin.to_str().unwrap())
            .expect("seed must succeed");

        let link = cursor_home.path().join(".local").join("bin").join("agent");
        assert!(link.exists(), "agent symlink must exist at {}", link.display());
    }
}
