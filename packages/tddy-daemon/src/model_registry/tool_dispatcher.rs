//! The engine-backed [`ToolDispatcher`] an assistant's ACP chat runs its tools through.
//!
//! `tddy-acp` names the tools and reports the calls but never executes one — tool execution is a
//! port, so that crate stays free of `tddy-tool-engine`. This is the daemon's implementation of
//! that port: the assistant's assigned tool names, described by the exec catalog, dispatched by
//! `tddy_tool_engine::execute_tool` inside the session's workspace.

use std::path::PathBuf;

use tddy_acp::provider_agent::{ProviderTool, ToolDispatcher, ToolOutcome};
use tddy_task::TaskRegistry;

use super::error::ModelRegistryError;

/// Runs an assistant's assigned tools through the daemon's tool engine, confined to one workspace.
pub struct EngineToolDispatcher {
    /// Every tool the assistant may call, already resolved against the exec catalog.
    tools: Vec<ProviderTool>,
    /// The root every path argument is confined to (`tddy_tool_engine` enforces it).
    workspace: PathBuf,
    /// The daemon's registry, so a dispatched call is observable as a task like any other.
    tasks: TaskRegistry,
    /// The chat session the calls are attributed to.
    session_id: String,
}

impl EngineToolDispatcher {
    /// Build the dispatcher for `tool_names`, refusing any name the engine cannot dispatch — an
    /// assistant offered a tool that does not exist would burn a whole turn discovering it.
    pub fn new(
        tool_names: &[String],
        workspace: PathBuf,
        tasks: TaskRegistry,
        session_id: impl Into<String>,
    ) -> Result<Self, ModelRegistryError> {
        let catalog = tddy_tool_engine::tool_catalog();
        let tools = tool_names
            .iter()
            .map(|name| {
                catalog
                    .iter()
                    .find(|tool| &tool.name == name)
                    .map(|tool| ProviderTool {
                        name: tool.name.clone(),
                        description: tool.description.clone(),
                        input_schema_json: tool.input_schema_json.clone(),
                    })
                    .ok_or_else(|| ModelRegistryError::UnknownTool(name.clone()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            tools,
            workspace,
            tasks,
            session_id: session_id.into(),
        })
    }
}

#[async_trait::async_trait(?Send)]
impl ToolDispatcher for EngineToolDispatcher {
    fn tool_defs(&self) -> Vec<ProviderTool> {
        self.tools.clone()
    }

    async fn execute(&self, name: &str, input_json: &str) -> ToolOutcome {
        // A model can ask for a tool it was never offered — hallucinated, or remembered from an
        // earlier conversation. Refusing here is what keeps the assistant's assigned tool list the
        // whole of what this dispatcher will run, rather than merely what it advertises.
        if !self.tools.iter().any(|tool| tool.name == name) {
            return ToolOutcome::failed(format!("tool '{name}' is not assigned to this assistant"));
        }
        let outcome = tddy_tool_engine::execute_tool(
            &self.workspace,
            name,
            input_json,
            &self.tasks,
            &self.session_id,
        )
        .await;
        // The model is shown the engine's own result either way: a failure it can read is what lets
        // it correct itself, where a hidden one just produces a confidently wrong next turn.
        match outcome.is_error {
            true => ToolOutcome::failed(outcome.result_json),
            false => ToolOutcome::ok(outcome.result_json),
        }
    }
}
