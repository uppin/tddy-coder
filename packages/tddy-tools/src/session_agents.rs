//! The in-jail registry that follows the session's live agent roster.
//!
//! See docs/ft/daemon/session-agent-roster.md § Invoking an agent and § The roster stream.
//!
//! `tddy-tools --mcp` used to build its subagent registry from `TDDY_SUBAGENTS_JSON` — an env var
//! fixed when the jail was spawned. An agent attached at minute forty was therefore uncallable
//! until the session restarted, and one detached stayed callable forever. The registry now follows
//! `StreamSessionAgents`, and the env var is demoted to the **seed** that covers the window between
//! spawn and the first frame.
//!
//! Two properties are what this module exists for, and both are about *silent* disagreement rather
//! than staleness:
//!
//! - a frame **replaces** the registry rather than merging into it, so an agent the daemon stopped
//!   listing stops being callable here too;
//! - a registry that can no longer be kept current **refuses**, because one frozen at its last
//!   known roster answers for detached agents and refuses attached ones without saying so.
//!
//! With one bound on the second: a withdrawal never outlives the reachability of its replacement.
//! A roster that was current and went stale still withdraws the tools its agents took over — those
//! agents were real — but one that never received a frame at all withdraws nothing, because there
//! it would take a tool away and offer nothing in its place ([`RosterCurrency`]).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use prost::Message;
use tddy_core::session_agent::AgentId;
use tddy_discovery::agent_def::SpecializedAgentDef;
use tddy_discovery::subagent::normalize_replaced_tools;
use tddy_service::proto::connection::{
    AgentCloneState, SessionAgentEntry, SessionAgentRoster, StreamSessionAgentsRequest,
};

use crate::session_tool_client::{
    detect_session_tool_transport, SessionToolEnvelope, SessionToolTransport,
};

/// Whether a conversation opened with a roster agent may still be prompted.
///
/// Two states, because that is the whole of what a caller can do about it: prompt, or report why it
/// cannot. A conversation whose agent was detached is cancelled rather than left open — an
/// in-flight `subagent_prompt` that never returns is worse than one that errors, since the main
/// agent waits on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConversationState {
    Open,
    Cancelled { reason: String },
}

/// Why the roster refused a call.
///
/// Every variant names the thing the caller must change — the agent id it asked for, the ids there
/// are, the reason the roster went dark, or the agent that now serves a tool. The main agent reads
/// these verbatim in a tool result, so "unknown agent" without the id is unactionable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RosterError {
    /// The roster can no longer be kept current, so nothing it holds may be answered from.
    Unavailable { reason: String },
    /// A conversation naming no agent. There is no default: with an unbounded roster, picking the
    /// first entry would make the main agent's choice depend on attach order, which it cannot see.
    NoAgentNamed { attached: Vec<String> },
    /// An id no entry in the current roster bears — never attached, or detached since.
    NotAttached { agent_id: String },
    /// A tool the roster's replaced union withdrew from the main agent.
    ToolWithdrawn { tool: String, agent_id: String },
}

impl std::fmt::Display for RosterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable { reason } => write!(
                f,
                "the session's agent roster cannot be kept current ({reason}), so no agent can be \
                 addressed — a registry serving its last known roster would answer for agents that \
                 have been detached"
            ),
            Self::NoAgentNamed { attached } if attached.is_empty() => write!(
                f,
                "'agent' is required and names the agent to open a conversation with; \
                 no agents are attached to this session"
            ),
            Self::NoAgentNamed { attached } => write!(
                f,
                "'agent' is required and names the agent to open a conversation with. \
                 Attached agents: [{}]",
                attached.join(", ")
            ),
            Self::NotAttached { agent_id } => {
                write!(f, "agent '{agent_id}' is not attached to this session")
            }
            Self::ToolWithdrawn { tool, agent_id } => write!(
                f,
                "{tool} is withdrawn from this session — the tool is served by agent \
                 \"{agent_id}\". Call subagent_new_session {{ agent: \"{agent_id}\" }} and prompt \
                 it instead."
            ),
        }
    }
}

impl std::error::Error for RosterError {}

/// One open conversation, as the roster tracks it: which agent it addresses, and whether that agent
/// is still attached.
struct Conversation {
    agent_id: String,
    state: ConversationState,
}

/// How current this process's view of the roster is.
///
/// One value rather than a "have I ever applied a frame" revision plus an "am I broken" flag,
/// because the two combinations that look alike mean opposite things, and the difference decides
/// whether a withdrawal may still be enforced: a roster that **was** current and went stale lists
/// agents that demonstrably existed, while one that never received a frame lists only what the
/// spawn env claimed and can address none of it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum RosterCurrency {
    /// Only the spawn seed is in force: no frame has arrived yet, and the stream has not given up.
    /// The state `tddy-sandbox-app` — which has no daemon in the loop — spends its whole life in.
    Seeded,
    /// A frame at this revision is in force and the stream is following it.
    Current { rev: u64 },
    /// A frame at this revision was in force, and the stream that kept it current has since died.
    Stale { rev: u64, reason: String },
    /// The stream gave up before delivering any frame. The seed is all this process has, and it
    /// cannot address a single agent in it.
    Unreachable { reason: String },
}

