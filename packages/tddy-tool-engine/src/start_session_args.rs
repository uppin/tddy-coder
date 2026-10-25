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
use serde_json::{json, Map, Value};
use tddy_core::cli_harness::{cli_harness_requires_model, cli_harnesses};
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

/// The permission modes a claude-cli session takes; empty is the daemon's default (`auto`).
const PERMISSION_MODES: [&str; 6] = [
    "",
    "auto",
    "default",
    "acceptEdits",
    "plan",
    "bypassPermissions",
];

const NEW_BRANCH_FROM_BASE: &str = "new_branch_from_base";
const WORK_ON_SELECTED_BRANCH: &str = "work_on_selected_branch";

/// `StartSession`'s JSON schema: every argument described, the closed lists as enums, and what
/// each session type and branch intent requires stated with `if`/`then`.
pub fn start_session_schema(choices: &StartChoices) -> Value {
    schema_offering(Some(choices))
}

/// [`start_session_schema`] for a client that does not know the daemon's hosts and recipes: the
/// same schema with `daemon_instance_id` and `recipe` left as free strings. The daemon that runs the
/// start still refuses a host it cannot route to and a recipe it does not support; this is what
/// `tddy-tools` registers before the daemon is asked.
pub fn start_session_schema_without_choices() -> Value {
    schema_offering(None)
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

fn schema_offering(choices: Option<&StartChoices>) -> Value {
    let harnesses = cli_harnesses();
    let session_types: Vec<&str> = ["tool", "workspace"]
        .into_iter()
        .chain(harnesses.iter().map(|harness| harness.session_type))
        .collect();
    let harness_list = harnesses
        .iter()
        .map(|harness| format!("`{}` ({})", harness.session_type, harness.label))
        .collect::<Vec<_>>()
        .join(", ");
    let needing_a_model: Vec<&str> = harnesses
        .iter()
        .map(|harness| harness.session_type)
        .filter(|session_type| cli_harness_requires_model(session_type))
        .collect();

    let mut properties = Map::new();
    let mut offer = |name: &str, property: Value| {
        properties.insert(name.to_string(), property);
    };
    offer(
        "session_type",
        json!({
            "type": "string",
            "enum": session_types,
            "description": format!(
                "What to start. `tool`: a tddy-coder workflow session run by `agent`. \
                 `workspace`: a worktree with no agent. Any other value runs that coding agent \
                 interactively in a terminal: {harness_list}."
            )
        }),
    );
    offer("daemon_instance_id", host_property(choices));
    offer(
        "tool_path",
        string("For `tool`: the tddy-coder binary to run, from the daemon's allowlist; empty is the daemon's default."),
    );
    offer(
        "agent",
        string("An agent id: a coding backend from the daemon's `allowed_agents`, an assistant in its registry, or a harness profile under `<tddyhome>/agents/` (which supplies its own session settings). Empty uses the session type's default."),
    );
    offer("recipe", recipe_property(choices));
    offer(
        "model",
        string(&format!(
            "Model id or alias for the session's agent (for `claude-cli`: `opus`, `sonnet`, …). \
             Required for {}: a start of those without one is refused. Other session types pick \
             their own when empty.",
            needing_a_model
                .iter()
                .map(|session_type| format!("`{session_type}`"))
                .collect::<Vec<_>>()
                .join(" and ")
        )),
    );
    offer(
        "provider_account_id",
        string("Run the session's harness on this connected model-provider account instead of its vendor. Empty leaves the choice to the agent definition or the person's default. Exclusive with `provider_built_in`."),
    );
    offer(
        "provider_built_in",
        boolean("Run the harness on its own vendor, overriding an agent definition's or the person's default account. Exclusive with `provider_account_id`."),
    );
    offer(
        "branch_worktree_intent",
        json!({
            "type": "string",
            "enum": ["", NEW_BRANCH_FROM_BASE, WORK_ON_SELECTED_BRANCH],
            "description": "How the session's worktree gets its branch. `new_branch_from_base`: a new branch (`new_branch_name`) cut from the base. `work_on_selected_branch`: check out the existing `selected_branch_to_work_on`. Empty is a new branch with a generated name."
        }),
    );
    offer(
        "new_branch_name",
        string("With `new_branch_from_base`: the branch to create; empty lets the start name one. A branch another session owns is refused, never suffixed."),
    );
    offer(
        "selected_integration_base_ref",
        string("The ref a new branch is cut from and diffed against (e.g. `origin/main`); empty is the project's main branch."),
    );
    offer(
        "selected_branch_to_work_on",
        string("With `work_on_selected_branch`: the existing branch to check out. Required then."),
    );
    offer(
        "create_remote_branch",
        boolean("With `new_branch_from_base`: push the new branch to origin when the session starts. A failed push fails the start."),
    );
    offer(
        "initial_prompt",
        string("The first turn sent to the session's agent; empty starts it idle. Ignored for `workspace`."),
    );
    offer(
        "permission_mode",
        json!({
            "type": "string",
            "enum": PERMISSION_MODES,
            "description": "The claude-cli permission mode; empty is `auto`. Ignored with `dangerously_skip_permissions`."
        }),
    );
    offer(
        "dangerously_skip_permissions",
        boolean("For claude-cli: skip every permission prompt (`--dangerously-skip-permissions`). Replaces `permission_mode`."),
    );
    offer(
        "stack_parent",
        string("A pr-stack orchestrator session this session is a child of: its base is resolved from that session's stack and the child is recorded on it. Empty for none."),
    );
    offer(
        "stack_parent_daemon_instance_id",
        string("The host `stack_parent` runs on; empty for the host the session starts on."),
    );
    offer(
        "stack_node_id",
        string("The planned pr-stack node this session materializes, on `stack_parent`'s stack. Empty derives it from the branch."),
    );
    offer(
        "pr_stack_base_session_id",
        string("With the `pr-stack` recipe: an existing session whose branch seeds the new stack as its single root node. Empty leaves the stack for the agent to plan."),
    );
    offer(
        "sandbox",
        boolean(
            "Run the session's agent (or, for `workspace`, its tools) inside a platform sandbox.",
        ),
    );
    offer(
        "managed_codebase",
        boolean("For claude-cli: the agent reaches the worktree only through the tddy-tools exec tools, never its own filesystem tools."),
    );
    offer(
        "sandboxed_codebase",
        boolean("For claude-cli: jail the session's checkout and run the agent beside it, unconfined, with its filesystem and shell tools withdrawn. Exclusive with `sandbox`, `managed_codebase` and `codebase_daemon_instance_id`."),
    );
    offer(
        "codebase_daemon_instance_id",
        string("The host that holds the session's worktree, when it is not the host the agent runs on. Requires `managed_codebase` and `claude-cli`. Empty keeps them together."),
    );
    offer(
        "ssh_config_host",
        string("An OpenSSH `Host` alias on the host that manages the code: the worktree lives on that SSH target and every exec tool runs there. Empty is the host's own filesystem."),
    );
    offer(
        "semantic_index",
        boolean("Index the worktree before the agent starts and offer it SemanticSearch."),
    );
    offer(
        "specialized_agents",
        json!({
            "type": "array",
            "items": { "type": "string" },
            "description": "Specialized subagents to wire into the session, each a builtin or a definition under `<tddyhome>/agents/`. An unknown name refuses the start."
        }),
    );
    offer("attachments", attachments_property());

    let mut conditions: Vec<Value> = needing_a_model
        .iter()
        .map(|session_type| {
            json!({
                "if": { "properties": { "session_type": { "const": session_type } } },
                "then": { "required": ["model"] }
            })
        })
        .collect();
    conditions.push(json!({
        "if": { "properties": { "branch_worktree_intent": { "const": WORK_ON_SELECTED_BRANCH } } },
        "then": { "required": ["selected_branch_to_work_on"] }
    }));

    json!({
        "type": "object",
        "properties": properties,
        "required": ["session_type"],
        "allOf": conditions,
        "additionalProperties": false
    })
}

fn string(description: &str) -> Value {
    json!({ "type": "string", "description": description })
}

fn boolean(description: &str) -> Value {
    json!({ "type": "boolean", "description": description })
}

fn host_property(choices: Option<&StartChoices>) -> Value {
    let mut property = string(
        "The host to start the session on, as the eligible-daemon roster names it; empty for this host.",
    );
    if let Some(choices) = choices {
        let hosts = choices
            .hosts
            .iter()
            .map(|host| format!("`{}` ({})", host.daemon_instance_id, host.label))
            .collect::<Vec<_>>()
            .join(", ");
        property["enum"] = std::iter::once("")
            .chain(choices.hosts.iter().map(|h| h.daemon_instance_id.as_str()))
            .collect();
        property["description"] = format!(
            "The host to start the session on; empty for this host. Eligible hosts: {hosts}."
        )
        .into();
    }
    property
}

fn recipe_property(choices: Option<&StartChoices>) -> Value {
    let mut property = string(
        "The workflow recipe a `tool` session (or a managed claude-cli / cursor-cli session) runs; empty is the default.",
    );
    if let Some(choices) = choices {
        property["enum"] = std::iter::once("")
            .chain(choices.recipes.iter().map(String::as_str))
            .collect();
    }
    property
}

fn attachments_property() -> Value {
    let scopes: Vec<&str> = ATTACHMENT_SCOPES
        .iter()
        .map(HostDocumentScope::as_str_name)
        .collect();
    json!({
        "type": "array",
        "description": "Documents on a connected host to attach to the session, materialized under its artifacts before the agent starts. A failed attachment fails the start.",
        "items": {
            "type": "object",
            "properties": {
                "basename": string("The file name the document is attached under; no path separators."),
                "daemon_instance_id": string("The host that owns the document; empty for the host the session starts on."),
                "scope": {
                    "type": "string",
                    "enum": scopes,
                    "description": "The root `relative_path` resolves against: a session's artifacts, uploads or worktree, the project's main checkout, or a staged upload."
                },
                "session_id": string("The owning session, for the `HOST_DOCUMENT_SCOPE_SESSION_*` scopes."),
                "project_id": string("The owning project, for `HOST_DOCUMENT_SCOPE_PROJECT_REPO`."),
                "relative_path": string("The document's path under the scope's root.")
            },
            "required": ["basename", "scope", "relative_path"],
            "additionalProperties": false
        }
    })
}

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
