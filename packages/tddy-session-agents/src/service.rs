//! The `session_agents.SessionAgentService` implementation, and the entry the daemon's wiring
//! layer registers.
//!
//! All nine methods answer about a session's **live** roster: which agents are attached to it right
//! now, what each is doing, and what has been asked of them. Every one starts by resolving the
//! session directory from the caller's own token — a caller holding a valid token must not be able
//! to name a roster it does not own — which is why [`crate::ports::SessionDirResolver`] is a
//! resolver rather than a base path.
//!
//! # What is deliberately not here
//!
//! `daemon_instance_id` routing is **not** implemented in this crate, for the reason node 6's
//! `tddy-session-files` gives: forwarding a call to the peer that holds the roster needs the
//! common-room slot, the eligible-daemon roster and the LiveKit forwarding clients, all of which
//! are the daemon's transport layer. The daemon wraps this implementation in its own routing layer
//! (`tddy-daemon`'s `PeerRoutedSessionAgents`), which is where that fork lives.
//!
//! The forward that follows the **agent's** owning daemon is a different decision and *is* made
//! here: it can only be taken after the roster entry naming the owner has been read, which is this
//! crate's read. Delivering it is [`crate::ports::AgentConversationPeers`]'.

use std::sync::Arc;

use async_trait::async_trait;
use tddy_daemon_kernel::HOST_DOCUMENT_FRAME_BYTES;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::session_agents_svc::{
    AgentConversationChunk, AttachSessionAgentRequest, CancelAgentConversationRequest,
    CancelAgentConversationResponse, DetachSessionAgentRequest, ListSessionAgentsRequest,
    OpenAgentConversationRequest, OpenAgentConversationResponse, PromptAgentConversationRequest,
    ReportAgentCloneStateRequest, ReportAgentCloneStateResponse,
    ReportAgentConversationStateRequest, ReportAgentConversationStateResponse, SessionAgentEntry,
    SessionAgentRoster, StreamSessionAgentsRequest,
};
use tddy_service::SessionAgentServiceServer;
use tddy_worktree_service::stream::MpscResultStream;

use crate::agent_conversations::{AgentConversation, PromptRouting};
use crate::ports::SessionAgentPorts;
use crate::session_agent_status::ManagedAgentState;

/// The `session_agents.SessionAgentService` implementation.
pub struct SessionAgentServiceImpl {
    ports: SessionAgentPorts,
}

impl SessionAgentServiceImpl {
    #[must_use]
    pub fn new(ports: SessionAgentPorts) -> Self {
        Self { ports }
    }

    /// The session directory a request addresses, authenticated and validated by the host.
    fn session_dir(
        &self,
        session_token: &str,
        session_id: &str,
    ) -> Result<std::path::PathBuf, Status> {
        (self.ports.session_dirs)(session_token, session_id)
    }

    /// Record what a roster agent is doing, and push the roster that says so — see
    /// [`crate::status_reporting::note_agent_activity`], which the daemon's own local-agent tool
    /// dispatch calls too.
    fn note_agent_activity(
        &self,
        session_id: &str,
        session_dir: &std::path::Path,
        agent_id: &str,
        state: ManagedAgentState,
        summary: impl AsRef<str>,
    ) {
        crate::status_reporting::note_agent_activity(
            &self.ports.rosters,
            self.ports.sessions.hosts_a_clone_for(session_id),
            session_id,
            session_dir,
            agent_id,
            state,
            summary,
        );
    }

