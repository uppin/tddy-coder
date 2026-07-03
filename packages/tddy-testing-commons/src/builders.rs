//! Fluent test data builders.
//!
//! Every constructor (`a_thing()`, `an_thing()`) returns a valid default value
//! or a builder whose `.build()` produces a valid value.
//! Tests override only the fields relevant to the scenario.

use tddy_core::backend::{InvokeRequest, InvokeResponse};
use tddy_core::changeset::{Changeset, ChangesetWorkflow};
use tddy_core::SessionMetadata;

// ─── InvokeResponse ─────────────────────────────────────────────────────────

/// Builder for [`InvokeResponse`].
pub struct InvokeResponseBuilder {
    output: String,
    exit_code: i32,
    session_id: Option<String>,
}

/// Start building an [`InvokeResponse`] with sensible defaults (empty output, exit 0, no session).
pub fn an_invoke_response() -> InvokeResponseBuilder {
    InvokeResponseBuilder {
        output: String::new(),
        exit_code: 0,
        session_id: None,
    }
}

impl InvokeResponseBuilder {
    pub fn with_output(mut self, output: impl Into<String>) -> Self {
        self.output = output.into();
        self
    }

    pub fn with_exit_code(mut self, code: i32) -> Self {
        self.exit_code = code;
        self
    }

    pub fn with_session_id(mut self, id: impl Into<String>) -> Self {
        self.session_id = Some(id.into());
        self
    }

    pub fn build(self) -> InvokeResponse {
        InvokeResponse {
            output: self.output,
            exit_code: self.exit_code,
            session_id: self.session_id,
            questions: vec![],
            raw_stream: None,
            stderr: None,
        }
    }
}

// ─── InvokeRequest ──────────────────────────────────────────────────────────

/// Start building an [`InvokeRequest`] from its `Default` impl.
/// Override only the fields relevant to the test.
pub fn an_invoke_request() -> InvokeRequest {
    InvokeRequest::default()
}

// ─── Changeset ──────────────────────────────────────────────────────────────

/// Builder for [`Changeset`].
pub struct ChangesetBuilder(Changeset);

/// Start building a [`Changeset`] (defaults to `Changeset::default()`).
pub fn a_changeset() -> ChangesetBuilder {
    ChangesetBuilder(Changeset::default())
}

impl ChangesetBuilder {
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.0.name = Some(name.into());
        self
    }

    pub fn with_branch_suggestion(mut self, branch: impl Into<String>) -> Self {
        self.0.branch_suggestion = Some(branch.into());
        self
    }

    pub fn with_worktree_suggestion(mut self, wt: impl Into<String>) -> Self {
        self.0.worktree_suggestion = Some(wt.into());
        self
    }

    pub fn with_recipe(mut self, recipe: impl Into<String>) -> Self {
        self.0.recipe = Some(recipe.into());
        self
    }

    pub fn with_workflow(mut self, workflow: ChangesetWorkflow) -> Self {
        self.0.workflow = Some(workflow);
        self
    }

    pub fn build(self) -> Changeset {
        self.0
    }
}

// ─── SessionMetadata ────────────────────────────────────────────────────────

/// Builder for [`SessionMetadata`].
pub struct SessionMetadataBuilder {
    session_id: String,
    project_id: String,
    created_at: String,
    updated_at: String,
    status: String,
    repo_path: Option<String>,
    pid: Option<u32>,
    tool: Option<String>,
    livekit_room: Option<String>,
    pending_elicitation: bool,
    previous_session_id: Option<String>,
    session_type: Option<String>,
    model: Option<String>,
    activity_status: Option<String>,
    hook_token: Option<String>,
    sandbox: Option<bool>,
    specialized_agents: Vec<String>,
}

/// Start building a [`SessionMetadata`] with sensible defaults.
///
/// Default `session_id`: `"session-1"`, `project_id`: `"proj-1"`, `status`: `"active"`.
pub fn a_session_metadata() -> SessionMetadataBuilder {
    SessionMetadataBuilder {
        session_id: "session-1".into(),
        project_id: "proj-1".into(),
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-01T00:00:00Z".into(),
        status: "active".into(),
        repo_path: None,
        pid: None,
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: None,
        model: None,
        activity_status: None,
        hook_token: None,
        sandbox: None,
        specialized_agents: Vec::new(),
    }
}

impl SessionMetadataBuilder {
    pub fn with_session_id(mut self, id: impl Into<String>) -> Self {
        self.session_id = id.into();
        self
    }

    pub fn with_project_id(mut self, id: impl Into<String>) -> Self {
        self.project_id = id.into();
        self
    }

    pub fn with_status(mut self, status: impl Into<String>) -> Self {
        self.status = status.into();
        self
    }

    pub fn with_pid(mut self, pid: u32) -> Self {
        self.pid = Some(pid);
        self
    }

    pub fn without_pid(mut self) -> Self {
        self.pid = None;
        self
    }

    pub fn with_repo_path(mut self, path: impl Into<String>) -> Self {
        self.repo_path = Some(path.into());
        self
    }

    pub fn with_tool(mut self, tool: impl Into<String>) -> Self {
        self.tool = Some(tool.into());
        self
    }

    pub fn with_livekit_room(mut self, room: impl Into<String>) -> Self {
        self.livekit_room = Some(room.into());
        self
    }

    pub fn with_pending_elicitation(mut self, pending: bool) -> Self {
        self.pending_elicitation = pending;
        self
    }

    pub fn with_hook_token(mut self, token: impl Into<String>) -> Self {
        self.hook_token = Some(token.into());
        self
    }

    pub fn with_specialized_agents(
        mut self,
        names: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.specialized_agents = names.into_iter().map(Into::into).collect();
        self
    }

    pub fn build(self) -> SessionMetadata {
        SessionMetadata {
            session_id: self.session_id,
            project_id: self.project_id,
            created_at: self.created_at,
            updated_at: self.updated_at,
            status: self.status,
            repo_path: self.repo_path,
            pid: self.pid,
            tool: self.tool,
            livekit_room: self.livekit_room,
            pending_elicitation: self.pending_elicitation,
            previous_session_id: self.previous_session_id,
            session_type: self.session_type,
            model: self.model,
            activity_status: self.activity_status,
            hook_token: self.hook_token,
            sandbox: self.sandbox,
            agent: None,
            recipe: None,
            specialized_agents: self.specialized_agents,
        }
    }
}
