//! Discovery agent — OpenAI-compatible multi-turn codebase exploration, the specialized
//! subagents built on it, and the session-scoped runtime that keeps track of both.

pub mod agent_def;
pub mod agent_list_mapping;
pub mod backend;
pub mod catalog_entry;
pub mod catalog_service;
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

pub use catalog_entry::{build_catalog_entry, CATALOG_SERVICE};
pub use catalog_service::{CatalogHandler, CatalogServiceImpl};

#[cfg(test)]
mod unbundle_catalog_entry_tests {
    #[test]
    fn names_the_service_family_a_moves_to() {
        struct EmptyCatalog;
        #[async_trait::async_trait]
        impl tddy_service::proto::catalog::CatalogService for EmptyCatalog {
            async fn list_tools(
                &self,
                _request: tddy_rpc::Request<tddy_service::proto::catalog::ListToolsRequest>,
            ) -> Result<
                tddy_rpc::Response<tddy_service::proto::catalog::ListToolsResponse>,
                tddy_rpc::Status,
            > {
                Ok(tddy_rpc::Response::new(
                    tddy_service::proto::catalog::ListToolsResponse { tools: vec![] },
                ))
            }
            async fn list_agents(
                &self,
                _request: tddy_rpc::Request<tddy_service::proto::catalog::ListAgentsRequest>,
            ) -> Result<
                tddy_rpc::Response<tddy_service::proto::catalog::ListAgentsResponse>,
                tddy_rpc::Status,
            > {
                Ok(tddy_rpc::Response::new(
                    tddy_service::proto::catalog::ListAgentsResponse { agents: vec![] },
                ))
            }
            async fn list_agent_models(
                &self,
                _request: tddy_rpc::Request<tddy_service::proto::catalog::ListAgentModelsRequest>,
            ) -> Result<
                tddy_rpc::Response<tddy_service::proto::catalog::ListAgentModelsResponse>,
                tddy_rpc::Status,
            > {
                Ok(tddy_rpc::Response::new(
                    tddy_service::proto::catalog::ListAgentModelsResponse {
                        models: vec![],
                        default_model: String::new(),
                    },
                ))
            }
            async fn list_subagents(
                &self,
                _request: tddy_rpc::Request<tddy_service::proto::catalog::ListSubagentsRequest>,
            ) -> Result<
                tddy_rpc::Response<tddy_service::proto::catalog::ListSubagentsResponse>,
                tddy_rpc::Status,
            > {
                Ok(tddy_rpc::Response::new(
                    tddy_service::proto::catalog::ListSubagentsResponse { subagents: vec![] },
                ))
            }
        }
        assert_eq!(
            super::build_catalog_entry(EmptyCatalog).name,
            "catalog.CatalogService"
        );
    }
}