impl RosterCurrency {
    /// The revision in force, when a frame ever arrived.
    fn applied_rev(&self) -> Option<u64> {
        match self {
            Self::Current { rev } | Self::Stale { rev, .. } => Some(*rev),
            Self::Seeded | Self::Unreachable { .. } => None,
        }
    }

    /// Why no agent may be addressed, when none may be.
    fn refusal(&self) -> Option<&str> {
        match self {
            Self::Stale { reason, .. } | Self::Unreachable { reason } => Some(reason),
            Self::Seeded | Self::Current { .. } => None,
        }
    }

    /// Whether a `replaces` entry this state holds may still withdraw a tool from the main agent.
    ///
    /// False in exactly one state: withdrawal must not outlive the reachability of its
    /// replacement. When no frame ever arrived, every agent the seed names is refused by
    /// [`LiveAgentRoster::resolve`], so enforcing its `replaces` union would leave the session
    /// without the tool *and* without the agent that took it over — no search capability at all,
    /// and no recovery short of a restart. A roster that was current and went stale enforces:
    /// those agents were real, and handing back access an operator withdrew is the worse of the
    /// two ways to be wrong.
    fn enforces_withdrawal(&self) -> bool {
        !matches!(self, Self::Unreachable { .. })
    }
}

/// Everything a frame replaces, plus what survives one.
struct RosterState {
    /// The current roster, in attach order. Replaced wholesale by every applied frame.
    entries: Vec<SessionAgentEntry>,
    /// The defs the jail was spawned with, by the qualified id they were seeded under.
    ///
    /// Not membership — [`Self::entries`] is the only answer to "which agents are attached". This
    /// is the *material* for running a local agent's turn loop in-process: a roster entry carries
    /// no endpoint, model credential or turn budget, so an agent this process is expected to run
    /// itself can only be run when its def is here. Survives a frame for the same reason a recipe
    /// survives a menu change.
    ///
    /// Keyed by the whole id rather than the bare name: two daemons may each contribute a def
    /// called `explorer`, and a name key would hand a newly attached agent the seed's endpoint,
    /// model and credential.
    seed_defs: HashMap<String, SpecializedAgentDef>,
    /// How current the view above is — and therefore what may be answered from it.
    currency: RosterCurrency,
    conversations: HashMap<String, Conversation>,
    tool_list_changes: u64,
}

/// The session's agent roster as this process sees it: seeded at spawn, replaced by every frame the
/// daemon publishes.
///
/// Every method takes `&self`: the registry is read from MCP tool handlers and written by the
/// roster stream task, concurrently, for the process lifetime.
pub struct LiveAgentRoster {
    session_id: String,
    state: Mutex<RosterState>,
}

impl LiveAgentRoster {
    /// The registry the jail starts with: the defs `TDDY_SUBAGENTS_JSON` carried, addressed under
    /// `local_daemon_instance_id`.
    ///
    /// The seed is what makes the roster usable between spawn and the first frame, and it is what
    /// `tddy-sandbox-app` — which has no daemon in the loop at all — runs on for its whole life. It
    /// is never merged with a frame.
    ///
    /// An empty `local_daemon_instance_id` leaves the seeded ids **bare**: a caller that cannot say
    /// which daemon resolved these defs must not invent a daemon part, because an id that names the
    /// wrong host is worse than one that names none. The first frame replaces them with the
    /// qualified ids the daemon minted.
    pub fn seeded_from(
        session_id: &str,
        seed_defs: Vec<SpecializedAgentDef>,
        local_daemon_instance_id: &str,
    ) -> Self {
        let mut entries = Vec::new();
        let mut defs = HashMap::new();
        for def in seed_defs {
            let Some(agent_id) = seed_agent_id(&def.name, local_daemon_instance_id) else {
                log::warn!(
                    target: "tddy_tools::session_agents",
                    "seeded agent def '{}' has no addressable id and is not attached",
                    def.name
                );
                continue;
            };
            entries.push(seed_entry(&agent_id, local_daemon_instance_id, &def));
            defs.insert(agent_id, def);
        }
        Self {
            session_id: session_id.to_string(),
            state: Mutex::new(RosterState {
                entries,
                seed_defs: defs,
                currency: RosterCurrency::Seeded,
                conversations: HashMap::new(),
                tool_list_changes: 0,
            }),
        }
    }

