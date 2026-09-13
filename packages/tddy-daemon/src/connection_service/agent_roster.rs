use tddy_service::proto::connection::ConnectionService as ConnectionServiceTrait;
use tddy_service::proto::connection::ExecuteToolResponse;

use tddy_service::proto::connection::SplitAgentPlacement;

use tddy_service::proto::connection::StartSessionRequest;

use tddy_service::proto::connection::SubagentInfo;

use tddy_service::proto::connection::GetWorktreeSnapshotRequest;

use tddy_rpc::Request;

use tddy_rpc::Status;

use crate::connection_service::{seed_codebase, seeded_clone_guard, SeededAgentClones};

use super::ConnectionServiceImpl;

/// The daemon in its capacity as the claimant of the clones a session's peer-owned agents read.
///
/// A shallow clone of the service (every mutable field is behind an `Arc`) rather than the service
/// itself, so the free spawn functions can be handed the one collaborator they need without naming
/// the concrete daemon type in their signatures.
pub(crate) struct DaemonSeedCloneClaimant {
    pub(crate) service: ConnectionServiceImpl,
}

/// The daemon measuring a checkout that lives on one of its peers.
///
/// Routed through its own `GetWorktreeSnapshot` handler rather than a bespoke client, so a remote
/// measurement takes exactly the path a caller's would — including the peer routing and the
/// blocking-pool budget.
#[async_trait::async_trait]
impl tddy_daemon_livekit::session_room::RemoteSnapshotSource for ConnectionServiceImpl {
    async fn snapshot(
        &self,
        session_token: &str,
        codebase_session_id: &str,
        codebase_instance_id: &str,
    ) -> Result<tddy_daemon_livekit::session_room::WorktreeSnapshot, Status> {
        let answered = ConnectionServiceTrait::get_worktree_snapshot(
            self,
            Request::new(GetWorktreeSnapshotRequest {
                session_token: session_token.to_string(),
                session_id: codebase_session_id.to_string(),
                daemon_instance_id: codebase_instance_id.to_string(),
            }),
        )
        .await?
        .into_inner();
        Ok(tddy_daemon_livekit::session_room::WorktreeSnapshot {
            head_commit: answered.head_commit,
            branch: answered.branch,
            changed_paths: answered.changed_paths,
            changed_files: answered.changed_files,
            lines_added: answered.lines_added,
            lines_removed: answered.lines_removed,
            untracked_files: answered.untracked_files,
            // FIXME(session-worktree-sync): a SPLIT session's snapshot arrives over
            // GetWorktreeSnapshot, whose response carries no tree — so the facilitating daemon
            // cannot diff a checkout it does not hold. Closing this means a `wip_tree` field on
            // GetWorktreeSnapshotResponse and the codebase daemon writing it. Until then a split
            // session syncs committed history only, and says so rather than mirroring silently
            // stale content. See docs/dev/TODO.md.
            wip_tree: String::new(),
        })
    }
}

#[async_trait::async_trait]
impl SeededAgentClones for DaemonSeedCloneClaimant {
    async fn claim_for_seed(
        &self,
        session_id: &str,
        codebase: &seed_codebase::SeedCodebase,
        session_token: &str,
        records: &mut [tddy_core::SessionAgentRecord],
    ) -> Result<seeded_clone_guard::SeededCloneGuard, Status> {
        self.service
            .claim_co_located_seed_clones(session_id, codebase, session_token, records)
            .await
    }
}

/// The exec-catalog names of the tools a def's own loop may call — the spelling the wire, the
/// roster and `execute_tool`'s dispatch all use, rather than the `UPPERCASE` YAML spelling.
fn def_tool_names(def: &tddy_discovery::agent_def::SpecializedAgentDef) -> Vec<String> {
    def.tools
        .iter()
        .map(|t| t.catalog_name().to_string())
        .collect()
}

