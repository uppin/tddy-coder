use std::path::Path;

/// Appended to CLAUDE.md and AGENTS.md in the sandbox read-only context dir.
pub const SANDBOX_REMOTE_APPENDIX: &str = r#"

## Appendix: Remote Codebase

The real codebase is REMOTE — it is NOT in this local directory.
This local directory is read-only and contains only documentation and synced skills.

You MUST use the `mcp__tddy-tools__*` tools (Read, Write, StrReplace, Delete, Grep, Glob, Shell,
Await, ReadLints, SemanticSearch) for ALL file and shell operations.
Do not use native tools to interact with the codebase.
"#;

/// RAII wrapper for a read-only context directory used inside the sandbox.
pub struct SandboxContextDir {
    dir: tempfile::TempDir,
}

impl SandboxContextDir {
    /// Creates a read-only temp context dir by copying files from `source_dir`.
    pub fn create(source_dir: &Path) -> anyhow::Result<Self> {
        let dir = tempfile::tempdir()?;
        copy_dir_recursive(source_dir, dir.path())?;

        for filename in &["CLAUDE.md", "AGENTS.md"] {
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
        };

        // When / Then
        assert!(spec.validate().is_err());
    }
}
