//! Measuring a checkout with `git`, under a deadline.
//!
//! The one place that runs `git` against a worktree somebody is working in, several times a second:
//! a hosted session room polls with it, and `GetWorktreeSnapshot` measures with it, so the two can
//! never quote different totals for one checkout. The counts are parsed by
//! [`crate::worktrees::parse_git_diff_numstat`], the same parser the Worktrees screen reads with.

use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use log::warn;
use tddy_daemon_extensions::WorktreeSnapshot;
use tddy_daemon_kernel::config::DEFAULT_SESSION_ROOM_GIT_TIMEOUT;

use crate::worktrees::{parse_git_diff_numstat, WorktreeNumstat};

/// Measure `worktree_root` as it stands right now, within [`DEFAULT_SESSION_ROOM_GIT_TIMEOUT`].
///
/// The counts come from the same `git diff --numstat HEAD` the Worktrees screen reads, parsed by
/// the same [`parse_git_diff_numstat`], so a room and the screen can never quote different totals
/// for one checkout. Untracked files are counted and nothing more: a HEAD-relative diff cannot
/// produce line counts for a path git does not track, and inventing them is exactly how the two
/// would diverge.
///
/// A checkout git cannot read snapshots as empty rather than failing — this feeds a periodic poll
/// whose only recourse is to try again on the next tick.
pub fn snapshot_worktree(worktree_root: &Path) -> WorktreeSnapshot {
    snapshot_worktree_within(worktree_root, DEFAULT_SESSION_ROOM_GIT_TIMEOUT)
}

/// [`snapshot_worktree`] with the whole measurement — every `git` it runs — bounded by `budget`.
///
/// The budget is a deadline over the sequence, not a per-command allowance, so a repository that
/// answers three commands slowly cannot spend three budgets: what the poll loop waits for is what
/// the operator configured (`session_room.git_timeout_ms`), once.
pub fn snapshot_worktree_within(worktree_root: &Path, budget: Duration) -> WorktreeSnapshot {
    let deadline = Instant::now() + budget;
    let numstat = numstat_within(worktree_root, deadline);
    WorktreeSnapshot {
        head_commit: git_stdout(worktree_root, &["rev-parse", "HEAD"], deadline),
        branch: git_stdout(
            worktree_root,
            &["rev-parse", "--abbrev-ref", "HEAD"],
            deadline,
        ),
        changed_paths: numstat.paths,
        changed_files: numstat.changed_files,
        lines_added: numstat.lines_added,
        lines_removed: numstat.lines_removed,
        untracked_files: untracked_file_count(worktree_root, deadline),
        // Deliberately NOT measured here. Writing a WIP tree is not a read: `git add -A` stages
        // the whole checkout and materialises loose blob and tree objects in the project's shared
        // object database. Doing that inside a function named "snapshot" would make every poll of
        // every hosted room — twice a second by default — leave objects behind that nothing
        // references, in a repository shared by every worktree, held for git's two-week prune
        // grace period.
        //
        // So the tree is measured by whoever is about to *use* it — the session room's
        // `write_wip_tree_within` — and published as a ref in the same breath (`publish_wip_ref`)
        // so the objects it writes are reachable from the moment they exist.
        wip_tree: String::new(),
    }
}

/// The working tree's diff against HEAD, run and parsed exactly as [`crate::worktrees`] does it for
/// the Worktrees screen — only under this module's deadline.
fn numstat_within(worktree_root: &Path, deadline: Instant) -> WorktreeNumstat {
    parse_git_diff_numstat(&git_stdout(
        worktree_root,
        &["diff", "--numstat", "HEAD"],
        deadline,
    ))
}

/// How many files in the checkout git has never been told about: the `??` entries of
/// `git status --porcelain`, and only those. Every other status line describes a path the numstat
/// diff has already accounted for.
fn untracked_file_count(worktree_root: &Path, deadline: Instant) -> u32 {
    git_stdout(worktree_root, &["status", "--porcelain"], deadline)
        .lines()
        .filter(|line| line.starts_with("??"))
        .count() as u32
}

/// Trimmed stdout of a git command in `worktree_root`, or the empty string when git could not
/// answer before `deadline`.
///
/// The child is **killed** on expiry rather than abandoned. This runs on a blocking-pool thread, and
/// a `git` that never returns — the stale `index.lock` case, or a network filesystem that stopped
/// answering — would otherwise cost one live process and one occupied pool thread *per poll*,
/// forever: at a two-second interval that is a room quietly eating a process-wide pool (512 threads
/// by default) that the spawn worker and every other blocking caller shares.
pub fn git_stdout(worktree_root: &Path, args: &[&str], deadline: Instant) -> String {
    git_stdout_with(worktree_root, args, &[], deadline)
}

/// [`git_stdout`] with extra environment for the child — `GIT_INDEX_FILE` for the WIP tree, the
/// identity the session room's `publish_wip_ref` signs with — and the same "empty string when git could not answer".
pub fn git_stdout_with(
    worktree_root: &Path,
    args: &[&str],
    envs: &[(&str, &OsStr)],
    deadline: Instant,
) -> String {
    match git_output(worktree_root, args, envs, deadline) {
        Ok(stdout) => String::from_utf8_lossy(&stdout).trim().to_string(),
        Err(reason) => {
            warn!("snapshot_worktree: {reason}");
            String::new()
        }
    }
}

