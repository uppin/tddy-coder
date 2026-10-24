use crate::session_notifications::SessionNotificationBus;
use std::path::PathBuf;
use std::sync::Arc;
use tddy_daemon_kernel::AgentActivityHub;
use tddy_rpc::Status;

/// Resolves a session token to the OS user that owns it, or to the refusal.
///
/// A `Result` rather than the kernel's `Option`-returning
/// [`tddy_daemon_kernel::SessionUserResolver`], for the reason node 6's
/// `tddy_session_files::service::OsUserResolver` gives: the two refusals send a caller to different
/// remedies and must not collapse into one — an unknown or expired token is `UNAUTHENTICATED` and
/// the caller re-authenticates, while a known GitHub user with no OS-user mapping is
/// `PERMISSION_DENIED` and only an operator can fix it. The mapping table is the daemon's config,
/// so the distinction can only be drawn where the closure is built.
pub type OsUserResolver = Arc<dyn Fn(&str) -> Result<String, Status> + Send + Sync>;

/// A session's display label, as every surface on this host names it.
///
/// A port rather than a rule this crate applies, because the label falls back to the workflow goal
/// out of `session_list_enrichment` — the module that serves `ListSessions`, family C, which stays
/// in `tddy-daemon`. Re-deriving it here from a different source would name sessions correctly
/// right up until the two disagreed, and the whole point of the shared rule is that the chat, the
/// drawer and this notification cannot disagree.
pub trait SessionLabels: Send + Sync {
    /// The label `session_id` is shown under, read from `sessions_base`. Never fails: a label is
    /// display text, and a session directory that cannot be read still has a short id.
    fn label_for(&self, sessions_base: &std::path::Path, session_id: &str) -> String;
}

/// Which slice of a tick's diff a delta lookup asks for.
///
/// Declared here rather than imported from `tddy-daemon-livekit`'s `session_room`, where the ring
/// that answers lives, because that crate now **depends on this one** — it numbers its ticks with
/// [`crate::next_tick`]. Importing its types back would be a cycle. Three unit variants restated against
/// a wire enum is the cheaper side of that trade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeltaScope {
    /// The paths the named call is credited with — the default.
    Call,
    /// The paths of that tick claimed by no call.
    Residual,
    /// The whole tick, unscoped.
    Tick,
}

/// One tick's measured change, as the session room that took it describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeasuredDelta {
    /// The tick this patch belongs to — [`crate::FIRST_TICK`] or later, never [`crate::NO_TICK`].
    pub seq: u64,
    /// The tick this one follows.
    pub prev_seq: u64,
    /// The commit the patch applies onto.
    pub base_commit: String,
    /// `git diff --binary` output, limited to `scoped_paths`.
    pub patch: Vec<u8>,
    /// The paths the patch is limited to — the request's scope, resolved.
    pub scoped_paths: Vec<String>,
}

/// What a delta lookup found. Total rather than a `Result<Option<_>, _>`, so the four answers a
/// caller has to tell apart each name themselves at the call site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeltaLookup {
    /// The delta, scoped as asked.
    Found(MeasuredDelta),
    /// No session room for this session is hosted on this host, so it has no deltas and never
    /// will. An absence, not a failure.
    NoRoomHere,
    /// This host has no record of that call in this session.
    UnknownCall,
    /// The call is known and its delta has aged out of the session's ring.
    AgedOut { seq: u64 },
}

/// The per-session ring of measured deltas `StreamAgentActivityDelta` answers from.
///
/// A port rather than a lookup this crate performs: the ring is filled by the session room's poll
/// loop in `tddy-daemon-livekit`, and a patch is a measurement of a live checkout that only the
/// host hosting that room ever took. What this crate owns is the authorization order, the four
/// refusals and the framing.
pub trait SessionDeltaStores: Send + Sync {
    /// What the ring holds for `call_id`, or why it could not be read at all.
    ///
    /// The `Err` is not a fifth absence: it is "this host could not answer the question", which a
    /// client retries, where all four [`DeltaLookup`] answers are settled facts it acts on.
    fn delta_for_call(
        &self,
        session_id: &str,
        call_id: &str,
        scope: DeltaScope,
    ) -> Result<DeltaLookup, Status>;
}

/// No session room is hosted here, so no session has a delta: every lookup is
/// [`DeltaLookup::NoRoomHere`], the absence rather than a failure.
pub struct NoSessionDeltaStores;

impl SessionDeltaStores for NoSessionDeltaStores {
    fn delta_for_call(
        &self,
        _session_id: &str,
        _call_id: &str,
        _scope: DeltaScope,
    ) -> Result<DeltaLookup, Status> {
        Ok(DeltaLookup::NoRoomHere)
    }
}

/// Everything the eight handlers need from the host they run on.
///
/// A struct rather than six positional parameters: they are all wiring, and the two `Arc<dyn _>`
/// ports have no type-level distinction from each other at a call site.
pub struct ActivityPorts {
    /// Session token to the OS user that owns it. Every session directory the five token-bearing
    /// methods read is resolved under this user, never under one the request named.
    ///
    /// Supplied by the daemon because the `users[]` mapping it draws its two refusals from is the
    /// daemon's config.
    pub os_users: OsUserResolver,
    /// The daemon's data dir. `user_paths::sessions_base_for_user` derives a user's sessions base
    /// from it, which is where every transcript, activity log and `.session.yaml` below is read.
    ///
    /// Supplied by the daemon because where a host keeps its data is an operator's choice, not a
    /// property of this subsystem — and the two hook reports resolve a sessions base from it for an
    /// OS user with no token at all, so it must be the same dir the rest of the daemon writes into.
    pub tddy_data_dir: PathBuf,
    /// Node 1's per-session live broadcast of agent activity, plus the stack of in-flight
    /// `call_id`s. Consumed unchanged: the sandbox subsystem publishes into this same hub, so a
    /// second one here would leave an in-jail tool call invisible to every stream.
    pub activity: Arc<AgentActivityHub>,
    /// The live frames of every daemon-run subagent conversation, keyed by session and conversation. The
    /// session-agents recorder publishes into the same channel, so a conversation's replay tails it.
    pub conversation_frames: Arc<tddy_daemon_kernel::conversation_frames::ConversationFrames>,
    /// Where this host publishes its session notifications. `None` means nothing is listening:
    /// the two reports skip publishing, and `StreamSessionNotifications` has no feed to hand a
    /// client and says so.
    ///
    /// An `Option` because the daemon's own field is one — a host with no subscribers and no
    /// stream clients raises no bus, and inventing one here would retain events nobody reads.
    pub notifications: Option<Arc<SessionNotificationBus>>,
    /// How a session is named to an operator. The daemon's, because the fallback reads
    /// `session_list_enrichment` — see [`SessionLabels`].
    pub session_labels: Arc<dyn SessionLabels>,
    /// Where a call's delta is served from. The session room's, because only the host that
    /// measured the checkout has one — see [`SessionDeltaStores`].
    pub deltas: Arc<dyn SessionDeltaStores>,
}
