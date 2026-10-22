//! The read-only exec catalog of a project-home session (`#project-home` 4/7).
//!
//! A project-home session may read and search its home worktree but never write, edit, delete or
//! run a process there. The same rule answers both questions the daemon asks — what to advertise
//! and what to execute — so the guarantee never rests on the agent's allowlist alone.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-10-22-project-home-session.md.

use crate::catalog::ToolDef;

/// Whether `name` (an exec-catalog tool name, e.g. `Read`) is one a project-home session may run.
pub fn is_read_only_tool(name: &str) -> bool {
    // TODO(home-session): implement
    let _ = name;
    false
}

/// The subset of [`crate::tool_catalog`] a project-home session is offered.
pub fn read_only_catalog() -> Vec<ToolDef> {
    // TODO(home-session): implement
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_read_and_search_tools_are_read_only() {
        // Given every tool the exec catalog offers
        let verdicts: Vec<(String, bool)> = crate::tool_catalog()
            .into_iter()
            .map(|t| {
                let read_only = is_read_only_tool(&t.name);
                (t.name, read_only)
            })
            .collect();

        // Then exactly the reading and searching ones are read-only
        let expected: Vec<(String, bool)> = [
            ("Read", true),
            ("Write", false),
            ("StrReplace", false),
            ("Delete", false),
            ("Grep", true),
            ("Glob", true),
            ("Shell", false),
            ("Await", false),
            ("ReadLints", true),
            ("SemanticSearch", true),
        ]
        .into_iter()
        .map(|(n, r)| (n.to_string(), r))
        .collect();
        assert_eq!(verdicts, expected);
    }

    #[test]
    fn the_read_only_catalog_offers_the_read_only_tools_in_catalog_order() {
        // When
        let names: Vec<String> = read_only_catalog().into_iter().map(|t| t.name).collect();

        // Then
        assert_eq!(
            names,
            vec!["Read", "Grep", "Glob", "ReadLints", "SemanticSearch"]
        );
    }
}
