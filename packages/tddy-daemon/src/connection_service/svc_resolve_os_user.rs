use super::cleanup_materialized_attachments;
use crate::tool_engine;
use tddy_service::proto::connection::session_attachment::Source as AttachmentSource;

use super::attachment_size_bytes;

use super::AttachmentProgressReporter;

use crate::{
    connection_service::seeded_clone_guard, session_attachments::validate_attachment_basename,
    workspace_session,
};

use tddy_sandbox_runner::ExecuteToolResponse;
use tddy_service::proto::connection::{ExecuteToolRequest, SessionAttachment};

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

use super::ConnectionServiceImpl;

impl ConnectionServiceImpl {
    pub(crate) fn resolve_os_user(&self, session_token: &str) -> Result<String, Status> {
        let github_user = (self.user_resolver)(session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        self.config
            .os_user_for_github(&github_user)
            .map(|s| s.to_string())
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))
    }

    pub(crate) fn eligible_instance_ids(&self) -> Vec<String> {
        self.eligible_daemon_source
            .list_eligible_daemons()
            .into_iter()
            .map(|e| e.instance_id.0)
            .collect()
    }

    pub(crate) fn classify_daemon_route(
        &self,
        requested_daemon: &str,
    ) -> Result<PeerRoute, Status> {
        let local_id = local_instance_id_for_config(&self.config);
        crate::livekit_peer_discovery::classify_peer_route(
            &local_id,
            requested_daemon,
            &self.eligible_instance_ids(),
        )
        .map_err(|msg| {
            log::info!("daemon routing rejected: {msg}");
            Status::failed_precondition(msg)
        })
    }

    /// Route a session-scoped RPC by the daemon its `daemon_instance_id` addresses, before any
    /// session lookup — a relay holds no sessions of its own and must still be able to forward.
    ///
    /// An unaddressed request (empty id) is this daemon's to serve: that is the protocol's other
    /// spelling for "the daemon this call arrived on".
    ///
    /// Unlike [`Self::classify_daemon_route`], an unroutable id is `InvalidArgument`: the caller
    /// named a daemon that cannot serve the call, which is a bad request rather than a deployment
    /// that is not ready. Shared by every handler that routes this way — the exec tools, the
    /// worktree snapshot and the roster RPCs — so they cannot diverge over which daemon owns a
    /// session's files or its roster.
    pub(crate) fn classify_addressed_daemon_route(
        &self,
        rpc_name: &str,
        requested_daemon: &str,
    ) -> Result<PeerRoute, Status> {
        let requested_daemon = requested_daemon.trim();
        if requested_daemon.is_empty() {
            return Ok(PeerRoute::Local);
        }
        let route = crate::livekit_peer_discovery::classify_peer_route(
            &local_instance_id_for_config(&self.config),
            requested_daemon,
            &self.eligible_instance_ids(),
        )
        .map_err(|msg| {
            log::info!("{rpc_name}: rejected daemon routing: {msg}");
            Status::invalid_argument(msg)
        })?;
        if let PeerRoute::Forward { peer_instance_id } = &route {
            log::info!(
                "{rpc_name}: forwarding RPC to remote daemon_instance_id={peer_instance_id}"
            );
        }
        Ok(route)
    }

    /// Authenticate an exec-tool caller, and answer with the OS user its tools run as here.
    ///
    /// Separate from [`Self::resolve_exec_tool_worktree`] because it has to run **before** the
    /// hosted-clone branch, which resolves no worktree of this daemon's at all and whose mutating
    /// half proxies to the facilitating daemon under the *clone's* stored credential. Reached with
    /// no check of its own, that branch would let any common-room participant that read a session id
    /// out of a `session.agents` broadcast land an arbitrary write in another host's authoritative
    /// worktree.
    ///
    /// Both refusals name **this** daemon. For a split session the tools are served on the codebase
    /// host while the error is rendered in the agent's transcript on the agent host, where an
    /// unattributed "invalid or expired session" reads as the agent host's own answer — and the two
    /// likeliest split misconfigurations land here: daemons not sharing `livekit.api_secret` (a
    /// session token is a stateless HMAC, verifiable only by daemons holding the same secret), and a
    /// GitHub user mapped on the agent host but not on the codebase host. Each is also logged here,
    /// because the operator debugging it is reading *this* daemon's log.
    pub(crate) fn authorize_exec_tool_caller(
        &self,
        req: &ExecuteToolRequest,
    ) -> Result<&str, Status> {
        let local_instance_id = local_instance_id_for_config(&self.config);
        let Some(github_user) = (self.user_resolver)(&req.session_token) else {
            log::warn!(
                "exec tool {tool:?} for session {session} refused on daemon {local_instance_id}: the session token could not be verified here (a split session's agent presents a token minted by its agent daemon, so both daemons must share livekit.api_secret)",
                tool = req.tool_name,
                session = req.session_id
            );
            return Err(Status::unauthenticated(format!(
                "daemon {local_instance_id} could not verify the session token (invalid or expired there); a split session's tools run on the daemon holding the codebase, which verifies the token with its own livekit.api_secret"
            )));
        };
        let Some(os_user) = self.config.os_user_for_github(&github_user) else {
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
    /// caller [`Self::authorize_exec_tool_caller`] has already accepted.
    pub(crate) fn resolve_exec_tool_worktree(
        &self,
        req: &ExecuteToolRequest,
    ) -> Result<(PathBuf, PathBuf), Status> {
        let os_user = self.authorize_exec_tool_caller(req)?;

        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;

        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let worktree_root =
            workspace_session::resolve_worktree_root_for_session(&sessions_base, &req.session_id)?;
        Ok((sessions_base, worktree_root))
    }

    /// The context allow-list a session is served — **its own row, not the one the request named**.
    ///
    /// The three context RPCs all carry an `agent` field, and it is advisory. Authorization on this
    /// path is per OS user rather than per session
    /// ([`Self::authorize_exec_tool_caller`]), so a caller holding a valid token for one of its
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

    /// Where `session_id`'s tools run: its own jail, or the checkout on this host.
    ///
    /// Read from what this daemon persisted about the session rather than from the request, because
    /// the request is the caller's claim and the metadata is the session's. A `workspace` session
    /// that recorded `sandbox: true` is served by the jail registered for it and by nothing else.
    pub(crate) async fn exec_tool_route(
        &self,
        session_dir: &Path,
        session_id: &str,
    ) -> seeded_clone_guard::ExecToolRoute {
        let meta = match tddy_core::read_session_metadata(session_dir) {
            Ok(meta) => meta,
            // The callers all resolved this session's worktree out of this same file moments ago, so
            // an unreadable one here is a transient failure rather than a session that is not
            // sandboxed — and "assume unconfined" is the wrong guess to make about a jail.
            Err(e) => {
                return seeded_clone_guard::ExecToolRoute::Refused(format!(
                    "session {session_id}: cannot tell whether this session is sandboxed \
                     (.session.yaml unreadable: {e}); refusing to run its tools on the host"
                ))
            }
        };
        let sandboxed_workspace =
            meta.session_type.as_deref() == Some("workspace") && meta.sandbox == Some(true);
        if !sandboxed_workspace {
            return seeded_clone_guard::ExecToolRoute::HostWorktree;
        }
        match self.workspace_sandboxes.get(session_id).await {
            Some(jail) => seeded_clone_guard::ExecToolRoute::Jail(jail),
            None => seeded_clone_guard::ExecToolRoute::Refused(format!(
                "session {session_id} is sandboxed and this daemon holds no jail for it; \
                 refusing to run its tools on the host worktree"
            )),
        }
    }

    /// Run one tool call for `req`'s session and durably record it.
    ///
    /// A tool failure is carried in the returned response, never raised as an RPC error: only
    /// routing and auth failures are RPC errors, so an agent can tell "the tool said no" from "the
    /// call never reached the tool".
    ///
    /// The single choke point for every exec tool this daemon serves out of its own sessions —
    /// `ExecuteTool`, `StreamExecuteTool`, and a roster agent's own loop
    /// ([`Self::local_agent_codebase_access`]) — which is why a sandboxed workspace session is
    /// routed to its jail here rather than three times over. `worktree_root` is where the tool runs
    /// when it runs on this host; inside the jail the same checkout is mounted at that very path.
    pub(crate) async fn run_exec_tool_locally(
        &self,
        req: &ExecuteToolRequest,
        sessions_base: &Path,
        worktree_root: &Path,
    ) -> ExecuteToolResponse {
        let session_dir = unified_session_dir_path(sessions_base, &req.session_id);
        let response = match self.exec_tool_route(&session_dir, &req.session_id).await {
            seeded_clone_guard::ExecToolRoute::HostWorktree => {
                let outcome = tool_engine::execute_tool(
                    worktree_root,
                    &req.tool_name,
                    &req.args_json,
                    &self.task_registry,
                    &req.session_id,
                )
                .await;
                ExecuteToolResponse {
                    result_json: outcome.result_json,
                    is_error: outcome.is_error,
                    error_message: outcome.error_message,
                    job_id: outcome.job_id,
                    job_running: outcome.job_running,
                }
            }
            seeded_clone_guard::ExecToolRoute::Jail(jail) => jail.execute_tool(req).await,
            seeded_clone_guard::ExecToolRoute::Refused(reason) => {
                log::warn!("exec tool: {reason}");
                ExecuteToolResponse {
                    result_json: String::new(),
                    is_error: true,
                    error_message: reason,
                    job_id: String::new(),
                    job_running: false,
                }
            }
        };

        // Durably record the tool call (non-fatal on failure). One log for both routes: which side
        // of the jail boundary a call ran on does not change that it is the session's tool call.
        let record = crate::tool_call_log::ToolCallRecord {
            task_id: response.job_id.clone(),
            tool_name: req.tool_name.clone(),
            args_json: req.args_json.clone(),
            result_json: response.result_json.clone(),
            is_error: response.is_error,
            error_message: response.error_message.clone(),
            job_running: response.job_running,
            created_unix_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        };
        if let Err(e) = crate::tool_call_log::append_tool_call(&session_dir, &record) {
            log::warn!(
                "tool_call_log: failed to persist tool call for session {}: {}",
                req.session_id,
                e
            );
        }

        response
    }

    pub(crate) fn common_room_slot(
        &self,
        rpc_name: &str,
    ) -> Result<&Arc<tokio::sync::RwLock<Option<Arc<Room>>>>, Status> {
        self.common_room_livekit_room.as_ref().ok_or_else(|| {
            Status::failed_precondition(format!(
                "cannot forward {rpc_name}: this process has no LiveKit common-room connection (configure livekit.common_room with url, api_key, api_secret)"
            ))
        })
    }

    /// Serve a unary RPC on the daemon its `daemon_instance_id` names, when that is not this one.
    ///
    /// `Ok(None)` means the call is this daemon's own to serve — an empty id, or this daemon's id.
    /// `rpc_name` is the proto method name, so a forwarded call lands on the same handler there.
    ///
    /// Called **before** the session is looked up, and before the caller is authenticated: a relay
    /// holds neither the session nor, necessarily, an answer about its caller, and the daemon that
    /// serves the call checks the token itself. A split session's roster and files live on the
    /// daemon holding the codebase while the agent's tools address the daemon running its loop, so
    /// resolved out of this daemon's own sessions the session does not exist at all — the in-jail
    /// roster stays empty, every subagent call is refused, and the main agent is told it has no such
    /// tool (PRD AC12, AC28).
    pub(crate) async fn rpc_served_by_peer<Req, Resp>(
        &self,
        rpc_name: &str,
        requested_daemon: &str,
        req: &Req,
    ) -> Result<Option<Resp>, Status>
    where
        Req: prost::Message,
        Resp: prost::Message + Default,
    {
        let PeerRoute::Forward { peer_instance_id } =
            self.classify_addressed_daemon_route(rpc_name, requested_daemon)?
        else {
            return Ok(None);
        };
        let slot = self.common_room_slot(rpc_name)?;
        let answered = crate::livekit_peer_discovery::forward_to_peer(
            slot,
            &peer_instance_id,
            "connection.ConnectionService",
            rpc_name,
            req.encode_to_vec(),
        )
        .await?;
        Resp::decode(answered.as_slice())
            .map(Some)
            .map_err(|e| Status::internal(format!("decode {rpc_name} response from peer: {e}")))
    }

    /// [`Self::rpc_served_by_peer`] for a **server-streaming** RPC: the peer's frames are relayed
    /// one by one, and a stream that stops without its end-of-stream marker terminates as an error
    /// rather than as a short roster the caller would take for the whole one.
    pub(crate) async fn stream_served_by_peer<Req, Frame>(
        &self,
        rpc_name: &str,
        requested_daemon: &str,
        req: &Req,
    ) -> Result<Option<tokio::sync::mpsc::UnboundedReceiver<Result<Frame, Status>>>, Status>
    where
        Req: prost::Message,
        Frame: prost::Message + Default + Send + 'static,
    {
        let PeerRoute::Forward { peer_instance_id } =
            self.classify_addressed_daemon_route(rpc_name, requested_daemon)?
        else {
            return Ok(None);
        };
        let slot = self.common_room_slot(rpc_name)?;
        let decoding = rpc_name.to_string();
        crate::livekit_peer_discovery::forward_server_stream_to_peer(
            slot,
            &peer_instance_id,
            "connection.ConnectionService",
            rpc_name,
            req.encode_to_vec(),
            move |bytes| {
                Frame::decode(bytes.as_slice()).map_err(|e| {
                    Status::internal(format!("decode {decoding} frame from peer: {e}"))
                })
            },
        )
        .await
        .map(Some)
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
