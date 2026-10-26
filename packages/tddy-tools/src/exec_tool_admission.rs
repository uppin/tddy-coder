//! Which exec tools a session's agent is advertised (`#project-home` 5/7).
//!
//! The exec catalog is merged into the tool router once, at construction, so every tool the daemon can
//! run is callable by name. What `tools/list` reports is filtered per call: a tool in the exec catalog
//! is advertised only when the daemon admits it for this session. A tool outside the exec catalog is
//! not this module's concern and is always advertised.
//!
//! The daemon's answer is the single source. It is the set `ListExecTools` returns for the session, and
//! for a project-home session that is its read-only tools and the project-home tools.
//!
//! A failed ask is a refusal: `tools/list` returns an error that names the failure, and no exec tool is
//! offered. There is no catalog of this crate's own to fall back to.
//!
//! Every transport asks: the daemon's HTTP relay and unix socket, the sandbox socket (the runner relays
//! the question to the host daemon) and LiveKit (the remote daemon's `ExecToolService`). With no transport
//! configured there is no daemon to ask, and no exec tool is registered to admit.

use std::collections::HashSet;
use std::sync::LazyLock;

use tddy_session_tool_client::SessionExecToolAnswer;
use tddy_tool_engine::project_home_git_tools::project_home_git_tool_defs;
use tddy_tool_engine::project_home_tools::project_home_tool_defs;

use crate::mcp_primitives::RemoteToolDef;

/// The exec tool names of the static catalog: the daemon-wide set an unscoped caller is given.
pub fn static_exec_tool_names() -> Vec<String> {
    tddy_tool_engine::tool_catalog()
        .into_iter()
        .map(|tool| tool.name)
        .collect()
}

/// The exec tool names this session may be offered, from the daemon's answer.
///
/// `Err` carries the reason the exec tools are refused. The caller surfaces it; this does not offer
/// anything in its place.
pub fn admission_from_answer(answer: SessionExecToolAnswer) -> Result<Vec<String>, String> {
    match answer {
        SessionExecToolAnswer::Admitted(names) => Ok(names),
        SessionExecToolAnswer::Failed(reason) => Err(format!(
            "tddy-tools cannot list this session's exec tools: {reason}. No exec tool is offered until \
             the daemon answers."
        )),
        // Nothing is configured to ask, and no exec tool is registered, so there is nothing to admit.
        SessionExecToolAnswer::NoTransport => Ok(Vec::new()),
    }
}

/// The exec tool names this session may be offered, asked of the daemon now (see [`admission_from_answer`]).
pub async fn admitted_exec_tool_names() -> Result<Vec<String>, String> {
    admission_from_answer(crate::session_tool_client::session_exec_tool_answer().await)
}

/// The project-home tools and the project-home git tools (`#project-home` 7/7), as MCP tool
/// definitions. Registered at construction so a project-home session's call to one of them
/// dispatches; advertised only to a project-home session.
pub fn project_home_remote_tool_defs() -> Vec<RemoteToolDef> {
    project_home_tool_defs()
        .into_iter()
        .chain(project_home_git_tool_defs())
        .map(|tool| RemoteToolDef {
            name: tool.name,
            description: tool.description,
            input_schema_json: tool.input_schema_json,
        })
        .collect()
}

/// Every name in the exec catalog: the static catalog, the project-home tools and the project-home git
/// tools. Built once, since `tools/list` asks it for every tool of every listing.
static EXEC_CATALOG_NAMES: LazyLock<HashSet<String>> = LazyLock::new(|| {
    static_exec_tool_names()
        .into_iter()
        .chain(
            project_home_remote_tool_defs()
                .into_iter()
                .map(|tool| tool.name),
        )
        .collect()
});

