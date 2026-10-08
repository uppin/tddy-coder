use tddy_spawn::spawner::{self, SpawnOptions};

/// Why a `tddy-coder` child is spawned. It is all that differs between a start's spawn and a
/// resume's: the label the deadline and its log lines carry, and whether the forked worker traces
/// itself (only a start's does).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToolSpawnPurpose {
    Start,
    Resume,
}

impl ToolSpawnPurpose {
    pub(super) fn supervisor_label(self) -> &'static str {
        match self {
            Self::Start => "StartSession: spawn via tddy-supervisor",
            Self::Resume => "ResumeSession: spawn via tddy-supervisor",
        }
    }

    pub(super) fn worker_label(self) -> &'static str {
        match self {
            Self::Start => "StartSession: spawn",
            Self::Resume => "ResumeSession: spawn",
        }
    }
}

/// What a `tddy-coder` child is spawned with, beyond what the daemon's own config supplies. The
/// optional fields are the child's optional flags ([`SpawnOptions`]), owned so the plan can cross
/// into the blocking pool.
pub struct ToolSpawnPlan {
    pub purpose: ToolSpawnPurpose,
    pub os_user: String,
    pub tool_path: String,
    pub repo_path: std::path::PathBuf,
    pub livekit: spawner::LiveKitCreds,
    pub resume_session_id: Option<String>,
    pub new_session_id: Option<String>,
    pub project_id: Option<String>,
    pub agent: Option<String>,
    pub agent_def_json: Option<String>,
    pub recipe: Option<String>,
    pub stack_parent: Option<String>,
    pub stack_node_id: Option<String>,
    pub stack_seed_base_session: Option<String>,
    pub model: Option<String>,
    pub host_session_socket: Option<String>,
    /// The commit identity pairs the child's agent authors under — the project's account, from
    /// `SessionAccountAccess::session_identity`. Empty when it resolves to none. Never a token.
    pub git_environment: Vec<(String, String)>,
}

impl ToolSpawnPlan {
    pub(super) fn options(&self, mouse: bool) -> SpawnOptions<'_> {
        SpawnOptions {
            resume_session_id: self.resume_session_id.as_deref(),
            new_session_id: self.new_session_id.as_deref(),
            project_id: self.project_id.as_deref(),
            agent: self.agent.as_deref(),
            agent_def_json: self.agent_def_json.as_deref(),
            mouse,
            recipe: self.recipe.as_deref(),
            stack_parent: self.stack_parent.as_deref(),
            stack_node_id: self.stack_node_id.as_deref(),
            stack_seed_base_session: self.stack_seed_base_session.as_deref(),
            model: self.model.as_deref(),
            host_session_socket: self.host_session_socket.as_deref(),
            git_environment: &self.git_environment,
        }
    }
}
