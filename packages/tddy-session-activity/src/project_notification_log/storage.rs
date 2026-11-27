//! The project notification log's JSON form (`#home-inbox` 7/8): the line a record is persisted as,
//! and the object a record is answered as. They are one shape, so a record a wait returns is the
//! record the file holds.
//!
//! A project's file is `<data dir>/projects/<project_id>/notifications.jsonl`, one JSON object per
//! line. Lines are appended as records arrive, and the file is rewritten to the log's window when
//! it has grown well past it, so it stays bounded. Sequence numbers live in the lines, so a replay
//! continues them and a number is never reused.

mod dto;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{Held, ProjectNotificationRecord};
use crate::project_notification_log::storage::dto::RecordDto;

const NOTIFICATIONS_FILE: &str = "notifications.jsonl";
const LOG_TARGET: &str = "tddy_session_activity::project_notification_log";

/// A record as the answers of the two tools spell it.
pub fn record_json(
    record: &ProjectNotificationRecord,
) -> Result<serde_json::Value, serde_json::Error> {
    serde_json::to_value(RecordDto::from(record))
}

/// [`record_json`], naming `this_host` as the host the record's session runs on when another host
/// did not relay it here: every record an orchestrator is handed names its session's host (n5).
pub fn record_json_on(
    record: &ProjectNotificationRecord,
    this_host: &str,
) -> Result<serde_json::Value, serde_json::Error> {
    let mut json = record_json(record)?;
    if let Some(object) = json.as_object_mut() {
        object
            .entry("daemon_instance_id")
            .or_insert_with(|| serde_json::Value::String(this_host.to_string()));
    }
    Ok(json)
}

/// A record from the JSON [`record_json`] spells it — how a record relayed from another host
/// arrives (n5).
pub fn record_from_json(json: &str) -> Result<ProjectNotificationRecord, serde_json::Error> {
    serde_json::from_str::<RecordDto>(json).map(ProjectNotificationRecord::from)
}

/// Where `project_id`'s records are kept under `data_dir`. An id that cannot name a directory is
/// refused rather than joined onto the path.
pub(super) fn project_file(data_dir: &Path, project_id: &str) -> anyhow::Result<PathBuf> {
    let names_a_directory = !project_id.is_empty()
        && project_id != "."
        && project_id != ".."
        && !project_id.contains(['/', '\\', '\0']);
    anyhow::ensure!(
        names_a_directory,
        "project id {project_id:?} cannot name a directory"
    );
    Ok(projects_dir(data_dir)
        .join(project_id)
        .join(NOTIFICATIONS_FILE))
}

fn projects_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("projects")
}

/// One project's file as it was read back at start.
pub(super) struct Replayed {
    pub(super) project_id: String,
    /// Its records, in the order the file holds them.
    pub(super) records: Vec<Held>,
    /// The lines the file holds, the malformed ones included.
    pub(super) lines: usize,
}

/// Every project with a notification file under `data_dir`. A daemon that has never recorded one
/// has none, and an unreadable file is an error, so a log is never started over records it could
/// not read.
pub(super) fn replay_all(fs: &dyn tddy_fs::Fs, data_dir: &Path) -> anyhow::Result<Vec<Replayed>> {
    let dir = projects_dir(data_dir);
    let entries = match fs.read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(anyhow::anyhow!("read {}: {e}", dir.display())),
    };
    let mut replayed = Vec::new();
    for entry in entries {
        let Some(project_id) = entry
            .file_name()
            .and_then(|n| n.to_str().map(str::to_string))
        else {
            continue;
        };
        let path = entry.join(NOTIFICATIONS_FILE);
        // `metadata_follow`, as before: `is_file` follows a link.
        if !matches!(fs.metadata_follow(&path), Ok(m) if m.kind == tddy_fs::FsKind::File) {
            continue;
        }
        let (records, lines) = replay_file(fs, &path)?;
        replayed.push(Replayed {
            project_id,
            records,
            lines,
        });
    }
    Ok(replayed)
}