    /// A callback that puts one agent back to IDLE, for the two places a turn can end.
    ///
    /// A callback rather than a second [`Self::note_agent_activity`] call at each site, because
    /// both sites end a turn from inside a spawned task that outlives the handler: whatever reports
    /// the end has to own everything it names.
    fn turn_end_reporter(
        &self,
        session_id: &str,
        session_dir: &std::path::Path,
        agent_id: &str,
    ) -> impl Fn(String) + Send + 'static {
        let rosters = Arc::clone(&self.ports.rosters);
        let sessions = Arc::clone(&self.ports.sessions);
        let session_id = session_id.to_string();
        let session_dir = session_dir.to_path_buf();
        let agent_id = agent_id.to_string();
        move |summary| {
            if sessions.hosts_a_clone_for(&session_id) {
                return;
            }
            // Guarded rather than written outright: by the time a turn is observed to have ended, a
            // cancel or a detach may already have moved this agent on, and an unconditional write
            // would resurrect a conversation that is gone.
            if rosters
                .activity()
                .record_turn_end(&session_id, &agent_id, summary)
            {
                crate::status_reporting::republish_quietly(
                    &rosters,
                    &session_id,
                    &session_dir,
                    &agent_id,
                );
            }
        }
    }

    /// Pass a peer's answer through unchanged, noting when it ends.
    ///
    /// The turn loop of a remote agent runs on its owning daemon, but the roster that reports its
    /// status is held *here* — so without this relay a forwarded turn would raise the badge to
    /// RUNNING and never lower it. Frames and errors are forwarded verbatim and in order: the
    /// caller must not be able to tell a relayed stream from a direct one, which is the same
    /// property [`AgentConversation`]'s two variants exist to hold.
    fn relay_watching_for_the_turn_to_end(
        &self,
        mut peer: tokio::sync::mpsc::UnboundedReceiver<Result<AgentConversationChunk, Status>>,
        session_id: &str,
        session_dir: &std::path::Path,
        agent_id: &str,
    ) -> tokio::sync::mpsc::UnboundedReceiver<Result<AgentConversationChunk, Status>> {
        let turn_ended = self.turn_end_reporter(session_id, session_dir, agent_id);
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Some(frame) = peer.recv().await {
                if tx.send(frame).is_err() {
                    // The caller hung up. The turn is no longer being read, so it is over as far as
                    // anything here can tell — and leaving the badge up would strand it.
                    break;
                }
            }
            turn_ended("answered".to_string());
        });
        rx
    }
}

/// The stop reason a turn ended with, as the wire names it.
#[must_use]
pub fn agent_stop_reason(reason: tddy_discovery::subagent::StopReason) -> &'static str {
    match reason {
        tddy_discovery::subagent::StopReason::EndTurn => "EndTurn",
        tddy_discovery::subagent::StopReason::MaxTurnRequests => "MaxTurnRequests",
        tddy_discovery::subagent::StopReason::Cancelled => "Cancelled",
    }
}

/// Split one agent turn's answer into ordered [`HOST_DOCUMENT_FRAME_BYTES`] frames.
///
/// The stop reason rides the **final** frame, and an empty answer still yields exactly one frame, so
/// a consumer never has to tell "said nothing" from "nothing arrived" and a stream that ends without
/// a `last` frame is unambiguously a truncation. Framed rather than sent whole for the reason
/// `StreamExecuteTool` frames its results: over LiveKit anything past `MAX_CHUNK_FRAME_BYTES` is
/// chunk-framed, and one lost chunk frame wedges the call with no error at all
/// (`docs/ft/coder/rpc-multi-transport.md`).
#[must_use]
pub fn agent_conversation_frames(content: &str, stop_reason: &str) -> Vec<AgentConversationChunk> {
    let mut frames: Vec<AgentConversationChunk> = Vec::new();
    let mut rest = content;
    while !rest.is_empty() {
        // Split on a char boundary at or below the budget: a frame cut mid-codepoint would not be a
        // `String` at all, and the two halves would each decode as replacement characters.
        let mut take = rest.len().min(HOST_DOCUMENT_FRAME_BYTES);
        while take > 0 && !rest.is_char_boundary(take) {
            take -= 1;
        }
        let (head, tail) = rest.split_at(take);
        frames.push(AgentConversationChunk {
            content_chunk: head.to_string(),
            ..Default::default()
        });
        rest = tail;
    }
    if frames.is_empty() {
        frames.push(AgentConversationChunk::default());
    }
    let last = frames.last_mut().expect("at least one frame");
    last.stop_reason = stop_reason.to_string();
    last.last = true;
    frames
}

/// One roster snapshot, re-addressed from the coordinate the stores build to the one this service
/// serves.
///
/// The two messages are field-for-field identical — `session_agents.proto` imports
/// `types.proto` for the two `ListSessions` also reaches, where `connection.proto` declares its
/// own copies — so this is a re-labelling rather than a mapping. It exists because the stores still
/// build the `connection` form: `ListSessions` (family C, which stays in the daemon) reports the
/// same rows through `SessionEntry`, and the `session.agents` room broadcast carries the same
/// message to clients that have not moved yet.
///
/// TODO(session-agent-services): retire this once the old coordinate is cut and the stores build
/// `session_agents.SessionAgentRoster` directly. It is a copy of every entry on every snapshot,
/// which a `StreamSessionAgents` keepalive pays for on its cadence.
#[must_use]
pub fn roster_at_the_new_coordinate(
    roster: tddy_service::proto::connection::SessionAgentRoster,
) -> SessionAgentRoster {
    SessionAgentRoster {
        session_id: roster.session_id,
        rev: roster.rev,
        agents: roster
            .agents
            .into_iter()
            .map(|agent| SessionAgentEntry {
                agent_id: agent.agent_id,
                name: agent.name,
                daemon_instance_id: agent.daemon_instance_id,
                label: agent.label,
                model: agent.model,
                replaces: agent.replaces,
                tools: agent.tools,
                codebase_session_id: agent.codebase_session_id,
                clone_state: agent.clone_state,
                clone_error: agent.clone_error,
                status: agent.status,
                last_activity: agent.last_activity.map(|activity| {
                    tddy_service::proto::types::SessionAgentActivity {
                        at_unix_ms: activity.at_unix_ms,
                        summary: activity.summary,
                    }
                }),
            })
            .collect(),
    }
}