/// One resolved def as the `ListSubagents` row a picker attaches from.
pub(crate) fn subagent_info(
    def: &tddy_discovery::agent_def::SpecializedAgentDef,
    daemon_instance_id: &str,
) -> Result<SubagentInfo, tddy_core::AgentIdError> {
    Ok(SubagentInfo {
        agent_id: qualified_agent_id(&def.name, daemon_instance_id)?,
        name: def.name.clone(),
        label: def
            .label
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| def.name.clone()),
        model: def.model.clone(),
        daemon_instance_id: daemon_instance_id.to_string(),
        replaces: tddy_discovery::subagent::normalize_replaced_tools(&def.replaces),
        tools: def_tool_names(def),
    })
}

/// One resolved def as the roster entry attaching it produces.
///
/// `replaces` and `tools` are copied in here and never re-read: editing the YAML def or the
/// registry assistant afterwards would otherwise silently change what a running session's main
/// agent is allowed to call (PRD § An entry). Detaching and re-attaching is the explicit way to
/// pick an edit up.
pub(crate) fn roster_record(
    def: &tddy_discovery::agent_def::SpecializedAgentDef,
    daemon_instance_id: &str,
) -> Result<tddy_core::SessionAgentRecord, tddy_core::AgentIdError> {
    Ok(tddy_core::SessionAgentRecord {
        agent_id: qualified_agent_id(&def.name, daemon_instance_id)?,
        name: def.name.clone(),
        daemon_instance_id: daemon_instance_id.to_string(),
        label: def.label.clone().filter(|s| !s.trim().is_empty()),
        model: def.model.clone(),
        replaces: tddy_discovery::subagent::normalize_replaced_tools(&def.replaces),
        tools: def_tool_names(def),
        // A local agent works the facilitating daemon's real worktree, so there is no clone to name.
        codebase_session_id: None,
    })
}

/// The qualified id a def resolved on `daemon_instance_id` is addressed by.
///
/// Refused at the point the id is minted when the def's own name contains `@`: such an id parses
/// back as a different pair, so letting it through would put an entry in the roster that routes
/// somewhere the operator never picked.
pub(crate) fn qualified_agent_id(
    name: &str,
    daemon_instance_id: &str,
) -> Result<String, tddy_core::AgentIdError> {
    tddy_core::AgentId {
        name: name.to_string(),
        daemon_instance_id: daemon_instance_id.to_string(),
    }
    .try_qualified()
}

/// The agent a `StartSessionRequest.specialized_agents` entry names.
///
/// The field keeps its wire shape (`repeated string`) and now carries either form: a qualified
/// `name@daemon_instance_id`, or a bare name. A bare name resolves against *this* daemon, and only
/// here — it is the one place where that reading is not a guess, because a start request has never
/// been able to name any other daemon. Attach takes no such reading (PRD § Identity is qualified,
/// always).
pub(crate) fn started_agent_id(
    reference: &str,
    local_instance_id: &str,
) -> Result<tddy_core::AgentId, Status> {
    match tddy_core::AgentId::parse(reference) {
        Ok(id) => Ok(id),
        Err(tddy_core::AgentIdError::Unqualified(_)) => Ok(tddy_core::AgentId {
            name: reference.to_string(),
            daemon_instance_id: local_instance_id.to_string(),
        }),
        Err(e) => Err(Status::invalid_argument(format!("specialized_agents: {e}"))),
    }
}

