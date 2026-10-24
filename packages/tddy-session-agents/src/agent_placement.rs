//! Where a roster agent's conversations work, as the wire and the record spell it.
//!
//! `shared` is the session's own worktree — the agent edits the main agent's checkout. `clone` gives
//! each conversation a worktree and branch of its own (see [`crate::clone_placement`]).

use tddy_core::{AgentPlacementRecord, SessionAgentRecord};
use tddy_rpc::Status;
use tddy_service::proto::types::AgentPlacement;

use crate::agent_host_callbacks::AgentRoster;

/// Refuse an attach that would change the placement of an agent the session already holds.
///
/// A conversation's worktree is cut when it opens, from the placement the roster holds for its agent,
/// so the entry already there cannot take a second placement. Answering the attach as an idempotent
/// success would tell the caller a change happened that did not.
pub(crate) fn refuse_placement_change(
    session_id: &str,
    attached: Option<&SessionAgentRecord>,
    requested: AgentPlacementRecord,
) -> Result<(), Status> {
    match attached {
        Some(entry) if entry.placement != requested => Err(Status::failed_precondition(format!(
            "agent '{}' is already attached with {} placement in session '{session_id}'; detach it \
             before attaching it with {} placement",
            entry.agent_id,
            placement_name(entry.placement),
            placement_name(requested),
        ))),
        _ => Ok(()),
    }
}

/// The word a placement is named by in a refusal.
fn placement_name(placement: AgentPlacementRecord) -> &'static str {
    match placement {
        AgentPlacementRecord::Shared => "shared",
        AgentPlacementRecord::Clone => "clone",
    }
}

impl AgentRoster {
    /// [`Self::seeded_roster_records`] for a start that also names the agents whose conversations
    /// work on a clone (`StartSessionRequest.clone_placed_agents`).
    ///
    /// Every name must be one the start picked: a placement for an agent the start does not attach
    /// is refused rather than dropped, because the picker only offers a placement for a picked agent.
    /// Records are answered in the order of `specialized_agents`, so the names are matched to them
    /// by position.
    pub async fn seeded_roster_records_placed(
        &self,
        specialized_agents: &[String],
        clone_placed_agents: &[String],
    ) -> Result<Vec<tddy_core::SessionAgentRecord>, Status> {
        if let Some(stray) = clone_placed_agents
            .iter()
            .find(|name| !specialized_agents.contains(name))
        {
            return Err(Status::invalid_argument(format!(
                "clone_placed_agents: '{stray}' is placed on a clone but is not one of \
                 specialized_agents"
            )));
        }
        let mut records = self.seeded_roster_records(specialized_agents).await?;
        for (record, reference) in records.iter_mut().zip(specialized_agents) {
            if clone_placed_agents.contains(reference) {
                record.placement = AgentPlacementRecord::Clone;
            }
        }
        Ok(records)
    }
}

/// The wire spelling of a record's placement.
#[must_use]
pub fn placement_to_wire(placement: AgentPlacementRecord) -> AgentPlacement {
    match placement {
        AgentPlacementRecord::Shared => AgentPlacement::Shared,
        AgentPlacementRecord::Clone => AgentPlacement::Clone,
    }
}

/// The persisted placement a wire `placement` stands for. A number this daemon does not know is
/// refused naming it: read as `shared`, a peer's newer placement would put an agent meant to work
/// on a clone onto the session's own worktree without saying so.
pub fn placement_from_wire(placement: i32) -> Result<AgentPlacementRecord, Status> {
    match AgentPlacement::try_from(placement) {
        Ok(AgentPlacement::Shared) => Ok(AgentPlacementRecord::Shared),
        Ok(AgentPlacement::Clone) => Ok(AgentPlacementRecord::Clone),
        Err(_) => Err(Status::invalid_argument(format!(
            "agent placement {placement} is not one this daemon knows (shared = {}, clone = {})",
            AgentPlacement::Shared as i32,
            AgentPlacement::Clone as i32
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_placement_survives_the_wire_and_back() {
        for placement in [AgentPlacementRecord::Shared, AgentPlacementRecord::Clone] {
            let wire = placement_to_wire(placement) as i32;
            assert_eq!(placement_from_wire(wire).expect("known"), placement);
        }
    }

    #[test]
    fn an_unknown_placement_is_refused_naming_it() {
        let refusal = placement_from_wire(77).expect_err("unknown");
        assert!(refusal.message().contains("77"), "{}", refusal.message());
    }
}
