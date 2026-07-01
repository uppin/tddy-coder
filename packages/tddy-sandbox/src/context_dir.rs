use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Appended to CLAUDE.md and AGENTS.md in the sandbox read-only context dir.
pub const SANDBOX_REMOTE_APPENDIX: &str = r#"

## Appendix: Managed Codebase

The real codebase is MANAGED — it is NOT in this local directory.
This local directory is read-only and contains only documentation and synced skills.

You MUST use the `mcp__tddy-tools__*` tools (Read, Write, StrReplace, Delete, Grep, Glob, Shell,
Await, ReadLints, SemanticSearch) for ALL file and shell operations.
Do not use native tools to interact with the codebase.
"#;

/// Root guidance files copied into the sandbox context dir.
const CONTEXT_ROOT_FILES: &[&str] = &["CLAUDE.md", "AGENTS.md"];

/// Directories copied into the sandbox context dir (docs + skills only — not the codebase).
const CONTEXT_DIRS: &[&str] = &[".claude", ".agents", "skills", "docs"];

/// RAII wrapper for a read-only context directory used inside the sandbox.
pub struct SandboxContextDir {
    dir: tempfile::TempDir,
}

impl SandboxContextDir {
    /// Creates a read-only temp context dir by copying guidance files from `source_dir`.
    ///
    /// Only `CLAUDE.md`, `AGENTS.md`, and documentation/skills trees are copied — not the full
    /// repository (which may contain symlinks and build artifacts that break naive `fs::copy`).
    pub fn create(source_dir: &Path) -> anyhow::Result<Self> {
        let dir = tempfile::tempdir()?;
        copy_context_from_repo(source_dir, dir.path())?;

        for filename in CONTEXT_ROOT_FILES {
            let dest = dir.path().join(filename);
            if dest.exists() {
                let mut content = std::fs::read_to_string(&dest)?;
                content.push_str(SANDBOX_REMOTE_APPENDIX);
                std::fs::write(&dest, &content)?;
            }
        }

        make_readonly_recursive(dir.path())?;
        Ok(Self { dir })
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }
}

/// Copy guidance files and skills from a repo/worktree into a context directory.
pub fn copy_context_from_repo(source: &Path, dest: &Path) -> anyhow::Result<()> {
    let source_root = std::fs::canonicalize(source)?;
    std::fs::create_dir_all(dest)?;
    let mut visited = HashSet::new();
    for name in CONTEXT_ROOT_FILES {
        let src = source.join(name);
        if src.exists() {
            copy_tree_within_root(&src, &dest.join(name), &source_root, &mut visited)?;
        }
    }
    for name in CONTEXT_DIRS {
        let src = source.join(name);
        if src.exists() {
            copy_tree_within_root(&src, &dest.join(name), &source_root, &mut visited)?;
        }
    }
    Ok(())
}

/// Recursively copy a file or directory tree, following symlinks with cycle detection.
pub fn copy_tree(src: &Path, dst: &Path) -> anyhow::Result<()> {
    copy_tree_inner(src, dst, None, &mut HashSet::new())
}

/// Like [`copy_tree`] but skips symlinks that resolve outside `root` (prevents copying `node_modules` via `.claude` links).
pub fn copy_tree_within_root(
    src: &Path,
    dst: &Path,
    root: &Path,
    visited: &mut HashSet<PathBuf>,
) -> anyhow::Result<()> {
    let root = std::fs::canonicalize(root)?;
    copy_tree_inner(src, dst, Some(&root), visited)
}

