//! Daemon-facing adapter over [`tddy_discovery::agent_list_mapping`].

use tddy_service::proto::models::AssistantEntry;

use crate::config::DaemonConfig;

pub use tddy_discovery::agent_list_mapping::{
    agent_allowlist_rows as agent_allowlist_rows_from_configured, AgentAllowlistRow,
    ConfiguredAgentAllowlistEntry,
};

/// Builds allowlist rows from this daemon's config and optional registry assistants.
pub fn agent_allowlist_rows(
    config: &DaemonConfig,
    assistants: &[AssistantEntry],
) -> Vec<AgentAllowlistRow> {
    let configured: Vec<ConfiguredAgentAllowlistEntry> = config
        .allowed_agents()
        .iter()
        .map(|a| ConfiguredAgentAllowlistEntry {
            id: a.id.clone(),
            label: a.label.clone(),
        })
        .collect();
    agent_allowlist_rows_from_configured(&configured, assistants)
}