/// The `workspace` start a split placement forwards to the daemon holding the codebase.
///
/// Pure, and named rather than inlined at the forward, because this is where every "the operator
/// asked for this, and that host is the one that can do it" decision lands:
///
/// - `session_type` becomes `workspace`, and both placement fields are cleared. The peer runs this
///   locally and holds the codebase for it, so it must not route the request onward — a codebase
///   host of its own would make it split the session again.
/// - `requested_session_id` is the id this daemon minted, so a forward that never answers still
///   leaves a name to tear the peer's worktree down under.
/// - `attachments` stay behind. They are read by the agent and by the browser's Docs listing, both
///   of which act against *this* session on *this* daemon; sending them on would put a second copy
///   on a host with no reader for it, and pay the transfer inside the forward's deadline.
/// - `split_agent` points the workspace session back at the agent. Named here rather than left for
///   the peer to infer: the workspace session persists it, and it is what tells that host — which
///   runs no agent of its own — that a withdrawal attached to this checkout is enforced against an
///   agent somewhere, and where.
/// - `specialized_agents` are **qualified with the agent host's instance id**. The peer reads a bare
///   name as its own daemon's agent, so forwarding `reviewer` verbatim would seed a *different*
///   agent of the same name — the substitution qualified ids exist to prevent — while
///   `reviewer@{agent_instance_id}` keeps meaning the agent the operator picked.
///
/// Everything else rides along, `semantic_index` included: the index is built where the worktree is,
/// and on a split placement that is the host this request is going to.
pub(crate) fn workspace_start_request(
    req: &StartSessionRequest,
    agent_instance_id: &str,
    agent_session_id: &str,
    codebase_session_id: &str,
) -> Result<StartSessionRequest, Status> {
    let specialized_agents = req
        .specialized_agents
        .iter()
        .map(|reference| started_agent_id(reference, agent_instance_id).map(|id| id.qualified()))
        .collect::<Result<Vec<String>, Status>>()?;
    Ok(StartSessionRequest {
        session_type: "workspace".to_string(),
        daemon_instance_id: String::new(),
        codebase_daemon_instance_id: String::new(),
        requested_session_id: codebase_session_id.to_string(),
        attachments: Vec::new(),
        specialized_agents,
        split_agent: Some(SplitAgentPlacement {
            session_id: agent_session_id.to_string(),
            agent_daemon_instance_id: agent_instance_id.to_string(),
        }),
        ..req.clone()
    })
}

/// The revision a freshly started session's roster is at: 1 when it was seeded with agents, 0
/// when it was started with none (PRD § Revision, not diff).
pub(crate) fn started_roster_rev(agents: &[tddy_core::SessionAgentRecord]) -> u64 {
    u64::from(!agents.is_empty())
}

/// The exec tools a roster agent's own loop serves from the checkout it reads.
///
/// Everything else — `Write`, `StrReplace`, `Delete`, `Shell`, `Await` — is proxied to the
/// facilitating daemon, because there is exactly one worktree that counts and it is that daemon's. A
/// mutation applied to a clone would be overwritten by the next sync tick and would never reach the
/// session's branch (docs/ft/daemon/session-agent-roster.md § Reads are local; writes proxy).
///
/// A name outside the catalog is **not** read-only. The split has to fail closed: a tool this list
/// has never heard of is one nobody has decided about, and running it against a mirror is the
/// outcome that loses work silently.
pub(crate) fn agent_tool_reads_the_clone(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "Read" | "Glob" | "Grep" | "SemanticSearch" | "ReadLints"
    )
}

/// One exec-tool result as an agent's managed-dispatch layer reads it.
///
/// A failure is rendered as the `{is_error, error}` envelope
/// [`tddy_discovery::subagent::CodebaseAccess`] surfaces as `Err`, rather than as a result string —
/// returning the error envelope as if it were a successful result is how a model is told a file
/// contains the words "file not found".
pub(crate) fn dispatch_envelope(response: ExecuteToolResponse) -> String {
    match response.is_error {
        true => {
            serde_json::json!({ "is_error": true, "error": response.error_message }).to_string()
        }
        false => response.result_json,
    }
}

