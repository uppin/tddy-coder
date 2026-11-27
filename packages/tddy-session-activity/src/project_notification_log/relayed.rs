//! Records another host relayed here, and what the relay reads of this host's own (n5, cross-host
//! watches).
//!
//! A relayed record is appended to the project's log like any other — a new local `seq`, the same
//! file, the same window — so every watch, inbox, wake and long poll of this host reads it as its
//! own. It keeps the host its session runs on ([`ProjectNotificationRecord::daemon_instance_id`])
//! and its `seq` there ([`ProjectNotificationRecord::origin_seq`]). Per origin host the project
//! remembers the highest origin `seq` it holds, so a batch relayed again — its acknowledgement lost
//! — appends nothing twice; that mark is recovered from the records on replay.
//!
//! A batch that says the origin evicted records before it could relay them appends a **relay gap**:
//! a record carrying the count in [`ProjectNotificationRecord::relay_lost`], which no reader lists
//! and no watch delivers as an event, but every watch reports as lost.

use std::collections::HashMap;

use tokio::sync::Notify;

use super::{append_held, storage, Held, ProjectNotificationLog, ProjectNotificationRecord};
use tddy_daemon_kernel::session_notification::{
    SessionNotificationKind, SessionNotificationSource,
};

/// What another host relayed for one project and one OS user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayedBatch {
    /// The host the records' sessions run on.
    pub origin: String,
    pub project_id: String,
    /// This host's OS user the records belong to.
    pub os_user: String,
    /// The records as the origin holds them, oldest first: each `seq` is the origin's.
    pub records: Vec<ProjectNotificationRecord>,
    /// How many of the origin's records for this host it evicted before relaying them.
    pub lost: u64,
    /// The highest origin `seq` the batch covers.
    pub through_seq: u64,
}

/// What appending a batch did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayedAppend {
    /// The highest origin `seq` the project now holds for the batch's origin: what is acknowledged.
    pub received_through: u64,
    /// The records appended, as this host now holds them; the ones already held are left out.
    pub appended: Vec<ProjectNotificationRecord>,
}

/// One of this host's records, and the OS user whose session made it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldRecord {
    pub os_user: String,
    pub record: ProjectNotificationRecord,
}

/// The relay gap `batch` reports, under `seq`: a record no reader lists and no watch delivers, whose
/// [`ProjectNotificationRecord::relay_lost`] every watch reports as lost.
fn gap_record(batch: &RelayedBatch, seq: u64) -> ProjectNotificationRecord {
    ProjectNotificationRecord {
        seq,
        session_id: String::new(),
        label: batch.origin.clone(),
        kind: SessionNotificationKind::Activity,
        source: SessionNotificationSource::ActivityStatus,
        text: format!(
            "{} records of host {} were evicted there before they could be relayed",
            batch.lost, batch.origin
        ),
        agent: None,
        question: None,
        ended: None,
        activity_status: None,
        message: None,
        at_unix_ms: tddy_daemon_kernel::now_unix_ms(),
        orchestrator_session_id: None,
        daemon_instance_id: Some(batch.origin.clone()),
        // A gap carries no origin `seq`, so it never moves the origin's mark past a record it did
        // not append.
        origin_seq: None,
        relay_lost: batch.lost,
    }
}

/// Per origin host, the highest origin `seq` among `records`.
pub(super) fn received_through_of(records: &[Held]) -> HashMap<String, u64> {
    let mut through = HashMap::new();
    for held in records {
        if let (Some(origin), Some(seq)) = (
            held.record.daemon_instance_id.as_ref(),
            held.record.origin_seq,
        ) {
            let mark = through.entry(origin.clone()).or_insert(0);
            *mark = seq.max(*mark);
        }
    }
    through
}

impl ProjectNotificationLog {
    /// Append `batch`'s records this project does not already hold from its origin, and a relay gap
    /// when it reports lost records — each under a new local `seq`, naming the origin host.
    pub fn append_relayed(&self, batch: RelayedBatch) -> anyhow::Result<RelayedAppend> {
        let path = storage::project_file(&self.data_dir, &batch.project_id)?;
        let project = self.project(&batch.project_id);
        let appended = {
            let mut state = project.lock();
            let held_through = state
                .received_through
                .get(&batch.origin)
                .copied()
                .unwrap_or(0);
            let mut appended = Vec::new();
            if batch.lost > 0 && batch.through_seq > held_through {
                let gap = gap_record(&batch, state.newest_seq + 1);
                append_held(
                    &*self.fs,
                    &path,
                    &mut state,
                    Held {
                        os_user: batch.os_user.clone(),
                        record: gap.clone(),
                    },
                )?;
                appended.push(gap);
            }
            let mut through = held_through;
            for record in batch.records {
                if record.seq <= through {
                    continue;
                }
                let origin_seq = record.seq;
                let relayed = ProjectNotificationRecord {
                    seq: state.newest_seq + 1,
                    daemon_instance_id: Some(batch.origin.clone()),
                    origin_seq: Some(origin_seq),
                    relay_lost: 0,
                    ..record
                };
                append_held(
                    &*self.fs,
                    &path,
                    &mut state,
                    Held {
                        os_user: batch.os_user.clone(),
                        record: relayed.clone(),
                    },
                )?;
                through = origin_seq;
                appended.push(relayed);
            }
            state.received_through.insert(batch.origin.clone(), through);
            RelayedAppend {
                received_through: through,
                appended,
            }
        };
        if !appended.appended.is_empty() {
            self.announce(&project);
        }
        Ok(appended)
    }

    /// Every project this log holds records of, or has been asked about.
    pub fn project_ids(&self) -> Vec<String> {
        self.projects
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .keys()
            .cloned()
            .collect()
    }

    /// The records of `project_id` still in the window, oldest first, each with its OS user; and the
    /// `seq` of the oldest one kept (`None` for an empty window).
    pub fn held_records(&self, project_id: &str) -> (Vec<HeldRecord>, Option<u64>) {
        let project = self.project(project_id);
        let state = project.lock();
        let records = state
            .window
            .iter()
            .map(|held| HeldRecord {
                os_user: held.os_user.clone(),
                record: held.record.clone(),
            })
            .collect();
        (records, state.window.front().map(|held| held.record.seq))
    }

    /// Tell `hook` of every record of this host's own sessions as it is appended. One hook per log;
    /// a second is refused.
    pub fn on_append(&self, hook: super::AppendHook) -> Result<(), String> {
        self.on_append
            .set(hook)
            .map_err(|_| "the log already has an append hook".to_string())
    }

    /// Woken whenever any project's log is appended to.
    pub fn appended(&self) -> std::sync::Arc<Notify> {
        std::sync::Arc::clone(&self.appended)
    }
}