    /// The session this roster belongs to — the id every `StreamSessionAgents` request carries.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Replace the registry with `roster`, cancelling every conversation whose agent it no longer
    /// lists.
    ///
    /// Wholesale, never merged: a merge would keep a seeded agent alive after the daemon stopped
    /// listing it, which is exactly the silent disagreement the frozen env var caused.
    ///
    /// A frame older than the one already applied is ignored. Frames are whole snapshots, so
    /// applying a stale one would move the registry backwards — and re-delivery is what a reconnect
    /// does. A frame at the revision already applied carries the same snapshot by construction, so
    /// it changes nothing and announces nothing.
    ///
    /// **Any** decoded frame restores currency, including one whose revision is not applied: a
    /// frame arriving is the proof that the stream is being served, and leaving the registry
    /// refusing while frames keep arriving is a stream that reads as healthy to the follower and
    /// dead to every caller.
    pub fn apply_snapshot(&self, roster: SessionAgentRoster) {
        let mut state = self.state.lock().expect("session agent roster");
        if let Some(applied) = state.currency.applied_rev() {
            if roster.rev < applied {
                log::warn!(
                    target: "tddy_tools::session_agents",
                    "roster rev {} for session {} is older than the rev {applied} already in \
                     force; keeping {applied}",
                    roster.rev,
                    self.session_id
                );
                state.currency = RosterCurrency::Current { rev: applied };
                return;
            }
            if roster.rev == applied {
                state.currency = RosterCurrency::Current { rev: applied };
                return;
            }
        }
        state.currency = RosterCurrency::Current { rev: roster.rev };
        state.entries = roster.agents;
        state.tool_list_changes += 1;

        let attached: Vec<String> = state
            .entries
            .iter()
            .map(|entry| entry.agent_id.clone())
            .collect();
        for conversation in state.conversations.values_mut() {
            if conversation.state != ConversationState::Open
                || attached.contains(&conversation.agent_id)
            {
                continue;
            }
            conversation.state = ConversationState::Cancelled {
                reason: format!(
                    "agent {} was detached from this session",
                    conversation.agent_id
                ),
            };
        }
    }

    /// Record that the roster can no longer be kept current, and refuse every later resolution
    /// naming `reason`.
    ///
    /// Deliberately not "keep serving what we have": a registry frozen at its last known roster
    /// answers `subagent_new_session` for an agent that was detached and refuses one that was
    /// attached, and does both silently. The state is not terminal — the next frame clears it, so a
    /// reconnect that succeeds restores service.
    ///
    /// Which unavailable state this is depends on whether a frame ever arrived, and the two are not
    /// interchangeable — see [`RosterCurrency::enforces_withdrawal`].
    pub fn mark_unavailable(&self, reason: &str) {
        let mut state = self.state.lock().expect("session agent roster");
        state.currency = match state.currency.applied_rev() {
            Some(rev) => RosterCurrency::Stale {
                rev,
                reason: reason.to_string(),
            },
            None => RosterCurrency::Unreachable {
                reason: reason.to_string(),
            },
        };
    }

    /// Whether the session has no agent to address right now.
    ///
    /// What decides whether the conversation tools are advertised at all: four tools that can only
    /// ever answer "no agents are attached" are noise in the main agent's tool list, and the tool
    /// list is re-listed on every revision, so this is a live answer rather than a spawn-time one.
    pub fn is_empty(&self) -> bool {
        self.state
            .lock()
            .expect("session agent roster")
            .entries
            .is_empty()
    }

    /// The roster entry `agent_id` names.
    ///
    /// `None` is refused listing the ids there are: with an unbounded roster there is no defensible
    /// default, and picking the first entry would make the main agent's choice depend on attach
    /// order.
    pub fn resolve(&self, agent_id: Option<&str>) -> Result<SessionAgentEntry, RosterError> {
        let state = self.state.lock().expect("session agent roster");
        Self::resolve_in(&state, agent_id)
    }

    /// Open a conversation with `agent_id` under an id this process mints.
    pub fn open_conversation(&self, agent_id: &str) -> Result<String, RosterError> {
        let conversation_id = uuid::Uuid::new_v4().to_string();
        self.open_conversation_as(&conversation_id, agent_id)?;
        Ok(conversation_id)
    }

    /// Open a conversation with `agent_id` under a **caller-chosen** id, returning the entry the id
    /// resolved to so the caller can build the session it needs.
    ///
    /// The main agent decides the conversation id (`subagent_new_session { sessionId }`), so the
    /// generating variant above is the special case rather than this one.
    pub fn open_conversation_as(
        &self,
        conversation_id: &str,
        agent_id: &str,
    ) -> Result<SessionAgentEntry, RosterError> {
        let mut state = self.state.lock().expect("session agent roster");
        let entry = Self::resolve_in(&state, Some(agent_id))?;
        state.conversations.insert(
            conversation_id.to_string(),
            Conversation {
                agent_id: entry.agent_id.clone(),
                state: ConversationState::Open,
            },
        );
        Ok(entry)
    }

