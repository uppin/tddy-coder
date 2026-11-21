use crate::start_ledger::now_unix_ms;
use crate::start_ledger::remote_calls::remote_call_record;
use crate::start_ledger::remote_calls::MAX_REMOTE_CALLS;
use crate::start_ledger::StartCloneProgress;
use crate::start_ledger::StartLedgerRecord;
use crate::start_ledger::StartStep;
use tddy_service::proto::session::start_phase::{Boundary, Step};
use tddy_service::proto::session::start_session_event::Event;
use tddy_service::proto::session::{StartPhase, StartSessionEvent};

/// Fold one progress event into `record`; whether anything the record shows changed.
pub(super) fn apply_progress(record: &mut StartLedgerRecord, event: StartSessionEvent) -> bool {
    match event.event {
        Some(Event::Phase(phase)) => match step_of(&phase) {
            Some(step) => {
                record.step = Some(step);
                true
            }
            None => false,
        },
        Some(Event::CloneProgress(progress)) => {
            record.clone_progress = Some(StartCloneProgress {
                stage: progress.stage,
                percent: progress.percent,
            });
            true
        }
        Some(Event::RemoteCall(call)) => {
            record.remote_calls.push(remote_call_record(call));
            if record.remote_calls.len() > MAX_REMOTE_CALLS {
                let overflow = record.remote_calls.len() - MAX_REMOTE_CALLS;
                record.remote_calls.drain(..overflow);
            }
            record.updated_at_unix_ms = now_unix_ms();
            true
        }
        Some(Event::WorktreeReady(ready)) => {
            record.branch = Some(ready.branch);
            record.worktree = Some(ready.worktree);
            // A start that could not tell its base reports it empty: nothing is recorded, rather
            // than a commit of "" claimed to be zero behind.
            if !ready.base_commit.is_empty() {
                record.base_commit = Some(ready.base_commit);
                record.base_behind = Some(ready.base_behind);
            }
            record.updated_at_unix_ms = now_unix_ms();
            true
        }
        _ => false,
    }
}

fn step_of(phase: &StartPhase) -> Option<StartStep> {
    let name = match Step::try_from(phase.step).ok()? {
        Step::Worktree => "worktree",
        Step::SemanticIndex => "semantic_index",
        Step::Agent => "agent",
        Step::Clone => "clone",
        Step::Unspecified => return None,
    };
    let status = match Boundary::try_from(phase.boundary).ok()? {
        Boundary::Begin => "running",
        Boundary::End => "done",
        Boundary::Unspecified => return None,
    };
    Some(StartStep {
        step: name.to_string(),
        status: status.to_string(),
    })
}
