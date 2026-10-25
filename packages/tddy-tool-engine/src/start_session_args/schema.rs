use crate::start_session_args::StartChoices;
use crate::start_session_args::ATTACHMENT_SCOPES;
use serde_json::{json, Map, Value};
use tddy_core::cli_harness::{cli_harness_requires_model, cli_harnesses};
use tddy_service::proto::types::HostDocumentScope;

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