    /// Whether `conversation_id` may still be prompted.
    ///
    /// A conversation this roster never opened reports as cancelled rather than open: the caller's
    /// next move on either answer is the same — say why it cannot be prompted — and reporting an
    /// unknown id as open would send a prompt to nothing.
    pub fn conversation_state(&self, conversation_id: &str) -> ConversationState {
        let state = self.state.lock().expect("session agent roster");
        match state.conversations.get(conversation_id) {
            Some(conversation) => conversation.state.clone(),
            None => ConversationState::Cancelled {
                reason: format!("conversation {conversation_id} is not open"),
            },
        }
    }

    /// Forget `conversation_id`, returning whether it was open. Called when the main agent cancels.
    pub fn close_conversation(&self, conversation_id: &str) -> bool {
        let mut state = self.state.lock().expect("session agent roster");
        state.conversations.remove(conversation_id).is_some()
    }

    /// How many MCP `notifications/tools/list_changed` this roster has earned: one per applied
    /// revision.
    ///
    /// One per revision, not one per entry — the main agent re-lists once and sees the whole new
    /// set. A reconnect that re-delivers the revision already applied announces nothing, so a flaky
    /// stream cannot spam the main agent.
    pub fn tool_list_change_count(&self) -> u64 {
        self.state
            .lock()
            .expect("session agent roster")
            .tool_list_changes
    }

    /// Every exec tool the roster has taken away from the main agent right now, mapped to the agent
    /// that serves it instead.
    ///
    /// The single source of the withdrawal decision: the catalog the server advertises and the check
    /// a call goes through are read at different moments, and a disagreement between them is
    /// invisible from either side — a tool advertised and then refused looks broken, and one withheld
    /// but callable is a withdrawal that only appears to have happened. Both derive from here, so the
    /// two cannot drift and the [`RosterCurrency`] gate governs both
    /// (docs/ft/daemon/session-agent-roster.md § Tool replacement, without behaviour).
    ///
    /// Empty while no frame ever arrived and the stream has given up
    /// ([`RosterCurrency::enforces_withdrawal`]): there the replacement is unreachable too, so
    /// withdrawing would take the tool away and offer nothing in its place.
    ///
    /// The agent named for a tool is the **first** attached one to claim it, so two agents replacing
    /// the same tool send the main agent to the one attached earliest rather than to whichever the
    /// iteration order happened to reach.
    pub fn withdrawn_exec_tools(&self) -> HashMap<String, String> {
        let state = self.state.lock().expect("session agent roster");
        if !state.currency.enforces_withdrawal() {
            return HashMap::new();
        }
        let mut withdrawn: HashMap<String, String> = HashMap::new();
        for entry in &state.entries {
            for tool in normalize_replaced_tools(&entry.replaces) {
                withdrawn
                    .entry(tool)
                    .or_insert_with(|| entry.agent_id.clone());
            }
        }
        withdrawn
    }

    /// Refuse `tool` when an attached agent has taken it over from the main agent.
    ///
    /// This is what makes live attach enforceable at all: `--allowedTools` is fixed when `claude`
    /// spawns, so an agent attached afterwards can only withdraw a tool by having the call refused
    /// where it is made. In a managed-codebase session the main agent's file tools *are*
    /// `mcp__tddy-tools__*`, so this is the path the call already takes.
    ///
    /// Enforced from the last known roster even while a roster that **was** current has gone
    /// [stale](Self::mark_unavailable): the two ways to be wrong are not symmetric. Withholding a
    /// tool a detach may since have restored costs the main agent one refusal it can report and
    /// work around; running one an attached agent has taken over hands back exactly the access the
    /// operator withdrew.
    ///
    /// Not enforced when no frame ever arrived and the stream has given up
    /// ([`RosterCurrency::Unreachable`]): there the replacement is unreachable too, so enforcing
    /// would take the tool away and offer nothing in its place.
    ///
    /// Decided by [`Self::withdrawn_exec_tools`] rather than by walking the entries again, so the
    /// tool list the server advertises and the refusal a call meets are the same decision.
    pub fn check_tool_available(&self, tool: &str) -> Result<(), RosterError> {
        let Some(canonical) = normalize_replaced_tools(&[tool.to_string()]).pop() else {
            // Not an exec-catalog name, so no `replaces` list can name it.
            return Ok(());
        };
        match self.withdrawn_exec_tools().remove(&canonical) {
            Some(agent_id) => Err(RosterError::ToolWithdrawn {
                tool: canonical,
                agent_id,
            }),
            None => Ok(()),
        }
    }

