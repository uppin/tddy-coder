//! Discovery agent — OpenAI-compatible multi-turn codebase exploration, the specialized
//! subagents built on it, and the session-scoped runtime that keeps track of both.

pub mod agent_def;
pub mod backend;
pub mod discovery;
pub mod openai;
/// The live roster of agents attached to a session: what the daemon says is attached, what those
/// agents have taken over, and the conversation RPCs that reach the ones this process cannot run.
pub mod roster;
pub mod subagent;
/// Every conversation a session has open with a subagent, and what each has spent.
pub mod subagent_runtime;
pub mod tools;
pub mod warmup;

/// The `catalog.CatalogService` entry — `#unbundle` node 8, family A.
///
/// Served from this crate because it already resolves every one of these answers: `agent_def`
/// owns `SpecializedAgentDef`, and since node 5 [`roster`] owns the live roster a `ListSubagents`
/// is answered from.
///
/// It takes **rows**, not a `DaemonConfig`. `main.rs` already extracts them with
/// `agent_list_mapping::agent_allowlist_rows`, and handing a leaf crate the daemon's whole
/// configuration type to read four lists would be the wrong trade.
pub fn build_catalog_entry(
    _allowed_tools: Vec<String>,
    _allowed_agents: Vec<String>,
) -> tddy_rpc::ServiceEntry {
    // TODO(exec-prstack-services): implement
    unimplemented!("build_catalog_entry")
}

#[cfg(test)]
mod unbundle_catalog_entry_tests {
    #[test]
    fn names_the_service_family_a_moves_to() {
        assert_eq!(
            super::build_catalog_entry(Vec::new(), Vec::new()).name,
            "catalog.CatalogService"
        );
    }
}

