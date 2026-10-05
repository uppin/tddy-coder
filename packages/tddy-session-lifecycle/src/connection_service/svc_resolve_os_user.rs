use tddy_service::proto::exec_tools::ExecuteToolRequest;

use tddy_core::session_lifecycle::unified_session_dir_path;

use std::path::Path;

use std::path::PathBuf;

use crate::livekit_peer_discovery::PeerRoute;

use tddy_rpc::Status;

use super::DaemonSessionHost;

impl DaemonSessionHost {
    pub(crate) fn resolve_os_user(&self, session_token: &str) -> Result<String, Status> {
        os_user_resolution::resolve_os_user(&self.config, &self.user_resolver, session_token)
    }

    /// [`PeerRouting::classify_daemon_route`](crate::peer_routing::PeerRouting::classify_daemon_route) against this host's roster.
    pub(crate) fn classify_daemon_route(
        &self,
        requested_daemon: &str,
    ) -> Result<PeerRoute, Status> {
        self.peer_routing.classify_daemon_route(requested_daemon)
    }

    /// [`PeerRouting::classify_addressed_daemon_route`](crate::peer_routing::PeerRouting::classify_addressed_daemon_route) against this host's roster.
    pub(crate) fn classify_addressed_daemon_route(
        &self,
        rpc_name: &str,
        requested_daemon: &str,
    ) -> Result<PeerRoute, Status> {
        self.peer_routing
            .classify_addressed_daemon_route(rpc_name, requested_daemon)
    }

    /// [`resolve_exec_tool_worktree`] over this host's configuration, token resolver and data root.
    pub(crate) fn resolve_exec_tool_worktree(
        &self,
        req: &ExecuteToolRequest,
    ) -> Result<(PathBuf, PathBuf), Status> {
        resolve_exec_tool_worktree(&self.config, &self.user_resolver, &self.tddy_data_dir, req)
    }

    /// The context allow-list a session is served — **its own row, not the one the request named**.
    ///
    /// The three context RPCs all carry an `agent` field, and it is advisory. Authorization on this
    /// path is per OS user rather than per session
    /// ([`authorize_exec_tool_caller`]), so a caller holding a valid token for one of its
    /// sessions can name any other session of the same user — and if the field decided the row, it
    /// could also name any row. A `codex` session asking as `cursor` would be served that
    /// checkout's `.claude/**`, `.cursor/**` and `.mcp.json`: the files that routinely carry API
    /// tokens in MCP `env` blocks, and exactly the gitignored ones the git-listing gate this reader
    /// replaces used to refuse. Trusting the field makes the enforced bound the union of every
    /// table row instead of the session's own.
    ///
    /// So the row comes from what this daemon persisted about the session
    /// ([`crate::context_files::context_agent_for_session`]), which it has already read to resolve
    /// the worktree. A disagreement is logged at `debug` and the session's row wins: the field is
    /// still worth carrying, because "the agent host believed it was reading Cursor's list" is the
    /// first thing anyone debugging a missing `.cursor/` will want in the log.
    pub(crate) fn context_globs_for_session(
        &self,
        rpc_name: &str,
        sessions_base: &Path,
        session_id: &str,
        requested_agent: &str,
    ) -> Result<&'static [&'static str], Status> {
        let session_dir = unified_session_dir_path(sessions_base, session_id);
        let meta = tddy_core::read_session_metadata(&session_dir).map_err(|e| {
            log::warn!("{rpc_name}: session {session_id} has no readable .session.yaml: {e}");
            Status::failed_precondition("session not found or .session.yaml missing")
        })?;
        let agent = crate::context_files::context_agent_for_session(&meta);
        if agent != requested_agent.trim() {
            log::debug!(
                "{rpc_name}: session {session_id} was asked for the {requested_agent:?} context \
                 allow-list; serving {agent:?}, the row its own persisted session_type names"
            );
        }
        Ok(tddy_core::backend::context_globs_for_agent(agent))
    }

    /// [`PeerRouting::common_room_slot`](crate::peer_routing::PeerRouting::common_room_slot) of this host.
    pub(crate) fn common_room_slot(
        &self,
        rpc_name: &str,
    ) -> Result<&crate::livekit_peer_discovery::CommonRoom, Status> {
        self.peer_routing.common_room_slot(rpc_name)
    }

    /// [`PeerRouting::rpc_served_by_peer`](crate::peer_routing::PeerRouting::rpc_served_by_peer) against this host's roster.
    pub(crate) async fn rpc_served_by_peer<Req, Resp>(
        &self,
        service: &'static str,
        rpc_name: &str,
        requested_daemon: &str,
        req: &Req,
    ) -> Result<Option<Resp>, Status>
    where
        Req: prost::Message,
        Resp: prost::Message + Default,
    {
        self.peer_routing
            .rpc_served_by_peer(service, rpc_name, requested_daemon, req)
            .await
    }

    /// [`PeerRouting::stream_served_by_peer`](crate::peer_routing::PeerRouting::stream_served_by_peer) against this host's roster.
    pub(crate) async fn stream_served_by_peer<Req, Frame>(
        &self,
        service: &'static str,
        rpc_name: &str,
        requested_daemon: &str,
        req: &Req,
    ) -> Result<Option<tokio::sync::mpsc::UnboundedReceiver<Result<Frame, Status>>>, Status>
    where
        Req: prost::Message,
        Frame: prost::Message + Default + Send + 'static,
    {
        self.peer_routing
            .stream_served_by_peer(service, rpc_name, requested_daemon, req)
            .await
    }
}

mod os_user_resolution;
pub use os_user_resolution::*;

pub use tddy_session_agents::exec_tool_caller::authorize_exec_tool_caller;

pub use crate::connection_service::peer_session_answer::resolve_exec_tool_worktree;
