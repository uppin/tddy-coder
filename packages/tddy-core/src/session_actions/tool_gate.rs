//! Whether this session's host actually serves the session-action tools.
//!
//! `request_action`, `list_actions` and `invoke_action` are host round-trips —
//! `EstablishAction` / `ListActions` / `InvokeAction` against a session directory that exists only
//! on the host. Until `#unbundle` node 5 they were advertised whenever *any* session-tool
//! transport was reachable, which is not the same question: the transport says a host is there, not
//! that it answers these three. `tddy-daemon`'s handler dispatches into
//! `tddy_tool_engine::execute_tool_with_env`, whose `match` has no arm for any of them, so a
//! daemon-hosted session advertised three tools that always came back
//! `{"error":"unknown tool: ListActions","is_error":true}`. That is worse than absent: an agent
//! read the refusal as "the specialized agent is not registered" and reported it as such
//! (`docs/dev/todo/2026-08-23-the-action-tools-are-advertised-where-nothing-implements-them.md`).
//!
//! So the host declares it, exactly as it declares an available language server with
//! `TDDY_LSP_TOOLS` (`tddy_lsp_executor::lsp_tools`). Only a host that routes the three somewhere
//! that answers them sets [`SESSION_ACTION_TOOLS_ENV`]; every other placement stays silent and the
//! tools are not offered.
//!
//! The gate lives here, in the crate that owns session actions, rather than in either the crate
//! that advertises them (`tddy-tools`) or the crate that answers them (`tddy-sandbox-app`) —
//! the same placement rule the LSP gate follows, and the only one both sides can name.

/// Env var a host sets per session when its tool transport routes `EstablishAction`,
/// `ListActions` and `InvokeAction` to something that implements them. Its presence (non-empty)
/// gates whether the three session-action MCP tools are advertised at all.
pub const SESSION_ACTION_TOOLS_ENV: &str = "TDDY_SESSION_ACTION_TOOLS";

/// Whether the session-action tools should be exposed for this session (the
/// [`SESSION_ACTION_TOOLS_ENV`] gate).
///
/// A blank value counts as unset: an outer process exporting the variable empty has not claimed to
/// serve anything.
pub fn session_action_tools_enabled() -> bool {
    std::env::var(SESSION_ACTION_TOOLS_ENV).is_ok_and(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    /// The default is silence: a host that has not said it serves the three does not get them
    /// advertised, however reachable its tool transport is.
    #[test]
    #[serial]
    fn a_host_that_has_not_claimed_the_action_surface_does_not_get_it() {
        // Given
        std::env::remove_var(SESSION_ACTION_TOOLS_ENV);

        // When
        let enabled = session_action_tools_enabled();

        // Then
        assert!(!enabled, "an unclaimed action surface must not be offered");
    }

    /// A host exporting the variable empty has claimed nothing — the same reading every other
    /// `TDDY_*` gate gives a blank value.
    #[test]
    #[serial]
    fn a_blank_claim_is_no_claim() {
        // Given
        std::env::set_var(SESSION_ACTION_TOOLS_ENV, "  ");

        // When
        let enabled = session_action_tools_enabled();
        std::env::remove_var(SESSION_ACTION_TOOLS_ENV);

        // Then
        assert!(!enabled, "a blank claim must not offer the action surface");
    }

    #[test]
    #[serial]
    fn a_host_that_claims_the_action_surface_gets_it() {
        // Given
        std::env::set_var(SESSION_ACTION_TOOLS_ENV, "1");

        // When
        let enabled = session_action_tools_enabled();
        std::env::remove_var(SESSION_ACTION_TOOLS_ENV);

        // Then
        assert!(enabled, "a claimed action surface must be offered");
    }
}
