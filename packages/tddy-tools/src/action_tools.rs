//! Session-action MCP tools: the declarative command surface of a managed session (see
//! docs/ft/coder/no-bash-mode.md, docs/ft/coder/session-actions.md).
//!
//! The main agent *requests* a new action in natural language (`request_action`) and names the
//! attached agent that writes the YAML manifest for it; once the manifest validates, the action is
//! auto-established under `<session_dir>/actions/` and invocable via `invoke_action`.
//!
//! Which agent authors a manifest is the caller's choice, named per call — not a role an agent
//! acquires by listing a particular tool in its `replaces`
//! (docs/ft/daemon/session-agent-roster.md § Tool replacement, without behaviour). A session whose
//! `Shell` *is* withdrawn is the case these tools were built for, since then there is no other way
//! to run a command, but they are neither granted by that withdrawal nor limited to it.
//!
//! Trust model: this module runs in the jail, so its validation is a cheap retry loop for the
//! author, never the authority. The manifest is re-validated and written host-side by the
//! `EstablishAction` relay handler (`tddy-sandbox-app::host_actions`), and `list_actions`/
//! `invoke_action` are host round-trips too — the session dir only exists on the host.
//!
//! `#unbundle` node 5 split this module: how a manifest is asked for, extracted and pre-validated
//! is manifest knowledge and moved to `tddy_core::session_actions::authoring`. What is left here
//! is the MCP surface — the three [`rmcp::model::Tool`] definitions and the handler bodies that
//! reach the host — because the shape of an MCP tool belongs to the crate that speaks MCP.

use tddy_core::session_actions::{
    author_prompt, extract_manifest_yaml, prevalidate_manifest_yaml, MAX_AUTHOR_ATTEMPTS,
};

use crate::mcp_primitives::{
    cancel_remote_conversation, open_roster_agent_session, schema_object, subagent_error_json,
    subagent_route,
};

/// `request_action`: describe a needed command; the agent the call names writes the manifest; on
/// successful validation it is established host-side and immediately invocable.
async fn request_action_tool(args: serde_json::Value) -> String {
    let Some(description) = args
        .get("description")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
    else {
        return subagent_error_json("missing required field: description");
    };
    let suggested_id = args.get("suggested_id").and_then(|v| v.as_str());

    // Addressed by id, like every other call on an attached agent: a call naming none is refused
    // listing the ids there are, rather than settled by a rule about what an agent replaces.
    let agent_id = args
        .get("agent")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let opened = match open_roster_agent_session(agent_id).await {
        Ok(opened) => opened,
        Err(e) => return subagent_error_json(e),
    };
    let result =
        author_a_manifest(opened.session, &opened.agent_id, description, suggested_id).await;
    // The exchange began and ended inside this call, so a conversation it opened on another daemon
    // ends with it: nothing outside this function can name it, and left open the owning daemon
    // keeps its turn loop for the life of its process.
    cancel_remote_conversation(opened.remote).await;
    result
}

/// Drive the author agent until it produces a manifest that pre-validates, and establish it.
///
/// Separated from the tool so every way out of the loop — an established manifest, a failed turn,
/// attempts exhausted — passes through one place that closes the conversation behind it.
async fn author_a_manifest(
    mut session: Box<dyn tddy_discovery::subagent::SubagentSession>,
    author: &str,
    description: &str,
    suggested_id: Option<&str>,
) -> String {
    let mut prompt = author_prompt(description, suggested_id);
    let mut last_error = String::new();
    for _attempt in 0..MAX_AUTHOR_ATTEMPTS {
        let outcome = match session.prompt(&prompt).await {
            Ok(outcome) => outcome,
            Err(e) => return subagent_error_json(format!("action author '{author}': {e}")),
        };
        let answer = outcome
            .content
            .iter()
            .map(|block| block.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let yaml = extract_manifest_yaml(&answer);
        match prevalidate_manifest_yaml(&yaml) {
            Ok(_) => {
                // Auto-establish: the host re-validates authoritatively and writes
                // `<session_dir>/actions/<id>.yaml`; its summary JSON is the tool result.
                return crate::session_tool_client::dispatch_session_tool(
                    "EstablishAction",
                    serde_json::json!({ "yaml": yaml }),
                )
                .await;
            }
            Err(e) => {
                last_error = e;
                prompt = format!(
                    "That manifest was rejected: {last_error}\n\
                     Reply with ONLY the corrected YAML manifest."
                );
            }
        }
    }
    subagent_error_json(format!(
        "action author '{author}' produced no valid manifest in {MAX_AUTHOR_ATTEMPTS} attempts; \
         last error: {last_error}"
    ))
}

/// `list_actions`: host round-trip to `tddy_core::session_actions::list_action_summaries`.
async fn list_actions_tool(args: serde_json::Value) -> String {
    crate::session_tool_client::dispatch_session_tool("ListActions", args).await
}

/// `invoke_action`: host round-trip to the blocking `invoke_action_core` (synchronous in v1 —
/// the relay already carries long-running Shell calls over the same path).
async fn invoke_action_tool(args: serde_json::Value) -> String {
    if args.get("action").and_then(|v| v.as_str()).is_none() {
        return subagent_error_json("missing required field: action");
    }
    crate::session_tool_client::dispatch_session_tool("InvokeAction", args).await
}

/// Build the `ToolRouter` for the three session-action tools. Merged into
/// `PermissionServer::new()`'s router whenever a session-tool transport is configured — the host
/// surface all three round-trip to.
pub(crate) fn action_tool_router<S>() -> rmcp::handler::server::router::tool::ToolRouter<S>
where
    S: rmcp::service::MaybeSend + 'static,
{
    use rmcp::handler::server::router::tool::ToolRouter;

    let mut router = ToolRouter::new();

    let request_tool = rmcp::model::Tool::new(
        "request_action",
        "Request a new session action: describe the command you need in natural language and name \
         the attached agent that should write it; that agent writes a bounded manifest for it. \
         Once established, run it with invoke_action. Returns {id, summary, path, \
         has_input_schema}.",
        schema_object(serde_json::json!({
            "type": "object",
            "required": ["agent", "description"],
            "properties": {
                "agent": {
                    "type": "string",
                    "description": "Id of the attached agent that writes the manifest (the same ids subagent_new_session takes)."
                },
                "description": {
                    "type": "string",
                    "description": "What the action should do, e.g. 'run the tddy-core test suite'."
                },
                "suggested_id": {
                    "type": "string",
                    "description": "Optional kebab-case id for the new action."
                }
            }
        })),
    );
    router.add_route(subagent_route(request_tool, |args| {
        Box::pin(request_action_tool(args))
    }));

    let list_tool = rmcp::model::Tool::new(
        "list_actions",
        "List the session actions available to invoke_action. \
         Returns {actions: [{id, summary, has_input_schema, has_output_schema}]}.",
        schema_object(serde_json::json!({
            "type": "object",
            "properties": {}
        })),
    );
    router.add_route(subagent_route(list_tool, |args| {
        Box::pin(list_actions_tool(args))
    }));

    let invoke_tool = rmcp::model::Tool::new(
        "invoke_action",
        "Invoke an established session action by id, blocking until it exits. \
         Returns {exit_code, stdout, stderr} (plus summary for result_kind: test_summary).",
        schema_object(serde_json::json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": {"type": "string", "description": "Action id (see list_actions)."},
                "data": {
                    "type": "object",
                    "description": "JSON arguments validated against the action's input_schema."
                }
            }
        })),
    );
    router.add_route(subagent_route(invoke_tool, |args| {
        Box::pin(invoke_action_tool(args))
    }));

    router
}
