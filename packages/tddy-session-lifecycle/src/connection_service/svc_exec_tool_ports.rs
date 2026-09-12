//! Family L exec-tool RPCs — host side of [`tddy_tool_engine::exec_tool_service::ExecToolHandler`].

use super::family_proto_bridge::wire_same;
use super::{exec_tool_result_frames, reject_exec_tool_path_traversal, DaemonSessionHost};
use crate::livekit_peer_discovery::{local_instance_id_for_config, PeerRoute};
use crate::tool_engine;
use async_trait::async_trait;
use prost::Message as _;
use tddy_core::session_lifecycle::{unified_session_dir_path, validate_session_id_segment};
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::exec_tools::{ExecuteToolChunk as ConnExecuteToolChunk, ExecuteToolRequest as ConnExecuteToolRequest, ExecuteToolResponse as ConnExecuteToolResponse, ListExecToolsResponse as ConnListExecToolsResponse, ListSessionToolCallsResponse as ConnListSessionToolCallsResponse};
use tddy_service::proto::exec_tools::{
    ExecuteToolChunk, ExecuteToolRequest, ExecuteToolResponse, ListExecToolsRequest,
    ListExecToolsResponse, ListSessionToolCallsRequest, ListSessionToolCallsResponse,
    ToolCallInfo as ExecToolCallInfo, ToolDef as ExecToolDef,
};
use tddy_tool_engine::EXEC_TOOL_SERVICE;
use tddy_worktree_service::stream::MpscResultStream;

#[async_trait]
impl tddy_tool_engine::exec_tool_service::ExecToolHandler for DaemonSessionHost {
    async fn execute_tool(
        &self,
        request: Request<ExecuteToolRequest>,
    ) -> Result<Response<ExecuteToolResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();
        let req_conn = wire_same::<ExecuteToolRequest, ConnExecuteToolRequest>(&req)?;

        // Route BEFORE session lookup so a relay (which has no local sessions) can forward.
        if let Some(answered) = self
            .rpc_served_by_peer(
                EXEC_TOOL_SERVICE,
                "ExecuteTool",
                &req.daemon_instance_id,
                &req,
            )
            .await?
        {
            return Ok(Response::new(answered));
        }

        // Auth before *any* worktree is chosen, because the hosted-clone branch below chooses one
        // that is not this daemon's and proxies its mutations under the clone's own credential.
        self.authorize_exec_tool_caller(&req_conn)?;

        // A session this daemon holds an *agent clone* for lives on another daemon, so the ordinary
        // "resolve the worktree from my own sessions base" would find nothing. Checked before that
        // resolution rather than after it, so the read/write split is what answers rather than a
        // not-found for a session that legitimately is not here.
        if let Some(clone) = self.hosted_clone_for(&req.session_id) {
            reject_exec_tool_path_traversal(&req.tool_name, &req.args_json)?;
            let answered = self.run_hosted_clone_tool(&req_conn, &clone).await;
            return Ok(Response::new(wire_same::<
                ConnExecuteToolResponse,
                ExecuteToolResponse,
            >(&answered)?));
        }