/// The raw stdout of a git command in `worktree_root`, or why it could not be had.
///
/// Bytes rather than a string, and un-trimmed, because a patch is neither: a `git diff --binary` of
/// a checkout holding a file that is not valid UTF-8 would not survive being decoded, and a patch
/// missing its final newline is one `git apply` rejects.
///
/// The failure is returned rather than logged here because the callers differ in what a failure
/// means: a measurement that could not be taken is a warning and an empty reading, while a WIP ref
/// that could not be published is an error its caller has to see.
pub fn git_output(
    worktree_root: &Path,
    args: &[&str],
    envs: &[(&str, &OsStr)],
    deadline: Instant,
) -> Result<Vec<u8>, String> {
    let budget = deadline.saturating_duration_since(Instant::now());
    if budget.is_zero() {
        return Err(format!(
            "no time left for git {args:?} in {worktree_root:?}; the measurement is incomplete"
        ));
    }
    let mut command = Command::new("git");
    for (name, value) in envs {
        command.env(name, value);
    }
    let mut child = match command
        .current_dir(worktree_root)
        .args(args)
        // This measures a checkout somebody is *working in*, several times a second. `git status`
        // and `git diff` refresh the index as a side effect, which takes `index.lock` — the same
        // lock the developer's own `git add`, `git commit` or rebase needs, and which only one
        // process can hold. Polling would then both lose its own measurement to whoever holds it
        // and fail the developer's git for as long as this one runs. `GIT_OPTIONAL_LOCKS=0` is
        // git's own answer for read-only observers: skip the refresh rather than take the lock.
        .env("GIT_OPTIONAL_LOCKS", "0")
        // A git that decides to ask for credentials would sit on a prompt no one can answer until
        // the kill below; closed stdin makes it fail instead.
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            return Err(format!(
                "git {args:?} in {worktree_root:?} could not be run: {e}"
            ))
        }
    };

    // Drained on its own thread, and the thread's completion is what the deadline is measured
    // against: git blocks once it has written ~64 KB into an unread pipe, so a caller that watched
    // only the exit status would time out and kill a healthy `git diff` over a large change set.
    // EOF here means git closed stdout, which it does as it exits.
    let Some(stdout) = child.stdout.take() else {
        return Err(format!(
            "git {args:?} in {worktree_root:?} produced no stdout pipe"
        ));
    };
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        // Bytes, decoded lossily below: a repository can hold a path that is not valid UTF-8, and
        // losing the whole measurement over one such filename would be worse than showing it with
        // replacement characters.
        let mut collected = Vec::new();
        let read = std::io::Read::read_to_end(&mut std::io::BufReader::new(stdout), &mut collected);
        let _ = finished_tx.send(read.map(|_| collected));
    });

    match finished_rx.recv_timeout(budget) {
        Ok(Ok(collected)) => match child.wait() {
            Ok(status) if status.success() => Ok(collected),
            Ok(status) => Err(format!("git {args:?} in {worktree_root:?} exited {status}")),
            Err(e) => Err(format!(
                "git {args:?} in {worktree_root:?} could not be waited on: {e}"
            )),
        },
        Ok(Err(e)) => Err(format!(
            "reading git {args:?} in {worktree_root:?} failed: {e}"
        )),
        Err(_) => {
            // Killed *and* reaped: a `git` left running holds an index lock and a file handle on a
            // directory the daemon may be about to remove, and an unreaped one stays a zombie for
            // the life of the daemon.
            let _ = child.kill();
            let _ = child.wait();
            Err(format!(
                "git {args:?} in {worktree_root:?} did not answer within {}ms and was killed",
                budget.as_millis()
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "t@t.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "t@t.com")
            .output()
            .expect("git must be on PATH");
        assert!(
            output.status.success(),
            "git {args:?} failed in {dir:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// A checkout with one committed file and one untracked file beside it.
    fn a_checkout_with_an_untracked_file() -> tempfile::TempDir {
        let repo = tempfile::tempdir().unwrap();
        git(repo.path(), &["init", "-q", "-b", "main"]);
        std::fs::write(repo.path().join("shipped.txt"), "shipped\n").unwrap();
        git(repo.path(), &["add", "shipped.txt"]);
        git(repo.path(), &["commit", "-qm", "seed"]);
        std::fs::write(repo.path().join("scratch.txt"), "not added\n").unwrap();
        repo
    }

    /// Take the index lock the way a developer's own `git add` does for the moment it runs.
    fn while_another_git_holds_the_index_lock(repo: &Path) {
        std::fs::write(repo.join(".git").join("index.lock"), b"").unwrap();
    }

    #[test]
    fn a_checkout_is_measured_while_another_git_holds_the_index_lock() {
        // Given a checkout whose index is locked, as it is for the moment any `git add`, `git
        // commit` or rebase in it is running
        let repo = a_checkout_with_an_untracked_file();
        while_another_git_holds_the_index_lock(repo.path());

        // When the room polls it
        let snapshot = snapshot_worktree(repo.path());

        // Then the poll still measured the checkout. This runs against a worktree a developer is
        // working in, several times a second: a measurement that needs the index lock both loses
        // the race — reporting `head_commit: ""`, which reads as a commit to the empty sha — and
        // takes the lock often enough to fail the developer's own git.
        assert_eq!(snapshot.branch, "main");
        assert_eq!(snapshot.untracked_files, 1);
        assert!(
            !snapshot.head_commit.is_empty(),
            "HEAD must still resolve while the index is locked"
        );
    }
}
