//! What keeps `connection.ConnectionService` answering the 17 methods `#unbundle` node 7 moved.
//!
//! Both new coordinates are served by their own crates now
//! ([`super::svc_session_agent_ports`], [`super::svc_activity_ports`]), but this one still
//! *declares* all 17 rpcs, and dropping a method from a service that declares it is a silent
//! capability removal on an interface `tddy-coder`, `tddy-tools`, `tddy-sandbox-app`,
//! `tddy-session-sync` and the web all still dial. So the 17 handlers delegate, and this module is
//! the whole of what delegation costs: two surface accessors and four re-addressings.
//!
//! The re-addressings are *relabellings*, not mappings. Each moved message is field-for-field
//! identical at the two coordinates — `session_agents.proto` and `activity.proto` were cut from
//! `connection.proto`'s own definitions, and the two shared types they import from `types.proto`
//! are byte-identical copies of `connection.proto`'s — so nothing here decides anything. It exists
//! because the two coordinates are two *generated Rust types*.
//!
//! TODO(session-agent-services): delete this file when `connection.proto` loses the 17 rpcs, which
//! is the next milestone. Nothing that is not a relabelling belongs in it.

use tddy_rpc::Status;
use tddy_service::proto::connection::{
    AgentActivityRecord as ProtoAgentActivityRecord, SessionAgentActivity, SessionAgentEntry,
    SessionAgentRoster,
};
use tokio_stream::{Stream, StreamExt as _};

use super::svc_activity_ports::PeerRoutedActivity;
use super::svc_session_agent_ports::PeerRoutedSessionAgents;
use super::{ConnectionServiceImpl, MpscResultStream};

impl ConnectionServiceImpl {
    /// The family-B surface this coordinate's nine handlers delegate to.
    ///
    /// Named separately from [`Self::session_agents_service`] so the nine delegations all read the
    /// same and grep as one block, which is what makes them deletable together.
    pub(crate) fn session_agents_surface(&self) -> PeerRoutedSessionAgents {
        self.session_agents_service()
    }

    /// The families-M-and-N surface this coordinate's eight handlers delegate to.
    pub(crate) fn activity_surface(&self) -> PeerRoutedActivity {
        self.activity_service()
    }
}

/// One roster snapshot, re-addressed from `session_agents.SessionAgentService` back to this
/// coordinate.
pub(crate) fn roster_at_the_old_coordinate(
    roster: tddy_service::proto::session_agents_svc::SessionAgentRoster,
) -> SessionAgentRoster {
    SessionAgentRoster {
        session_id: roster.session_id,
        rev: roster.rev,
        agents: roster
            .agents
            .into_iter()
            .map(|agent| SessionAgentEntry {
                agent_id: agent.agent_id,
                name: agent.name,
                daemon_instance_id: agent.daemon_instance_id,
                label: agent.label,
                model: agent.model,
                replaces: agent.replaces,
                tools: agent.tools,
                codebase_session_id: agent.codebase_session_id,
                clone_state: agent.clone_state,
                clone_error: agent.clone_error,
                status: agent.status,
                last_activity: agent.last_activity.map(|activity| SessionAgentActivity {
                    at_unix_ms: activity.at_unix_ms,
                    summary: activity.summary,
                }),
            })
            .collect(),
    }
}

/// One activity record, re-addressed from `activity.ActivityService` back to this coordinate.
pub(crate) fn activity_record_at_the_old_coordinate(
    record: tddy_service::proto::activity::AgentActivityRecord,
) -> ProtoAgentActivityRecord {
    ProtoAgentActivityRecord {
        call_id: record.call_id,
        tool_name: record.tool_name,
        input: record.input,
        status: record.status,
        result: record.result,
        error_message: record.error_message,
        started_unix_ms: record.started_unix_ms,
        completed_unix_ms: record.completed_unix_ms,
        source: record.source,
        head_commit: record.head_commit,
        activity_seq: record.activity_seq,
        changed_paths: record.changed_paths,
    }
}

/// Relay a moved coordinate's stream back onto this coordinate's frame type.
///
/// Errors are relayed too, and terminate the relay after being delivered: a refusal raised
/// mid-stream is the moved surface's own, and a caller that saw frames and then nothing could not
/// tell it from a truncation.
pub(crate) fn relayed_onto_this_coordinate<S, T, U>(
    moved: S,
    convert: impl Fn(T) -> U + Send + 'static,
) -> MpscResultStream<U>
where
    S: Stream<Item = Result<T, Status>> + Send + Unpin + 'static,
    T: Send + 'static,
    U: Send + 'static,
{
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<U, Status>>();
    tokio::spawn(async move {
        let mut moved = moved;
        while let Some(item) = moved.next().await {
            let failed = item.is_err();
            if tx.send(item.map(&convert)).is_err() || failed {
                break;
            }
        }
    });
    MpscResultStream { rx }
}
