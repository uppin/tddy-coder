use tddy_rpc::Status;

pub async fn agent_def_for_spawn(
    agent: &str,
    caller: &str,
    model_registry: &Option<std::sync::Arc<tddy_model_registry::ModelRegistryStore>>,
    state: crate::AgentRosterState<'_>,
) -> Result<Option<tddy_discovery::agent_def::SpecializedAgentDef>, Status> {
    if let Some(registry) = &model_registry {
        // The registry wins over a YAML def of the same name, the same way it does in
        // `resolvable_agent_defs`.
        if let Some(def) =
            tddy_model_registry::registry_agent_def_with_credential(registry, agent, caller)
                .await
                .map_err(Status::from)?
        {
            return Ok(Some(def));
        }
    }
    let agents_dir = state.tddy_data_dir.join("agents");
    Ok(tddy_discovery::agent_def::resolve_agent_defs(&agents_dir)
        .into_iter()
        .find(|d| d.name == agent))
}
