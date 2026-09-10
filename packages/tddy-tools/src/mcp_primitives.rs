//! The handful of items `server.rs` shared with the modules that import *back* from it.
//!
//! `#unbundle` node 5's **step zero**, and nothing else in the node compiles until it lands.
//! `action_tools`, `lsp_tools` and `session_agents/{seed,stream}` all imported from `server`, while
//! `server` imported from all three — three cycles over eight small items. A cycle inside one crate
//! is legal and merely awkward; the moment the three modules move to three *different* crates it is
//! a build error, so the shared items have to have a home of their own first.
//!
//! Keeping them in `tddy-tools` rather than pushing them to a library is deliberate: they are MCP
//! plumbing — a tool route, a JSON-Schema object, an error envelope — and `tddy-tools` is the crate
//! that speaks MCP. What moves out of this crate is the tools' *implementations*, not the shape of
//! an MCP tool.
//!
//! No item here names `server::PermissionServer`. [`subagent_route`] is generic over the server
//! type for exactly that reason: `action_tools` builds its routes with it and is destined for
//! `tddy-core`, so a route type that named this crate's `ServerHandler` would turn today's
//! in-crate cycle into a cross-crate one — the error step zero exists to prevent.

use tddy_discovery::agent_def::SpecializedAgentDef;
use tddy_discovery::subagent::{CodebaseAccess, SubagentConfig, SubagentRegistry, SubagentSession};

// --- The spawn environment ---

/// An environment variable's value, or `None` when it is unset **or blank**.
///
/// Blank is treated as unset because every caller here reads a variable an outer process may have
/// exported empty, and a blank `TDDY_SOCKET` is not a socket path — it is the absence of one.
pub(crate) fn env_non_empty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

/// Parse `TDDY_SUBAGENTS_JSON` (a JSON array of [`SpecializedAgentDef`] — see
/// docs/ft/coder/specialized-subagents.md) into the resolved specialized-agent defs for this
/// process. Empty when the env var is unset or blank: with no def there is no agent, since every
/// agent this process can address came from a def source someone wrote.
///
/// A value that is *set* and does not parse is an error, never an empty seed. `SpecializedAgentDef`
/// is `deny_unknown_fields`, so a `tddy-tools` older than the daemon that wrote the value parses
/// exactly this way — and an empty seed means no agent is attached and none of the withdrawn tools
/// are served by anyone, with nothing naming the variable that caused it.
///
/// The message carries serde's position, never the value: a def carries a provider credential.
pub fn subagents_from_env() -> Result<Vec<SpecializedAgentDef>, String> {
    let Some(json) = env_non_empty("TDDY_SUBAGENTS_JSON") else {
        return Ok(Vec::new());
    };
    serde_json::from_str::<Vec<SpecializedAgentDef>>(&json).map_err(|e| {
        format!(
            "TDDY_SUBAGENTS_JSON is set but does not parse as an array of agent defs: {e}. \
             This is what a tddy-tools older than the daemon that spawned it sees, and treating \
             it as 'no agents are attached' would silently un-withdraw every tool the session's \
             agents took over"
        )
    })
}

/// The spawn seed for the two lazy constructions that have no caller to refuse to — the MCP
/// server's router and the process-wide roster.
///
/// `--mcp` already refused to start on an unparseable value (see `run_mcp_server`), so reaching the
/// error arm means a caller that never passed that gate. It is reported at `error` naming the
/// variable rather than passed off as a session nobody attached an agent to.
pub(crate) fn seed_subagents_or_report() -> Vec<SpecializedAgentDef> {
    subagents_from_env().unwrap_or_else(|e| {
        log::error!(target: "tddy_tools::server", "{e}");
        Vec::new()
    })
}

/// Resolve how a subagent's internal READ/GLOB/GREP calls reach the codebase: explicit
/// `TDDY_SUBAGENT_CODEBASE_ACCESS` override, else `Managed` when a session-tool transport is
/// configured (mirrors the exec-tool gating in `server`), else `Local`.
fn subagent_codebase_access_from_env() -> CodebaseAccess {
    match env_non_empty("TDDY_SUBAGENT_CODEBASE_ACCESS").as_deref() {
        Some("local") => CodebaseAccess::Local,
        Some("managed") => managed_codebase_access(),
        _ => {
            if crate::session_tool_client::detect_session_tool_transport().is_some() {
                managed_codebase_access()
            } else {
                CodebaseAccess::Local
            }
        }
    }
}

/// Wrap [`crate::session_tool_client::dispatch_session_tool`] as a `CodebaseAccess::Managed`
/// dispatch fn — the same proxy transport the exec-tool catalog already uses.
fn managed_codebase_access() -> CodebaseAccess {
    CodebaseAccess::managed(|tool_name: String, args: serde_json::Value| {
        Box::pin(async move {
            crate::session_tool_client::dispatch_session_tool(&tool_name, args).await
        })
    })
}