#[async_trait]
impl tddy_service::proto::session_agents_svc::SessionAgentService for SessionAgentServiceImpl {
    /// Attach one agent to a session's live roster.
    ///
    /// Idempotent on `(session, agent_id)`: re-attaching an already attached agent returns the
    /// roster unchanged and does not bump `rev`.
    async fn attach_session_agent(
        &self,
        request: Request<AttachSessionAgentRequest>,
    ) -> Result<Response<SessionAgentRoster>, Status> {
        let req = request.into_inner();
        let session_dir = self.session_dir(&req.session_token, &req.session_id)?;
        let mut record = self.ports.catalog.record_for(&req.agent_id).await?;

        // An agent owned by a peer reads a checkout on that peer, so the entry has to name one
        // before it is written. Claiming it is also what opens the session's room — and both happen
        // before the roster is touched, so an attach that cannot be completed leaves the session
        // looking exactly as it did (PRD § What attach does: "no roster entry, no half-built clone,
        // no room membership").
        let admitted = self
            .ports
            .admission
            .admit(&req.session_id, &session_dir, &record, &req.session_token)
            .await?;
        // Assigned only when the admission actually claimed one: a *local* agent works the real
        // worktree and the catalog's record already says so, and blanking that would have the entry
        // report no checkout for an agent that has the session's own.
        if let Some(codebase_session_id) = admitted.codebase_session_id.clone() {
            record.codebase_session_id = Some(codebase_session_id);
        }

        // A roster this host could not write is an attach that did not happen, and the clone
        // claimed a moment ago is the half of it the peer has already been told to build: without
        // this the caller would be handed an error while a checkout it can no longer name kept being
        // cut on another host ("no roster entry, no half-built clone, no room membership").
        let roster = match self
            .ports
            .rosters
            .attach(&req.session_id, &session_dir, record)
        {
            Ok(roster) => roster,
            Err(e) => {
                self.ports
                    .admission
                    .withdraw(&req.session_id, &admitted, &req.session_token)
                    .await;
                return Err(e);
            }
        };
        self.ports
            .broadcast
            .broadcast(&req.session_id, &roster)
            .await;
        log::info!(
            "AttachSessionAgent: session {} holds {} agent(s) at rev {}",
            req.session_id,
            roster.agents.len(),
            roster.rev
        );
        Ok(Response::new(roster_at_the_new_coordinate(roster)))
    }

