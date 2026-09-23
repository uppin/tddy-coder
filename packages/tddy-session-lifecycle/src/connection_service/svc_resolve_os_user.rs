use super::cleanup_materialized_attachments;
use crate::config::DaemonConfig;
use tddy_daemon_kernel::SessionUserResolver;
use tddy_service::proto::session::session_attachment::Source as AttachmentSource;

use super::attachment_size_bytes;

use super::AttachmentProgressReporter;

use crate::{session_attachments::validate_attachment_basename, workspace_session};

use tddy_sandbox_runner::ExecuteToolResponse;
use tddy_service::proto::exec_tools::ExecuteToolRequest;
use tddy_service::proto::session::SessionAttachment;

use super::AttachmentMaterialization;

use livekit::prelude::Room;

use std::sync::Arc;

use tddy_core::session_lifecycle::unified_session_dir_path;

use std::path::Path;

use tddy_core::session_lifecycle::validate_session_id_segment;

use std::path::PathBuf;

use crate::livekit_peer_discovery::local_instance_id_for_config;

use crate::livekit_peer_discovery::PeerRoute;

use tddy_rpc::Status;

use super::DaemonSessionHost;

impl DaemonSessionHost {
    pub(crate) fn resolve_os_user(&self, session_token: &str) -> Result<String, Status> {
        resolve_os_user(&self.config, &self.user_resolver, session_token)
    }

    pub(crate) fn eligible_instance_ids(&self) -> Vec<String> {
        self.peer_routing.eligible_instance_ids()
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

    /// [`LocalExecTools::run_exec_tool_locally`](super::LocalExecTools::run_exec_tool_locally) over this host's task registry and jails.
    ///
    /// The single choke point for every exec tool this daemon serves out of its own sessions —
    /// `ExecuteTool`, `StreamExecuteTool`, and a roster agent's own loop
    /// ([`Self::local_agent_codebase_access`]).
    pub(crate) async fn run_exec_tool_locally(
        &self,
        req: &ExecuteToolRequest,
        sessions_base: &Path,
        worktree_root: &Path,
    ) -> ExecuteToolResponse {
        self.local_exec_tools()
            .run_exec_tool_locally(req, sessions_base, worktree_root)
            .await
    }

    /// [`PeerRouting::common_room_slot`](crate::peer_routing::PeerRouting::common_room_slot) of this host.
    pub(crate) fn common_room_slot(
        &self,
        rpc_name: &str,
    ) -> Result<&Arc<tokio::sync::RwLock<Option<Arc<Room>>>>, Status> {
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

    /// Pre-creates `session_dir` when needed and materializes the request's attachments before spawn.
    ///
    /// Answers with the attachments that reached the session's store, which is what a caller
    /// deriving anything from them — the pr-stack changeset the child's prompt names, say — must
    /// read: the request says what was asked for, this says what the child actually holds.
    pub(crate) async fn prepare_session_attachments(
        &self,
        ctx: &AttachmentMaterialization<'_>,
    ) -> Result<Vec<SessionAttachment>, Status> {
        if ctx.attachments.is_empty() {
            return Ok(Vec::new());
        }
        let session_dir = ctx.session_dir();
        std::fs::create_dir_all(&session_dir)
            .map_err(|e| Status::internal(format!("failed to create session dir: {e}")))?;
        self.materialize_session_attachments(ctx).await
    }

    pub(crate) async fn materialize_session_attachments(
        &self,
        ctx: &AttachmentMaterialization<'_>,
    ) -> Result<Vec<SessionAttachment>, Status> {
        if ctx.attachments.is_empty() {
            return Ok(Vec::new());
        }

        let session_dir = ctx.session_dir();
        let local_instance_id = local_instance_id_for_config(&self.config);
        let mut seen_basenames = std::collections::HashSet::new();
        for att in ctx.attachments {
            let safe = validate_attachment_basename(&att.basename)?;
            if !seen_basenames.insert(safe.to_string()) {
                return Err(Status::invalid_argument(
                    "duplicate attachment basename in request",
                ));
            }
        }

        let staging_root = crate::session_attachment_staging::staging_root_for(
            ctx.os_user,
            &self.staging_base_dir,
        );
        let mut written: Vec<SessionAttachment> = Vec::new();
        let attachment_count = ctx.attachments.len() as u32;

        for (index, att) in ctx.attachments.iter().enumerate() {
            let basename = validate_attachment_basename(&att.basename)?.to_string();
            let source = att
                .source
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("attachment source must be set"))?;
            let reporter = AttachmentProgressReporter {
                sink: ctx.progress,
                basename: &basename,
                attachment_index: index as u32,
                attachment_count,
            };

            let materialize_result = match source {
                AttachmentSource::Staged(staged) => {
                    self.materialize_staged_attachment(
                        ctx.session_token,
                        &staging_root,
                        &session_dir,
                        staged,
                        &basename,
                        &reporter,
                    )
                    .await
                }
                AttachmentSource::HostDocument(host_doc) => {
                    self.materialize_host_document_attachment(
                        ctx.session_token,
                        ctx.os_user,
                        &session_dir,
                        host_doc,
                        &basename,
                        &local_instance_id,
                    )
                    .await
                }
            };

            match materialize_result {
                Ok(()) => {
                    // The attachment is on disk now, so its final size is the honest byte count to
                    // report — and it is the only report a source that copies in one step makes.
                    let bytes = attachment_size_bytes(&session_dir, &basename);
                    reporter.report(bytes, bytes);
                    // Under the basename it was written as, not the one that was requested — the
                    // two differ whenever validation trimmed it.
                    written.push(SessionAttachment {
                        basename,
                        source: att.source.clone(),
                    });
                }
                Err(e) => {
                    cleanup_materialized_attachments(&session_dir, &written);
                    return Err(e);
                }
            }
        }

        Ok(written)
    }
}

/// Resolve a caller's session token to the OS user this daemon runs its work as.
///
/// Free rather than a method so a family handler above this crate (`tddy-daemon-rpc`) authenticates
/// a caller exactly as the session host does, from the two fields it reads, without holding the
/// host.
pub fn resolve_os_user(
    config: &DaemonConfig,
    user_resolver: &SessionUserResolver,
    session_token: &str,
) -> Result<String, Status> {
    let github_user = (user_resolver)(session_token)
        .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
    config
        .os_user_for_github(&github_user)
        .map(|s| s.to_string())
        .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))
}

