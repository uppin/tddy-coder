//! `StartSession`'s arguments — every option the web UI's New-session form can send, under the
//! names `session.StartSessionRequest` gives them (`#start-parity` 2/9).
//!
//! The proto request, not the form's model, is the one shape both surfaces produce, so it is what
//! this module mirrors. Every `StartSessionRequest` field is exactly one of: an argument here, a
//! field the caller's context fills ([`CONTEXT_FILLED_FIELDS`]), or one the tool withholds
//! ([`INTERNAL_START_FIELDS`], each with its reason). `tests/start_session_args_parity.rs` reads the
//! compiled-in proto descriptor and fails on a field that is none of the three, and the web's
//! `createSessionRequestParity.cy.ts` fails on a key the form's request builder sends that is not
//! offered here.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tddy_service::proto::session::StartSessionRequest;

/// The exec tool's name.
pub const START_SESSION: &str = "StartSession";

/// `StartSession`'s arguments. Absent fields take the proto default, which is what the form sends
/// for a control it does not show.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StartSessionArgs {
    pub session_type: String,
    pub daemon_instance_id: String,
    pub tool_path: String,
    pub agent: String,
    pub recipe: String,
    pub model: String,
    pub provider_account_id: String,
    pub provider_built_in: bool,
    pub branch_worktree_intent: String,
    pub new_branch_name: String,
    pub selected_integration_base_ref: String,
    pub selected_branch_to_work_on: String,
    pub create_remote_branch: bool,
    pub initial_prompt: String,
    pub permission_mode: String,
    pub dangerously_skip_permissions: bool,
    pub stack_parent: String,
    pub stack_parent_daemon_instance_id: String,
    pub stack_node_id: String,
    pub pr_stack_base_session_id: String,
    pub sandbox: bool,
    pub managed_codebase: bool,
    pub sandboxed_codebase: bool,
    pub codebase_daemon_instance_id: String,
    pub ssh_config_host: String,
    pub semantic_index: bool,
    pub specialized_agents: Vec<String>,
    pub attachments: Vec<HostDocumentAttachment>,
}

/// A document on a connected host to attach to the new session — the form's host-document
/// attachment. (A staged upload needs a browser upload first, so it is not offered.)
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HostDocumentAttachment {
    /// Basename the document is materialized under.
    pub basename: String,
    /// Host that owns the document; empty is the host the session starts on.
    pub daemon_instance_id: String,
    /// `types.HostDocumentScope` by its proto name (`HOST_DOCUMENT_SCOPE_…`).
    pub scope: String,
    pub session_id: String,
    pub project_id: String,
    pub relative_path: String,
}

/// `StartSessionRequest` fields the caller's context fills, never the calling agent.
pub const CONTEXT_FILLED_FIELDS: &[(&str, &str)] = &[
    ("session_token", "the calling session's own token"),
    ("project_id", "the caller's own project: a session starts only in it"),
    (
        "on_branch_conflict",
        "always \"reject\", as the form sends it: a branch another session owns is reported, never suffixed",
    ),
    ("orchestrator_session_id", "the calling session"),
    ("orchestrator_daemon_instance_id", "the calling session's daemon"),
];

/// `StartSessionRequest` fields the tool withholds, each with the reason. A field here is one the
/// New-session form does not send either.
pub const INTERNAL_START_FIELDS: &[(&str, &str)] = &[
    (
        "repo_path",
        "a local client's own checkout; a session's worktree comes from its project",
    ),
    (
        "claude_args",
        "raw CLI arguments from a local client's own config, not a form option",
    ),
    (
        "requested_session_id",
        "only for the codebase half of a split placement, chosen by the daemon placing it",
    ),
    ("agent_clone", "set by a daemon placing an agent clone"),
    (
        "split_agent",
        "set by a daemon placing a split session's codebase half",
    ),
    (
        "github_source",
        "filled by a daemon forwarding a start to a peer, from its own project row",
    ),
    (
        "session_role",
        "refused on StartSession; only EnsureProjectHomeSession sets a role",
    ),
];

/// One host a session can start on, as the eligible-daemon roster names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostChoice {
    pub daemon_instance_id: String,
    pub label: String,
}

/// The closed lists the schema offers as enums that only the daemon knows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartChoices {
    /// Every eligible host; `""` (this host) is always offered besides.
    pub hosts: Vec<HostChoice>,
    /// The workflow recipes the daemon supports.
    pub recipes: Vec<String>,
}

/// What the caller's context supplies for [`CONTEXT_FILLED_FIELDS`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartContext {
    pub session_token: String,
    pub project_id: String,
    pub orchestrator_session_id: String,
    pub orchestrator_daemon_instance_id: String,
}

/// `StartSession`'s JSON schema: every argument described, the closed lists as enums, and what
/// each session type and branch intent requires stated with `if`/`then`.
pub fn start_session_schema(choices: &StartChoices) -> Value {
    // TODO(start-tool): implement
    let _ = choices;
    serde_json::json!({ "type": "object", "properties": {} })
}

/// The argument names [`start_session_schema`] offers, sorted. What the web parity fixture holds.
pub fn offered_start_fields(choices: &StartChoices) -> Vec<String> {
    let schema = start_session_schema(choices);
    let mut names: Vec<String> = schema["properties"]
        .as_object()
        .map(|properties| properties.keys().cloned().collect())
        .unwrap_or_default();
    names.sort();
    names
}

impl StartSessionArgs {
    /// The `StartSessionRequest` these arguments ask for, in `context`.
    pub fn into_request(self, context: StartContext) -> StartSessionRequest {
        // TODO(start-tool): implement
        let _ = (self, context);
        unimplemented!("TODO(start-tool): map StartSessionArgs onto StartSessionRequest")
    }
}
