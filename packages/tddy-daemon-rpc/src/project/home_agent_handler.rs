//! `SetProjectHomeAgent` at the project coordinate: route like the default branch, check the id
//! against the agent catalog, then write `projects.yaml`.
//!
//! Its own module because `coordinate_handlers.rs` is over the file-size budget
//! (`packages/tddy-daemon-rpc/docs/code-issues/oversized-file-project-coordinate-handlers.md`).

use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::project::{SetProjectHomeAgentRequest, SetProjectHomeAgentResponse};

use super::ProjectRpcHandler;

impl ProjectRpcHandler {
    /// Set or clear `project_id`'s home agent on this host or the peer named by
    /// `daemon_instance_id`. An id `ListAgents` does not list is `invalid_argument`, and nothing is
    /// written.
    pub(crate) async fn set_project_home_agent_at_project_coordinate(
        &self,
        request: Request<SetProjectHomeAgentRequest>,
    ) -> Result<Response<SetProjectHomeAgentResponse>, Status> {
        // TODO(home-agent-setting): implement
        let _ = request;
        Err(Status::unimplemented(
            "SetProjectHomeAgent is not implemented",
        ))
    }
}
