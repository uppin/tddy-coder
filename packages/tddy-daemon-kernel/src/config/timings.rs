use std::time::Duration;

/// Matches the sandbox-ready timeout: a cold model load is allowed to take minutes.
fn default_agent_warmup_timeout_secs() -> u64 {
    120
}

fn default_agent_warmup_retry_interval_ms() -> u64 {
    1000
}

fn default_agent_warmup_request_timeout_secs() -> u64 {
    120
}

/// Timing budget for the specialized-subagent warm-up gate (`agent_warmup:` YAML section).
///
/// The defaults reproduce `tddy_discovery::warmup::WarmupOptions::default()` exactly — this
/// section exists so a host whose agents live somewhere faster (or slower) than a cold local
/// model server can say so, without any code path branching on how it was started.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentWarmupConfig {
    /// Total budget to get a single agent ready before the session fails to start.
    #[serde(default = "default_agent_warmup_timeout_secs")]
    pub timeout_secs: u64,
    /// Wait between transient-failure retries.
    #[serde(default = "default_agent_warmup_retry_interval_ms")]
    pub retry_interval_ms: u64,
    /// Per-probe HTTP timeout — a cold model load must fit inside it.
    #[serde(default = "default_agent_warmup_request_timeout_secs")]
    pub request_timeout_secs: u64,
}

impl Default for AgentWarmupConfig {
    fn default() -> Self {
        Self {
            timeout_secs: default_agent_warmup_timeout_secs(),
            retry_interval_ms: default_agent_warmup_retry_interval_ms(),
            request_timeout_secs: default_agent_warmup_request_timeout_secs(),
        }
    }
}

/// Today's freshness for a session room: fast enough that an agent notices a commit within a
/// couple of seconds, slow enough that a host with many checkouts is not spending its time shelling
/// out to git.
fn default_session_room_poll_interval_ms() -> u64 {
    2000
}

/// A `git rev-parse` / `status` / `diff` trio that has not answered in this long is a repository in
/// trouble (a stale index lock, a filesystem that stopped responding), not a slow one. It is also
/// the budget one measurement of a checkout gets when the caller names none, which is why
/// `tddy_daemon::session_room` reads it from here — a single definition, so the shipped default and the
/// unconfigured default cannot drift apart.
///
/// The number lives on this side of that pair deliberately. Read the other way round — `config`
/// naming `session_room`, and `session_room` naming `config` for its [`DaemonConfig`] — the two
/// modules are mutually dependent, and no crate boundary can be drawn between a wiring layer and a
/// LiveKit worktree room while that is true.
pub const DEFAULT_SESSION_ROOM_GIT_TIMEOUT: Duration = Duration::from_millis(5_000);

fn default_session_room_git_timeout_ms() -> u64 {
    DEFAULT_SESSION_ROOM_GIT_TIMEOUT.as_millis() as u64
}

/// The shortest interval and git budget a session room accepts.
///
/// Not a clamp. Below this a room is not "fast", it is a git storm: one poll runs four `git`
/// subprocesses against the checkout, so a mistyped `poll_interval_ms: 0` or `: 1` would ask for
/// thousands of processes a second, per room, forever. An operator who wrote that meant something
/// else, and being told so at startup is cheaper than finding it in a load average. The same floor
/// covers `git_timeout_ms`, where anything shorter kills every measurement before git can answer.
pub(super) const MIN_WORKTREE_ROOM_INTERVAL_MS: u64 = 100;

/// Cadence and patience of the session-room poll loop (`session_room:` YAML section).
///
/// Polling is how a checkout's activity is detected (`docs/ft/daemon/session-room.md`
/// FR5), so its interval is the freshness the room offers. The defaults reproduce the shipped
/// behaviour exactly — this section exists so an operator can say something different, not so a
/// caller can branch on how the daemon was started.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct SessionRoomConfig {
    /// How often the facilitating daemon measures the checkout it hosts. At least
    /// [`MIN_WORKTREE_ROOM_INTERVAL_MS`].
    #[serde(default = "default_session_room_poll_interval_ms")]
    pub poll_interval_ms: u64,
    /// Wall-clock budget for one poll's git work; a `git` that overruns it is killed. Exceeding it
    /// costs that worktree's freshness for a tick and nothing else — the daemon never waits on git
    /// on a request path. At least [`MIN_WORKTREE_ROOM_INTERVAL_MS`].
    #[serde(default = "default_session_room_git_timeout_ms")]
    pub git_timeout_ms: u64,
}

impl SessionRoomConfig {
    /// Refuse a section that asks for something no room should do, naming the value and the floor.
    ///
    /// Refused rather than clamped: a clamp turns an operator's mistake into behaviour they did not
    /// ask for and cannot see, and this particular mistake — a sub-100 ms poll — is one that
    /// saturates a host.
    pub(super) fn validate(&self) -> anyhow::Result<()> {
        for (field, value) in [
            ("poll_interval_ms", self.poll_interval_ms),
            ("git_timeout_ms", self.git_timeout_ms),
        ] {
            if value < MIN_WORKTREE_ROOM_INTERVAL_MS {
                anyhow::bail!(
                    "session_room.{field} is {value}; it must be at least {MIN_WORKTREE_ROOM_INTERVAL_MS} \
                     (each poll runs four git subprocesses per hosted worktree). Remove the field to \
                     use the default."
                );
            }
        }
        Ok(())
    }
}

impl Default for SessionRoomConfig {
    fn default() -> Self {
        Self {
            poll_interval_ms: default_session_room_poll_interval_ms(),
            git_timeout_ms: default_session_room_git_timeout_ms(),
        }
    }
}
