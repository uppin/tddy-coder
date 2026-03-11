//! DaemonService — TddyRemote implementation for headless daemon mode.
//!
//! Implements GetSession, ListSessions, and Stream (StartSession flow).

use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;

use tokio::sync::mpsc as tokio_mpsc;
use tonic::{Request, Response, Status};

use tddy_core::output::{create_session_dir_under, parse_planning_response, PlanningOutput};
use tddy_core::workflow::graph::{ElicitationEvent, ExecutionStatus};
use tddy_core::workflow::tdd_hooks::TddWorkflowHooks;
use tddy_core::{create_worktree, SharedBackend, WorkflowEngine};
use tddy_core::{read_changeset, write_changeset};

use crate::convert::workflow_event_to_server_message;
use crate::gen::{
    client_message, server_message, tddy_remote_server::TddyRemote, GetSessionRequest,
    GetSessionResponse, ListSessionsRequest, ListSessionsResponse, ServerMessage, SessionCreated,
    SessionInfo, WorktreeElicitation,
};

/// Daemon gRPC service. Reads session state from disk; runs workflow on Stream.
pub struct DaemonService {
    sessions_base: PathBuf,
    backend: SharedBackend,
}

impl DaemonService {
    pub fn new(sessions_base: PathBuf, backend: SharedBackend) -> Self {
        Self {
            sessions_base,
            backend,
        }
    }

    /// Derive session status from changeset state.
    fn status_from_state(state: &str) -> &'static str {
        match state {
            "Init" | "Planned" | "AcceptanceTestsReady" | "RedTestsReady" => "Active",
            "GreenComplete"
            | "DemoComplete"
            | "Evaluated"
            | "ValidateComplete"
            | "ValidateRefactorComplete"
            | "RefactorComplete"
            | "DocsUpdated" => "Completed",
            "Failed" => "Failed",
            _ => "Active",
        }
    }
}

#[tonic::async_trait]
impl TddyRemote for DaemonService {
    type StreamStream = tokio_stream::wrappers::ReceiverStream<Result<ServerMessage, Status>>;