fn replay_file(fs: &dyn tddy_fs::Fs, path: &Path) -> anyhow::Result<(Vec<Held>, usize)> {
    end_on_a_line_break(fs, path)?;
    let text = fs
        .read_to_string(path)
        .map_err(|e| anyhow::anyhow!("read {}: {e}", path.display()))?;
    let mut records = Vec::new();
    let mut lines = 0;
    for (number, line) in text.lines().enumerate() {
        let line = line.to_string();
        if line.trim().is_empty() {
            continue;
        }
        lines += 1;
        match serde_json::from_str::<Line>(&line) {
            Ok(stored) => records.push(stored.into()),
            Err(e) => log::warn!(
                target: LOG_TARGET,
                "skipping line {} of {}: {e}",
                number + 1,
                path.display()
            ),
        }
    }
    Ok((records, lines))
}

/// A crash part-way through an append leaves a line with no line break; the next append would
/// then run on into it and be lost with it. Ending the file on a line break first keeps the torn
/// line alone, where the replay skips it.
fn end_on_a_line_break(fs: &dyn tddy_fs::Fs, path: &Path) -> anyhow::Result<()> {
    // The port has no seek, so the file is read whole and only its last byte decides.
    let bytes = fs
        .read(path)
        .map_err(|e| anyhow::anyhow!("read {}: {e}", path.display()))?;
    match bytes.last() {
        None | Some(b'\n') => Ok(()),
        Some(_) => {
            log::warn!(target: LOG_TARGET, "{} ends in a torn line; ending it", path.display());
            fs.append(path, b"\n")
                .map_err(|e| anyhow::anyhow!("write {}: {e}", path.display()))
        }
    }
}

/// Append `held` to the file at `path`, creating its directory and file as needed.
pub(super) fn append(fs: &dyn tddy_fs::Fs, path: &Path, held: &Held) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs.create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("create {}: {e}", parent.display()))?;
    }
    let mut line = serde_json::to_string(&Line::from(held))
        .map_err(|e| anyhow::anyhow!("serialize a notification record: {e}"))?;
    line.push('\n');
    fs.append(path, line.as_bytes())
        .map_err(|e| anyhow::anyhow!("write {}: {e}", path.display()))
}

/// Replace the file at `path` with `window`'s records, written to a sibling and renamed over it so
/// a failure part-way leaves the previous file whole.
pub(super) fn rewrite<'a>(
    fs: &dyn tddy_fs::Fs,
    path: &Path,
    window: impl Iterator<Item = &'a Held>,
) -> anyhow::Result<()> {
    let mut body = String::new();
    for held in window {
        let line = serde_json::to_string(&Line::from(held))
            .map_err(|e| anyhow::anyhow!("serialize a notification record: {e}"))?;
        body.push_str(&line);
        body.push('\n');
    }
    // The port's atomic write is the swap-then-rename this did by hand, with the same
    // durability: bytes synced to disk before the rename, so a crash never leaves an empty or
    // short file where the previous one stood.
    fs.write_atomic(path, body.as_bytes(), None)
        .map_err(|e| anyhow::anyhow!("write {}: {e}", path.display()))
}

// ── the JSON form ────────────────────────────────────────────────────────────────────────────────

/// One line of the file: the record, and the OS user whose session made it.
#[derive(Serialize, Deserialize)]
struct Line {
    os_user: String,
    #[serde(flatten)]
    record: RecordDto,
}

impl From<&Held> for Line {
    fn from(held: &Held) -> Self {
        Self {
            os_user: held.os_user.clone(),
            record: RecordDto::from(&held.record),
        }
    }
}

impl From<Line> for Held {
    fn from(line: Line) -> Self {
        Self {
            os_user: line.os_user,
            record: line.record.into(),
        }
    }
}