/// The only thing a caller supplies that a def cannot: how this process reaches the codebase.
/// Endpoint, model, credential and turn budget come from the def itself.
pub(crate) fn subagent_config_from_env() -> SubagentConfig {
    SubagentConfig {
        access: subagent_codebase_access_from_env(),
    }
}

// --- The shape of an MCP tool ---

/// A tool definition fetched from the relay daemon (or configured statically for testing).
pub struct RemoteToolDef {
    pub name: String,
    pub description: String,
    pub input_schema_json: String,
}

/// The `inputSchema` an `rmcp` [`rmcp::model::Tool`] wants, from the JSON object that spells it out.
///
/// A value that is not a JSON object becomes the empty schema rather than a panic: an input schema
/// is a description of what a tool accepts, and the honest description of an unreadable one is
/// "nothing is known about the arguments", which every MCP client already handles.
pub(crate) fn schema_object(
    json: serde_json::Value,
) -> std::sync::Arc<serde_json::Map<String, serde_json::Value>> {
    std::sync::Arc::new(json.as_object().cloned().unwrap_or_default())
}

/// Wraps a subagent tool handler (`async fn(Value) -> String`) into a `ToolRoute` — the same
/// success-envelope-with-embedded-error convention `dynamic_tool_router` uses for exec tools.
///
/// Generic over the server type: the modules that build routes with this are on their way out of
/// this crate, and a route fixed to `PermissionServer` would follow them there as a dependency
/// edge back onto `tddy-tools`. The caller supplies its own `ServerHandler` as `S`.
pub(crate) fn subagent_route<S, F>(
    tool: rmcp::model::Tool,
    handler: F,
) -> rmcp::handler::server::router::tool::ToolRoute<S>
where
    S: rmcp::service::MaybeSend + 'static,
    F: Fn(serde_json::Value) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send>>
        + Send
        + Sync
        + 'static,
{
    rmcp::handler::server::router::tool::ToolRoute::new_dyn(tool, move |ctx| {
        let arguments = serde_json::Value::Object(ctx.arguments.clone().unwrap_or_default());
        let result_future = handler(arguments);
        Box::pin(async move {
            let result_string = result_future.await;
            Ok(rmcp::model::CallToolResult::success(vec![
                rmcp::model::Content::text(result_string),
            ]))
        })
    })
}

/// The error envelope a subagent tool returns, so a failure is a *result* an agent can read rather
/// than a transport error it never sees.
pub(crate) fn subagent_error_json(message: impl std::fmt::Display) -> String {
    serde_json::json!({ "error": message.to_string(), "is_error": true }).to_string()
}

// --- Opening and closing a turn loop with an attached agent ---

/// An agent opened for one turn loop: what to prompt, and — when the loop runs on another daemon —
/// what to close when the conversation ends.
pub(crate) struct OpenedAgent {
    pub(crate) agent_id: String,
    pub(crate) session: Box<dyn SubagentSession>,
    /// `None` for an agent this process runs itself: there is nothing on another host to close.
    pub(crate) remote: Option<crate::session_agents::RemoteConversationHandle>,
}

/// Open a conversation with `entry` on the daemon that runs it, for an agent this process holds no
/// def for.
///
/// A roster entry carries no endpoint or credential — deliberately — so an agent owned by another
/// daemon, and a local one attached after spawn, are run by asking the facilitating daemon to run
/// them (docs/ft/daemon/session-agent-roster.md § Invoking an agent). The refusal names the agent
/// and the daemon its conversations are routed by, so an operator reads which host to go and look
/// at rather than "this session cannot reach it".
pub(crate) async fn open_remote_agent_session(
    entry: &tddy_service::proto::connection::SessionAgentEntry,
    conversation_id: &str,
) -> Result<OpenedAgent, String> {
    let refused = |e: String| {
        format!(
            "agent '{}' is routed by daemon '{}': {e}",
            entry.agent_id, entry.daemon_instance_id
        )
    };
    let link = std::sync::Arc::new(
        crate::session_agents::AgentConversationLink::connect()
            .await
            .map_err(refused)?,
    );
    let opened = link
        .open(&entry.agent_id, conversation_id)
        .await
        .map_err(refused)?;
    Ok(OpenedAgent {
        agent_id: entry.agent_id.clone(),
        session: Box::new(link.session(opened.clone(), &entry.model)),
        remote: Some(link.handle(opened)),
    })
}

/// Open a turn loop with the roster agent `agent_id`, for a tool that runs one bounded exchange of
/// its own instead of handing a conversation to the main agent (`request_action`).
///
/// Resolved against the live roster exactly as `server::subagent_new_session_tool` resolves it —
/// same ids, same refusals, no default for a call that names none — so which agents are addressable
/// does not depend on which tool is asking, and no tool confers a role on an agent by inspecting
/// what it `replaces`.
///
/// No conversation is registered with the roster: the exchange opens and ends inside the call, so
/// there is nothing a later `subagent_cancel` or a detach could address.
pub(crate) async fn open_roster_agent_session(agent_id: &str) -> Result<OpenedAgent, String> {
    let roster = crate::session_agents::session_agent_roster();
    let entry = roster.resolve(Some(agent_id)).map_err(|e| e.to_string())?;
    let Some(def) = roster.local_def_for(&entry) else {
        // The daemon mints the conversation id here: nothing outside this call can name the
        // exchange, so there is nothing a caller-chosen id would let it cancel. The caller closes
        // it through the handle instead.
        return open_remote_agent_session(&entry, "").await;
    };
    let name = def.name.clone();
    let session = SubagentRegistry::from_defs(vec![def])
        .create(&name, subagent_config_from_env())
        .map_err(|e| format!("agent '{}': {e}", entry.agent_id))?;
    Ok(OpenedAgent {
        agent_id: entry.agent_id,
        session,
        remote: None,
    })
}

