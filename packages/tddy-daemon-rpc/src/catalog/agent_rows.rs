//! The agent rows `CatalogService.ListAgents` answers with: the config allowlist, the model
//! registry's assistants, and the harness profiles under `<data>/agents`.

use std::path::Path;

use tddy_coding_agent_specs::CodingAgentRegistry;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_discovery::harness_profile::{harness_profile_rows, load_harness_profiles};
use tddy_model_registry::ModelRegistryStore;
use tddy_rpc::Status;
use tddy_service::proto::catalog::AgentInfo as CatalogAgentInfo;
use tddy_service::proto::catalog::SessionTypeInfo;
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
) -> Result<(Vec<CatalogAgentInfo>, Vec<SessionTypeInfo>), Status> {
    let assistants = match model_registry {
        Some(registry) => registry.list_assistants().await.map_err(Status::from)?,
        None => Vec::new(),
    };
    let registry = CodingAgentRegistry::load(&tddy_data_dir.join("tools"))
        .map_err(|e| Status::internal(format!("load the coding agent tools: {e}")))?;
    let profiles = load_harness_profiles(&tddy_data_dir.join("agents"));
    let agents = agent_allowlist_rows(config, &assistants)
        .into_iter()
        .map(|row| (row.id, row.display_label))
        .chain(harness_profile_rows(&profiles))
        .map(|(id, label)| agent_info(&registry, id, label))
        .collect();
    Ok((agents, session_type_infos(&registry)))
}

/// Every session type `registry` declares, as `ListAgents` serves them.
fn session_type_infos(registry: &CodingAgentRegistry) -> Vec<SessionTypeInfo> {
    registry
        .session_types()
        .into_iter()
        .map(|declared| SessionTypeInfo {
            session_type: declared.session_type.clone(),
            label: declared.label.clone(),
            launch: declared.launch.name().to_string(),
        })
        .collect()
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
            // The agent's metadata, from its spec, for a client that names no agent: the
            // session type that starts it, what it can do, what its launch argv binds, the
            // permission modes it takes, and its default model.
            session_type: registry
                .session_type_of(&spec.id)
                .filter(|session_type| registry.for_session_type(session_type).is_some())
                .map(str::to_string)
                .unwrap_or_default(),
            capabilities: spec
                .agent
                .capabilities
                .iter()
                .map(|capability| capability.name().to_string())
                .collect(),
            run_params: spec.argv_params().into_iter().collect(),
            permission_modes: spec.agent.permission_modes.clone(),
            default_model: spec.agent.models.default.clone(),
            placements: spec
                .agent
                .placements
                .iter()
                .map(|placement| placement.to_string())
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

#[cfg(test)]
mod session_type_tests {
    use super::*;

    #[test]
    fn listagents_serves_every_declared_session_type_with_its_label_and_launch() {
        // Given
        let registry = CodingAgentRegistry::builtin().expect("the built-ins load");

        // When
        let served: Vec<(String, String, String)> = session_type_infos(&registry)
            .into_iter()
            .map(|info| (info.session_type, info.label, info.launch))
            .collect();

        // Then — tddy-coder.yaml's `tool` among the coding agents' CLIs; no `workspace`
        assert_eq!(
            (
                served.contains(&(
                    "tool".to_string(),
                    "Tool".to_string(),
                    "tddy_coder".to_string()
                )),
                served.contains(&(
                    "claude-cli".to_string(),
                    "Claude Code".to_string(),
                    "agent_cli".to_string()
                )),
                served
                    .iter()
                    .any(|(session_type, _, _)| session_type == "workspace"),
                served.len(),
            ),
            (true, true, false, 8)
        );
    }
}
