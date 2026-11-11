//! The one session type no tool file declares: a session with no agent of its own.
//!
//! Every other session type is declared by a tool file's `extensions.session.session_type`
//! (`tddy-coding-agent-specs`), so nothing in Rust or TypeScript lists them. A session with zero
//! agents — a worktree and a session room, whose agents (if any) join it from its roster — has no
//! tool file to declare it, so its wire value is this constant, and every check of it goes through
//! [`is_no_agent_session_type`]. It lives here, the lowest crate every reader of a session type
//! already depends on.

/// The wire value of a session with no agent of its own. A tool file may not declare it.
pub const NO_AGENT_SESSION_TYPE: &str = "workspace";

/// Whether `session_type` (trimmed) names a session with no agent of its own.
#[must_use]
pub fn is_no_agent_session_type(session_type: &str) -> bool {
    session_type.trim() == NO_AGENT_SESSION_TYPE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_no_agent_word_is_recognised_trimmed_and_nothing_else_is() {
        // When
        let recognised = [
            is_no_agent_session_type("workspace"),
            is_no_agent_session_type(" workspace "),
            is_no_agent_session_type("tool"),
            is_no_agent_session_type(""),
            is_no_agent_session_type("claude-cli"),
        ];

        // Then
        assert_eq!(recognised, [true, true, false, false, false]);
    }
}