fn copy_tree_inner(
    src: &Path,
    dst: &Path,
    root: Option<&Path>,
    visited: &mut HashSet<PathBuf>,
) -> anyhow::Result<()> {
    let meta = std::fs::symlink_metadata(src)?;
    if meta.file_type().is_symlink() {
        let target = std::fs::read_link(src)?;
        let resolved = if target.is_absolute() {
            target
        } else {
            src.parent()
                .ok_or_else(|| anyhow::anyhow!("symlink without parent: {}", src.display()))?
                .join(target)
        };
        let canonical = std::fs::canonicalize(&resolved).unwrap_or(resolved);
        if let Some(root) = root {
            if !canonical.starts_with(root) {
                return Ok(());
            }
        }
        if !visited.insert(canonical.clone()) {
            return Ok(());
        }
        return copy_tree_inner(&canonical, dst, root, visited);
    }
    if meta.is_dir() {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            copy_tree_inner(&entry.path(), &dst.join(entry.file_name()), root, visited)?;
        }
        Ok(())
    } else if meta.is_file() {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, dst)?;
        Ok(())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn make_readonly_recursive(dir: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    for entry in walkdir::WalkDir::new(dir).follow_links(false) {
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
    for entry in walkdir::WalkDir::new(dir).follow_links(false) {
        let entry = entry?;
        if entry.file_type().is_file() {
            let mut perms = std::fs::metadata(entry.path())?.permissions();
            perms.set_readonly(true);
            std::fs::set_permissions(entry.path(), perms)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::SandboxSpec;

    #[test]
    fn sandbox_context_dir_appends_remote_appendix_to_claude_md() {
        // Given
        let source_dir = tempfile::tempdir().unwrap();
        std::fs::write(
            source_dir.path().join("CLAUDE.md"),
            "# CLAUDE.md\n\nOriginal instructions.\n",
        )
        .unwrap();

        // When
        let ctx = SandboxContextDir::create(source_dir.path()).expect("create must succeed");
        let claude_md =
            std::fs::read_to_string(ctx.path().join("CLAUDE.md")).expect("CLAUDE.md must exist");

        // Then
        assert!(claude_md.contains("Original instructions."));
        assert!(claude_md.contains("mcp__tddy-tools__"));
    }

    #[test]
    fn sandbox_context_dir_copies_only_guidance_not_full_repo() {
        // Given — repo layout with node_modules symlink that breaks naive fs::copy
        let source_dir = tempfile::tempdir().unwrap();
        std::fs::write(source_dir.path().join("CLAUDE.md"), "# project\n").unwrap();
        std::fs::write(source_dir.path().join("secret.rs"), "fn main() {}").unwrap();
        std::fs::create_dir_all(source_dir.path().join("node_modules/.bin")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            "../../../secret.rs",
            source_dir.path().join("node_modules/.bin/secret"),
        )
        .unwrap();

        // When
        let ctx = SandboxContextDir::create(source_dir.path()).expect("create must succeed");

        // Then — guidance copied, codebase and node_modules omitted
        assert!(ctx.path().join("CLAUDE.md").exists());
        assert!(!ctx.path().join("secret.rs").exists());
        assert!(!ctx.path().join("node_modules").exists());
    }

    #[test]
    fn copy_tree_follows_symlink_to_directory_within_repo() {
        // Given
        let root = tempfile::tempdir().unwrap();
        let real_dir = root.path().join("real-skills");
        std::fs::create_dir_all(real_dir.join("nested")).unwrap();
        std::fs::write(real_dir.join("nested/skill.md"), "skill").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("real-skills", root.path().join("skills")).unwrap();
        #[cfg(not(unix))]
        return;

        let dest = tempfile::tempdir().unwrap();
        let mut visited = HashSet::new();

        // When
        copy_tree_within_root(
            &root.path().join("skills"),
            &dest.path().join("skills"),
            root.path(),
            &mut visited,
        )
        .expect("copy_tree_within_root");

        // Then
        let copied = std::fs::read_to_string(dest.path().join("skills/nested/skill.md")).unwrap();
        assert_eq!(copied, "skill");
    }

    #[test]
    fn copy_context_skips_symlink_outside_repo() {
        // Given
        let source_dir = tempfile::tempdir().unwrap();
        std::fs::write(source_dir.path().join("CLAUDE.md"), "# ok\n").unwrap();
        std::fs::create_dir_all(source_dir.path().join(".claude")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("/etc/passwd", source_dir.path().join(".claude/leak")).unwrap();
        #[cfg(not(unix))]
        return;

        // When
        let ctx = SandboxContextDir::create(source_dir.path()).expect("create must succeed");

        // Then
        assert!(!ctx.path().join(".claude/leak").exists());
    }

    #[test]
    fn sandbox_spec_rejects_empty_command() {
        // Given
        let spec = SandboxSpec {
            project_root: "/tmp/project".into(),
            scratch_dir: "/tmp/project/.work".into(),
            egress_dir: "/tmp/project/out".into(),
            allow_read_paths: vec![],
            command: vec![],
            env: Default::default(),
            profile_path: "/tmp/project/profile.sb".into(),
            loopback_allow_ports: vec![],
            ipc_socket: None,
            cwd: None,
        };

        // When / Then
        assert!(spec.validate().is_err());
    }
}