    /// Detach one agent. An id the roster does not hold is `NOT_FOUND`, never a silent success.
    ///
    /// The entry is removed first and the checkout torn down after, in that order: a teardown that
    /// ran first and then failed to remove the entry would leave the roster naming a checkout that
    /// is gone, which is the state a prompt is served from.
    async fn detach_session_agent(
        &self,
        request: Request<DetachSessionAgentRequest>,
    ) -> Result<Response<SessionAgentRoster>, Status> {
        let req = request.into_inner();
        let session_dir = self.session_dir(&req.session_token, &req.session_id)?;
        let detached = self
            .ports
            .rosters
            .entry(&req.session_id, &session_dir, &req.agent_id)?;
        let roster = self
            .ports
            .rosters
            .detach(&req.session_id, &session_dir, &req.agent_id)?;
        self.cancel_conversations_with(&req.session_token, &req.session_id, &req.agent_id)
            .await;
        // Forgotten rather than set to "no conversation": the agent is off the roster, and a record
        // left behind would have a re-attach show the previous attachment's last activity as this
        // one's.
        self.ports
            .rosters
            .activity()
            .forget(&req.session_id, &req.agent_id);

        // The clone survives while another agent on that host still reads it — two agents on one
        // host share one checkout, so the last one out is what removes it.
        if let Some(record) = detached.filter(|r| r.codebase_session_id.is_some()) {
            let still_used = !self
                .ports
                .rosters
                .agents_owned_by(&req.session_id, &session_dir, &record.daemon_instance_id)?
                .is_empty();
            if !still_used {
                let codebase_session_id = record
                    .codebase_session_id
                    .clone()
                    .expect("filtered to entries naming a clone");
                // The entry is already gone and persisted by now, so the refusal says so: a message
                // that read as "the agent was left attached, retry" would send an operator into a
                // retry that answers NOT_FOUND while the checkout stays exactly where it is.
                if let Err(e) = self
                    .ports
                    .admission
                    .tear_down(
                        &req.session_id,
                        &record.daemon_instance_id,
                        &codebase_session_id,
                        &req.session_token,
                    )
                    .await
                {
                    self.ports
                        .broadcast
                        .broadcast(&req.session_id, &roster)
                        .await;
                    return Err(Status {
                        code: e.code(),
                        message: format!(
                            "agent '{}' was detached from session '{}' (rev {}), but its clone \
                             could not be removed: {}. Retrying the detach reports NOT_FOUND — the \
                             checkout has to be deleted where it is.",
                            req.agent_id,
                            req.session_id,
                            roster.rev,
                            e.message()
                        ),
                    });
                }
            }
        }

        self.ports
            .broadcast
            .broadcast(&req.session_id, &roster)
            .await;
        log::info!(
            "DetachSessionAgent: session {} holds {} agent(s) at rev {}",
            req.session_id,
            roster.agents.len(),
            roster.rev
        );
        Ok(Response::new(roster_at_the_new_coordinate(roster)))
    }

    async fn list_session_agents(
        &self,
        request: Request<ListSessionAgentsRequest>,
    ) -> Result<Response<SessionAgentRoster>, Status> {
        let req = request.into_inner();
        let session_dir = self.session_dir(&req.session_token, &req.session_id)?;
        let roster = self.ports.rosters.snapshot(&req.session_id, &session_dir)?;
        Ok(Response::new(roster_at_the_new_coordinate(roster)))
    }

    type StreamSessionAgentsStream = MpscResultStream<SessionAgentRoster>;

