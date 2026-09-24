use tddy_spawn::spawner::{self, SpawnOptions};

/// Why a `tddy-coder` child is spawned. It is all that differs between a start's spawn and a
/// resume's: the label the deadline and its log lines carry, and whether the forked worker traces
/// itself (only a start's does).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::connection_service) enum ToolSpawnPurpose {
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
pub(in crate::connection_service) struct ToolSpawnPlan {
    pub(in crate::connection_service) purpose: ToolSpawnPurpose,
    pub(in crate::connection_service) os_user: String,
    pub(in crate::connection_service) tool_path: String,
    pub(in crate::connection_service) repo_path: std::path::PathBuf,
    pub(in crate::connection_service) livekit: spawner::LiveKitCreds,
    pub(in crate::connection_service) resume_session_id: Option<String>,
    pub(in crate::connection_service) new_session_id: Option<String>,
    pub(in crate::connection_service) project_id: Option<String>,
    pub(in crate::connection_service) agent: Option<String>,
    pub(in crate::connection_service) agent_def_json: Option<String>,
    pub(in crate::connection_service) recipe: Option<String>,
    pub(in crate::connection_service) stack_parent: Option<String>,
    pub(in crate::connection_service) stack_node_id: Option<String>,
    pub(in crate::connection_service) stack_seed_base_session: Option<String>,
    pub(in crate::connection_service) model: Option<String>,
    pub(in crate::connection_service) host_session_socket: Option<String>,
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
        }
    }
}
