//! The only component that touches the filesystem.
//!
//! Renames go through `git mv` so history and blame survive a restructure; engine-authored new
//! files are `git add -N`'d before their text edits land. There is no `fs::rename` fallback — a
//! non-git worktree is a hard error.

use crate::edit::{FileEdit, Range, TextEdit, WorkspaceEdit};
use crate::{RestructureError, Result};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

/// Apply one operation's complete edit set to disk.
///
/// Creations are materialised first so that a text edit addressed to a file the engine just
/// invented has somewhere to land.
pub fn apply_workspace_edit(root: &Path, edit: &WorkspaceEdit) -> Result<()> {
    for change in &edit.changes {
        if let FileEdit::Create { path } = change {
            create_tracked_file(root, path)?;
        }
    }
    for change in &edit.changes {
        if let FileEdit::Change { path, edits } = change {
            apply_text_edits(&root.join(path), edits)?;
        }
    }
    for change in &edit.changes {
        if let FileEdit::Rename { from, to } = change {
            git_move(root, from, to)?;
        }
    }
    Ok(())
}

/// Confirm `root` is inside a git worktree. `git mv` is mandatory, so this is checked up front.
pub fn ensure_git_worktree(root: &Path) -> Result<()> {
    let inside = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(root)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);

    if inside {
        Ok(())
    } else {
        Err(RestructureError::NotAGitWorktree {
            path: root.display().to_string(),
        })
    }
}

/// SHA-256 of every path the edit touches, for the journal's pre/post records.
pub fn hash_touched_files(root: &Path, edit: &WorkspaceEdit) -> Result<BTreeMap<String, String>> {
    let mut hashes = BTreeMap::new();
    for path in touched_paths(edit) {
        hashes.insert(path.clone(), hash_file(&root.join(&path))?);
    }
    Ok(hashes)
}

/// SHA-256 of one file. A file that does not exist hashes as empty, so a creation's pre-state and
/// a deletion's post-state are both expressible.
pub fn hash_file(path: &Path) -> Result<String> {
    let contents = match std::fs::read(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error.into()),
    };
    Ok(format!("sha256:{:x}", Sha256::digest(&contents)))
}

