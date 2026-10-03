//! What a conversation worktree starts from: the repository's common dir, the exclude entry that
//! keeps the worktrees out of the caller's status, and the caller's uncommitted state as a commit.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::git::{git, git_raw};
use crate::worktree::{WorktreeError, SUBAGENT_WORKTREES_DIR};

/// The repository's common dir, shared by the main checkout and every linked worktree — where
/// `info/exclude` and the refs live.
pub(crate) async fn common_dir(caller: &Path) -> Result<PathBuf, WorktreeError> {
    let (_, output) = git_raw(caller, ["rev-parse", "--git-common-dir"], &[], None).await?;
    if !output.success {
        return Err(WorktreeError::NotARepository(caller.to_path_buf()));
    }
    // Relative to `caller` when git reports it so; `join` keeps an absolute answer as it is.
    Ok(caller.join(String::from_utf8_lossy(&output.stdout).trim()))
}

/// Add `/tmp/subagent-worktrees/` to `<common dir>/info/exclude` unless it is already there.
pub(crate) async fn exclude_conversation_worktrees(common: &Path) -> Result<(), WorktreeError> {
    let entry = format!("/{SUBAGENT_WORKTREES_DIR}/");
    let file = common.join("info").join("exclude");
    let io = |context: &str| {
        let context = format!("{context} {}", file.display());
        move |source| WorktreeError::Io { context, source }
    };
    let existing = match tokio::fs::read_to_string(&file).await {
        Ok(text) => text,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(source) => return Err(io("reading")(source)),
    };
    if existing.lines().any(|line| line.trim() == entry) {
        return Ok(());
    }
    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(&entry);
    updated.push('\n');
    tokio::fs::create_dir_all(common.join("info"))
        .await
        .map_err(io("creating the directory of"))?;
    tokio::fs::write(&file, updated)
        .await
        .map_err(io("writing"))
}

/// The commit a conversation starts from: the caller's `HEAD` when the caller is clean, else a commit
/// on top of `HEAD` holding the caller's uncommitted state — staged, unstaged and untracked, not
/// ignored. The caller's index, branch and files are never touched.
pub(crate) async fn base_commit(caller: &Path) -> Result<String, WorktreeError> {
    uncommitted_state_as_commit(caller, "Uncommitted changes inherited from the caller").await
}

/// `dir`'s `HEAD` when `dir` is clean, else a commit on top of `HEAD`, with `subject`, holding its
/// uncommitted state — staged, unstaged and untracked, not ignored. No branch points at the commit.
/// Built through a scratch index seeded from `dir`'s own, so `dir`'s index, branch and files are
/// never touched.
pub(crate) async fn uncommitted_state_as_commit(
    dir: &Path,
    subject: &str,
) -> Result<String, WorktreeError> {
    let head = git(dir, ["rev-parse", "HEAD"], &[], None)
        .await?
        .trim()
        .to_string();
    let index = git(dir, ["rev-parse", "--git-path", "index"], &[], None).await?;
    let index = dir.join(index.trim());
    let scratch = scratch_path(&index);
    let result = snapshot_tree(dir, &index, &scratch, &head, subject).await;
    // Best effort: a scratch file left behind is harmless, and must not mask the snapshot's result.
    if let Err(error) = tokio::fs::remove_file(&scratch).await {
        log::debug!("scratch index {} not removed: {error}", scratch.display());
    }
    result
}

async fn snapshot_tree(
    dir: &Path,
    index: &Path,
    scratch: &Path,
    head: &str,
    subject: &str,
) -> Result<String, WorktreeError> {
    tokio::fs::copy(index, scratch)
        .await
        .map_err(|source| WorktreeError::Io {
            context: format!("seeding the scratch index from {}", index.display()),
            source,
        })?;
    let env = [("GIT_INDEX_FILE", scratch.as_os_str())];
    // The conversation worktrees are kept out by `info/exclude`, which `ensure` writes first. An
    // explicit `:(exclude)` pathspec for the same directory makes `git add` refuse once that
    // directory exists but is empty ("paths are ignored"), as it is after a worktree was deleted.
    git(dir, ["add", "-A", "--", "."], &env, None).await?;
    let tree = git(dir, ["write-tree"], &env, None).await?;
    let tree = tree.trim();
    let head_tree = git(dir, ["rev-parse", "HEAD^{tree}"], &[], None).await?;
    if tree == head_tree.trim() {
        return Ok(head.to_string());
    }
    let commit = git(
        dir,
        ["commit-tree", tree, "-p", head, "-m", subject],
        &[],
        None,
    )
    .await?;
    Ok(commit.trim().to_string())
}

/// A path beside `index` no other call of this process, or of another one, is using: the process
/// id tells processes apart and a counter tells this process's calls apart.
pub(crate) fn scratch_path(index: &Path) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    // Beside the index it is seeded from, so it is on the same filesystem and in the same git dir.
    let name = format!(
        "index.tddy-subagent-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    index.with_file_name(name)
}
