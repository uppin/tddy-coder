//! Git worktree listing, stats cache, path policy, and removal helpers (Worktrees manager PRD).

use std::path::{Component, Path, PathBuf};
use std::process::Command;

use log::{debug, info, warn};
use serde::{Deserialize, Serialize};

/// One row from `git worktree list` after parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeListRow {
    pub path: PathBuf,
    /// Branch name, or a clear marker for detached HEAD (e.g. `(detached)`).
    pub branch_label: String,
    pub lock_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeStatSnapshot {
    pub path: PathBuf,
    pub branch_label: String,
    pub disk_bytes: u64,
    pub changed_files: u32,
    pub lines_added: i64,
    pub lines_removed: i64,
    pub updated_at_unix_ms: i64,
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorktreePathError {
    OutsideRepoRoot {
        repo_root: PathBuf,
        candidate: PathBuf,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoveWorktreeError {
    GitFailed { message: String },
    NotListed,
    CannotRemovePrimary,
    Io(String),
}

impl std::fmt::Display for RemoveWorktreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GitFailed { message } => f.write_str(message),
            Self::NotListed => f.write_str("the path is not one of the repository's worktrees"),
            Self::CannotRemovePrimary => {
                f.write_str("the repository's main worktree is never removed")
            }
            Self::Io(message) => write!(f, "git could not run: {message}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CleanWorktreeError {
    GitFailed { message: String },
    NotListed,
    CannotCleanPrimary,
    Io(String),
}

/// Parse `git worktree list` stdout into structured rows (fixtures: see acceptance tests).
///
/// Baseline format matches default `git worktree list` (non-porcelain): path, abbreviated
/// commit, then `[branch]` or `(detached HEAD)`. Detached rows are normalized to branch
/// label `(detached)` for UI consistency.
pub fn parse_git_worktree_list(stdout: &str) -> Vec<WorktreeListRow> {
    debug!("parse_git_worktree_list: {} bytes of stdout", stdout.len());
    let mut out = Vec::new();
    for line in stdout.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        if let Some(row) = parse_git_worktree_list_line(line) {
            out.push(row);
        }
    }
    info!(
        "parse_git_worktree_list: parsed {} worktree row(s)",
        out.len()
    );
    out
}

fn parse_git_worktree_list_line(line: &str) -> Option<WorktreeListRow> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    let (rest, branch_label, lock_path) = if let Some(detached) = line.find("(detached HEAD)") {
        let rest = line[..detached].trim_end();
        (rest, "(detached)".to_string(), None)
    } else if let Some(open) = line.rfind(" [") {
        let close = line.rfind(']')?;
        let branch = line[open + 2..close].to_string();
        let rest = line[..open].trim_end();
        (rest, branch, None)
    } else {
        warn!(
            "parse_git_worktree_list_line: unrecognized line: {:?}",
            line
        );
        return None;
    };

    let rest = rest.trim_end();
    let commit_end = rest.rfind(char::is_whitespace)?;
    let path_part = rest[..commit_end].trim_end();
    if path_part.is_empty() {
        return None;
    }

    Some(WorktreeListRow {
        path: PathBuf::from(path_part),
        branch_label,
        lock_path,
    })
}

/// Root directory for persisted per-project worktree stats (`{base}/projects/`).
pub fn projects_stats_cache_root(base: &Path) -> PathBuf {
    debug!("projects_stats_cache_root: base={:?}", base);
    base.join("projects")
}

/// Lexical path normalization (resolves `.` and `..`) without filesystem access.
/// Used so policy checks work when paths do not exist yet and to detect `..` escapes.
fn lexical_normalize(path: &Path) -> PathBuf {
    let mut stack: Vec<std::ffi::OsString> = Vec::new();
    let mut has_root = false;
    for c in path.components() {
        match c {
            Component::RootDir => {
                has_root = true;
                stack.clear();
            }
            Component::Prefix(p) => {
                stack.push(p.as_os_str().to_owned());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if !stack.is_empty() {
                    stack.pop();
                }
            }
            Component::Normal(s) => stack.push(s.to_owned()),
        }
    }
    let mut out = PathBuf::new();
    if has_root {
        out.push(Component::RootDir);
    }
    for s in stack {
        out.push(s);
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    out
}

/// Ensure `candidate` resolves to a path under `repo_root` (lexical prefix policy).
pub fn validate_worktree_path_within_repo_root(
    repo_root: &Path,
    candidate: &Path,
) -> Result<PathBuf, WorktreePathError> {
    debug!(
        "validate_worktree_path_within_repo_root: repo={:?} candidate={:?}",
        repo_root, candidate
    );
    let resolved = if candidate.is_absolute() {
        lexical_normalize(candidate)
    } else {
        lexical_normalize(&repo_root.join(candidate))
    };
    let repo_norm = lexical_normalize(repo_root);
    if !resolved.starts_with(&repo_norm) {
        warn!(
            "validate_worktree_path_within_repo_root: rejected {:?} (not under {:?})",
            resolved, repo_norm
        );
        return Err(WorktreePathError::OutsideRepoRoot {
            repo_root: repo_root.to_path_buf(),
            candidate: candidate.to_path_buf(),
        });
    }
    info!("validate_worktree_path_within_repo_root: ok {:?}", resolved);
    Ok(resolved)
}

pub(crate) fn git_worktree_list_stdout(main_repo: &Path) -> String {
    match Command::new("git")
        .current_dir(main_repo)
        .args(["worktree", "list"])
        .output()
    {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        Ok(o) => {
            warn!(
                "git_worktree_list_stdout: git worktree list failed: {:?}",
                o.status
            );
            String::new()
        }
        Err(e) => {
            warn!("git_worktree_list_stdout: git worktree list: {}", e);
            String::new()
        }
    }
}

pub use crate::worktree_stats_cache::WorktreeStatsCache;

pub use crate::worktree_sizes::{
    WorktreeSizeCalculator, WorktreeSizeState, WorktreeSizeStatus, WorktreeSizeUpdate,
    DAEMON_SIZE_WALK_PERMITS,
};

pub use crate::worktree_diff::{git_diff_numstat, parse_git_diff_numstat, WorktreeNumstat};

pub(crate) fn paths_equal(a: &Path, b: &Path) -> bool {
    lexical_normalize(a) == lexical_normalize(b)
}

/// True when `worktree_path` matches one of the parsed `git worktree list` rows.
fn worktree_path_in_rows(rows: &[WorktreeListRow], worktree_path: &Path) -> bool {
    rows.iter().any(|r| paths_equal(&r.path, worktree_path))
}

pub use crate::worktree_diff::{
    list_worktree_diff_rows, project_worktree_snapshot, WorktreeDiffRow, WorktreeSnapshotRow,
};

/// True when `worktree_path` appears in `git worktree list` for `repo_root`. Used to gate
/// filesystem access to a worktree behind git's own membership view.
pub fn worktree_path_is_listed(repo_root: &Path, worktree_path: &Path) -> bool {
    let stdout = git_worktree_list_stdout(repo_root);
    let rows = parse_git_worktree_list(&stdout);
    worktree_path_in_rows(&rows, worktree_path)
}

/// Remove a secondary worktree via `git worktree remove` as the project OS user.
/// The path must appear in `git worktree list` for `repo_root` and must not be the primary
/// (first-listed) worktree. Worktrees may live outside the main repo directory (sibling paths).
pub fn remove_worktree_under_repo(
    repo_root: &Path,
    worktree_path: &Path,
) -> Result<(), RemoveWorktreeError> {
    info!(
        "remove_worktree_under_repo: repo_root={:?} worktree_path={:?}",
        repo_root, worktree_path
    );
    let out = Command::new("git")
        .current_dir(repo_root)
        .args(["worktree", "list"])
        .output()
        .map_err(|e| RemoveWorktreeError::Io(e.to_string()))?;
    if !out.status.success() {
        return Err(RemoveWorktreeError::GitFailed {
            message: format!("git worktree list failed: {:?}", out.status),
        });
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let rows = parse_git_worktree_list(&stdout);
    let listed = worktree_path_in_rows(&rows, worktree_path);
    if !listed {
        warn!(
            "remove_worktree_under_repo: path not in worktree list {:?}",
            worktree_path
        );
        return Err(RemoveWorktreeError::NotListed);
    }
    if let Some(first) = rows.first() {
        if paths_equal(&first.path, worktree_path) {
            warn!("remove_worktree_under_repo: refusing to remove primary worktree");
            return Err(RemoveWorktreeError::CannotRemovePrimary);
        }
    }

    let wt_str = worktree_path
        .to_str()
        .ok_or_else(|| RemoveWorktreeError::Io("worktree path is not valid UTF-8".to_string()))?;

    // `output()` rather than `status()`: git explains its refusals on stderr — "contains modified or
    // untracked files" is the usual one — and a `status()` child inherits the daemon's own stderr,
    // which puts that message somewhere nothing collects it and, under the TUI, straight through the
    // display. Captured, it becomes the reason the caller's error carries.
    let out = Command::new("git")
        .current_dir(repo_root)
        .args(["worktree", "remove", wt_str])
        .output()
        .map_err(|e| RemoveWorktreeError::Io(e.to_string()))?;

    if !out.status.success() {
        let msg = format!(
            "git worktree remove failed ({:?}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim_end()
        );
        warn!("remove_worktree_under_repo: {}", msg);
        return Err(RemoveWorktreeError::GitFailed { message: msg });
    }
    info!("remove_worktree_under_repo: removed {:?}", worktree_path);
    Ok(())
}

/// Remove `worktree` from `repo_root`'s repository whatever it holds — `git worktree remove
/// --force`, then `git worktree prune` — for a caller that has already established that nothing in
/// it would be lost (a confirmed close). A worktree already gone counts as removed: one removed
/// before leaves nothing to do, and one whose directory was deleted outside git leaves only its
/// registration, which the prune clears. `git worktree remove` runs only while the directory is
/// there, since git refuses a path it does not list.
pub fn force_remove_worktree_under_repo(
    repo_root: &Path,
    worktree_path: &Path,
) -> Result<(), RemoveWorktreeError> {
    info!(
        "force_remove_worktree_under_repo: repo_root={:?} worktree_path={:?}",
        repo_root, worktree_path
    );
    let wt_str = worktree_path
        .to_str()
        .ok_or_else(|| RemoveWorktreeError::Io("worktree path is not valid UTF-8".to_string()))?;
    if path_is_present_on(&tddy_fs::DiskFs, worktree_path)? {
        run_worktree_command(repo_root, &["worktree", "remove", "--force", wt_str])?;
    }
    run_worktree_command(repo_root, &["worktree", "prune"])?;
    worktree_is_gone_from(&tddy_fs::DiskFs, repo_root, worktree_path)
}

/// Whether `path` is on disk. Only "no such file" is absence: a path that cannot be looked at (a
/// parent that cannot be searched) is the error, never read as gone.
pub fn path_is_present(path: &Path) -> Result<bool, RemoveWorktreeError> {
    path_is_present_on(&tddy_fs::DiskFs, path)
}

/// [`path_is_present`] through the filesystem port.
pub fn path_is_present_on(fs: &dyn tddy_fs::Fs, path: &Path) -> Result<bool, RemoveWorktreeError> {
    // `metadata`, as before: the question stays about the name itself, so a dangling link
    // counts as present and is removed as a link rather than reported missing.
    match fs.metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(RemoveWorktreeError::Io(format!(
            "{} cannot be looked at: {error}",
            path.display()
        ))),
    }
}

/// The post-condition of a removal: `worktree_path` is neither on disk nor among `repo_root`'s
/// worktrees.
fn worktree_is_gone_from(
    fs: &dyn tddy_fs::Fs,
    repo_root: &Path,
    worktree_path: &Path,
) -> Result<(), RemoveWorktreeError> {
    if path_is_present_on(fs, worktree_path)? {
        return Err(RemoveWorktreeError::Io(format!(
            "{} is still there after its removal",
            worktree_path.display()
        )));
    }
    let out = Command::new("git")
        .current_dir(repo_root)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .map_err(|e| RemoveWorktreeError::Io(e.to_string()))?;
    if !out.status.success() {
        return Err(RemoveWorktreeError::GitFailed {
            message: format!(
                "git worktree list failed ({:?}): {}",
                out.status,
                String::from_utf8_lossy(&out.stderr).trim_end()
            ),
        });
    }
    let still_listed = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .any(|listed| paths_equal(Path::new(listed), worktree_path));
    if still_listed {
        return Err(RemoveWorktreeError::GitFailed {
            message: format!(
                "the repository still lists {} among its worktrees after its removal",
                worktree_path.display()
            ),
        });
    }
    Ok(())
}

/// `git <args>` in `repo_root`, its stderr the error's message when it fails.
fn run_worktree_command(repo_root: &Path, args: &[&str]) -> Result<(), RemoveWorktreeError> {
    let out = Command::new("git")
        .current_dir(repo_root)
        .args(args)
        .output()
        .map_err(|e| RemoveWorktreeError::Io(e.to_string()))?;
    if out.status.success() {
        return Ok(());
    }
    Err(RemoveWorktreeError::GitFailed {
        message: format!(
            "git {} failed ({:?}): {}",
            args.join(" "),
            out.status,
            String::from_utf8_lossy(&out.stderr).trim_end()
        ),
    })
}

/// Clear a secondary worktree in place with `git clean -fdx` (drops untracked + ignored files,
/// e.g. build output, to reclaim disk without removing the worktree). The path must appear in
/// `git worktree list` for `repo_root` and must not be the primary (first-listed) worktree.
pub fn clean_worktree_under_repo(
    repo_root: &Path,
    worktree_path: &Path,
) -> Result<(), CleanWorktreeError> {
    info!(
        "clean_worktree_under_repo: repo_root={:?} worktree_path={:?}",
        repo_root, worktree_path
    );
    let out = Command::new("git")
        .current_dir(repo_root)
        .args(["worktree", "list"])
        .output()
        .map_err(|e| CleanWorktreeError::Io(e.to_string()))?;
    if !out.status.success() {
        return Err(CleanWorktreeError::GitFailed {
            message: format!("git worktree list failed: {:?}", out.status),
        });
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let rows = parse_git_worktree_list(&stdout);
    let listed = worktree_path_in_rows(&rows, worktree_path);
    if !listed {
        warn!(
            "clean_worktree_under_repo: path not in worktree list {:?}",
            worktree_path
        );
        return Err(CleanWorktreeError::NotListed);
    }
    if let Some(first) = rows.first() {
        if paths_equal(&first.path, worktree_path) {
            warn!("clean_worktree_under_repo: refusing to clean primary worktree");
            return Err(CleanWorktreeError::CannotCleanPrimary);
        }
    }

    let status = Command::new("git")
        .current_dir(worktree_path)
        .args(["clean", "-fdx"])
        .status()
        .map_err(|e| CleanWorktreeError::Io(e.to_string()))?;

    if !status.success() {
        let msg = format!("git clean -fdx failed: {:?}", status);
        warn!("clean_worktree_under_repo: {}", msg);
        return Err(CleanWorktreeError::GitFailed { message: msg });
    }
    info!("clean_worktree_under_repo: cleaned {:?}", worktree_path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::worktree_sizes::directory_size_bytes_best_effort;

    use super::*;

    /// BUG: When the main repo has linked worktrees under `.worktrees/` (a subdirectory of the
    /// main checkout), `directory_size_bytes_best_effort` recurses into those nested worktree
    /// directories, inflating the main worktree's `disk_bytes` with bytes that are separately
    /// reported under each secondary worktree row.
    ///
    /// This test creates a realistic directory layout:
    ///   main_repo/
    ///     README.md          (100 bytes)
    ///     src/lib.rs         (200 bytes)
    ///     .worktrees/
    ///       wt1/
    ///         README.md      (100 bytes)
    ///         feature.rs     (500 bytes)
    ///
    /// The main worktree's own files total 300 bytes. But the current implementation reports
    /// 300 + 600 = 900, because it walks into `.worktrees/wt1/`.
    #[test]
    fn main_worktree_size_excludes_nested_worktree_directories() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let main_repo = tmp.path().join("repo");
        let wt_dir = main_repo.join(".worktrees").join("wt1");
        fs::create_dir_all(main_repo.join("src")).unwrap();
        fs::create_dir_all(&wt_dir).unwrap();

        let main_readme = vec![b'A'; 100];
        let main_lib = vec![b'B'; 200];
        let wt_readme = vec![b'C'; 100];
        let wt_feature = vec![b'D'; 500];

        fs::write(main_repo.join("README.md"), &main_readme).unwrap();
        fs::write(main_repo.join("src").join("lib.rs"), &main_lib).unwrap();
        fs::write(wt_dir.join("README.md"), &wt_readme).unwrap();
        fs::write(wt_dir.join("feature.rs"), &wt_feature).unwrap();

        let main_own_bytes: u64 = 100 + 200; // README.md + src/lib.rs
        let wt_bytes: u64 = 100 + 500; // wt1/README.md + wt1/feature.rs

        let reported = directory_size_bytes_best_effort(&tddy_fs::DiskFs, &main_repo);

        // BUG: currently reports main_own_bytes + wt_bytes (900) instead of main_own_bytes (300).
        assert_eq!(
            reported, main_own_bytes,
            "main worktree size must exclude nested .worktrees/ directory; \
             got {reported} (includes {wt_bytes} bytes from nested worktree)"
        );
    }

    /// Acceptance: parser maps branch and detached HEAD rows from a fixed fixture.
    #[test]
    fn worktree_list_parsing_handles_detached_and_branch_rows() {
        let fixture = r#"/tmp/demo-main                 abcd123 [main]
/tmp/demo-main/.worktrees/wt1  efgh456 [feature-x]
/tmp/demo-main/.worktrees/wt2  1111111 (detached HEAD)
"#;
        let rows = parse_git_worktree_list(fixture);
        assert_eq!(rows.len(), 3, "expected three worktree rows");
        assert_eq!(rows[0].path, PathBuf::from("/tmp/demo-main"));
        assert_eq!(rows[0].branch_label, "main");
        assert_eq!(rows[1].branch_label, "feature-x");
        assert_eq!(rows[2].branch_label, "(detached)");
    }

    /// Acceptance: traversal / escape attempts outside `main_repo_path` are rejected.
    #[test]
    fn project_path_validation_rejects_traversal_outside_repo_root() {
        let repo = PathBuf::from("/tmp/tddy-accept-repo");
        let evil = PathBuf::from("/tmp/tddy-accept-repo/../../../etc/passwd");
        let err = validate_worktree_path_within_repo_root(&repo, &evil).unwrap_err();
        match err {
            WorktreePathError::OutsideRepoRoot { .. } => {}
        }
    }

    /// `git -C <dir> <args>`, which must succeed.
    fn git(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "t@t.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "t@t.com")
            .output()
            .expect("git runs");
        assert!(out.status.success(), "git {args:?}: {out:?}");
    }

    fn set_mode(directory: &Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(directory, fs::Permissions::from_mode(mode)).unwrap();
    }

    #[test]
    fn a_forced_removal_of_a_worktree_that_cannot_be_looked_at_is_refused_and_keeps_it() {
        // Given a repository whose worktree sits in a directory that cannot be searched
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo = tmp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["commit", "-q", "--allow-empty", "-m", "init"]);
        let worktree = repo.join(".worktrees").join("wt1");
        git(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "wt1",
                worktree.to_str().unwrap(),
            ],
        );
        set_mode(&repo.join(".worktrees"), 0o000);

        // When
        let removed = force_remove_worktree_under_repo(&repo, &worktree);
        set_mode(&repo.join(".worktrees"), 0o755);

        // Then
        assert_eq!(
            (
                removed.map_err(|error| error.to_string().contains("cannot be looked at")),
                worktree.is_dir(),
            ),
            (Err(true), true)
        );
    }

    #[test]
    fn a_forced_removal_leaves_the_worktree_neither_on_disk_nor_listed() {
        // Given a repository with a dirty worktree
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo = tmp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["commit", "-q", "--allow-empty", "-m", "init"]);
        let worktree = repo.join(".worktrees").join("wt1");
        git(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "wt1",
                worktree.to_str().unwrap(),
            ],
        );
        fs::write(worktree.join("notes.md"), "dirty\n").unwrap();

        // When
        let removed = force_remove_worktree_under_repo(&repo, &worktree);

        // Then
        assert_eq!(
            (
                removed.map_err(|error| error.to_string()),
                worktree.exists(),
                worktree_path_is_listed(&repo, &worktree),
            ),
            (Ok(()), false, false)
        );
    }
}