/// Every path an edit reads or writes, deduplicated and ordered.
pub fn touched_paths(edit: &WorkspaceEdit) -> Vec<String> {
    let mut paths: Vec<String> = edit
        .changes
        .iter()
        .flat_map(|change| match change {
            FileEdit::Change { path, .. } | FileEdit::Create { path } => vec![path.clone()],
            FileEdit::Rename { from, to } => vec![from.clone(), to.clone()],
        })
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

/// Replace the given ranges in one file.
fn apply_text_edits(path: &Path, edits: &[TextEdit]) -> Result<()> {
    if edits.is_empty() {
        return Ok(());
    }

    let contents = std::fs::read_to_string(path)?;
    std::fs::write(path, edited(contents, edits)?)?;
    Ok(())
}

/// `contents` with every range replaced.
///
/// Ranges arriving in a single edit share one coordinate space, so they are applied last-first; that
/// way an earlier replacement cannot shift the offsets of one still to be applied.
///
/// This is the one place that folds edits into text. The dry run needs the same fold in memory, and
/// a second implementation of it would be free to disagree with the one that writes to disk.
pub fn edited(mut contents: String, edits: &[TextEdit]) -> Result<String> {
    let mut ordered: Vec<&TextEdit> = edits.iter().collect();
    ordered.sort_by_key(|edit| (edit.range.start.line, edit.range.start.col));

    for edit in ordered.into_iter().rev() {
        let span = byte_span(&contents, edit.range)?;
        contents.replace_range(span, &edit.new_text);
    }

    Ok(contents)
}

/// Resolve a one-based line/column range to a byte range in `contents`.
fn byte_span(contents: &str, range: Range) -> Result<std::ops::Range<usize>> {
    let start = byte_offset(contents, range.start.line, range.start.col)?;
    let end = byte_offset(contents, range.end.line, range.end.col)?;
    Ok(start..end.max(start))
}

fn byte_offset(contents: &str, line: u32, col: u32) -> Result<usize> {
    let mut offset = 0usize;
    for _ in 1..line {
        match contents[offset..].find('\n') {
            // Past the last line: address the end of the file, which is how an append is expressed.
            None => return Ok(contents.len()),
            Some(index) => offset += index + 1,
        }
    }
    let line_end = contents[offset..]
        .find('\n')
        .map_or(contents.len(), |i| offset + i + 1);
    let column_offset = contents[offset..line_end]
        .char_indices()
        .nth(col.saturating_sub(1) as usize)
        .map_or(line_end - offset, |(index, _)| index);
    Ok((offset + column_offset).min(contents.len()))
}

/// Move a file with `git mv`, creating the destination directory first. Preserves history.
fn git_move(root: &Path, from: &str, to: &str) -> Result<()> {
    if let Some(parent) = Path::new(to).parent() {
        std::fs::create_dir_all(root.join(parent))?;
    }
    run_git(root, &["mv", from, to])
}

/// Materialise a file the engine invented and register it with git, so its text edits are staged
/// as part of the restructure rather than left as an untracked stray.
fn create_tracked_file(root: &Path, path: &str) -> Result<()> {
    let absolute = root.join(path);
    if let Some(parent) = absolute.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if !absolute.exists() {
        std::fs::write(&absolute, "")?;
    }
    run_git(root, &["add", "-N", path])
}

/// What a git command printed, for the reads that need an answer rather than an effect.
///
/// Lives here because this module is the only one that touches the filesystem, and reading a blob out
/// of a ref is the same kind of access as moving a file into place.
pub fn git_output(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git").args(args).current_dir(root).output()?;
    if !output.status.success() {
        return Err(RestructureError::MalformedPlan(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn run_git(root: &Path, args: &[&str]) -> Result<()> {
    let output = Command::new("git").args(args).current_dir(root).output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(RestructureError::MalformedPlan(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{FileEdit, Position, Range, TextEdit, WorkspaceEdit};
    use crate::RestructureError;
    use std::process::Command;

    fn git_workspace() -> tempfile::TempDir {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/shapes.ts"), "export const a = 1;\n").unwrap();
        for args in [
            vec!["init", "--quiet"],
            vec!["add", "-A"],
            vec![
                "-c",
                "user.name=fixture",
                "-c",
                "user.email=fixture@example.com",
                "commit",
                "--quiet",
                "-m",
                "baseline",
            ],
        ] {
            Command::new("git")
                .args(args)
                .current_dir(root)
                .status()
                .unwrap();
        }
        workspace
    }

    fn git(root: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .unwrap();
        String::from_utf8(output.stdout).unwrap()
    }

    #[test]
    fn accepts_a_directory_inside_a_git_worktree() {
        let workspace = git_workspace();

        assert!(ensure_git_worktree(workspace.path()).is_ok());
    }

    /// `git mv` is how history survives a restructure, so a plain directory is a hard error rather
    /// than a quiet downgrade to `fs::rename`.
    #[test]
    fn refuses_a_directory_that_is_not_a_git_worktree() {
        let workspace = tempfile::tempdir().unwrap();

        let outcome = ensure_git_worktree(workspace.path());

        assert!(matches!(
            outcome,
            Err(RestructureError::NotAGitWorktree { .. })
        ));
    }

    #[test]
    fn writes_text_edits_into_the_named_file() {
        let workspace = git_workspace();
        let edit = WorkspaceEdit {
            changes: vec![FileEdit::Change {
                path: "src/shapes.ts".to_string(),
                edits: vec![TextEdit {
                    range: Range {
                        start: Position { line: 1, col: 1 },
                        end: Position { line: 2, col: 1 },
                    },
                    new_text: "export const a = 2;\n".to_string(),
                }],
            }],
        };

        apply_workspace_edit(workspace.path(), &edit).unwrap();

        let contents = std::fs::read_to_string(workspace.path().join("src/shapes.ts")).unwrap();
        assert_eq!(contents, "export const a = 2;\n");
    }

    #[test]
    fn records_a_move_as_a_git_rename() {
        let workspace = git_workspace();
        let edit = WorkspaceEdit {
            changes: vec![FileEdit::Rename {
                from: "src/shapes.ts".to_string(),
                to: "src/geometry/shapes.ts".to_string(),
            }],
        };

        apply_workspace_edit(workspace.path(), &edit).unwrap();

        let status = git(workspace.path(), &["status", "--short", "--renames"]);
        assert!(status.starts_with('R'), "expected a rename, got: {status}");
    }

    #[test]
    fn keeps_history_reachable_from_a_moved_file() {
        let workspace = git_workspace();
        let edit = WorkspaceEdit {
            changes: vec![FileEdit::Rename {
                from: "src/shapes.ts".to_string(),
                to: "src/geometry/shapes.ts".to_string(),
            }],
        };

        apply_workspace_edit(workspace.path(), &edit).unwrap();
        // `--follow` reads committed history, and the executor deliberately does not commit for
        // the caller. Committing the staged rename here is what the caller would do next.
        Command::new("git")
            .args([
                "-c",
                "user.name=fixture",
                "-c",
                "user.email=fixture@example.com",
                "commit",
                "--quiet",
                "-m",
                "move",
            ])
            .current_dir(workspace.path())
            .status()
            .unwrap();

        let history = git(
            workspace.path(),
            &[
                "log",
                "--follow",
                "--oneline",
                "--",
                "src/geometry/shapes.ts",
            ],
        );
        assert!(history.contains("baseline"), "history was lost: {history}");
    }

    #[test]
    fn tracks_a_file_the_engine_created_so_its_edits_are_staged() {
        let workspace = git_workspace();
        let edit = WorkspaceEdit {
            changes: vec![FileEdit::Create {
                path: "src/geometry.ts".to_string(),
            }],
        };

        apply_workspace_edit(workspace.path(), &edit).unwrap();

        let status = git(workspace.path(), &["status", "--short", "src/geometry.ts"]);
        assert!(
            !status.starts_with("??"),
            "file was left untracked: {status}"
        );
    }

    #[test]
    fn hashes_every_file_an_edit_touches() {
        let workspace = git_workspace();
        let edit = WorkspaceEdit {
            changes: vec![FileEdit::Change {
                path: "src/shapes.ts".to_string(),
                edits: vec![],
            }],
        };

        let hashes = hash_touched_files(workspace.path(), &edit).unwrap();

        assert_eq!(hashes.len(), 1);
        assert!(hashes["src/shapes.ts"].starts_with("sha256:"));
    }
}