/// Close a conversation on the daemon running its turn loop, when it runs on one.
///
/// Failure is logged rather than returned: the conversation is already gone on this side, so there
/// is nothing the caller could do differently, and reporting a cancel as failed would tell the main
/// agent a conversation it can no longer prompt is still open. Logged at `error` because a
/// conversation left open on the owning daemon is a leak an operator has to be able to find.
pub(crate) async fn cancel_remote_conversation(
    remote: Option<crate::session_agents::RemoteConversationHandle>,
) {
    let Some(remote) = remote else {
        return;
    };
    if let Err(e) = remote.cancel().await {
        log::error!(
            target: "tddy_tools::session_agents",
            "conversation '{}' was closed here but not on the daemon running it: {e}",
            remote.conversation_id()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use serial_test::serial;

    /// The server type a caller outside this module supplies to [`subagent_route`]. It is a bare
    /// marker on purpose: what the route needs of `S` is a type parameter, nothing more.
    struct AnMcpServer;

    fn a_tool_named(name: &'static str) -> rmcp::model::Tool {
        rmcp::model::Tool::new(
            name,
            "A tool an attached agent may call.",
            schema_object(json!({"type": "object"})),
        )
    }

    /// A blank exported variable is the absence of a value, not a value of "". Every caller reads
    /// something an outer process may have exported empty.
    #[test]
    #[serial]
    fn reads_an_empty_variable_as_unset() {
        // Given
        std::env::set_var("TDDY_TEST_BLANK", "");

        // When
        let found = env_non_empty("TDDY_TEST_BLANK");

        // Then
        assert_eq!(found, None);
    }

    /// A variable exported as whitespace is blank too — a spawn environment that interpolated an
    /// empty value into a quoted assignment has still said nothing.
    #[test]
    #[serial]
    fn reads_a_whitespace_only_variable_as_unset() {
        // Given
        std::env::set_var("TDDY_TEST_WHITESPACE", "  \t ");

        // When
        let found = env_non_empty("TDDY_TEST_WHITESPACE");

        // Then
        assert_eq!(found, None);
    }

    #[test]
    #[serial]
    fn reads_a_variable_that_has_a_value() {
        // Given
        std::env::set_var("TDDY_TEST_SET", "/run/tddy.sock");

        // When
        let found = env_non_empty("TDDY_TEST_SET");

        // Then
        assert_eq!(found.as_deref(), Some("/run/tddy.sock"));
    }

    /// A tool failure has to come back as a readable result, because an agent cannot act on a
    /// transport error it never sees — hence the `is_error` flag *inside* the payload.
    #[test]
    fn renders_a_failure_as_a_result_an_agent_can_read() {
        // When
        let rendered = subagent_error_json("the roster has no addressable agent");

        // Then
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&rendered)
                .expect("the error envelope must be JSON"),
            json!({"error": "the roster has no addressable agent", "is_error": true})
        );
    }

    /// The declared schema reaches the tool as `rmcp` wants it: the object's own members, not the
    /// object wrapped in anything.
    #[test]
    fn carries_a_declared_schema_through_as_the_tools_input_schema() {
        // Given
        let declared = json!({
            "type": "object",
            "required": ["agent"],
            "properties": {"agent": {"type": "string"}}
        });

        // When
        let schema = schema_object(declared.clone());

        // Then
        assert_eq!(serde_json::Value::Object((*schema).clone()), declared);
    }

    /// A schema that is not an object describes nothing, and "nothing is known about the arguments"
    /// is a schema every MCP client handles — unlike a panic inside router assembly.
    #[test]
    fn describes_a_non_object_schema_as_accepting_nothing_known() {
        // When
        let schema = schema_object(json!("an object was expected here"));

        // Then
        assert_eq!(*schema, serde_json::Map::new());
    }

    /// The route is built for a server type this module never names. That is the whole point of
    /// step zero: `action_tools` routes its three tools through here and is destined for
    /// `tddy-core`, which cannot name `tddy-tools`' `ServerHandler`.
    #[test]
    fn routes_a_handler_for_a_server_type_this_module_does_not_name() {
        // Given
        let tool = a_tool_named("request_action");

        // When
        let route = subagent_route::<AnMcpServer, _>(tool, |_args| {
            Box::pin(async { subagent_error_json("no agent was named") })
        });

        // Then
        assert_eq!(route.name(), "request_action");
    }
}