        let (sessions_base, worktree_root) = self.resolve_exec_tool_worktree(&req_conn)?;
        reject_exec_tool_path_traversal(&req.tool_name, &req.args_json)?;
        let response = self
            .run_exec_tool_locally(&req_conn, &sessions_base, &worktree_root)
            .await;
        Ok(Response::new(wire_same::<
            ConnExecuteToolResponse,
            ExecuteToolResponse,
        >(&response)?))
    }

    /// Server-streaming sibling of [`Self::execute_tool`], carrying the same result in bounded
    /// frames.
    ///
    /// The unary call returns `result_json` as one string; over LiveKit anything past
    /// `MAX_CHUNK_FRAME_BYTES` is chunk-framed, and one lost chunk frame wedges the call with no
    /// error at all (`docs/ft/coder/rpc-multi-transport.md`). A `Read` of a large file crosses that
    /// on day one of a split session, so the split path streams instead — routing, auth and worktree
    /// resolution are shared with the unary handler so the two cannot drift.
    async fn stream_execute_tool(
        &self,
        request: Request<ExecuteToolRequest>,
    ) -> Result<Response<MpscResultStream<ExecuteToolChunk>>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();
        let req_conn = wire_same::<ExecuteToolRequest, ConnExecuteToolRequest>(&req)?;

        if let PeerRoute::Forward { peer_instance_id } =
            self.classify_addressed_daemon_route("StreamExecuteTool", &req.daemon_instance_id)?
        {
            log::info!(
                "StreamExecuteTool: forwarding stream to remote daemon_instance_id={peer_instance_id}"
            );
            let slot = self.common_room_slot("StreamExecuteTool")?;
            // A forwarded stream that stalls terminates as an *error*, so a truncated tool result
            // can never reach the caller looking complete.
            let mut conn_rx =
                tddy_daemon_livekit::livekit_peer_discovery::forward_stream_execute_tool_via_livekit(
                    slot,
                    &peer_instance_id,
                    &req_conn,
                )
                .await?;
            let (tx, rx) =
                tokio::sync::mpsc::unbounded_channel::<Result<ExecuteToolChunk, Status>>();
            tokio::spawn(async move {
                while let Some(frame) = conn_rx.recv().await {
                    let out = frame.and_then(|chunk| {
                        wire_same::<ConnExecuteToolChunk, ExecuteToolChunk>(&chunk)
                    });
                    if tx.send(out).is_err() {
                        break;
                    }
                }
            });
            return Ok(Response::new(MpscResultStream::from(rx)));
        }

        // See the unary handler: auth first, because the hosted-clone branch resolves no worktree of
        // this daemon's and would otherwise be reachable with no credential at all.
        self.authorize_exec_tool_caller(&req_conn)?;

        // A session this daemon holds an agent clone for is served by the read/write split, from a
        // checkout that is not in this daemon's own sessions base.
        let response = match self.hosted_clone_for(&req.session_id) {
            Some(clone) => {
                reject_exec_tool_path_traversal(&req.tool_name, &req.args_json)?;
                self.run_hosted_clone_tool(&req_conn, &clone).await
            }
            None => {
                let (sessions_base, worktree_root) = self.resolve_exec_tool_worktree(&req_conn)?;
                reject_exec_tool_path_traversal(&req.tool_name, &req.args_json)?;
                self.run_exec_tool_locally(&req_conn, &sessions_base, &worktree_root)
                    .await
            }
        };

        // The result is already complete in memory, so every frame can be queued now: the stream
        // exists to bound each frame's size, not to interleave with the tool's execution.
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<ExecuteToolChunk, Status>>();
        for frame in exec_tool_result_frames(response) {
            if tx
                .send(Ok(wire_same::<ConnExecuteToolChunk, ExecuteToolChunk>(
                    &frame,
                )?))
                .is_err()
            {
                break;
            }
        }
        Ok(Response::new(MpscResultStream::from(rx)))
    }

    async fn list_exec_tools(
        &self,
        request: Request<ListExecToolsRequest>,
    ) -> Result<Response<ListExecToolsResponse>, Status> {
        let req = request.into_inner();

        // Route BEFORE auth so a relay (which has no local user table) can forward.
        let requested_daemon = req.daemon_instance_id.trim();
        if !requested_daemon.is_empty() {
            let local_id = local_instance_id_for_config(&self.config);
            let eligible_rows = self.eligible_daemon_source.list_eligible_daemons();
            let eligible_ids: Vec<String> = eligible_rows
                .iter()
                .map(|e| e.instance_id.0.clone())
                .collect();
            match crate::livekit_peer_discovery::classify_peer_route(
                &local_id,
                requested_daemon,
                &eligible_ids,
            ) {
                Err(msg) => {
                    log::info!("ListExecTools: rejected daemon routing: {}", msg);
                    return Err(Status::invalid_argument(msg));
                }
                Ok(crate::livekit_peer_discovery::PeerRoute::Forward { peer_instance_id }) => {
                    log::info!(
                        "ListExecTools: forwarding RPC to remote daemon_instance_id={}",
                        peer_instance_id
                    );
                    let slot = self.common_room_livekit_room.as_ref().ok_or_else(|| {
                        Status::failed_precondition(
                            "cannot forward ListExecTools: this process has no LiveKit common-room connection",
                        )
                    })?;
                    let body = req.encode_to_vec();
                    let out = crate::livekit_peer_discovery::forward_to_peer(
                        slot,
                        &peer_instance_id,
                        "exec_tools.ExecToolService",
                        "ListExecTools",
                        body,
                    )
                    .await?;
                    let inner = ConnListExecToolsResponse::decode(out.as_slice()).map_err(|e| {
                        Status::internal(format!("decode ListExecToolsResponse: {e}"))
                    })?;
                    return Ok(Response::new(wire_same(&inner)?));
                }
                Ok(crate::livekit_peer_discovery::PeerRoute::Local) => {
                    // Fall through to local execution below.
                }
            }
        }

        // Minimal auth — verify caller is a known user.
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let _os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        Ok(Response::new(ListExecToolsResponse {
            tools: tool_engine::tool_catalog()
                .into_iter()
                .map(|t| ExecToolDef {
                    name: t.name,
                    description: t.description,
                    input_schema_json: t.input_schema_json,
                })
                .collect(),
        }))
    }

    async fn list_session_tool_calls(
        &self,
        request: Request<ListSessionToolCallsRequest>,
    ) -> Result<Response<ListSessionToolCallsResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        // Route BEFORE session lookup so a relay can forward.
        let requested_daemon = req.daemon_instance_id.trim();
        if !requested_daemon.is_empty() {
            let local_id = local_instance_id_for_config(&self.config);
            let eligible_rows = self.eligible_daemon_source.list_eligible_daemons();
            let eligible_ids: Vec<String> = eligible_rows
                .iter()
                .map(|e| e.instance_id.0.clone())
                .collect();
            match crate::livekit_peer_discovery::classify_peer_route(
                &local_id,
                requested_daemon,
                &eligible_ids,
            ) {
                Err(msg) => {
                    log::info!("ListSessionToolCalls: rejected daemon routing: {}", msg);
                    return Err(Status::invalid_argument(msg));
                }
                Ok(crate::livekit_peer_discovery::PeerRoute::Forward { peer_instance_id }) => {
                    log::info!(
                        "ListSessionToolCalls: forwarding RPC to remote daemon_instance_id={}",
                        peer_instance_id
                    );
                    let slot = self.common_room_livekit_room.as_ref().ok_or_else(|| {
                        Status::failed_precondition(
                            "cannot forward ListSessionToolCalls: this process has no LiveKit common-room connection",
                        )
                    })?;
                    let body = req.encode_to_vec();
                    let out = crate::livekit_peer_discovery::forward_to_peer(
                        slot,
                        &peer_instance_id,
                        "exec_tools.ExecToolService",
                        "ListSessionToolCalls",
                        body,
                    )
                    .await?;
                    let inner =
                        ConnListSessionToolCallsResponse::decode(out.as_slice()).map_err(|e| {
                            Status::internal(format!("decode ListSessionToolCallsResponse: {e}"))
                        })?;
                    return Ok(Response::new(wire_same(&inner)?));
                }
                Ok(crate::livekit_peer_discovery::PeerRoute::Local) => {
                    // Fall through to local execution below.
                }
            }
        }

        // Authenticate caller.
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        // Validate session ID segment to prevent path traversal.
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;

        // Resolve the sessions base path.
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;

        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);

        let records =
            tddy_tool_engine::tool_call_log::read_tool_calls(&session_dir).unwrap_or_default();

        let tool_calls: Vec<ExecToolCallInfo> = records
            .into_iter()
            .map(|r| ExecToolCallInfo {
                task_id: r.task_id,
                tool_name: r.tool_name,
                args_json: r.args_json,
                result_json: r.result_json,
                is_error: r.is_error,
                error_message: r.error_message,
                job_running: r.job_running,
                created_unix_ms: r.created_unix_ms,
            })
            .collect();

        Ok(Response::new(ListSessionToolCallsResponse { tool_calls }))
    }
}
