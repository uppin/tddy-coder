//! The work queue's op journal beside the queue, through the filesystem port.

use std::path::PathBuf;

use super::GitEnv;

/// Beside the queue, in its owning session's directory (`#workqueue` 5/14).
pub(super) fn journal_path(env: &GitEnv) -> Result<PathBuf, String> {
    env.store
        .op_journal_path(&env.queue_id)
        .map_err(|e| e.to_string())
}

pub(super) fn write_journal(
    env: &GitEnv,
    journal: &tddy_work_queue::op_journal::WorkQueueOpJournal,
) -> Result<(), String> {
    write_journal_on(&tddy_fs::DiskFs, env, journal)
}

/// [`write_journal`] through the filesystem port.
pub(super) fn write_journal_on(
    fs: &dyn tddy_fs::Fs,
    env: &GitEnv,
    journal: &tddy_work_queue::op_journal::WorkQueueOpJournal,
) -> Result<(), String> {
    let path = journal_path(env)?;
    if let Some(parent) = path.parent() {
        fs.create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = serde_json::to_string_pretty(journal).map_err(|e| e.to_string())?;
    fs.write(&path, content.as_bytes())
        .map_err(|e| e.to_string())
}

/// Drop the journal beside the queue — the landed run leaves no op journal behind.
pub(super) fn remove_journal(env: &GitEnv) {
    remove_journal_on(&tddy_fs::DiskFs, env)
}

/// [`remove_journal`] through the filesystem port.
pub(super) fn remove_journal_on(fs: &dyn tddy_fs::Fs, env: &GitEnv) {
    if let Ok(path) = journal_path(env) {
        let _ = fs.remove_file(&path);
    }
}
