//! Managed-codebase mode constants and helpers for `tddy-coder --remote`.
//!
//! When `--remote` is passed to tddy-coder, the agent operates against a managed (remote)
//! worktree via the `mcp__tddy-tools__*` MCP tools instead of native Claude Code filesystem tools.
//!
//! # This is not what the daemon writes
//!
//! Everything below is a **legacy duplicate** of the managed-codebase context dir, kept only
//! because `remote_bootstrap_acceptance` and `remote_mode_acceptance` still exercise it. No `src/`
//! in the workspace calls any of it. The path the daemon actually takes is
//! [`tddy_sandbox::MANAGED_CODEBASE_PREAMBLE`] and [`tddy_sandbox::SandboxContextDir`], and the two
//! have diverged in ways that matter — see [`REMOTE_APPENDIX`]. Read the `tddy-sandbox` one to learn
//! how managed sessions behave; this one will mislead you.
//!
//! `tddy-coder` does not depend on `tddy-sandbox`, so this cannot simply delegate without adding
//! that dependency. Deleting it (with its two acceptance tests, its only callers) is the better
//! answer and needs the developer's consent per AGENTS.md's ASK rule.

use std::path::Path;

/// Appended to CLAUDE.md and AGENTS.md by [`RemoteContextDir`], and to the agent system prompt.
///
/// # Do not read this as a description of managed sessions
///
/// Two of its claims are false of what the daemon ships, and both were true when it was written:
///
/// - *"read-only"* — the context dir is deliberately writable now. Continuous re-sync writes into
///   it, and the jailed agent is held out by a seatbelt `(deny file-write* …)` rule plus the native
///   tool disallowlist, neither of which is a file mode.
/// - *"contains only documentation and synced skills"* — it carries the target repo's **agent
///   configuration**, chosen by a per-backend glob allow-list. `docs/` and `skills/` are
///   deliberately *not* synced; they are this project's own conventions, not agent-tool ones.
///
/// It is also *appended*, where the shipped preamble is **prepended**, so the agent reads the rule
/// before the project's own instructions rather than after them.
///
/// See `docs/ft/daemon/agent-context-sync.md`.
pub const REMOTE_APPENDIX: &str = r#"

## Appendix: Managed Codebase

The real codebase is MANAGED — it is NOT in this local directory.
This local directory is read-only and contains only documentation and synced skills.

You MUST use the `mcp__tddy-tools__*` tools (Read, Write, StrReplace, Delete, Grep, Glob, Shell,
Await, ReadLints, SemanticSearch) for ALL file and shell operations.
Do not use native tools to interact with the codebase.
"#;

/// RAII wrapper for the temporary read-only context directory used in managed-codebase mode.
///
/// Created by copying source_dir contents into a new tempdir, appending [`REMOTE_APPENDIX`]
/// to CLAUDE.md and AGENTS.md, then making all files read-only (mode 0o444 on Unix).
/// Cleaned up on Drop.
///
/// **Legacy.** No `src/` caller exists; the daemon uses [`tddy_sandbox::SandboxContextDir`], which
/// copies by glob allow-list, prepends the preamble, and leaves the tree writable. See the module
/// docs.
pub struct RemoteContextDir {
    dir: tempfile::TempDir,
}

impl RemoteContextDir {
    /// Creates a read-only temp context dir by copying files from `source_dir`.
    pub fn create(source_dir: &Path) -> anyhow::Result<Self> {
        let dir = tempfile::tempdir()?;

        // Copy all files from source_dir recursively.
        copy_dir_recursive(source_dir, dir.path())?;

        // Append REMOTE_APPENDIX to CLAUDE.md and AGENTS.md if they exist.
        for filename in &["CLAUDE.md", "AGENTS.md"] {
            let dest = dir.path().join(filename);
            if dest.exists() {
                let mut content = std::fs::read_to_string(&dest)?;
                content.push_str(REMOTE_APPENDIX);
                std::fs::write(&dest, &content)?;
            }
        }

        // Make all files read-only.
        make_readonly_recursive(dir.path())?;

        Ok(Self { dir })
    }

    /// Returns the path to the temporary context directory.
    pub fn path(&self) -> &Path {
        self.dir.path()
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let dest_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            std::fs::create_dir_all(&dest_path)?;
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            std::fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn make_readonly_recursive(dir: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    for entry in walkdir::WalkDir::new(dir) {
        let entry = entry?;
        if entry.file_type().is_file() {
            let mut perms = std::fs::metadata(entry.path())?.permissions();
            perms.set_mode(0o444);
            std::fs::set_permissions(entry.path(), perms)?;
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn make_readonly_recursive(dir: &Path) -> anyhow::Result<()> {
    for entry in walkdir::WalkDir::new(dir) {
        let entry = entry?;
        if entry.file_type().is_file() {
            let mut perms = std::fs::metadata(entry.path())?.permissions();
            perms.set_readonly(true);
            std::fs::set_permissions(entry.path(), perms)?;
        }
    }
    Ok(())
}

pub use tddy_workflow_recipes::permissions::build_remote_allowlist;

/// Parse a JSON array of tool names (as produced by `tddy-tools remote list-tools`) and
/// return the remote allowlist built by [`build_remote_allowlist`].
///
/// Returns `Err` when `tools_json` is not valid JSON or not a JSON array of strings.
pub fn run_remote_with_tools_output(tools_json: &str) -> anyhow::Result<Vec<String>> {
    let names: Vec<String> = serde_json::from_str(tools_json)?;
    let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
    Ok(build_remote_allowlist(&refs))
}