    /// The def this process can run `entry`'s turn loop from, when it has one.
    ///
    /// `None` for an agent owned by another daemon (its loop runs there, by construction) and for a
    /// local agent attached after spawn, whose def only its owning daemon holds — a roster entry
    /// carries no endpoint or credential, deliberately, so there is nothing here to run it from.
    ///
    /// Matched on the whole `agent_id`, not the bare name: an agent attached after spawn that
    /// happens to share a seeded agent's name is a *different* agent, and answering it with the
    /// seed's endpoint, model and credential would run someone else's def under its id.
    pub fn local_def_for(&self, entry: &SessionAgentEntry) -> Option<SpecializedAgentDef> {
        if entry.clone_state != AgentCloneState::Local as i32 {
            return None;
        }
        let state = self.state.lock().expect("session agent roster");
        state.seed_defs.get(&entry.agent_id).cloned()
    }

    /// Shared by [`Self::resolve`] and [`Self::open_conversation_as`] so the two cannot disagree
    /// about what is addressable — an open that resolved differently from a resolve would be the
    /// silent disagreement this module exists to prevent, one layer in.
    fn resolve_in(
        state: &RosterState,
        agent_id: Option<&str>,
    ) -> Result<SessionAgentEntry, RosterError> {
        if let Some(reason) = state.currency.refusal() {
            return Err(RosterError::Unavailable {
                reason: reason.to_string(),
            });
        }
        let Some(agent_id) = agent_id.filter(|id| !id.is_empty()) else {
            return Err(RosterError::NoAgentNamed {
                attached: state
                    .entries
                    .iter()
                    .map(|entry| entry.agent_id.clone())
                    .collect(),
            });
        };
        state
            .entries
            .iter()
            .find(|entry| entry.agent_id == agent_id)
            .cloned()
            .ok_or_else(|| RosterError::NotAttached {
                agent_id: agent_id.to_string(),
            })
    }
}

/// The id a seeded def is addressed by: qualified when the daemon that resolved it is known, bare
/// when it is not (see [`LiveAgentRoster::seeded_from`]). `None` for a name that cannot produce an
/// id parsing back to itself.
fn seed_agent_id(name: &str, local_daemon_instance_id: &str) -> Option<String> {
    if local_daemon_instance_id.is_empty() {
        return (!name.is_empty()).then(|| name.to_string());
    }
    AgentId {
        name: name.to_string(),
        daemon_instance_id: local_daemon_instance_id.to_string(),
    }
    .try_qualified()
    .ok()
}

/// A seeded def as the roster entry it stands in for.
///
/// `clone_state` is `LOCAL`: the seed came from the facilitating daemon's own def sources, so there
/// is no clone and nothing to wait for.
fn seed_entry(
    agent_id: &str,
    local_daemon_instance_id: &str,
    def: &SpecializedAgentDef,
) -> SessionAgentEntry {
    SessionAgentEntry {
        agent_id: agent_id.to_string(),
        name: def.name.clone(),
        daemon_instance_id: local_daemon_instance_id.to_string(),
        label: def.label.clone().unwrap_or_default(),
        model: def.model.clone(),
        replaces: def.replaces.clone(),
        tools: def
            .tools
            .iter()
            .map(|tool| tool.catalog_name().to_string())
            .collect(),
        codebase_session_id: String::new(),
        clone_state: AgentCloneState::Local as i32,
        clone_error: String::new(),
    }
}

// --- The roster stream ---------------------------------------------------------------------

/// The session's roster for this process.
///
/// Process-wide because the registry outlives any one MCP `tools/call`: the stream task writes it
/// and every tool handler reads it, and a per-request registry is exactly the thing that used to
/// re-read a frozen env var on every call.
pub fn session_agent_roster() -> &'static LiveAgentRoster {
    static ROSTER: OnceLock<LiveAgentRoster> = OnceLock::new();
    ROSTER.get_or_init(|| {
        let transport = detect_session_tool_transport();
        LiveAgentRoster::seeded_from(
            &seed_session_id(transport.as_ref()),
            crate::server::seed_subagents_or_report(),
            &seed_daemon_instance_id(transport.as_ref()),
        )
    })
}

/// The session the roster belongs to, as the spawn environment names it.
fn seed_session_id(transport: Option<&SessionToolTransport>) -> String {
    match transport {
        Some(SessionToolTransport::DaemonHttp { session_id, .. })
        | Some(SessionToolTransport::LiveKit { session_id, .. }) => session_id.clone(),
        // The sandbox socket implies the session by the connection itself; the jail is still told
        // which session it is, for everything that has to name one.
        _ => crate::session_tool_client::session_id_from_env(),
    }
}

