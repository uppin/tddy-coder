use crate::apply::Delta;
use crate::attach::SessionAddress;
use crate::sync::MIRROR_DELTA_SCOPE;
use tddy_service::proto::activity::{
    AgentActivityDeltaChunk, AgentActivityDeltaRequest, AgentActivityRecord,
};
use tddy_service::proto::worktree_activity::{WorktreeActivityEvent, WorktreeActivityKind};

/// What the syncer does about one activity record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordDecision {
    /// Fetch this tick's patch, addressed by the call that reported it.
    FetchDelta { call_id: String, seq: u64 },
    /// Nothing to fetch, and why — every one of these is logged rather than passed over in silence.
    Ignore(IgnoreReason),
}

/// Why a record produced no fetch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IgnoreReason {
    /// `activity_seq` is 0: no poll tick has measured this call yet, so its patch does not exist.
    /// The tick that covers it will be reported by a later record.
    NoTickYet,
    /// The mirror is already at or past this tick — the ordinary case for the second, third and
    /// fourth call of one poll window.
    AlreadyApplied { seq: u64, last_seq: u64 },
    /// The record carries no `call_id`, and a delta request has nothing else to address.
    Unaddressable { seq: u64 },
}

impl std::fmt::Display for IgnoreReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IgnoreReason::NoTickYet => {
                write!(f, "no poll tick has measured this call yet")
            }
            IgnoreReason::AlreadyApplied { seq, last_seq } => write!(
                f,
                "tick {seq} is already applied; the mirror is at {last_seq}"
            ),
            IgnoreReason::Unaddressable { seq } => write!(
                f,
                "the record for tick {seq} carries no call_id, so its delta cannot be addressed"
            ),
        }
    }
}

/// Whether a record's tick is one the mirror still needs.
///
/// Deliberately not filtered by `tool_name`: a whitelist of editing tools is a list that goes out
/// of date, and a tool missing from it is a change that reaches the mirror never. What decides is
/// the tick — if the mirror has not applied it, it needs it, whatever ran.
pub fn decide_record(record: &AgentActivityRecord, last_seq: u64) -> RecordDecision {
    // `NO_TICK`, not the literal 0, and the two are now genuinely different facts: `#unbundle`
    // node 7 numbers a session's deltas from `FIRST_TICK` (1), so this arm means "no tick has
    // covered this call yet" and nothing else. On the coordinate before it, a first delta was also
    // numbered 0 and this arm silently discarded it.
    if record.activity_seq == tddy_service::session_activity::NO_TICK {
        return RecordDecision::Ignore(IgnoreReason::NoTickYet);
    }
    if record.activity_seq <= last_seq {
        return RecordDecision::Ignore(IgnoreReason::AlreadyApplied {
            seq: record.activity_seq,
            last_seq,
        });
    }
    if record.call_id.is_empty() {
        return RecordDecision::Ignore(IgnoreReason::Unaddressable {
            seq: record.activity_seq,
        });
    }
    RecordDecision::FetchDelta {
        call_id: record.call_id.clone(),
        seq: record.activity_seq,
    }
}

/// What the syncer does about one worktree-activity event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorktreeDecision {
    /// The checkout's `HEAD` moved; restore from git (AC28).
    Reconcile,
    /// Nothing to do: the event carries no paths and no content, and whatever it reports is
    /// delivered as a delta by the tick that measured it.
    Ignore,
}

pub fn decide_worktree(event: &WorktreeActivityEvent) -> WorktreeDecision {
    match event.kind() {
        WorktreeActivityKind::Commit => WorktreeDecision::Reconcile,
        WorktreeActivityKind::FilesChanged | WorktreeActivityKind::Unspecified => {
            WorktreeDecision::Ignore
        }
    }
}

/// The request that fetches one tick's patch.
pub fn delta_request(
    address: &SessionAddress,
    session_token: &str,
    call_id: &str,
) -> AgentActivityDeltaRequest {
    AgentActivityDeltaRequest {
        session_token: session_token.to_string(),
        session_id: address.session_id.clone(),
        // Routed explicitly rather than left empty: the daemon in the room is the one that owns
        // the session, but saying so costs nothing and a silently mis-routed delta is a mirror
        // built from another host's worktree.
        daemon_instance_id: address.daemon_instance_id.clone(),
        call_id: call_id.to_string(),
        scope: MIRROR_DELTA_SCOPE as i32,
    }
}

/// Why a stream of frames is not a patch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeltaError {
    /// The stream ended without a single frame. Distinct from a patch of zero bytes, which is one
    /// frame carrying nothing — that is how "the call changed nothing" is said (AC9), and reading
    /// a failed stream as it would be a mirror that quietly skipped a tick.
    NoFrames,
    /// The frames disagree about what they are describing. Every describing field repeats on every
    /// frame precisely so this is detectable rather than resolved by trusting the first one.
    Inconsistent {
        field: &'static str,
        first: String,
        later: String,
    },
    /// Fewer bytes arrived than the frames said the patch has. `git apply` would reject the tail
    /// half of a hunk anyway; this says why, at the point the bytes went missing.
    Truncated { declared: u64, received: usize },
}

impl std::fmt::Display for DeltaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeltaError::NoFrames => write!(
                f,
                "the delta stream ended without a frame; a patch of no bytes is one empty frame, \
                 not none"
            ),
            DeltaError::Inconsistent {
                field,
                first,
                later,
            } => write!(
                f,
                "the delta's frames disagree about {field}: the first said {first}, a later one \
                 said {later}"
            ),
            DeltaError::Truncated { declared, received } => write!(
                f,
                "the delta declared {declared} bytes and {received} arrived"
            ),
        }
    }
}

impl std::error::Error for DeltaError {}

/// Reassemble the frames of one `StreamAgentActivityDelta` call into the patch they carry.
pub fn reassemble(chunks: &[AgentActivityDeltaChunk]) -> Result<Delta, DeltaError> {
    let Some(first) = chunks.first() else {
        return Err(DeltaError::NoFrames);
    };

    for later in &chunks[1..] {
        for (field, first, later) in [
            ("seq", first.seq.to_string(), later.seq.to_string()),
            (
                "prev_seq",
                first.prev_seq.to_string(),
                later.prev_seq.to_string(),
            ),
            (
                "base_commit",
                first.base_commit.clone(),
                later.base_commit.clone(),
            ),
            (
                "total_byte_size",
                first.total_byte_size.to_string(),
                later.total_byte_size.to_string(),
            ),
        ] {
            if first != later {
                return Err(DeltaError::Inconsistent {
                    field,
                    first,
                    later,
                });
            }
        }
    }

    let patch: Vec<u8> = chunks
        .iter()
        .flat_map(|chunk| chunk.patch.iter().copied())
        .collect();
    if patch.len() as u64 != first.total_byte_size {
        return Err(DeltaError::Truncated {
            declared: first.total_byte_size,
            received: patch.len(),
        });
    }

    Ok(Delta {
        seq: first.seq,
        prev_seq: first.prev_seq,
        base_commit: first.base_commit.clone(),
        patch,
        scoped_paths: first.scoped_paths.clone(),
    })
}
