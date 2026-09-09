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

use serde_json::Value;

/// An environment variable's value, or `None` when it is unset **or empty**.
///
/// Empty is treated as unset because every caller here reads a variable an outer process may have
/// exported blank, and a blank `TDDY_SOCKET` is not a socket path — it is the absence of one.
pub fn env_non_empty(_name: &str) -> Option<String> {
    // TODO(tools-thinning): implement
    unimplemented!("mcp_primitives::env_non_empty")
}

/// A JSON-Schema `object` node with the given properties, as `rmcp` wants an input schema.
pub fn schema_object(_properties: Value, _required: &[&str]) -> Value {
    // TODO(tools-thinning): implement
    unimplemented!("mcp_primitives::schema_object")
}

/// One tool a remote host advertises, as the dynamic router sees it.
///
/// Named `Remote` because it is the *host's* declaration of a tool, not this process's — the router
/// forwards a call rather than executing it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: String,
}

/// Where a subagent-addressed tool call should be routed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubagentRoute {
    /// Serve it in this process, from the local roster.
    Local,
    /// Forward it to the facilitating daemon for `agent_id`.
    Remote { agent_id: String },
}

/// Decide where a subagent tool call goes.
pub fn subagent_route(_agent_id: Option<&str>) -> SubagentRoute {
    // TODO(tools-thinning): implement
    unimplemented!("mcp_primitives::subagent_route")
}

/// The error envelope a subagent tool returns, so a failure is a *result* an agent can read rather
/// than a transport error it cannot.
pub fn subagent_error_json(_message: &str) -> Value {
    // TODO(tools-thinning): implement
    unimplemented!("mcp_primitives::subagent_error_json")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A blank exported variable is the absence of a value, not a value of "". Every caller reads
    /// something an outer process may have exported empty.
    #[test]
    fn reads_an_empty_variable_as_unset() {
        // Given
        std::env::set_var("TDDY_TEST_BLANK", "");

        // When
        let found = env_non_empty("TDDY_TEST_BLANK");

        // Then
        assert_eq!(found, None);
    }

    #[test]
    fn reads_a_variable_that_has_a_value() {
        // Given
        std::env::set_var("TDDY_TEST_SET", "/run/tddy.sock");

        // When
        let found = env_non_empty("TDDY_TEST_SET");

        // Then
        assert_eq!(found.as_deref(), Some("/run/tddy.sock"));
    }

    #[test]
    fn routes_an_unaddressed_call_to_the_local_roster() {
        assert_eq!(subagent_route(None), SubagentRoute::Local);
    }

    #[test]
    fn routes_an_addressed_call_to_the_facilitating_daemon() {
        assert_eq!(
            subagent_route(Some("reviewer@daemon-7")),
            SubagentRoute::Remote {
                agent_id: "reviewer@daemon-7".to_string()
            }
        );
    }

    /// A tool failure has to come back as a readable result, because an agent cannot act on a
    /// transport error it never sees.
    #[test]
    fn renders_a_failure_as_a_result_an_agent_can_read() {
        // When
        let rendered = subagent_error_json("the roster has no addressable agent");

        // Then
        assert_eq!(
            rendered.get("error").and_then(|e| e.as_str()),
            Some("the roster has no addressable agent")
        );
    }
}