/// The daemon that resolved the seeded defs, when the jail was told which one that is.
///
/// TODO(session-agent-roster): a sandbox-IPC jail is told nothing about its facilitating daemon's
/// instance id, so its seeded ids stay bare until the first frame replaces them with qualified
/// ones. Exporting the id at spawn is a `tddy-daemon` change and belongs with the roster's
/// daemon-side tranche.
fn seed_daemon_instance_id(transport: Option<&SessionToolTransport>) -> String {
    match transport {
        Some(SessionToolTransport::DaemonHttp {
            daemon_instance_id, ..
        })
        | Some(SessionToolTransport::LiveKit {
            daemon_instance_id, ..
        }) => daemon_instance_id.clone(),
        _ => String::new(),
    }
}

/// How long opening the stream may take before the connection is treated as dead.
const STREAM_OPEN_TIMEOUT: Duration = Duration::from_secs(10);

/// How long to wait for the snapshot every subscribe begins with.
///
/// `StreamSessionAgents` emits the current roster immediately, so a subscribe that produces nothing
/// in this window is a connection nobody is serving. It is the only deadline the stream carries:
/// once a snapshot has arrived, silence is what a roster nobody is changing looks like, and an idle
/// deadline would tear down a perfectly good subscription every few minutes.
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(15);

/// The delay before the first reconnect, doubled per consecutive failure.
const RECONNECT_BACKOFF_START: Duration = Duration::from_millis(500);

/// The longest the reconnect loop waits between attempts. It never stops trying: an unavailable
/// roster is a state, not a terminal one, and a daemon that comes back must restore service without
/// the session being restarted.
const RECONNECT_BACKOFF_CEILING: Duration = Duration::from_secs(30);

/// Consecutive failures before the roster is declared unavailable and subagent calls are refused.
///
/// More than one, because a daemon restart drops the stream and is recovered from in well under a
/// second; few, because every attempt after the first is time the registry spends possibly
/// disagreeing with the daemon.
const RECONNECT_ATTEMPTS_BEFORE_GIVING_UP: u32 = 3;

/// What one pass of the roster stream achieved, and how it ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RosterStreamOutcome {
    /// Snapshots applied before the pass ended — keepalives, which re-deliver the applied revision,
    /// included.
    applied: u64,
    /// The error the pass ended with, when it did not end with the far end closing cleanly.
    failure: Option<String>,
}

impl RosterStreamOutcome {
    /// A pass the far end closed after `applied` snapshots.
    pub fn closed(applied: u64) -> Self {
        Self {
            applied,
            failure: None,
        }
    }

    /// A pass that ended in an error after `applied` snapshots.
    pub fn broke(applied: u64, failure: String) -> Self {
        Self {
            applied,
            failure: Some(failure),
        }
    }

    /// Snapshots applied before the pass ended.
    pub fn applied(&self) -> u64 {
        self.applied
    }

    /// Whether this pass counts against [`RECONNECT_ATTEMPTS_BEFORE_GIVING_UP`].
    ///
    /// A pass that applied a snapshot proved the subscription was being served, so however it ended
    /// it is a reconnect — a daemon restart, a relay giving up on a stream that went quiet — and not
    /// the broken setup the budget exists to declare. Only a pass that produced nothing counts, and
    /// how it ended makes no difference to that: counting an error against the budget would let
    /// three passes that each delivered a good roster add up to a registry that refuses every
    /// subagent call.
    pub fn counts_as_a_failure(&self) -> bool {
        self.applied == 0
    }

    /// Why the pass ended, as the refusal spells it.
    pub fn reason(&self) -> String {
        match &self.failure {
            Some(failure) => failure.clone(),
            None if self.applied == 0 => "the stream ended before any snapshot arrived".to_string(),
            None => "the stream ended".to_string(),
        }
    }
}