    async fn stream(
        &self,
        request: Request<tonic::codec::Streaming<crate::gen::ClientMessage>>,
    ) -> Result<Response<Self::StreamStream>, Status> {
        let (tx, rx) = tokio_mpsc::channel(64);
        let sessions_base = self.sessions_base.clone();
        let backend = self.backend.clone();

        let mut client_stream = request.into_inner();

        tokio::spawn(async move {
            let mut state = DaemonStreamState::Idle;
            let mut session_id: Option<String> = None;
            let mut plan_dir: Option<PathBuf> = None;
            let mut repo_root: Option<PathBuf> = None;
            let mut engine: Option<WorkflowEngine> = None;
            let mut event_rx: Option<mpsc::Receiver<tddy_core::WorkflowEvent>> = None;

            loop {
                match state {
                    DaemonStreamState::Idle => {
                        let msg = match client_stream.message().await {
                            Ok(Some(m)) => m,
                            Ok(None) => break,
                            Err(e) => {
                                let _ = tx
                                    .send(Err(Status::internal(format!("stream error: {}", e))))
                                    .await;
                                break;
                            }
                        };

                        if let Some(client_message::Intent::StartSession(start)) = msg.intent {
                            let prompt = start.prompt;
                            let repo = if start.repo_root.is_empty() {
                                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
                            } else {
                                PathBuf::from(start.repo_root)
                            };

                            std::fs::create_dir_all(&sessions_base)
                                .map_err(|e| {
                                    Status::internal(format!("create sessions base: {}", e))
                                })
                                .ok();

                            let sid = uuid::Uuid::new_v4().to_string();
                            let plan = create_session_dir_under(&sessions_base, &sid)
                                .map_err(|e| Status::internal(format!("create session dir: {}", e)))
                                .unwrap();

                            session_id = Some(sid.clone());
                            plan_dir = Some(plan.clone());
                            repo_root = Some(repo.clone());

                            let storage_dir = std::env::temp_dir().join("tddy-daemon-session");
                            std::fs::create_dir_all(&storage_dir).ok();

                            let (event_tx, rx) = mpsc::channel();
                            event_rx = Some(rx);
                            let hooks = Arc::new(TddWorkflowHooks::with_event_tx(event_tx));
                            let eng =
                                WorkflowEngine::new(backend.clone(), storage_dir, Some(hooks));
                            engine = Some(eng);

                            let mut ctx = std::collections::HashMap::new();
                            ctx.insert("feature_input".to_string(), serde_json::json!(prompt));
                            ctx.insert(
                                "output_dir".to_string(),
                                serde_json::to_value(repo.clone()).unwrap(),
                            );
                            ctx.insert(
                                "session_base".to_string(),
                                serde_json::to_value(sessions_base.clone()).unwrap(),
                            );
                            ctx.insert("session_id".to_string(), serde_json::json!(sid.clone()));
                            ctx.insert(
                                "plan_dir".to_string(),
                                serde_json::to_value(plan.clone()).unwrap(),
                            );

                            let _ = tx
                                .send(Ok(ServerMessage {
                                    event: Some(server_message::Event::SessionCreated(
                                        SessionCreated {
                                            session_id: sid.clone(),
                                        },
                                    )),
                                }))
                                .await;

                            let rt = tokio::runtime::Handle::current();
                            let result = rt
                                .block_on(engine.as_ref().unwrap().run_goal("plan", ctx))
                                .map_err(|e| Status::internal(format!("run_goal: {}", e)))
                                .unwrap();

                            match result.status {
                                ExecutionStatus::ElicitationNeeded {
                                    event: ElicitationEvent::PlanApproval { prd_content },
                                } => {
                                    let _ = tx
                                        .send(Ok(crate::convert::plan_approval_to_server_message(
                                            prd_content,
                                        )))
                                        .await;
                                    state = DaemonStreamState::WaitingApprovePlan;
                                }
                                ExecutionStatus::Completed => {
                                    state = DaemonStreamState::PlanComplete;
                                }
                                ExecutionStatus::Error(msg) => {
                                    let _ = tx
                                        .send(Ok(ServerMessage {
                                            event: Some(server_message::Event::WorkflowComplete(
                                                crate::gen::WorkflowComplete {
                                                    ok: false,
                                                    message: msg,
                                                },
                                            )),
                                        }))
                                        .await;
                                    break;
                                }
                                _ => {
                                    state = DaemonStreamState::PlanComplete;
                                }
                            }

                            while let Ok(ev) = event_rx.as_ref().unwrap().try_recv() {
                                if let Some(msg) = workflow_event_to_server_message(ev) {
                                    let _ = tx.send(Ok(msg)).await;
                                }
                            }
                        }
                    }
                    DaemonStreamState::WaitingApprovePlan => {
                        let msg = match client_stream.message().await {
                            Ok(Some(m)) => m,
                            Ok(None) => break,
                            Err(e) => {
                                let _ = tx
                                    .send(Err(Status::internal(format!("stream error: {}", e))))
                                    .await;
                                break;
                            }
                        };

                        if let Some(client_message::Intent::ApprovePlan(_)) = msg.intent {
                            let plan_dir_path = plan_dir.as_ref().unwrap();
                            let rt = tokio::runtime::Handle::current();
                            let planning: PlanningOutput = engine
                                .as_ref()
                                .and_then(|e| {
                                    rt.block_on(e.get_session(session_id.as_ref().unwrap()))
                                        .ok()
                                        .flatten()
                                })
                                .and_then(|s| s.context.get_sync::<String>("output"))
                                .and_then(|o| parse_planning_response(&o).ok())
                                .unwrap_or_else(|| {
                                    let prd = std::fs::read_to_string(plan_dir_path.join("PRD.md"))
                                        .unwrap_or_default();
                                    PlanningOutput {
                                        prd,
                                        todo: String::new(),
                                        name: None,
                                        discovery: None,
                                        demo_plan: None,
                                        branch_suggestion: None,
                                        worktree_suggestion: None,
                                    }
                                });

                            let suggested_branch = planning
                                .branch_suggestion
                                .clone()
                                .unwrap_or_else(|| "feature/impl".to_string());
                            let suggested_worktree = planning
                                .worktree_suggestion
                                .clone()
                                .or_else(|| planning.name.clone())
                                .unwrap_or_else(|| "feature-impl".to_string());

                            let _ = tx
                                .send(Ok(ServerMessage {
                                    event: Some(server_message::Event::WorktreeElicitation(
                                        WorktreeElicitation {
                                            suggested_branch: suggested_branch.clone(),
                                            suggested_worktree: suggested_worktree.clone(),
                                        },
                                    )),
                                }))
                                .await;
                            state = DaemonStreamState::WaitingConfirmWorktree {
                                suggested_branch,
                                suggested_worktree,
                            };
                        }
                    }
                    DaemonStreamState::WaitingConfirmWorktree {
                        ref suggested_branch,
                        ref suggested_worktree,
                    } => {
                        let msg = match client_stream.message().await {
                            Ok(Some(m)) => m,
                            Ok(None) => break,
                            Err(e) => {
                                let _ = tx
                                    .send(Err(Status::internal(format!("stream error: {}", e))))
                                    .await;
                                break;
                            }
                        };

                        if let Some(client_message::Intent::ConfirmWorktree(confirm)) = msg.intent {
                            let branch = if confirm.branch.is_empty() {
                                suggested_branch.clone()
                            } else {
                                confirm.branch.clone()
                            };
                            let worktree_name = if confirm.worktree_name.is_empty() {
                                suggested_worktree.clone()
                            } else {
                                confirm.worktree_name.clone()
                            };

                            let repo = repo_root.as_ref().unwrap();
                            let worktree_path = match create_worktree(repo, &worktree_name, &branch)
                            {
                                Ok(p) => p,
                                Err(e) => {
                                    let _ = tx
                                        .send(Ok(ServerMessage {
                                            event: Some(server_message::Event::WorkflowComplete(
                                                crate::gen::WorkflowComplete {
                                                    ok: false,
                                                    message: e,
                                                },
                                            )),
                                        }))
                                        .await;
                                    break;
                                }
                            };

                            let plan_dir_path = plan_dir.as_ref().unwrap();
                            let mut cs = read_changeset(plan_dir_path).unwrap_or_default();
                            cs.branch = Some(branch.clone());
                            cs.worktree = Some(worktree_path.to_string_lossy().to_string());
                            let _ = write_changeset(plan_dir_path, &cs);

                            let eng = engine.as_ref().unwrap();
                            let sid = session_id.as_ref().unwrap();
                            let mut updates = std::collections::HashMap::new();
                            updates.insert(
                                "worktree_dir".to_string(),
                                serde_json::to_value(worktree_path.clone()).unwrap(),
                            );
                            let rt = tokio::runtime::Handle::current();
                            rt.block_on(eng.update_session_context(sid, updates))
                                .map_err(|e| Status::internal(format!("update session: {}", e)))
                                .unwrap();

                            let mut result = rt
                                .block_on(eng.run_session(sid))
                                .map_err(|e| Status::internal(format!("run_session: {}", e)))
                                .unwrap();

                            loop {
                                while let Ok(ev) = event_rx.as_ref().unwrap().try_recv() {
                                    if let Some(msg) = workflow_event_to_server_message(ev) {
                                        let _ = tx.send(Ok(msg)).await;
                                    }
                                }
                                match &result.status {
                                    ExecutionStatus::Completed => {
                                        let _ = tx
                                            .send(Ok(ServerMessage {
                                                event: Some(
                                                    server_message::Event::WorkflowComplete(
                                                        crate::gen::WorkflowComplete {
                                                            ok: true,
                                                            message: "Workflow complete"
                                                                .to_string(),
                                                        },
                                                    ),
                                                ),
                                            }))
                                            .await;
                                        break;
                                    }
                                    ExecutionStatus::Error(msg) => {
                                        let _ = tx
                                            .send(Ok(ServerMessage {
                                                event: Some(
                                                    server_message::Event::WorkflowComplete(
                                                        crate::gen::WorkflowComplete {
                                                            ok: false,
                                                            message: msg.clone(),
                                                        },
                                                    ),
                                                ),
                                            }))
                                            .await;
                                        break;
                                    }
                                    ExecutionStatus::ElicitationNeeded { .. }
                                    | ExecutionStatus::WaitingForInput { .. } => {
                                        let _ = tx
                                            .send(Err(Status::unimplemented(
                                                "daemon does not support clarification after worktree",
                                            )))
                                            .await;
                                        break;
                                    }
                                    ExecutionStatus::Paused { .. } => {
                                        result = rt
                                            .block_on(eng.run_session(sid))
                                            .map_err(|e| {
                                                Status::internal(format!("run_session: {}", e))
                                            })
                                            .unwrap();
                                    }
                                }
                            }
                            break;
                        }
                    }
                    DaemonStreamState::PlanComplete => break,
                }
            }
        });

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }

