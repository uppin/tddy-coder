use tddy_service::proto::exec_tools::ExecuteToolResponse;

use tddy_service::proto::session::SplitAgentPlacement;

use tddy_service::proto::session::StartSessionRequest;

use tddy_rpc::Status;

use crate::connection_service::seed_codebase;

use std::time::Duration;
use tddy_session_agents::agent_records;
pub use tddy_session_agents::agent_records::*;

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
        .map(|reference| {
            agent_records::started_agent_id(reference, agent_instance_id).map(|id| id.qualified())
        })
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
    tddy_subagent_worktree::ToolEffect::of(tool_name)
        == tddy_subagent_worktree::ToolEffect::ReadOnly
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
        || crate::connection_service::peer_session_answer::split_pairing(meta).is_some()
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

/// How long to wait for the codebase daemon's answer to a split session's forwarded start.
///
/// Not the ordinary forward deadline (`peer_forward_timeout_secs`): the peer serves this call by
/// resolving the project — cloning it if it does not have it yet — and cutting a worktree, work it
/// bounds by its own `spawn_worker_request_timeout` (5 minutes by default). Giving up after that
/// would mean erroring while the peer is still building, which is the state that used to strand a
/// worktree. This daemon can only assume the peer's budget matches its own, so it waits that budget
/// out plus one ordinary forward deadline of round-trip headroom. A peer configured with a *larger*
/// budget still times out here — the teardown at the call site is what keeps that from becoming
/// an orphan.
///
/// The cost is that a peer whose RPC participant is gone surfaces after this wait rather than
/// after the forward deadline. Accepted: the placement check already required the peer to be
/// visible in the common room moments earlier, so that is the rarer failure, and the alternative
/// trades a rare slow error for a routine orphaned worktree.
pub fn split_forward_deadline(config: &crate::config::DaemonConfig) -> Duration {
    config.spawn_worker_request_timeout() + config.peer_forward_timeout()
}
