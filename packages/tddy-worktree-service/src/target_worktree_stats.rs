//! The Worktree tab's rows for target sessions' worktrees (`#target-parity` 7/7).
//!
//! A target session's worktree is a directory on its target, so this host's `git worktree list`
//! never holds it. The project's stream adds one row per target session of the caller: first as
//! `CALCULATING` inside the local snapshot — which therefore never waits on a target — then, once
//! its target answered, as an `updated` row measured there (branch, `git diff --numstat`, `du`).
//! A target that cannot be reached, or does not answer within the request timeout, leaves the row
//! `stale` with no numbers.
//!
//! Feature: docs/ft/web/session-worktree-inspector.md § On a target session.

use std::collections::HashMap;
use std::path::Path;

use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use tddy_remote_shell::RemoteShell;
use tddy_rpc::Status;
use tddy_service::proto::worktree::{WorktreeRow, WorktreeSizeStatus, WorktreeStatsEvent};
use tokio::sync::broadcast;
use tokio::sync::mpsc::UnboundedSender;

use crate::service::WorktreeServiceImpl;
use crate::target_worktrees::{run_on_target, target_session_from, TargetSession};
use crate::worktrees::parse_git_diff_numstat;

const BUS_CAPACITY: usize = 64;

/// Target rows measured outside a stream (by `CalculateWorktreeSize`), fanned out per project to
/// the project's open streams.
#[derive(Default)]
pub struct TargetRowBus {
    senders: Mutex<HashMap<String, broadcast::Sender<WorktreeRow>>>,
}

impl TargetRowBus {
    fn sender(&self, project_id: &str) -> broadcast::Sender<WorktreeRow> {
        self.senders
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(project_id.to_string())
            .or_insert_with(|| broadcast::channel(BUS_CAPACITY).0)
            .clone()
    }

    fn subscribe(&self, project_id: &str) -> broadcast::Receiver<WorktreeRow> {
        self.sender(project_id).subscribe()
    }

    fn publish(&self, project_id: &str, row: WorktreeRow) {
        // No open stream is not an error: nobody is watching the row.
        let _ = self.sender(project_id).send(row);
    }
}

/// The target sessions of `project_id` under `sessions_dir`.
#[must_use]
pub fn target_sessions_of_project(
    fs: &dyn tddy_fs::Fs,
    sessions_dir: &Path,
    project_id: &str,
) -> Vec<TargetSession> {
    // `metadata_follow`, as before: `is_dir` follows a link.
    let Ok(entries) = fs.read_dir(sessions_dir) else {
        return Vec::new();
    };
    entries
        .into_iter()
        .filter(|dir| {
            matches!(
                fs.metadata_follow(dir),
                Ok(m) if m.kind == tddy_fs::FsKind::Dir
            )
        })
        .filter_map(|dir| {
            let metadata = tddy_core::read_session_metadata(&dir).ok()?;
            (metadata.project_id == project_id)
                .then(|| target_session_from(&dir, metadata))
                .flatten()
        })
        .collect()
}

/// The row of a worktree whose measurement is under way: no numbers yet.
#[must_use]
pub fn calculating_row(worktree: &str) -> WorktreeRow {
    WorktreeRow {
        path: worktree.to_string(),
        size_status: WorktreeSizeStatus::Calculating as i32,
        ..Default::default()
    }
}

fn stale_row(worktree: &str) -> WorktreeRow {
    WorktreeRow {
        path: worktree.to_string(),
        stale: true,
        size_status: WorktreeSizeStatus::None as i32,
        ..Default::default()
    }
}

fn quoted(text: &str) -> String {
    tddy_core::ssh_exec::shell_single_quote(text)
}

fn text_of(stdout: Vec<u8>) -> String {
    String::from_utf8_lossy(&stdout).into_owned()
}

