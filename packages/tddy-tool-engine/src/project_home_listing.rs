/// One session as `ListProjectSessions` reports it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ListedSession {
    pub session_id: String,
    pub branch: String,
    pub worktree: String,
    pub is_active: bool,
    /// The agent status name (`idle`, `running`, `waiting_for_input`, …).
    pub agent_status: String,
    pub pending_elicitation: bool,
    /// The daemon the session runs on, as the eligible-daemon roster names it. A target tool given
    /// this id reaches the session on that host (`#start-parity` 1/9).
    #[serde(default)]
    pub daemon_instance_id: String,
    /// The name the session was given (`RenameSession`, or `StartSession`'s `display_name`), or
    /// `null` for an unnamed session. Defaulted so a peer that lists no names still answers.
    #[serde(default)]
    pub display_name: Option<String>,
}

/// What `ListProjectSessions` answers: the project's sessions on every eligible host, and the hosts
/// that did not answer — named rather than silently left out of the list.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ListProjectSessionsResult {
    pub sessions: Vec<ListedSession>,
    pub unreachable_hosts: Vec<UnreachableHost>,
}

/// An eligible host that did not answer `ListProjectSessions`'s fan-out, and why.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UnreachableHost {
    pub daemon_instance_id: String,
    pub reason: String,
}
