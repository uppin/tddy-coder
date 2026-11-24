use crate::worktree_sizes::{WorktreeSizeCalculator, WorktreeSizeState};
use crate::worktrees::git_worktree_list_stdout;
use crate::worktrees::parse_git_worktree_list;
use log::info;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

/// One `git diff --numstat HEAD` run: the paths it named, in git's order, and the totals over them.
///
/// The paths are what the Worktrees screen throws away and a session room's metadata keeps, so both
/// read the same measurement rather than each running their own diff and disagreeing about it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorktreeNumstat {
    /// The paths exactly as git printed them. Without `-z`, git C-quotes any path that is not plain
    /// ASCII (`"src/caf\303\251.rs"`) and writes a rename as `{old => new}`, so these are for
    /// display and comparison between two runs — not for opening a file with.
    pub paths: Vec<String>,
    pub changed_files: u32,
    pub lines_added: i64,
    pub lines_removed: i64,
}

/// Run `git diff --numstat HEAD` in `cwd` and parse it.
///
/// A repository git cannot read reports as no changes: this feeds a status display and a room
/// summary, neither of which has anywhere to put an error.
pub fn git_diff_numstat(cwd: &Path) -> WorktreeNumstat {
    let out = match Command::new("git")
        .current_dir(cwd)
        .args(["diff", "--numstat", "HEAD"])
        .output()
    {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return WorktreeNumstat::default(),
    };
    parse_git_diff_numstat(&out)
}

/// Parse `git diff --numstat` output. Each line is `added\tremoved\tpath`, with `-\t-` in place of
/// the counts for a binary file — which still changed, so it counts toward `changed_files` while
/// contributing no lines.
///
/// Shared with [`crate::session_room`], which runs the same command under its own deadline: the
/// room and the Worktrees screen must never quote different totals for one checkout, and one parser
/// is how that stays true.
pub fn parse_git_diff_numstat(stdout: &str) -> WorktreeNumstat {
    let mut summary = WorktreeNumstat::default();
    for line in stdout.lines() {
        let line = line.trim_end_matches(['\r', '\n']);
        if line.trim().is_empty() {
            continue;
        }
        let mut fields = line.splitn(3, '\t');
        let (a, b, path) = (fields.next(), fields.next(), fields.next());
        if a == Some("-") && b == Some("-") {
            summary.changed_files += 1;
            summary.paths.extend(path.map(str::to_string));
            continue;
        }
        if let (Some(a), Some(b)) = (a, b) {
            if let (Ok(ai), Ok(bi)) = (a.trim().parse::<i64>(), b.trim().parse::<i64>()) {
                summary.changed_files += 1;
                summary.lines_added += ai;
                summary.lines_removed += bi;
                summary.paths.extend(path.map(str::to_string));
            }
        }
    }
    summary
}

/// A worktree's identity plus its git diff summary, but **without** the on-disk size walk.
///
/// The streaming size RPC ([`crate::connection_service`]) builds rows from this and fills each
/// worktree's disk size lazily via [`WorktreeSizeCalculator`], so it never pays for the eager
/// directory walk that [`WorktreeStatsCache::refresh_stats_for_project`] performs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeDiffRow {
    pub path: PathBuf,
    pub branch_label: String,
    pub changed_files: u32,
    pub lines_added: i64,
    pub lines_removed: i64,
}

/// List `main_repo`'s worktrees with each one's `git diff --numstat` summary, skipping disk-size
/// calculation. Reuses the same `git worktree list` parse and diff summary as the stats refresh.
pub fn list_worktree_diff_rows(main_repo: &Path) -> Vec<WorktreeDiffRow> {
    let stdout = git_worktree_list_stdout(main_repo);
    let rows = parse_git_worktree_list(&stdout);
    info!(
        "list_worktree_diff_rows: {} worktree row(s) for repo {:?}",
        rows.len(),
        main_repo
    );
    rows.into_iter()
        .map(|row| {
            let numstat = git_diff_numstat(&row.path);
            WorktreeDiffRow {
                path: row.path,
                branch_label: row.branch_label,
                changed_files: numstat.changed_files,
                lines_added: numstat.lines_added,
                lines_removed: numstat.lines_removed,
            }
        })
        .collect()
}

/// What the Worktrees screen's snapshot holds for one worktree: its diff summary and the size state
/// the daemon's calculator has for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeSnapshotRow {
    pub diff: WorktreeDiffRow,
    pub size: WorktreeSizeState,
}

/// The snapshot of `main_repo`'s worktrees that `StreamWorktreeStats` opens with and the project-home
/// `ListWorktrees` tool reads: every worktree with its diff summary, and the size state `calculator`
/// holds for it now. Starts no size walk; that is the caller's to ask for.
pub fn project_worktree_snapshot(
    main_repo: &Path,
    calculator: &WorktreeSizeCalculator,
    project_id: &str,
) -> Vec<WorktreeSnapshotRow> {
    list_worktree_diff_rows(main_repo)
        .into_iter()
        .map(|diff| {
            let size = calculator.state(project_id, &diff.path);
            WorktreeSnapshotRow { diff, size }
        })
        .collect()
}