/// Whether the tool `name` is advertised, given the exec tool names the daemon admits.
pub fn is_advertised(name: &str, admitted: &[String]) -> bool {
    !EXEC_CATALOG_NAMES.contains(name) || admitted.iter().any(|admitted| admitted == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_tool_engine::project_home_tools::project_home_offered_catalog;

    fn advertised_exec_names(admitted: &[String]) -> Vec<String> {
        let mut names: Vec<String> = EXEC_CATALOG_NAMES
            .iter()
            .filter(|name| is_advertised(name, admitted))
            .cloned()
            .collect();
        names.sort();
        names
    }

    fn sorted(names: &[&str]) -> Vec<String> {
        let mut names: Vec<String> = names.iter().map(|name| name.to_string()).collect();
        names.sort();
        names
    }

    #[test]
    fn a_project_home_session_is_advertised_its_read_only_tools_and_the_project_home_tools() {
        // Given the answer the daemon gives a project-home session
        let admitted: Vec<String> = project_home_offered_catalog()
            .into_iter()
            .map(|tool| tool.name)
            .collect();

        // When the advertised exec tools are read
        let advertised = advertised_exec_names(&admitted);

        // Then exactly the read-only tools, the project-home tools and the seven git tools, and
        // no write or shell tool
        assert_eq!(
            advertised,
            sorted(&[
                "Read",
                "Grep",
                "Glob",
                "ReadLints",
                "SemanticSearch",
                "ListProjectSessions",
                "StartSession",
                "GetSessionStart",
                "CancelSessionStart",
                "DescribeSessionStartOptions",
                "ReadSessionWorktree",
                "DiffSessionWorktree",
                "ListWorktrees",
                "GetWorktree",
                "ReadSessionActivity",
                "ReadSessionTranscript",
                "TerminateSession",
                "CommitSessionWorktree",
                "PushSessionBranch",
                "RebaseSessionBranch",
                "StashSessionWorktree",
                "ResetSessionWorktree",
                "CheckoutSessionWorktree",
                "CleanSessionWorktree",
            ])
        );
    }

    #[test]
    fn an_ordinary_session_is_advertised_the_exec_catalog_without_the_project_home_tools() {
        // Given the answer the daemon gives an ordinary session: the catalog, no project-home tools
        let admitted = static_exec_tool_names();

        // When the advertised exec tools are read
        let advertised = advertised_exec_names(&admitted);

        // Then every catalog tool and none of the project-home tools
        assert_eq!(
            advertised,
            sorted(
                &static_exec_tool_names()
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
            )
        );
        assert!(!advertised.iter().any(|name| name == "ListProjectSessions"));
        assert!(!advertised.iter().any(|name| name == "DiffSessionWorktree"));
        assert!(!advertised
            .iter()
            .any(|name| name == "CommitSessionWorktree"));
    }

    #[test]
    fn a_failed_ask_refuses_the_exec_tools_and_names_the_failure() {
        // Given the daemon was asked and did not answer usefully
        let answer =
            SessionExecToolAnswer::Failed("ListExecTools did not answer within 10s".into());

        // When the admission is read
        let admission = admission_from_answer(answer);

        // Then it is a refusal naming the failure, and nothing is offered in its place
        let reason = admission.expect_err("a failed ask refuses the exec tools");
        assert!(reason.contains("did not answer within 10s"), "{reason}");
        assert!(reason.contains("No exec tool is offered"), "{reason}");
    }

    #[test]
    fn an_admitted_answer_is_advertised_exactly_as_the_daemon_gave_it() {
        // Given the daemon's answer for a project-home session
        let names: Vec<String> = project_home_offered_catalog()
            .into_iter()
            .map(|tool| tool.name)
            .collect();

        // When the admission is read
        let admitted = admission_from_answer(SessionExecToolAnswer::Admitted(names.clone()))
            .expect("an admitted answer is not refused");

        // Then the advertised set is that answer
        assert_eq!(admitted, names);
        assert!(!is_advertised("ReadSessionWorktree", &[]));
        assert!(is_advertised("ReadSessionWorktree", &admitted));
    }

    #[test]
    fn no_configured_transport_admits_nothing_and_is_not_a_refusal() {
        // Given no session-tool transport: no exec tool is registered, so there is nothing to admit
        let admitted = admission_from_answer(SessionExecToolAnswer::NoTransport)
            .expect("no transport is not a failed ask");

        // Then nothing is admitted, and a tool outside the exec catalog is still advertised
        assert_eq!(admitted, Vec::<String>::new());
        assert!(is_advertised("approval_prompt", &admitted));
        assert!(!is_advertised("Write", &admitted));
    }

    #[test]
    fn a_tool_outside_the_exec_catalog_is_advertised_whatever_the_answer() {
        // Given an answer that admits nothing
        let admitted: Vec<String> = Vec::new();

        // When a tool that is not an exec tool is checked
        // Then it is advertised: this module only shapes the exec catalog
        assert!(is_advertised("approval_prompt", &admitted));
    }
}
