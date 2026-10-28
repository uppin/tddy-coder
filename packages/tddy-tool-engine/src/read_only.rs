//! The read-only exec catalog of a project-home session (`#project-home` 4/7).
//!
//! A project-home session may read and search its home worktree but never write, edit, delete or
//! run a process there. The same rule answers both questions the daemon asks — what to advertise
//! and what to execute — so the guarantee never rests on the agent's allowlist alone.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home session.

use crate::catalog::ToolDef;

/// The exec-catalog tools that only read or search.
const READ_ONLY_TOOLS: [&str; 5] = ["Read", "Grep", "Glob", "ReadLints", "SemanticSearch"];

/// Whether `name` (an exec-catalog tool name, e.g. `Read`) is one a project-home session may run.
pub fn is_read_only_tool(name: &str) -> bool {
    READ_ONLY_TOOLS.contains(&name)
}

/// Whether a project-home session may run `name`: a read-only exec tool, one of the four
/// project-home session tools (`#project-home` 5/7), or one of the seven project-home git tools
/// (`#project-home` 7/7). [`is_read_only_tool`] itself is unchanged, so the read-only catalog and
/// every other answer it gives stay as they were.
pub fn is_project_home_allowed_tool(name: &str) -> bool {
    is_read_only_tool(name)
        || crate::project_home_tools::is_project_home_tool(name)
        || crate::project_home_git_tools::is_project_home_git_tool(name)
}

/// The prefix `claude` gives a tool served by the `tddy-tools` MCP server.
const MCP_TOOL_PREFIX: &str = "mcp__tddy-tools__";

/// What a project-home session's `claude` is pre-approved to call, in the names `--allowedTools` takes:
/// every tool [`is_project_home_allowed_tool`] lets it run (the read-only tools, the project-home
/// session tools and the project-home git tools), as the `mcp__tddy-tools__*` tools they are reached through. Derived from
/// [`project_home_offered_catalog`](crate::project_home_tools::project_home_offered_catalog), so the
/// set the spawn names, the set the daemon advertises and the set it executes are one set.
pub fn project_home_allowed_tools() -> Vec<String> {
    crate::project_home_tools::project_home_offered_catalog()
        .into_iter()
        .map(|tool| format!("{MCP_TOOL_PREFIX}{}", tool.name))
        .collect()
}

/// The subset of [`crate::tool_catalog`] a project-home session is offered.
pub fn read_only_catalog() -> Vec<ToolDef> {
    crate::tool_catalog()
        .into_iter()
        .filter(|tool| is_read_only_tool(&tool.name))
        .collect()
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
    fn the_project_home_allowed_tools_are_the_read_only_and_project_home_tools_as_mcp_names() {
        // When
        let names = project_home_allowed_tools();

        // Then exactly the read-only tools, the project-home tools and the seven git tools, under
        // their MCP names
        assert_eq!(
            names,
            vec![
                "mcp__tddy-tools__Read",
                "mcp__tddy-tools__Grep",
                "mcp__tddy-tools__Glob",
                "mcp__tddy-tools__ReadLints",
                "mcp__tddy-tools__SemanticSearch",
                "mcp__tddy-tools__ListProjectSessions",
                "mcp__tddy-tools__StartSession",
                "mcp__tddy-tools__GetSessionStart",
                "mcp__tddy-tools__CancelSessionStart",
                "mcp__tddy-tools__ReadSessionWorktree",
                "mcp__tddy-tools__DiffSessionWorktree",
                "mcp__tddy-tools__DescribeSessionStartOptions",
                "mcp__tddy-tools__ListWorktrees",
                "mcp__tddy-tools__GetWorktree",
                "mcp__tddy-tools__ReadSessionActivity",
                "mcp__tddy-tools__ReadSessionTranscript",
                "mcp__tddy-tools__ReadSessionTerminal",
                "mcp__tddy-tools__ClaimSessionTerminal",
                "mcp__tddy-tools__SendSessionKeys",
                "mcp__tddy-tools__AnswerSessionPermission",
                "mcp__tddy-tools__TerminateSession",
                "mcp__tddy-tools__RefreshWorktreeDiskUsage",
                "mcp__tddy-tools__CommitSessionWorktree",
                "mcp__tddy-tools__PushSessionBranch",
                "mcp__tddy-tools__RebaseSessionBranch",
                "mcp__tddy-tools__StashSessionWorktree",
                "mcp__tddy-tools__ResetSessionWorktree",
                "mcp__tddy-tools__CheckoutSessionWorktree",
                "mcp__tddy-tools__CleanSessionWorktree",
            ]
        );
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
