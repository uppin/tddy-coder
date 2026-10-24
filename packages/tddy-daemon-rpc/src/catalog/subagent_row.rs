//! How a resolved agent def becomes the `ListSubagents` row a picker attaches from.
//!
//! Moved from `tddy-session-lifecycle`'s `agent_roster` with the catalogue handlers, its only
//! caller. The id and tool spellings stay there, shared with the roster entry an attach produces.

use tddy_service::proto::catalog::SubagentInfo;
use tddy_session_lifecycle::connection_service::{
    agent_kind_to_wire, coding_agent_record, def_tool_names, qualified_agent_id,
};

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
        kind: tddy_service::proto::types::AgentKind::Model as i32,
        harness: String::new(),
        transport: String::new(),
        permission_policy: String::new(),
    })
}

/// One coding agent as the `ListSubagents` row a picker attaches from: the roster entry attaching
/// it produces ([`coding_agent_record`]), spelled for the wire, so the two cannot disagree.
pub(super) fn coding_agent_info(
    def: &tddy_coding_subagent::CodingAgentDef,
    daemon_instance_id: &str,
) -> Result<SubagentInfo, tddy_core::AgentIdError> {
    let record = coding_agent_record(def, daemon_instance_id)?;
    Ok(SubagentInfo {
        agent_id: record.agent_id,
        label: record.label.unwrap_or_else(|| record.name.clone()),
        name: record.name,
        model: record.model,
        daemon_instance_id: record.daemon_instance_id,
        replaces: record.replaces,
        tools: record.tools,
        kind: agent_kind_to_wire(record.kind) as i32,
        harness: record.harness.unwrap_or_default(),
    })
}

/// The coding-agent rows of `ListSubagents`, after the model rows: ACP harness profiles, then ACP
/// `allowed_agents` ids. A coding agent named like a model row is dropped with a warning, since two
/// rows with one name would attach as whichever resolved first.
pub(super) fn coding_agent_rows(
    tddy_data_dir: &std::path::Path,
    config: &tddy_daemon_kernel::config::DaemonConfig,
    daemon_instance_id: &str,
    model_rows: &[SubagentInfo],
) -> Vec<SubagentInfo> {
    tddy_session_lifecycle::connection_service::coding_agent_defs_for(tddy_data_dir, config)
        .iter()
        .filter(|def| {
            let taken = model_rows.iter().any(|row| row.name == def.name);
            if taken {
                log::warn!(
                    "list_subagents RPC: not advertising coding agent '{}' — a model agent has \
                     that name",
                    def.name
                );
            }
            !taken
        })
        .filter_map(|def| match coding_agent_info(def, daemon_instance_id) {
            Ok(info) => Some(info),
            Err(e) => {
                log::warn!("list_subagents RPC: not advertising a coding agent — {e}");
                None
            }
        })
        .collect()
}
