//! How a resolved agent def becomes the `ListSubagents` row a picker attaches from.
//!
//! Moved from `tddy-session-lifecycle`'s `agent_roster` with the catalogue handlers, its only
//! caller. The id and tool spellings stay there, shared with the roster entry an attach produces.

use tddy_service::proto::catalog::SubagentInfo;
use tddy_session_lifecycle::connection_service::{def_tool_names, qualified_agent_id};

/// One resolved def as the `ListSubagents` row a picker attaches from.
pub(super) fn subagent_info(
    def: &tddy_discovery::agent_def::SpecializedAgentDef,
    daemon_instance_id: &str,
) -> Result<SubagentInfo, tddy_core::AgentIdError> {
    Ok(SubagentInfo {
        agent_id: qualified_agent_id(&def.name, daemon_instance_id)?,
        name: def.name.clone(),
        label: def
            .label
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| def.name.clone()),
        model: def.model.clone(),
        daemon_instance_id: daemon_instance_id.to_string(),
        replaces: tddy_discovery::subagent::normalize_replaced_tools(&def.replaces),
        tools: def_tool_names(def),
    })
}
