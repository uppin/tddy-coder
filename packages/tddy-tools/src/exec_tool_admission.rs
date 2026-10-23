//! Which exec tools a session's agent is advertised (`#project-home` 5/7).
//!
//! The exec catalog is merged into the tool router once, at construction, so every tool the daemon can
//! run is callable by name. What `tools/list` reports is filtered per call: a tool in the exec catalog
//! is advertised only when the daemon admits it for this session. A tool outside the exec catalog is
//! not this module's concern and is always advertised.
//!
//! The daemon's answer is the single source. It is the set `ListExecTools` returns for the session, and
//! for a project-home session that is its read-only tools and the project-home tools. With no answer, the
//! static exec catalog is admitted, which is what every session was offered before the daemon answered
//! per session; the project-home tools are then not admitted, since only a project-home session is.

use tddy_tool_engine::project_home_tools::project_home_tool_defs;

use crate::mcp_primitives::RemoteToolDef;

/// The exec tool names admitted when the daemon gives no session-scoped answer: the static catalog.
pub fn static_exec_tool_names() -> Vec<String> {
    tddy_tool_engine::tool_catalog()
        .into_iter()
        .map(|tool| tool.name)
        .collect()
}

/// The exec tool names the daemon admits this session, or the static catalog when it gives no answer.
pub async fn admitted_exec_tool_names() -> Vec<String> {
    match crate::session_tool_client::session_exec_tool_names().await {
        Some(names) => names,
        // No session-scoped answer (the transport cannot ask, or the ask failed and was logged): the
        // static catalog, as before the daemon answered per session.
        None => static_exec_tool_names(),
    }
}

/// The project-home tools, as MCP tool definitions. Registered at construction so a project-home
/// session's call to one of them dispatches; advertised only to a project-home session.
pub fn project_home_remote_tool_defs() -> Vec<RemoteToolDef> {
    project_home_tool_defs()
        .into_iter()
        .map(|tool| RemoteToolDef {
            name: tool.name,
            description: tool.description,
            input_schema_json: tool.input_schema_json,
        })
        .collect()
}

/// Every name in the exec catalog: the static catalog and the project-home tools.
fn exec_catalog_names() -> Vec<String> {
    let mut names = static_exec_tool_names();
    names.extend(project_home_tool_defs().into_iter().map(|tool| tool.name));
    names
}

/// Whether the tool `name` is advertised, given the exec tool names the daemon admits.
pub fn is_advertised(name: &str, admitted: &[String]) -> bool {
    let in_exec_catalog = exec_catalog_names().iter().any(|exec| exec == name);
    !in_exec_catalog || admitted.iter().any(|admitted| admitted == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_tool_engine::project_home_tools::project_home_offered_catalog;

    fn advertised_exec_names(admitted: &[String]) -> Vec<String> {
        let mut names: Vec<String> = exec_catalog_names()
            .into_iter()
            .filter(|name| is_advertised(name, admitted))
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

        // Then exactly the read-only tools and the four project-home tools, and no write or shell tool
        assert_eq!(
            advertised,
            sorted(&[
                "Read",
                "Grep",
                "Glob",
                "ReadLints",
                "SemanticSearch",
                "ListProjectSessions",
                "StartProjectSession",
                "ReadSessionWorktree",
                "DiffSessionWorktree",
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
        assert_eq!(advertised, sorted(&static_exec_tool_names().iter().map(String::as_str).collect::<Vec<_>>()));
        assert!(!advertised.iter().any(|name| name == "ListProjectSessions"));
        assert!(!advertised.iter().any(|name| name == "DiffSessionWorktree"));
    }

    #[test]
    fn without_an_answer_the_static_catalog_is_advertised_and_no_project_home_tool() {
        // Given no session-scoped answer, so the static catalog is admitted
        let admitted = static_exec_tool_names();

        // When a project-home tool and a catalog tool are checked
        // Then the project-home tool is withheld and the catalog tool is advertised
        assert!(!is_advertised("ReadSessionWorktree", &admitted));
        assert!(is_advertised("Write", &admitted));
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