    async fn get_session(
        &self,
        request: Request<GetSessionRequest>,
    ) -> Result<Response<GetSessionResponse>, Status> {
        let session_id = request.into_inner().session_id;
        if session_id.is_empty() {
            return Err(Status::invalid_argument("session_id is required"));
        }

        let plan_dir = self.sessions_base.join(&session_id);
        let changeset = read_changeset(&plan_dir)
            .map_err(|e| Status::not_found(format!("session not found: {} — {}", session_id, e)))?;

        let status = Self::status_from_state(&changeset.state.current);
        let plan_dir_str = plan_dir.to_string_lossy().to_string();
        let worktree = changeset.worktree.clone().unwrap_or_default();
        let branch = changeset.branch.clone().unwrap_or_default();

        Ok(Response::new(GetSessionResponse {
            session: Some(SessionInfo {
                session_id: session_id.clone(),
                status: status.to_string(),
                plan_dir: plan_dir_str,
                worktree,
                branch,
            }),
        }))
    }

    async fn list_sessions(
        &self,
        _request: Request<ListSessionsRequest>,
    ) -> Result<Response<ListSessionsResponse>, Status> {
        let mut sessions = Vec::new();

        if !self.sessions_base.exists() {
            return Ok(Response::new(ListSessionsResponse { sessions }));
        }

        let entries = std::fs::read_dir(&self.sessions_base)
            .map_err(|e| Status::internal(format!("read sessions dir: {}", e)))?;

        for entry in entries {
            let entry = entry.map_err(|e| Status::internal(format!("read dir entry: {}", e)))?;
            let path = entry.path();
            if path.is_dir() {
                let changeset_path = path.join("changeset.yaml");
                if changeset_path.exists() {
                    if let Ok(changeset) = read_changeset(&path) {
                        let session_id = path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("")
                            .to_string();
                        let status = Self::status_from_state(&changeset.state.current);
                        sessions.push(SessionInfo {
                            session_id,
                            status: status.to_string(),
                            plan_dir: path.to_string_lossy().to_string(),
                            worktree: changeset.worktree.clone().unwrap_or_default(),
                            branch: changeset.branch.clone().unwrap_or_default(),
                        });
                    }
                }
            }
        }

        Ok(Response::new(ListSessionsResponse { sessions }))
    }
}

#[derive(Clone)]
enum DaemonStreamState {
    Idle,
    WaitingApprovePlan,
    WaitingConfirmWorktree {
        suggested_branch: String,
        suggested_worktree: String,
    },
    PlanComplete,
}