    /// The roster, now and on every change.
    ///
    /// The first frame is the current snapshot, taken with the subscription under one lock, so a
    /// late subscriber — the in-jail `tddy-tools` reconnecting, a browser tab opening — needs no
    /// separate priming read and cannot miss a change published between the two.
    async fn stream_session_agents(
        &self,
        request: Request<StreamSessionAgentsRequest>,
    ) -> Result<Response<Self::StreamSessionAgentsStream>, Status> {
        let req = request.into_inner();
        let session_dir = self.session_dir(&req.session_token, &req.session_id)?;
        let (snapshot, mut published) = self
            .ports
            .rosters
            .subscribe(&req.session_id, &session_dir)?;

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        // The roster this subscription has last sent, re-sent whenever the keepalive cadence elapses
        // with nothing published. A quiet roster still has to talk — see
        // [`crate::ports::SessionAgentPorts::roster_keepalive`]. It tracks the last frame *sent*
        // rather than the opening snapshot, so a subscriber that reads only a keepalive is never
        // told a superseded roster is the current one.
        let mut last_sent = roster_at_the_new_coordinate(snapshot);
        if tx.send(Ok(last_sent.clone())).is_err() {
            return Err(Status::internal(
                "StreamSessionAgents: the subscriber went away before its first frame",
            ));
        }
        let session_id = req.session_id.clone();
        let keepalive = self.ports.roster_keepalive;
        tokio::spawn(async move {
            loop {
                match tokio::time::timeout(keepalive, published.recv()).await {
                    Ok(Ok(roster)) => {
                        last_sent = roster_at_the_new_coordinate(roster);
                        if tx.send(Ok(last_sent.clone())).is_err() {
                            break;
                        }
                    }
                    // Every frame is a whole roster, so a subscriber that fell behind is brought
                    // fully current by the next one — the dropped frames carried nothing the
                    // survivor does not also carry.
                    Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(missed))) => {
                        log::debug!(
                            "StreamSessionAgents: subscriber to session {session_id} fell {missed} \
                             snapshot(s) behind; the next one supersedes them"
                        );
                    }
                    Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => break,
                    // Nothing changed for a whole cadence. Re-send, which also gives this task its
                    // only chance to notice a subscriber that went away: without a frame to fail on,
                    // it would park on a roster nobody changes for the life of the process.
                    Err(_) => {
                        if tx.send(Ok(last_sent.clone())).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        Ok(Response::new(MpscResultStream::from(rx)))
    }

    /// Open a conversation with one roster agent.
    ///
    /// A local entry gets a turn loop in this process; a remote entry gets a routing record and the
    /// same call forwarded to its owning daemon. The caller cannot tell which happened, which is the
    /// property that makes remote agents usable at all (PRD AC28).
    ///
    /// A clone that is still being built refuses the open naming its state. Queuing it would make a
    /// 90-second `git clone` look like a hung agent, and serving it would read an empty checkout and
    /// report "not found" for a file that is simply not there yet (AC33).
    async fn open_agent_conversation(
        &self,
        request: Request<OpenAgentConversationRequest>,
    ) -> Result<Response<OpenAgentConversationResponse>, Status> {
        let req = request.into_inner();
        let session_dir = self.session_dir(&req.session_token, &req.session_id)?;
        // Caller-chosen where offered, so an open that times out still leaves the caller able to
        // name — and therefore cancel — whatever this host built.
        let conversation_id = match req.conversation_id.trim().is_empty() {
            true => uuid::Uuid::now_v7().to_string(),
            false => req.conversation_id.trim().to_string(),
        };

        // This host *owns* the agent: the session is another daemon's, its roster is over there,
        // and the def is here. Resolving it against a roster this host does not hold would report
        // a session that legitimately is not here as the reason an agent it does own cannot answer.
        let owned = self
            .ports
            .sessions
            .open_owned(&req.session_id, &req.agent_id)
            .await?;
        let conversation = match owned {
            Some(session) => AgentConversation::Local {
                session_id: req.session_id.clone(),
                agent_id: req.agent_id.clone(),
                session: Arc::new(tokio::sync::Mutex::new(session)),
                closed: Arc::new(tokio::sync::Notify::new()),
            },
            None => {
                let record = self
                    .ports
                    .rosters
                    .entry(&req.session_id, &session_dir, &req.agent_id)?
                    .ok_or_else(|| {
                        Status::invalid_argument(format!(
                            "agent '{}' is not attached to session '{}'",
                            req.agent_id, req.session_id
                        ))
                    })?;
                match record.daemon_instance_id == self.ports.local_instance_id {
                    true => AgentConversation::Local {
                        session_id: req.session_id.clone(),
                        agent_id: record.agent_id.clone(),
                        session: Arc::new(tokio::sync::Mutex::new(
                            self.ports
                                .sessions
                                .open_local(
                                    &req.session_id,
                                    &session_dir,
                                    &record,
                                    &req.session_token,
                                )
                                .await?,
                        )),
                        closed: Arc::new(tokio::sync::Notify::new()),
                    },
                    false => {
                        self.ports
                            .sessions
                            .refuse_unready_clone(&req.session_id, &record)?;
                        self.ports
                            .sessions
                            .refuse_departed_owner(&record.daemon_instance_id)
                            .await?;
                        self.ports
                            .peers
                            .open(&req, &record.daemon_instance_id, &conversation_id)
                            .await?;
                        AgentConversation::Remote {
                            session_id: req.session_id.clone(),
                            agent_id: record.agent_id.clone(),
                            daemon_instance_id: record.daemon_instance_id.clone(),
                        }
                    }
                }
            }
        };
        self.ports
            .conversations
            .insert(conversation_id.clone(), conversation)
            .await;
        // Open, not running: the conversation exists and has been asked nothing. This is also the
        // first moment an entry stops reporting UNSPECIFIED, which is what a reader needs to tell
        // "attached and reachable" from "attached, and this daemon has never heard from it".
        self.note_agent_activity(
            &req.session_id,
            &session_dir,
            &req.agent_id,
            ManagedAgentState::Open,
            "conversation opened",
        );
        Ok(Response::new(OpenAgentConversationResponse {
            conversation_id,
        }))
    }

    type PromptAgentConversationStream = MpscResultStream<AgentConversationChunk>;

    /// Prompt an open conversation, streaming the agent's answer back.
    ///
    /// Both variants end with exactly one `last` frame carrying the stop reason, so a consumer never
    /// has to distinguish "said nothing" from "nothing arrived", and a stream that ends without one
    /// was truncated rather than completed.
    async fn prompt_agent_conversation(
        &self,
        request: Request<PromptAgentConversationRequest>,
    ) -> Result<Response<Self::PromptAgentConversationStream>, Status> {
        let req = request.into_inner();
        let session_dir = self.session_dir(&req.session_token, &req.session_id)?;

        // Everything the turn needs is taken out of the map under one lock, and the guard is
        // dropped before anything is awaited on it. The agent id comes out with it: the request
        // names a conversation, not an agent, and the status is recorded per agent.
        let (routing, agent_id) = self
            .ports
            .conversations
            .routing_for(&req.conversation_id)
            .await
            .ok_or_else(|| {
                Status::not_found(format!(
                    "conversation '{}' is not open on session '{}'",
                    req.conversation_id, req.session_id
                ))
            })?;

        // Stamped before either branch runs, so the badge changes when the turn starts rather than
        // when it is first observed to have started.
        self.note_agent_activity(
            &req.session_id,
            &session_dir,
            &agent_id,
            ManagedAgentState::Prompting,
            format!("prompted: {}", req.prompt),
        );

        let (session, closed) = match routing {
            PromptRouting::Local { session, closed } => (session, closed),
            PromptRouting::Remote(daemon_instance_id) => {
                self.ports
                    .sessions
                    .refuse_departed_owner(&daemon_instance_id)
                    .await?;
                let rx = self.ports.peers.prompt(&req, &daemon_instance_id).await?;
                // Relayed rather than handed straight back, for one reason: the roster this host
                // holds is what reports the status, and the end of the peer's stream is the only
                // moment this side learns the turn is over. Passed through unchanged — the caller
                // sees the peer's frames in the peer's order, errors included.
                return Ok(Response::new(MpscResultStream::from(
                    self.relay_watching_for_the_turn_to_end(
                        rx,
                        &req.session_id,
                        &session_dir,
                        &agent_id,
                    ),
                )));
            }
        };

        // The turn loop runs here. Spawned rather than awaited so the stream's frames are produced
        // while the caller reads them, and awaited on the *conversation's* lock alone: two prompts on
        // one conversation are still serialized, but the map of open conversations is not held, so a
        // cancel can land while this turn is in flight — which is the only moment a cancel matters.
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let conversation_id = req.conversation_id.clone();
        let prompt = req.prompt.clone();
        let turn_ended = self.turn_end_reporter(&req.session_id, &session_dir, &agent_id);
        tokio::spawn(async move {
            let outcome = tokio::select! {
                // Biased so a conversation already closed is reported as closed rather than racing
                // one more turn out of a model.
                biased;
                _ = closed.notified() => {
                    let _ = tx.send(Err(Status::failed_precondition(format!(
                        "conversation '{conversation_id}' was closed while its turn was in flight"
                    ))));
                    return;
                }
                outcome = async { session.lock().await.prompt(&prompt).await } => outcome,
            };
            match outcome {
                // Framed rather than sent whole: over LiveKit anything past MAX_CHUNK_FRAME_BYTES is
                // chunk-framed, and one lost chunk frame wedges the call with no error at all.
                Ok(outcome) => {
                    let content = outcome
                        .content
                        .iter()
                        .map(|block| block.text.as_str())
                        .collect::<Vec<_>>()
                        .join("");
                    // Reported after the frames are on the wire, not before: a badge that drops to
                    // idle while the answer is still arriving is one a reader acts on too early.
                    let answered = format!("answered ({} chars)", content.chars().count());
                    for frame in
                        agent_conversation_frames(&content, agent_stop_reason(outcome.stop_reason))
                    {
                        if tx.send(Ok(frame)).is_err() {
                            // The caller hung up mid-answer. The turn is over either way, and a
                            // badge left up would strand it.
                            turn_ended(answered);
                            return;
                        }
                    }
                    turn_ended(answered);
                }
                Err(e) => {
                    // Idle, not ERROR: the agent is still attached and still promptable, and it is
                    // the *clone* that ERROR is reserved for. The summary is what says what happened.
                    turn_ended(format!("turn failed: {e}"));
                    let _ = tx.send(Err(Status::internal(format!(
                        "agent conversation '{conversation_id}' failed: {e}"
                    ))));
                }
            }
        });
        Ok(Response::new(MpscResultStream::from(rx)))
    }

    /// Cancel an open conversation. An id nothing holds is `NOT_FOUND`, never a silent success — a
    /// caller told a turn was cancelled when it is still running would go on to read a stale answer.
    async fn cancel_agent_conversation(
        &self,
        request: Request<CancelAgentConversationRequest>,
    ) -> Result<Response<CancelAgentConversationResponse>, Status> {
        let req = request.into_inner();
        let session_dir = self.session_dir(&req.session_token, &req.session_id)?;
        let removed = self.ports.conversations.remove(&req.conversation_id).await;
        if let Some(conversation) = removed.as_ref() {
            // Back to "asked nothing", whichever daemon ran the loop: the conversation is gone, so
            // an agent left reporting a turn in flight would be one nothing can ever finish.
            self.note_agent_activity(
                &req.session_id,
                &session_dir,
                conversation.agent_id(),
                ManagedAgentState::NoConversation,
                "conversation cancelled",
            );
        }
        match removed {
            None => Err(Status::not_found(format!(
                "conversation '{}' is not open on session '{}'",
                req.conversation_id, req.session_id
            ))),
            Some(AgentConversation::Local { closed, .. }) => {
                // A turn already in flight is interrupted rather than left to finish: the caller has
                // been told the conversation is cancelled, and an answer arriving afterwards would
                // be one it has no reason to expect.
                closed.notify_one();
                Ok(Response::new(CancelAgentConversationResponse {}))
            }
            Some(AgentConversation::Remote {
                daemon_instance_id, ..
            }) => {
                self.ports
                    .peers
                    .cancel(
                        &req.session_token,
                        &req.session_id,
                        &daemon_instance_id,
                        &req.conversation_id,
                    )
                    .await?;
                Ok(Response::new(CancelAgentConversationResponse {}))
            }
        }
    }

    /// The owning daemon telling this one how its clone is doing.
    ///
    /// Pushed rather than polled because only the daemon holding the checkout can say any of it, and
    /// accepted only for a clone this host actually asked that daemon for — the report is what
    /// authorizes an entry to start serving prompts.
    ///
    /// Authenticated first, and that is not ceremony: the (session, daemon, clone) triple the store
    /// matches on is published in the session's `session.agents` broadcast, so on the triple alone
    /// any participant that saw a roster frame could report a still-provisioning clone READY and
    /// have the next prompt served from an empty checkout.
    ///
    /// TODO(session-agent-roster): also bind the report to the *reporting participant*. The verified
    /// LiveKit participant identity is known at the transport but is not carried into
    /// `RequestMetadata` — `sender_identity` there is taken from the request envelope, which the
    /// sender writes itself, so checking `daemon_instance_id` against it would look like a check
    /// while refusing nothing.
    async fn report_agent_clone_state(
        &self,
        request: Request<ReportAgentCloneStateRequest>,
    ) -> Result<Response<ReportAgentCloneStateResponse>, Status> {
        let req = request.into_inner();
        let session_dir = self.session_dir(&req.session_token, &req.session_id)?;
        // An unrecognised state number becomes `Unspecified`, which the store's own checks refuse.
        // Defaulting it to a real state would let a garbled report mark a checkout ready.
        let state = tddy_service::proto::connection::AgentCloneState::try_from(req.clone_state)
            .unwrap_or(tddy_service::proto::connection::AgentCloneState::Unspecified);
        self.ports
            .clones
            .record_report(&crate::session_agent_clone::AgentCloneReport {
                session_id: req.session_id.clone(),
                daemon_instance_id: req.daemon_instance_id.clone(),
                codebase_session_id: req.codebase_session_id.clone(),
                state,
                error: req.clone_error.clone(),
                worktree_path: Some(req.worktree_path.clone())
                    .filter(|p| !p.is_empty())
                    .map(std::path::PathBuf::from),
                divergences: req.divergences.clone(),
            })?;
        log::info!(
            "ReportAgentCloneState: daemon {} reports session {}'s clone {} as {state:?}{}",
            req.daemon_instance_id,
            req.session_id,
            req.codebase_session_id,
            match req.divergences.len() {
                0 => String::new(),
                n => format!(" with {n} divergence(s)"),
            }
        );
        for divergence in &req.divergences {
            log::error!(
                "session {}'s clone on daemon {} diverged and was reconciled: {divergence}",
                req.session_id,
                req.daemon_instance_id
            );
        }
        // Republished *and* broadcast: a clone finishing moves what every reader sees without
        // moving `rev`, so a subscriber that only heard about `rev` changes would show
        // `provisioning` until the next attach, which may never come.
        if let Err(e) = self.ports.rosters.republish(&req.session_id, &session_dir) {
            log::warn!(
                "could not republish session {}'s roster to its subscribers: {}",
                req.session_id,
                e.message()
            );
            return Ok(Response::new(ReportAgentCloneStateResponse {}));
        }
        if let Ok(roster) = self.ports.rosters.snapshot(&req.session_id, &session_dir) {
            self.ports
                .broadcast
                .broadcast(&req.session_id, &roster)
                .await;
        }
        Ok(Response::new(ReportAgentCloneStateResponse {}))
    }

    /// An agent whose turn loop runs in the jail, telling this host what that loop is doing.
    ///
    /// The daemon infers a status from the conversation RPCs for every agent whose loop it runs. An
    /// agent the in-jail `tddy-tools` was *seeded* with runs its loop there instead, and this host
    /// is never asked to open anything — so without this report the row would sit at UNSPECIFIED for
    /// an agent that is demonstrably working.
    ///
    /// Two things are checked here, and each is a way the roster could otherwise be made to lie —
    /// the third, that the report reached the daemon holding the roster at all, is the routing
    /// preamble the daemon keeps:
    ///
    /// - **The agent must be attached.** An id the roster does not hold is `NOT_FOUND`, so an
    ///   in-jail registry that has gone stale cannot put a row on a roster an operator emptied.
    /// - **Only a conversation state is accepted.** `CONNECTING` and `ERROR` describe the checkout,
    ///   which this daemon measures itself and which outranks the conversation at snapshot time; a
    ///   reporter allowed to send them could hide a broken clone behind a cheerful conversation.
    ///
    /// Authenticated as `ReportAgentCloneState` is, and for the same reason: the (session, agent)
    /// pair is published in the `session.agents` broadcast, so on the pair alone any participant
    /// that saw a frame could park an agent at RUNNING for ever.
    async fn report_agent_conversation_state(
        &self,
        request: Request<ReportAgentConversationStateRequest>,
    ) -> Result<Response<ReportAgentConversationStateResponse>, Status> {
        let req = request.into_inner();
        let session_dir = self.session_dir(&req.session_token, &req.session_id)?;
        if self
            .ports
            .rosters
            .entry(&req.session_id, &session_dir, &req.agent_id)?
            .is_none()
        {
            return Err(Status::not_found(format!(
                "agent '{}' is not attached to session '{}', so there is no row to report on",
                req.agent_id, req.session_id
            )));
        }

        let status = tddy_service::proto::connection::SessionAgentStatus::try_from(req.status)
            .unwrap_or(tddy_service::proto::connection::SessionAgentStatus::Unspecified);
        let state = crate::session_agent_status::reported_state(status).ok_or_else(|| {
            Status::invalid_argument(format!(
                "{status:?} is not a conversation state: CONNECTING and ERROR describe the \
                 checkout, which this daemon measures itself, and UNSPECIFIED claims nothing"
            ))
        })?;

        self.note_agent_activity(
            &req.session_id,
            &session_dir,
            &req.agent_id,
            state,
            &req.summary,
        );
        Ok(Response::new(ReportAgentConversationStateResponse {}))
    }
}

impl SessionAgentServiceImpl {
    /// Close every open conversation with `agent_id` — what a detach does.
    ///
    /// A conversation whose loop runs on another daemon is cancelled *there* as well as forgotten
    /// here, and a failure to reach that daemon is loud and non-fatal: the entry is already gone
    /// here, so failing the detach would leave the roster and the conversation map disagreeing about
    /// an agent that is no longer on it.
    async fn cancel_conversations_with(
        &self,
        session_token: &str,
        session_id: &str,
        agent_id: &str,
    ) {
        let cancelled_remotely = self
            .ports
            .conversations
            .close_every_conversation_with(session_id, agent_id)
            .await;
        for (daemon_instance_id, conversation_id) in cancelled_remotely {
            if let Err(e) = self
                .ports
                .peers
                .cancel(
                    session_token,
                    session_id,
                    &daemon_instance_id,
                    &conversation_id,
                )
                .await
            {
                log::error!(
                    "could not cancel conversation {conversation_id} with '{agent_id}' on daemon \
                     {daemon_instance_id} ({}); its turn loop may still be running there",
                    e.message()
                );
            }
        }
    }
}

/// The `session_agents.SessionAgentService` entry the daemon's wiring layer registers.
///
/// `#unbundle` node 7 moved family B out of `connection.ConnectionService` and into the crate that
/// owns its four modules. The ports stay injected because each one is *wiring*: which directory a
/// token may reach, which def an agent id resolves to, whether a checkout on a peer could be
/// claimed, how a roster change reaches a room and how a conversation reaches the daemon owning the
/// agent are the daemon's answers, not this subsystem's behaviour.
#[must_use]
pub fn build_session_agents_entry(ports: SessionAgentPorts) -> tddy_rpc::ServiceEntry {
    let server = SessionAgentServiceServer::new(SessionAgentServiceImpl::new(ports));
    tddy_rpc::ServiceEntry {
        name: "session_agents.SessionAgentService",
        service: Arc::new(server) as Arc<dyn tddy_rpc::RpcService>,
    }
}
