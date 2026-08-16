//! Projecting a stored assistant onto a [`SpecializedAgentDef`] — the mapping that makes an
//! assistant created in the web a selectable `--agent <name>`, since `create_backend` resolves a
//! named def before anything else.

use tddy_discovery::agent_def::{SpecializedAgentDef, SubagentTool};
use tddy_service::proto::models::{AssistantEntry, ProviderEntry};

use super::error::ModelRegistryError;
use super::store::ModelRegistryStore;

/// How many model round trips an assistant's internal tool loop may take before it must answer.
/// The same default a YAML-defined subagent gets.
const DEFAULT_MAX_TURNS: u32 = 10;

/// Build the agent def for `assistant`, given the provider row it names.
///
/// `provider` must be that provider: pairing an assistant with a different row would silently
/// point the def's `base_url` at an endpoint the assistant was never built for, so a mismatch is
/// [`ModelRegistryError::NotFound`] rather than a def nobody would notice was wrong.
pub fn assistant_to_agent_def(
    assistant: &AssistantEntry,
    provider: &ProviderEntry,
) -> Result<SpecializedAgentDef, ModelRegistryError> {
    if assistant.provider_id != provider.provider_id {
        return Err(ModelRegistryError::NotFound(format!(
            "assistant '{}' names provider {}, not {}",
            assistant.name, assistant.provider_id, provider.provider_id
        )));
    }

    let tools = assistant
        .tools
        .iter()
        .map(|name| {
            SubagentTool::from_catalog_name(name)
                .ok_or_else(|| ModelRegistryError::UnknownTool(name.clone()))
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(SpecializedAgentDef {
        name: assistant.name.clone(),
        label: non_empty(&assistant.label),
        model: assistant.model_id.clone(),
        base_url: provider.base_url.clone(),
        system_prompt: non_empty(&assistant.system_prompt),
        system_prompt_path: None,
        tools,
        max_turns: DEFAULT_MAX_TURNS,
        // An assistant is an agent a session is started *as*, not a subagent that stands in for a
        // main-agent tool, so it replaces nothing.
        replaces: Vec::new(),
    })
}

/// Every assistant in `store`, projected onto its agent def — the registry as a third source of
/// specialized agent defs alongside the builtins and `<tddyhome>/agents/*.yaml`.
///
/// An assistant whose provider row has gone missing fails the whole call rather than being dropped
/// from the list: silently omitting it would turn "this assistant's endpoint is unknown" into
/// "there is no such agent", and the session would be started as something else entirely.
pub async fn registry_agent_defs(
    store: &ModelRegistryStore,
) -> Result<Vec<SpecializedAgentDef>, ModelRegistryError> {
    let providers = store.list_providers().await?;
    let assistants = store.list_assistants().await?;
    assistants
        .iter()
        .map(|assistant| {
            let provider = providers
                .iter()
                .find(|p| p.provider_id == assistant.provider_id)
                .ok_or_else(|| {
                    ModelRegistryError::NotFound(format!(
                        "assistant '{}' names provider {}, which is no longer in this registry",
                        assistant.name, assistant.provider_id
                    ))
                })?;
            assistant_to_agent_def(assistant, provider)
        })
        .collect()
}

/// A proto string field carries "" for absent; the def's optional fields carry `None`.
fn non_empty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}
