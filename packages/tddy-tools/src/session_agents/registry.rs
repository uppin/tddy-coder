//! The registry itself: what the session's roster currently says, and what that permits.
//!
//! The two questions asked of it are "which agent may this call address" and "which exec tools has
//! an attached agent taken over" — and both have to be answerable while the stream that feeds it is
//! broken, which is why [`RosterCurrency`] is part of the state rather than a detail of
//! [`super::stream`]. See the module doc of [`super`] for the properties this exists to hold.

use std::collections::HashMap;
use std::sync::Mutex;

use tddy_discovery::agent_def::SpecializedAgentDef;
use tddy_discovery::subagent::normalize_replaced_tools;
use tddy_service::proto::connection::{AgentCloneState, SessionAgentEntry, SessionAgentRoster};

use super::seed::{seed_agent_id, seed_entry};

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
/// One consistent answer to the two questions the tool catalog asks the roster.
///
/// See [`LiveAgentRoster::catalog_visibility`] for why they are answered together.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CatalogVisibility {
    /// Every agent the session can address, in roster order. Empty is what decides that the
    /// conversation tools are not advertised at all; non-empty is also what those tools *offer*,
    /// since an id the schema does not name is one the main agent cannot reach.
    pub addressable_agents: Vec<AddressableAgent>,
    /// The exec tools attached agents have taken over from the main agent.
    pub withdrawn_exec_tools: WithdrawnExecTools,
}

impl CatalogVisibility {
    /// Whether the session has an agent to address at all.
    pub fn has_an_agent_to_address(&self) -> bool {
        !self.addressable_agents.is_empty()
    }
}

/// One agent the main agent may address, as the tool catalog needs to name it.
///
/// `agent_id` is the only spelling that resolves — [`LiveAgentRoster::resolve`] matches the whole
/// id, deliberately, so an agent attached after spawn that shares a seeded agent's bare name is a
/// different agent. Anything offering a choice of agents must therefore offer *these* strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressableAgent {
    /// The qualified id a call must name.
    pub agent_id: String,
    /// The human label the def carried, empty when it carried none.
    pub label: String,
    /// The daemon that owns the agent — where its turn loop runs.
    pub daemon_instance_id: String,
}

/// One exec tool an attached agent serves in the main agent's place.
pub struct Takeover<'a> {
    /// The tool, in the catalog's canonical spelling — the name a refusal should quote back.
    pub tool: &'a str,
    /// The qualified id of the agent to delegate to instead.
    pub agent_id: &'a str,
}

/// The exec tools attached agents have taken over from the main agent, and who took each.
///
/// A newtype rather than the `HashMap` it wraps because the two enforcement layers ask this the
/// same question in two spellings: the catalog filters `tools/list` by the router's tool names,
/// while a call arrives with whatever name the caller used. Answering both through
/// [`Self::taken_over_by`] — which canonicalizes before it looks — is what stops a tool being
/// advertised and then refused, or withheld and still callable, off nothing but a difference in
/// casing. Construction is the roster's alone: a set assembled anywhere else could name a
/// withdrawal no attached agent claimed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WithdrawnExecTools(HashMap<String, String>);

impl WithdrawnExecTools {
    /// Fold the attached agents' `replaces` lists into one takeover set.
    ///
    /// The agent named for a tool is the **first** in `entries` to claim it, so two agents replacing
    /// the same tool send the main agent to the one attached earliest rather than to whichever the
    /// iteration order happened to reach.
    fn claimed_by(entries: &[SessionAgentEntry]) -> Self {
        let mut withdrawn: HashMap<String, String> = HashMap::new();
        for entry in entries {
            for tool in normalize_replaced_tools(&entry.replaces) {
                withdrawn
                    .entry(tool)
                    .or_insert_with(|| entry.agent_id.clone());
            }
        }
        Self(withdrawn)
    }

    /// The agent serving `tool` in the main agent's place, if one has taken it over.
    ///
    /// `tool` is canonicalized first, so any spelling the catalog or a call site holds it in reaches
    /// the same answer; a name that is not in the exec catalog at all can carry no `replaces` claim
    /// and is never taken over.
    pub fn taken_over_by(&self, tool: &str) -> Option<Takeover<'_>> {
        let canonical = normalize_replaced_tools(&[tool.to_string()]).pop()?;
        let (tool, agent_id) = self.0.get_key_value(&canonical)?;
        Some(Takeover { tool, agent_id })
    }

    /// Whether anything has been taken over at all.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

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
    /// Which agent serves which tool is [`WithdrawnExecTools::claimed_by`]'s.
    pub fn withdrawn_exec_tools(&self) -> WithdrawnExecTools {
        let state = self.state.lock().expect("session agent roster");
        Self::withdrawn_exec_tools_in(&state)
    }

    /// Everything the tool catalog needs from the roster, read under **one** lock.
    ///
    /// Both halves describe the same revision, which is the point. Asking
    /// [`Self::is_empty`] and [`Self::withdrawn_exec_tools`] separately lets a revision land between
    /// the two calls and publishes a catalog that never existed: the conversation tools of an empty
    /// roster together with the withdrawals of a populated one, or the reverse — conversation tools
    /// advertised for agents whose exec tools are still on offer. `tools/list` is re-answered on
    /// every applied revision, so the interleaving is not hypothetical.
    pub fn catalog_visibility(&self) -> CatalogVisibility {
        let state = self.state.lock().expect("session agent roster");
        CatalogVisibility {
            addressable_agents: state
                .entries
                .iter()
                .map(|entry| AddressableAgent {
                    agent_id: entry.agent_id.clone(),
                    label: entry.label.clone(),
                    daemon_instance_id: entry.daemon_instance_id.clone(),
                })
                .collect(),
            withdrawn_exec_tools: Self::withdrawn_exec_tools_in(&state),
        }
    }

    fn withdrawn_exec_tools_in(state: &RosterState) -> WithdrawnExecTools {
        if !state.currency.enforces_withdrawal() {
            return WithdrawnExecTools::default();
        }
        WithdrawnExecTools::claimed_by(&state.entries)
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
        match self.withdrawn_exec_tools().taken_over_by(tool) {
            Some(takeover) => Err(RosterError::ToolWithdrawn {
                tool: takeover.tool.to_string(),
                agent_id: takeover.agent_id.to_string(),
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