/// Follow the session's roster for the process lifetime, calling `on_change` once per applied
/// revision.
///
/// `on_change` is what emits the MCP `notifications/tools/list_changed`: this module holds no MCP
/// peer, and the roster's business is the roster.
pub fn follow_session_agent_roster(on_change: impl Fn() + Send + Sync + 'static) {
    let roster = session_agent_roster();
    let Some(transport) = detect_session_tool_transport() else {
        // No daemon in the loop at all — `tddy-sandbox-app`'s case. There is no roster to follow and
        // nothing that can go stale: the spawn seed is the whole roster, for the whole run.
        log::debug!(
            target: "tddy_tools::session_agents",
            "no session-tool transport is configured; the spawn seed is this session's whole roster"
        );
        return;
    };
    match &transport {
        // The sandbox tool-IPC socket now bridges `StreamSessionAgents` (and the conversation RPCs)
        // to the facilitating daemon over the `SessionChannel` — see
        // `tddy_sandbox_runner::ToolExecService` and `run_host_relay_with_rpc`. The subscription
        // proceeds exactly as it does over LiveKit: a fresh connection per stream, the first
        // frame replaces the seed, reconnect-on-drop with backoff. A daemon that does not serve
        // the RPC (the standalone app, via `NullRpcHandler`) refuses it, the roster goes
        // `Unavailable`, and `subagent_*` calls are refused — the safe behaviour for a session
        // with no daemon in the loop.
        SessionToolTransport::SandboxIpc { .. } | SessionToolTransport::LiveKit { .. } => {}
        // Refused rather than left on the seed: a registry frozen at spawn answers for agents that
        // have since been detached and refuses ones that have been attached, and says nothing.
        // TODO(session-agent-roster): give the HTTP transport a `StreamSessionAgents` client (or a
        // `ListSessionAgents` poll) so a daemon-HTTP session can address agents at all.
        SessionToolTransport::DaemonHttp { .. } => {
            let reason = "the roster stream has no client for the daemon-HTTP transport";
            log::error!(target: "tddy_tools::session_agents", "{reason}; subagent calls are refused");
            roster.mark_unavailable(reason);
            return;
        }
        SessionToolTransport::IncompleteLiveKit { missing } => {
            let reason = format!(
                "a LiveKit environment is set but {} is empty or unset",
                missing.join(", ")
            );
            log::error!(target: "tddy_tools::session_agents", "{reason}; subagent calls are refused");
            roster.mark_unavailable(&reason);
            return;
        }
    }
    tokio::spawn(async move { follow_roster(transport, roster, on_change).await });
}

/// Hold the stream open for the process lifetime, reconnecting when it drops.
async fn follow_roster(
    transport: SessionToolTransport,
    roster: &'static LiveAgentRoster,
    on_change: impl Fn() + Send + Sync,
) {
    let mut consecutive_failures: u32 = 0;
    loop {
        let pass = stream_roster_once(&transport, roster, &on_change).await;
        let last_failure = pass.reason();
        if pass.counts_as_a_failure() {
            consecutive_failures += 1;
            log::warn!(
                target: "tddy_tools::session_agents",
                "roster stream for session {}: {last_failure}",
                roster.session_id()
            );
        } else {
            consecutive_failures = 0;
            log::warn!(
                target: "tddy_tools::session_agents",
                "roster stream for session {} ended after {} snapshot(s) ({last_failure}); \
                 reconnecting",
                roster.session_id(),
                pass.applied()
            );
        }
        if consecutive_failures >= RECONNECT_ATTEMPTS_BEFORE_GIVING_UP {
            // The last failure is carried into the refusal, not just the log: the main agent reads
            // the refusal and an operator reads it in the transcript, and "the roster went away" is
            // not diagnosable without the reason the connection gave.
            let reason = format!(
                "roster stream closed after {consecutive_failures} reconnect attempts \
                 ({last_failure})"
            );
            log::error!(
                target: "tddy_tools::session_agents",
                "{reason}; every subagent call for session {} is refused until it recovers",
                roster.session_id()
            );
            roster.mark_unavailable(&reason);
        }
        tokio::time::sleep(reconnect_backoff(consecutive_failures)).await;
    }
}

/// Exponential backoff from [`RECONNECT_BACKOFF_START`], capped at [`RECONNECT_BACKOFF_CEILING`].
fn reconnect_backoff(consecutive_failures: u32) -> Duration {
    RECONNECT_BACKOFF_START
        .saturating_mul(2u32.saturating_pow(consecutive_failures.min(8)))
        .min(RECONNECT_BACKOFF_CEILING)
}

/// Open the stream once and apply every snapshot it delivers, reporting what the pass achieved and
/// how it ended.
///
/// The applied count is carried out of *every* ending, the error ones included: a stream that served
/// a roster and then broke is a reconnect, and [`RosterStreamOutcome::counts_as_a_failure`] can only
/// tell that from a setup nothing is serving if it is told how much arrived.
async fn stream_roster_once(
    transport: &SessionToolTransport,
    roster: &LiveAgentRoster,
    on_change: &(impl Fn() + Send + Sync),
) -> RosterStreamOutcome {
    // The client is held for as long as the frames are read: it owns the connection they ride, and
    // dropping it would close the subscription it just opened.
    let (_client, mut frames) = match open_roster_stream(transport).await {
        Ok(opened) => opened,
        // Nothing was opened, so nothing was applied.
        Err(failure) => return RosterStreamOutcome::broke(0, failure),
    };

    let mut applied: u64 = 0;
    loop {
        // Only the first frame is waited for with a deadline: the daemon sends the current snapshot
        // on subscribe, so nothing by now means nothing is serving this subscription. After that a
        // silent stream is a roster nobody is changing — and one the daemon keeps alive by re-sending
        // the applied revision, so silence for long enough is the connection, not the roster.
        let frame = if applied == 0 {
            match tokio::time::timeout(FIRST_FRAME_TIMEOUT, frames.recv()).await {
                Ok(frame) => frame,
                Err(_) => {
                    return RosterStreamOutcome::broke(
                        applied,
                        format!(
                            "no roster snapshot within {}s of subscribing",
                            FIRST_FRAME_TIMEOUT.as_secs()
                        ),
                    )
                }
            }
        } else {
            frames.recv().await
        };
        let Some(frame) = frame else {
            return RosterStreamOutcome::closed(applied);
        };
        let bytes = match frame {
            Ok(bytes) => bytes,
            Err(e) => return RosterStreamOutcome::broke(applied, format!("roster stream: {e}")),
        };
        let snapshot = match SessionAgentRoster::decode(bytes.as_slice()) {
            Ok(snapshot) => snapshot,
            Err(e) => {
                return RosterStreamOutcome::broke(
                    applied,
                    format!("undecodable roster frame ({} bytes): {e}", bytes.len()),
                )
            }
        };
        let rev = snapshot.rev;
        let announced_before = roster.tool_list_change_count();
        roster.apply_snapshot(snapshot);
        applied += 1;
        // One notification per *applied* revision, so a re-delivered snapshot — a keepalive, or a
        // reconnect's opening frame — does not make the main agent re-list for nothing.
        if roster.tool_list_change_count() != announced_before {
            log::debug!(
                target: "tddy_tools::session_agents",
                "roster rev {rev} applied for session {}",
                roster.session_id()
            );
            on_change();
        }
    }
}