/// Authenticate an exec-tool caller, and answer with the OS user its tools run as here.
///
/// Separate from [`resolve_exec_tool_worktree`] because it has to run **before** the hosted-clone
/// branch, which resolves no worktree of this daemon's at all and whose mutating half proxies to the
/// facilitating daemon under the *clone's* stored credential. Reached with no check of its own, that
/// branch would let any common-room participant that read a session id out of a `session.agents`
/// broadcast land an arbitrary write in another host's authoritative worktree.
///
/// Both refusals name **this** daemon. For a split session the tools are served on the codebase
/// host while the error is rendered in the agent's transcript on the agent host, where an
/// unattributed "invalid or expired session" reads as the agent host's own answer — and the two
/// likeliest split misconfigurations land here: a codebase host that has not learned the agent
/// host's signing key (a session token is verifiable only by a daemon that has seen its signer's
/// public key advertised in the common room), and a GitHub user mapped on the agent host but not
/// on the codebase host. Each is also logged here, because the operator debugging it is reading
/// *this* daemon's log.
pub fn authorize_exec_tool_caller<'c>(
    config: &'c DaemonConfig,
    user_resolver: &SessionUserResolver,
    req: &ExecuteToolRequest,
) -> Result<&'c str, Status> {
    let local_instance_id = local_instance_id_for_config(config);
    let Some(github_user) = (user_resolver)(&req.session_token) else {
        log::warn!(
            "exec tool {tool:?} for session {session} refused on daemon {local_instance_id}: the session token could not be verified here (a split session's agent presents a token its agent daemon signed with its own key, so this daemon must have seen that daemon's signing key advertised in the common room)",
            tool = req.tool_name,
            session = req.session_id
        );
        return Err(Status::unauthenticated(format!(
            "daemon {local_instance_id} could not verify the session token (invalid or expired there); a split session's tools run on the daemon holding the codebase, which verifies the token against the agent daemon's signing key as advertised in the common room"
        )));
    };
    let Some(os_user) = config.os_user_for_github(&github_user) else {
        log::warn!(
            "exec tool {tool:?} for session {session} refused on daemon {local_instance_id}: GitHub user {github_user} has no users[] entry here",
            tool = req.tool_name,
            session = req.session_id
        );
        return Err(Status::permission_denied(format!(
            "daemon {local_instance_id} has no OS user mapped for GitHub user {github_user}; add a users[] entry there — a split session's tools run as that user on the daemon holding the codebase"
        )));
    };
    Ok(os_user)
}

/// Resolve, on this daemon, the sessions base and the worktree an exec tool runs in — for a
/// caller [`authorize_exec_tool_caller`] has already accepted.
pub fn resolve_exec_tool_worktree(
    config: &DaemonConfig,
    user_resolver: &SessionUserResolver,
    tddy_data_dir: &Path,
    req: &ExecuteToolRequest,
) -> Result<(PathBuf, PathBuf), Status> {
    let os_user = authorize_exec_tool_caller(config, user_resolver, req)?;

    validate_session_id_segment(&req.session_id)
        .map_err(|e| Status::invalid_argument(e.message()))?;

    let sessions_base =
        crate::user_sessions_path::sessions_base_for_user(os_user, Some(tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
    let worktree_root =
        workspace_session::resolve_worktree_root_for_session(&sessions_base, &req.session_id)?;
    Ok((sessions_base, worktree_root))
}
