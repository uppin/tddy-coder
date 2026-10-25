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

mod schema;

use serde::{Deserialize, Serialize};
use tddy_service::proto::session::session_attachment::Source;
use tddy_service::proto::session::{HostDocumentRef, SessionAttachment, StartSessionRequest};
use tddy_service::proto::types::HostDocumentScope;

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
    pub clone_placed_agents: Vec<String>,
    pub attachments: Vec<HostDocumentAttachment>,
}

/// A document on a connected host to attach to the new session — the form's host-document
/// attachment. (A staged upload needs a browser upload first, so it is not offered.)
///
/// `basename`, `scope` and `relative_path` are required; the rest default to empty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostDocumentAttachment {
    /// Basename the document is materialized under.
    pub basename: String,
    /// Host that owns the document; empty is the host the session starts on.
    #[serde(default)]
    pub daemon_instance_id: String,
    /// `types.HostDocumentScope`, written and read by its proto name (`HOST_DOCUMENT_SCOPE_…`). An
    /// unknown name, or `HOST_DOCUMENT_SCOPE_UNSPECIFIED`, is refused when the arguments are parsed.
    #[serde(with = "scope_by_proto_name")]
    pub scope: HostDocumentScope,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub project_id: String,
    pub relative_path: String,
}

/// The scopes an attachment may name, in proto order: every `HostDocumentScope` but
/// `UNSPECIFIED`.
const ATTACHMENT_SCOPES: [HostDocumentScope; 5] = [
    HostDocumentScope::SessionArtifact,
    HostDocumentScope::SessionUpload,
    HostDocumentScope::SessionWorktree,
    HostDocumentScope::ProjectRepo,
    HostDocumentScope::StagedAttachment,
];

/// `HostDocumentScope` as its proto name, so the calling agent reads and writes the names the
/// schema lists.
mod scope_by_proto_name {
    use serde::{de::Error, Deserialize, Deserializer, Serializer};
    use tddy_service::proto::types::HostDocumentScope;

    use super::ATTACHMENT_SCOPES;

    pub fn serialize<S: Serializer>(
        scope: &HostDocumentScope,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(scope.as_str_name())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<HostDocumentScope, D::Error> {
        let name = String::deserialize(deserializer)?;
        HostDocumentScope::from_str_name(&name)
            .filter(|scope| ATTACHMENT_SCOPES.contains(scope))
            .ok_or_else(|| {
                let accepted: Vec<&str> = ATTACHMENT_SCOPES
                    .iter()
                    .map(HostDocumentScope::as_str_name)
                    .collect();
                D::Error::custom(format!(
                    "unknown attachment scope {name:?}; expected one of {}",
                    accepted.join(", ")
                ))
            })
    }
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

pub use crate::start_session_args::schema::{
    offered_start_fields, start_session_schema, start_session_schema_without_choices,
};

impl StartSessionArgs {
    /// The `StartSessionRequest` these arguments ask for, in `context`. A branch another session
    /// owns is always rejected rather than suffixed.
    pub fn into_request(self, context: StartContext) -> StartSessionRequest {
        StartSessionRequest {
            session_token: context.session_token,
            project_id: context.project_id,
            on_branch_conflict: "reject".to_string(),
            orchestrator_session_id: context.orchestrator_session_id,
            orchestrator_daemon_instance_id: context.orchestrator_daemon_instance_id,
            session_type: self.session_type,
            daemon_instance_id: self.daemon_instance_id,
            tool_path: self.tool_path,
            agent: self.agent,
            recipe: self.recipe,
            model: self.model,
            provider_account_id: self.provider_account_id,
            provider_built_in: self.provider_built_in,
            branch_worktree_intent: self.branch_worktree_intent,
            new_branch_name: self.new_branch_name,
            selected_integration_base_ref: self.selected_integration_base_ref,
            selected_branch_to_work_on: self.selected_branch_to_work_on,
            create_remote_branch: self.create_remote_branch,
            initial_prompt: self.initial_prompt,
            permission_mode: self.permission_mode,
            dangerously_skip_permissions: self.dangerously_skip_permissions,
            stack_parent: self.stack_parent,
            stack_parent_daemon_instance_id: self.stack_parent_daemon_instance_id,
            stack_node_id: self.stack_node_id,
            pr_stack_base_session_id: self.pr_stack_base_session_id,
            sandbox: self.sandbox,
            managed_codebase: self.managed_codebase,
            sandboxed_codebase: self.sandboxed_codebase,
            codebase_daemon_instance_id: self.codebase_daemon_instance_id,
            ssh_config_host: self.ssh_config_host,
            semantic_index: self.semantic_index,
            specialized_agents: self.specialized_agents,
            clone_placed_agents: self.clone_placed_agents,
            attachments: self
                .attachments
                .into_iter()
                .map(HostDocumentAttachment::into_session_attachment)
                .collect(),
            ..Default::default()
        }
    }
}

impl HostDocumentAttachment {
    fn into_session_attachment(self) -> SessionAttachment {
        SessionAttachment {
            basename: self.basename,
            source: Some(Source::HostDocument(HostDocumentRef {
                daemon_instance_id: self.daemon_instance_id,
                scope: self.scope as i32,
                session_id: self.session_id,
                project_id: self.project_id,
                relative_path: self.relative_path,
            })),
        }
    }
}