/// Open one `StreamSessionAgents` subscription over `transport`, returning the connection it rides
/// alongside its frames.
#[allow(clippy::type_complexity)]
async fn open_roster_stream(
    transport: &SessionToolTransport,
) -> Result<
    (
        std::sync::Arc<dyn tddy_rpc::RpcClientTransport>,
        tokio::sync::mpsc::Receiver<Result<Vec<u8>, tddy_rpc::Status>>,
    ),
    String,
> {
    let (client, envelope) = connect_roster_stream(transport).await?;
    let request = StreamSessionAgentsRequest {
        session_token: envelope.session_token,
        session_id: envelope.session_id,
        daemon_instance_id: envelope.daemon_instance_id,
    };
    let call = client.call_server_stream(
        "connection.ConnectionService",
        "StreamSessionAgents",
        request.encode_to_vec(),
    );
    let frames = tokio::time::timeout(STREAM_OPEN_TIMEOUT, call)
        .await
        .map_err(|_| {
            format!(
                "StreamSessionAgents was not accepted within {}s",
                STREAM_OPEN_TIMEOUT.as_secs()
            )
        })?
        .map_err(|e| format!("StreamSessionAgents call: {e}"))?;
    Ok((client, frames))
}

/// The connection the roster stream rides, and the identity its request carries.
///
/// The sandbox socket gets a connection of its **own**: it opens a fresh `UnixStream` per dispatch
/// today, and a stream held for the process lifetime must not be torn down with a tool call's.
async fn connect_roster_stream(
    transport: &SessionToolTransport,
) -> Result<
    (
        std::sync::Arc<dyn tddy_rpc::RpcClientTransport>,
        SessionToolEnvelope,
    ),
    String,
> {
    match transport {
        SessionToolTransport::SandboxIpc { socket_path } => {
            let client = crate::session_tool_client::connect_sandbox_ipc(socket_path).await?;
            // The socket identifies the session to the sandbox-runner, as it does for every tool
            // call over it, so the envelope stays empty.
            Ok((client, SessionToolEnvelope::default()))
        }
        #[cfg(feature = "livekit")]
        SessionToolTransport::LiveKit {
            url,
            room,
            token,
            server_identity,
            session_id,
            session_token,
            daemon_instance_id,
        } => {
            let key = crate::session_tool_client::LiveKitRoomKey {
                url: url.clone(),
                room: room.clone(),
                token: token.clone(),
                server_identity: server_identity.clone(),
            };
            let session = crate::session_tool_client::livekit_session(&key).await?;
            if !session.peer_present() {
                return Err(format!(
                    "daemon \"{server_identity}\" is not in room \"{room}\""
                ));
            }
            Ok((
                std::sync::Arc::clone(session.transport()),
                SessionToolEnvelope {
                    session_id: session_id.clone(),
                    session_token: session_token.clone(),
                    daemon_instance_id: daemon_instance_id.clone(),
                },
            ))
        }
        #[cfg(not(feature = "livekit"))]
        SessionToolTransport::LiveKit { .. } => Err(
            "this tddy-tools was built without the 'livekit' feature, so the session's daemon \
             cannot be reached at all"
                .to_string(),
        ),
        // Both are refused before the loop starts; reaching here would mean the two disagreed.
        SessionToolTransport::DaemonHttp { .. }
        | SessionToolTransport::IncompleteLiveKit { .. } => {
            Err("the roster stream has no client for this transport".to_string())
        }
    }
}
