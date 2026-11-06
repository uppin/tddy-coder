//! The agent rows `CatalogService.ListAgents` answers with: the config allowlist, the model
//! registry's assistants, and the harness profiles under `<data>/agents`.

use std::path::Path;

use tddy_coding_agent_specs::CodingAgentRegistry;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_discovery::harness_profile::{harness_profile_rows, load_harness_profiles};
use tddy_model_registry::ModelRegistryStore;
use tddy_rpc::Status;
use tddy_service::proto::catalog::AgentInfo as CatalogAgentInfo;
use tddy_service::proto::types::AgentKind;
use tddy_session_lifecycle::agent_list_mapping::agent_allowlist_rows;

/// Every agent this daemon lists, in `ListAgents` order.
///
/// A registry this daemon has but cannot read is an error, not "there are no assistants" — a session
/// started against a missing agent id fails much later and much less clearly.
pub(super) async fn listed_agents(
    config: &DaemonConfig,
    tddy_data_dir: &Path,
    model_registry: Option<&ModelRegistryStore>,
) -> Result<Vec<CatalogAgentInfo>, Status> {
    let assistants = match model_registry {
        Some(registry) => registry.list_assistants().await.map_err(Status::from)?,
        None => Vec::new(),
    };
    let registry = CodingAgentRegistry::load(&tddy_data_dir.join("tools"))
        .map_err(|e| Status::internal(format!("load the coding agent tools: {e}")))?;
    let profiles = load_harness_profiles(&tddy_data_dir.join("agents"));
    Ok(agent_allowlist_rows(config, &assistants)
        .into_iter()
        .map(|row| (row.id, row.display_label))
        .chain(harness_profile_rows(&profiles))
        .map(|(id, label)| agent_info(&registry, id, label))
        .collect())
}

/// The row of agent `id`: a coding agent the registry holds, with its spec's id and the provider
/// kinds a route can point it at, or a model row with neither.
fn agent_info(registry: &CodingAgentRegistry, id: String, label: String) -> CatalogAgentInfo {
    match registry.get(&id) {
        Some(spec) => CatalogAgentInfo {
            harness: spec.id.clone(),
            kind: AgentKind::CodingAgent as i32,
            route_providers: spec
                .route_support()
                .providers
                .iter()
                .map(ToString::to_string)
                .collect(),
            id,
            label,
        },
        None => CatalogAgentInfo {
            id,
            label,
            kind: AgentKind::Model as i32,
            ..Default::default()
        },
    }
}
