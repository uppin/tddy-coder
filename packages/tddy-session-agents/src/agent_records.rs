use tddy_rpc::Status;

/// The exec-catalog names of the tools a def's own loop may call — the spelling the wire, the
/// roster and `execute_tool`'s dispatch all use, rather than the `UPPERCASE` YAML spelling.
///
/// `pub` because the `ListSubagents` row (`tddy-daemon-rpc`) and the roster entry must spell a
/// def's tools the same way.
pub fn def_tool_names(def: &tddy_discovery::agent_def::SpecializedAgentDef) -> Vec<String> {
    def.tools
        .iter()
        .map(|t| t.catalog_name().to_string())
        .collect()
}

/// One resolved def as the roster entry attaching it produces.
///
/// `replaces` and `tools` are copied in here and never re-read: editing the YAML def or the
/// registry assistant afterwards would otherwise silently change what a running session's main
/// agent is allowed to call (PRD § An entry). Detaching and re-attaching is the explicit way to
/// pick an edit up.
pub fn roster_record(
    def: &tddy_discovery::agent_def::SpecializedAgentDef,
    daemon_instance_id: &str,
) -> Result<tddy_core::SessionAgentRecord, tddy_core::AgentIdError> {
    Ok(tddy_core::SessionAgentRecord {
        agent_id: qualified_agent_id(&def.name, daemon_instance_id)?,
        name: def.name.clone(),
        daemon_instance_id: daemon_instance_id.to_string(),
        label: def.label.clone().filter(|s| !s.trim().is_empty()),
        model: def.model.clone(),
        replaces: tddy_discovery::subagent::normalize_replaced_tools(&def.replaces),
        tools: def_tool_names(def),
        // A local agent works the facilitating daemon's real worktree, so there is no clone to name.
        codebase_session_id: None,
    })
}

/// The qualified id a def resolved on `daemon_instance_id` is addressed by.
///
/// Refused at the point the id is minted when the def's own name contains `@`: such an id parses
/// back as a different pair, so letting it through would put an entry in the roster that routes
/// somewhere the operator never picked.
pub fn qualified_agent_id(
    name: &str,
    daemon_instance_id: &str,
) -> Result<String, tddy_core::AgentIdError> {
    tddy_core::AgentId {
        name: name.to_string(),
        daemon_instance_id: daemon_instance_id.to_string(),
    }
    .try_qualified()
}

/// The agent a `StartSessionRequest.specialized_agents` entry names.
///
/// The field keeps its wire shape (`repeated string`) and now carries either form: a qualified
/// `name@daemon_instance_id`, or a bare name. A bare name resolves against *this* daemon, and only
/// here — it is the one place where that reading is not a guess, because a start request has never
/// been able to name any other daemon. Attach takes no such reading (PRD § Identity is qualified,
/// always).
pub fn started_agent_id(
    reference: &str,
    local_instance_id: &str,
) -> Result<tddy_core::AgentId, Status> {
    match tddy_core::AgentId::parse(reference) {
        Ok(id) => Ok(id),
        Err(tddy_core::AgentIdError::Unqualified(_)) => Ok(tddy_core::AgentId {
            name: reference.to_string(),
            daemon_instance_id: local_instance_id.to_string(),
        }),
        Err(e) => Err(Status::invalid_argument(format!("specialized_agents: {e}"))),
    }
}