/// Measure `worktree` on the target `shell` reaches. Any command that fails is an error.
async fn measure(shell: &dyn RemoteShell, worktree: &str) -> Result<WorktreeRow, Status> {
    let wt = quoted(worktree);
    let branch = text_of(run_on_target(shell, format!("git -C {wt} branch --show-current")).await?);
    let diff = text_of(run_on_target(shell, format!("git -C {wt} diff --numstat HEAD")).await?);
    let du = text_of(run_on_target(shell, format!("du -sk {wt}")).await?);
    let kibibytes: u64 = du
        .split_whitespace()
        .next()
        .and_then(|size| size.parse().ok())
        .ok_or_else(|| Status::internal(format!("`du` printed no size for {worktree}: {du}")))?;
    let numstat = parse_git_diff_numstat(&diff);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64);
    Ok(WorktreeRow {
        path: worktree.to_string(),
        branch_label: branch.trim().to_string(),
        disk_bytes: kibibytes * 1024,
        changed_files: numstat.changed_files,
        lines_added: numstat.lines_added,
        lines_removed: numstat.lines_removed,
        updated_at_unix_ms: now,
        stale: false,
        size_status: WorktreeSizeStatus::Cached as i32,
        size_calculated_at_unix_ms: now,
    })
}

/// Measure `session`'s worktree on its target in a task of its own, bounded by the service's
/// request timeout, and hand the row — `stale` when the target did not answer — to `deliver`.
pub(crate) fn measure_in_background(
    service: &WorktreeServiceImpl,
    session_token: &str,
    session: TargetSession,
    deliver: impl FnOnce(WorktreeRow) + Send + 'static,
) {
    let service = service.clone();
    let token = session_token.to_string();
    tokio::spawn(async move {
        let within = service.request_timeout();
        let measured = tokio::time::timeout(within, async {
            let shell = service.shell_of_target(&token, &session).await?;
            measure(shell.as_ref(), &session.worktree).await
        })
        .await;
        let row = match measured {
            Ok(Ok(row)) => row,
            Ok(Err(status)) => {
                log::warn!(
                    "target worktree {} is not measured: {}",
                    session.worktree,
                    status.message()
                );
                stale_row(&session.worktree)
            }
            Err(_) => {
                log::warn!(
                    "target worktree {} is not measured within {within:?}",
                    session.worktree
                );
                stale_row(&session.worktree)
            }
        };
        deliver(row);
    });
}

/// Add the project's target sessions to a stream: their `CALCULATING` rows join `snapshot`, each
/// measurement follows as an `updated` event on `tx`, and rows measured by `CalculateWorktreeSize`
/// are forwarded for as long as the stream is open.
pub(crate) fn add_target_rows(
    service: &WorktreeServiceImpl,
    session_token: &str,
    project_id: &str,
    sessions: Vec<TargetSession>,
    snapshot: &mut Vec<WorktreeRow>,
    tx: &UnboundedSender<WorktreeStatsEvent>,
) {
    if sessions.is_empty() {
        return;
    }
    let mut published = service.target_row_bus().subscribe(project_id);
    let paths: Vec<String> = sessions.iter().map(|s| s.worktree.clone()).collect();
    let forward = tx.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                () = forward.closed() => break,
                row = published.recv() => match row {
                    Ok(row) if paths.contains(&row.path) => {
                        let event = WorktreeStatsEvent { snapshot: Vec::new(), updated: Some(row) };
                        if forward.send(event).is_err() {
                            break;
                        }
                    }
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                },
            }
        }
    });
    for session in sessions {
        snapshot.push(calculating_row(&session.worktree));
        let tx = tx.clone();
        measure_in_background(service, session_token, session, move |row| {
            let _ = tx.send(WorktreeStatsEvent {
                snapshot: Vec::new(),
                updated: Some(row),
            });
        });
    }
}

/// Re-measure a target session's worktree for `CalculateWorktreeSize`: the row reaches the
/// project's open streams.
pub(crate) fn recalculate_target_row(
    service: &WorktreeServiceImpl,
    session_token: &str,
    project_id: &str,
    session: TargetSession,
) {
    let bus = Arc::clone(service.target_row_bus());
    let project_id = project_id.to_string();
    measure_in_background(service, session_token, session, move |row| {
        bus.publish(&project_id, row);
    });
}
