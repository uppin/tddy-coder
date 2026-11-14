use crate::start_session_args::ATTACHMENT_SCOPES;
use crate::start_session_args::{AgentChoice, StartChoices};
use serde_json::{json, Map, Value};
use tddy_core::session_metadata::ORCHESTRATOR_ROLE;
use tddy_service::proto::types::HostDocumentScope;

/// The permission modes the injected agents' launches take, in injected order, with the empty
/// choice (the agent's own default) first.
fn permission_modes(agents: &[AgentChoice]) -> Vec<String> {
    let mut modes = vec![String::new()];
    for agent in agents {
        for mode in &agent.permission_modes {
            if !modes.contains(mode) {
                modes.push(mode.clone());
            }
        }
    }
    modes
}

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
    let no_agents: &[AgentChoice] = &[];
    let agents: &[AgentChoice] = choices.map_or(no_agents, |choices| choices.agents.as_slice());
    let harness_list = agents
        .iter()
        .map(|agent| format!("`{}` ({})", agent.session_type, agent.label))
        .collect::<Vec<_>>()
        .join(", ");
    let needing_a_model: Vec<&str> = agents
        .iter()
        .filter(|agent| agent.requires_model)
        .map(|agent| agent.session_type.as_str())
        .collect();

    let mut properties = Map::new();
    let mut offer = |name: &str, property: Value| {
        properties.insert(name.to_string(), property);
    };
    offer(
        "session_type",
        session_type_property(choices, &harness_list),
    );
    offer("daemon_instance_id", host_property(choices));
    offer(
        "tool_path",
        string("For a tddy-coder session type: the tddy-coder binary to run, one of the daemon's `allowed_tools`. Required for those session types: an empty or unlisted path is refused, naming the allowlist."),
    );
    offer(
        "agent",
        string("An agent id: a coding backend from the daemon's `allowed_agents`, an assistant in its registry, or a harness profile under `<tddyhome>/agents/` (which supplies its own session settings). Empty uses the session type's default."),
    );
    offer("recipe", recipe_property(choices));
    offer(
        "model",
        string(&format!(
            "Model id or alias for the session's agent. \
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
            "enum": permission_modes(agents),
            "description": "The session agent's permission mode; empty is its own default. Ignored with `dangerously_skip_permissions`."
        }),
    );
    offer(
        "dangerously_skip_permissions",
        boolean("Skip every permission prompt (`--dangerously-skip-permissions`). Replaces `permission_mode`."),
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
        boolean("The agent reaches the worktree only through the tddy-tools exec tools, never its own filesystem tools."),
    );
    offer(
        "sandboxed_codebase",
        boolean("Jail the session's checkout and run the agent beside it, unconfined, with its filesystem and shell tools withdrawn. Exclusive with `sandbox`, `managed_codebase` and `codebase_daemon_instance_id`."),
    );
    offer(
        "codebase_daemon_instance_id",
        string("The host that holds the session's worktree, when it is not the host the agent runs on. Requires `managed_codebase`. Empty keeps them together."),
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
        "run_in_tmux",
        boolean("Run the agent in a tmux session of its own on the host, as the session's OS user, so it outlives the terminal attached to it. Exclusive with `sandbox`, `sandboxed_codebase` and `codebase_daemon_instance_id`."),
    );
    offer(
        "effort",
        string("The reasoning effort level the session's agent starts at. Levels are per agent: `DescribeSessionStartOptions` lists each agent's under `effort_levels`. Empty is the agent's own default; a level the agent does not declare, or any level for an agent that declares none, refuses the start."),
    );
    offer(
        "specialized_agents",
        json!({
            "type": "array",
            "items": { "type": "string" },
            "description": "Specialized subagents to wire into the session, each a builtin or a definition under `<tddyhome>/agents/`. An unknown name refuses the start."
        }),
    );
    offer(
        "clone_placed_agents",
        json!({
            "type": "array",
            "items": { "type": "string" },
            "description": "The subset of `specialized_agents` whose conversations each work on a clone of their own; the rest work on the session's worktree. A name not in `specialized_agents` refuses the start."
        }),
    );
    offer("attachments", attachments_property());
    offer(
        "session_role",
        json!({
            "type": "string",
            "enum": [ORCHESTRATOR_ROLE],
            "description": "`orchestrator`: start an orchestrator session — read-only on the project's home worktree with main's tools, as many as wanted beside main. The daemon places it (managed, no branch, no stack, on this host) and refuses a placement, a sandbox, or a session type whose agent cannot be held read-only. Only a person, main or another orchestrator may ask for it. Omit for an ordinary session."
        }),
    );

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

/// `session_type`: with `choices`, an enum of the session types the daemon declares plus the
/// no-agent one; without, a free string the daemon validates (a client without the daemon's choices
/// asks `DescribeSessionStartOptions` for them).
fn session_type_property(choices: Option<&StartChoices>, harness_list: &str) -> Value {
    let no_agent = tddy_workflow::NO_AGENT_SESSION_TYPE;
    let Some(choices) = choices else {
        return string(&format!(
            "What to start: a session type this daemon declares, or `{no_agent}` for a worktree \
             with no agent. `DescribeSessionStartOptions` lists the declared ones in its \
             `session_types`; any other value is refused."
        ));
    };
    let declared = choices
        .session_types
        .iter()
        .map(|choice| format!("`{}` ({})", choice.session_type, choice.label))
        .collect::<Vec<_>>()
        .join(", ");
    let session_types: Vec<&str> = choices
        .session_types
        .iter()
        .map(|choice| choice.session_type.as_str())
        .chain(std::iter::once(no_agent))
        .collect();
    json!({
        "type": "string",
        "enum": session_types,
        "description": format!(
            "What to start: one of the session types this daemon declares — {declared} — or \
             `{no_agent}`, a worktree with no agent. The coding agents among them run \
             interactively in a terminal: {harness_list}. The others run the tddy-coder \
             workflow tool with `agent`."
        )
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
        "The workflow recipe a `tool` session (or a managed CLI-agent session) runs; empty is the default.",
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