/// The wire spelling of a turn's stop reason.
///
/// ACP's spelling, matched character for character, because that is what the main agent's
/// `subagent_prompt` hands back and a consumer comparing against `"EndTurn"` has no way to learn
/// this daemon chose another.
pub(crate) fn agent_stop_reason(reason: tddy_discovery::subagent::StopReason) -> &'static str {
    match reason {
        tddy_discovery::subagent::StopReason::EndTurn => "EndTurn",
        tddy_discovery::subagent::StopReason::MaxTurnRequests => "MaxTurnRequests",
        tddy_discovery::subagent::StopReason::Cancelled => "Cancelled",
    }
}

/// Refuse an attach whose withdrawal the session could not enforce.
///
/// In a managed-codebase session the main agent's file tools **are** `mcp__tddy-tools__*` — the
/// jail is what puts them there — so a withdrawn tool is refused on the path the call already
/// takes. The main agent of a session that runs no jail holds native tools that never reach
/// `tddy-tools`, so accepting the attach would advertise an enforcement that does not exist, and
/// the operator would believe the main agent had been forced through the agent when it had not
/// (PRD § Enforced at two layers, AC24).
///
/// An agent that replaces nothing has nothing to enforce and attaches to either kind of session.
pub(crate) fn refuse_unenforceable_withdrawal(
    session_id: &str,
    codebase: &seed_codebase::SeedCodebase,
    record: &tddy_core::SessionAgentRecord,
) -> Result<(), Status> {
    if record.replaces.is_empty() {
        return Ok(());
    }
    if codebase.enforces_withdrawal {
        return Ok(());
    }
    Err(Status::failed_precondition(format!(
        "agent '{}' replaces {} on session '{session_id}', which does not run a managed codebase: \
         its main agent calls those tools natively, never through tddy-tools, so the withdrawal \
         could not be enforced. Attach it to a managed-codebase session, or attach an agent that \
         replaces nothing.",
        record.agent_id,
        record.replaces.join(", ")
    )))
}

/// Whether a withdrawal attached to this session is actually enforced against its main agent.
///
/// Three shapes qualify, for one reason: the main agent's file tools are `mcp__tddy-tools__*`, so
/// the tool the roster took away is refused on the path the call already takes.
///
/// - **A managed codebase.** The jail is what puts the tools there.
/// - **A split session.** No jail here, but no codebase either: it spawns with every native
///   filesystem tool in `--disallowedTools`
///   ([`crate::split_session::split_claude_extra_args`]), so the proxy is the only route it has.
/// - **A `workspace` session an agent is paired with.** The codebase half of a split session, which
///   is where that session's roster lives and where its attaches are routed — so this is the
///   metadata the refusal above actually reads for a split session. It runs no agent loop of its
///   own: the withdrawal is enforced by the agent host, and a split placement is only ever granted
///   to a `claude-cli` session (`classify_codebase_placement`), which is the shape above.
///
/// The pairing is the whole of what that last arm turns on, not the session type. A `workspace`
/// session is also what an operator's standalone checkout is, and what an agent clone's mirror is —
/// neither has an agent anywhere whose tools could be taken away, so accepting a withdrawal on one
/// would report an enforcement no process performs, which is the exact failure this refusal exists
/// to prevent. Only a split placement records the back-pointer
/// ([`tddy_core::paired_agent`]), so only the half that has an agent qualifies.
pub(crate) fn session_enforces_a_withdrawal(meta: &tddy_core::SessionMetadata) -> bool {
    meta.sandbox == Some(true)
        || crate::split_session::split_pairing(meta).is_some()
        || tddy_core::paired_agent(meta).is_some()
}

/// The qualified ids a persisted roster holds, for the resume paths that re-resolve each agent
/// before relaunching the jail.
///
/// Qualified rather than bare: a resume that resolved `explorer` locally would run *this* daemon's
/// `explorer` for an entry the operator attached from another host, and report it under the id they
/// picked. An id naming a peer is refused by resolution instead.
pub(crate) fn roster_agent_ids(agents: &[tddy_core::SessionAgentRecord]) -> Vec<String> {
    agents.iter().map(|a| a.agent_id.clone()).collect()
}
