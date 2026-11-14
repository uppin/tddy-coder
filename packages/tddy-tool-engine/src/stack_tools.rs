//! The PR-stack plan and read tools, served by the daemon and addressed by stack id
//! (`#orchestrators` 7/11).
//!
//! A stack is a project record ([`tddy_core::project_stack_store`]), not a session's: any session
//! that may plan the project's work names the stack it acts on with `stack_id`, and a stack another
//! host holds with [`TARGET_DAEMON_INSTANCE_ID`] — the daemon forwards that call whole to the host
//! that owns the stack, so the port below is only ever asked about this host's stacks.
//!
//! | Tool | Stack | Writes |
//! |---|---|---|
//! | `pr_stack_list` | — (the project's stacks on the host) | — |
//! | `pr_stack_create` | — (answers the new `stack_id`) | the store |
//! | `pr_stack_status` | required | derived statuses |
//! | `pr_read`, `pr_comments` | required for `node_id`, optional for `pull_number` | — |
//! | `pr_search` | optional (the project's repository) | — |
//! | `pr_add_planned`, `pr_update_planned`, `pr_delete_planned`, `pr_set_status`, `pr_adopt` | required | the store; `pr_update_planned` with `sync_pr` also the PR's title and body |
//!
//! **Who is offered them** ([`StackCallerRole`]) is decided by role alone, never by recipe
//! (developer, 2026-11-14): an orchestrator-role session (main and every orchestrator) every one,
//! for any stack of its project — its **own stack** (the one its changeset names in `stack_id`)
//! being the default when it names none; a session bound to a node of a stack only `pr_stack_status`
//! and `pr_read`, for that stack alone. Every other session is offered none — a session with no
//! orchestrator role is offered none even when its changeset names a stack — and the daemon refuses
//! its call. A read-only session may call all of them: they write only the
//! project's stack store and, for a synced update, the PR on GitHub.
//!
//! GitHub is reached as the **project's account**, not the calling session's: a read-only session
//! holds no credential of its own.
//!
//! Feature doc: docs/ft/coder/pr-stacking.md § PR-management tools.
//! PRD: docs/ft/coder/1-WIP/PRD-2026-11-10-stack-plan-tools.md.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tddy_core::project_stack_store::StackRecord;
use tddy_core::session_metadata::is_orchestrator_role;

use crate::catalog::ToolDef;
use crate::project_home_tools::{tool_def, TARGET_DAEMON_INSTANCE_ID};
use crate::ToolOutcome;

pub const PR_STACK_LIST: &str = "pr_stack_list";
pub const PR_STACK_CREATE: &str = "pr_stack_create";
pub const PR_STACK_STATUS: &str = "pr_stack_status";
pub const PR_READ: &str = "pr_read";
pub const PR_SEARCH: &str = "pr_search";
pub const PR_COMMENTS: &str = "pr_comments";
pub const PR_ADD_PLANNED: &str = "pr_add_planned";
pub const PR_UPDATE_PLANNED: &str = "pr_update_planned";
pub const PR_DELETE_PLANNED: &str = "pr_delete_planned";
pub const PR_SET_STATUS: &str = "pr_set_status";
pub const PR_ADOPT: &str = "pr_adopt";

/// The argument naming the stack a tool acts on.
pub const STACK_ID: &str = "stack_id";

/// The eleven tools, in the order they are advertised.
pub const STACK_TOOL_NAMES: [&str; 11] = [
    PR_STACK_LIST,
    PR_STACK_CREATE,
    PR_STACK_STATUS,
    PR_READ,
    PR_SEARCH,
    PR_COMMENTS,
    PR_ADD_PLANNED,
    PR_UPDATE_PLANNED,
    PR_DELETE_PLANNED,
    PR_SET_STATUS,
    PR_ADOPT,
];

/// Whether `name` is one of the eleven stack tools.
pub fn is_stack_tool(name: &str) -> bool {
    STACK_TOOL_NAMES.contains(&name)
}

/// Whether `name` is one of the two tools a node of a stack is offered for its own stack:
/// `pr_stack_status` and `pr_read`.
pub fn is_stack_read_tool(name: &str) -> bool {
    matches!(name, PR_STACK_STATUS | PR_READ)
}

/// What a session is entitled to among the stack tools. A session with none of these roles is
/// offered no stack tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StackCallerRole {
    /// An orchestrator-role session (`is_orchestrator_role`: main or an orchestrator): every stack
    /// tool, for any stack of its project. `own_stack_id` — the stack its changeset names — is the
    /// default when a call names none; `None` when it names no stack, and then every
    /// stack-requiring call must name one.
    Orchestrator { own_stack_id: Option<String> },
    /// A session bound to a node of `stack_id`: [`is_stack_read_tool`]'s two, for that stack alone.
    StackNode { stack_id: String },
}

/// The session calling a stack tool, as the daemon classified it from its own records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackCaller {
    pub session_id: String,
    pub project_id: String,
    pub role: StackCallerRole,
}

/// The stack a node session's orchestrator planned, as the daemon read it for
/// [`stack_caller_role`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParentStack<'a> {
    pub stack_id: &'a str,
    /// Whether a node of that stack records the calling session as its `session_id`.
    pub binds_this_session: bool,
}

/// What the daemon read about a session to classify it: its `.session.yaml` role, the stack its
/// changeset names (`Changeset.stack_id`), and its orchestrator's stack when it names an
/// orchestrator. No recipe: the pr-stack recipe is deprecated and never decides a role.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StackCallerFacts<'a> {
    pub session_role: Option<&'a str>,
    pub own_stack_id: Option<&'a str>,
    pub parent_stack: Option<ParentStack<'a>>,
}

/// The role `facts` entitle a session to, or `None` for a session offered no stack tool. An
/// orchestrator role wins over a node binding; a stack the changeset names gives no role by itself.
pub fn stack_caller_role(facts: &StackCallerFacts<'_>) -> Option<StackCallerRole> {
    if facts.session_role.is_some_and(is_orchestrator_role) {
        return Some(StackCallerRole::Orchestrator {
            own_stack_id: facts.own_stack_id.map(str::to_string),
        });
    }
    facts
        .parent_stack
        .filter(|parent| parent.binds_this_session)
        .map(|parent| StackCallerRole::StackNode {
            stack_id: parent.stack_id.to_string(),
        })
}

/// Whether `role` is offered (and served) `tool_name`.
pub fn is_stack_tool_offered_to(role: &StackCallerRole, tool_name: &str) -> bool {
    match role {
        StackCallerRole::Orchestrator { .. } => is_stack_tool(tool_name),
        StackCallerRole::StackNode { .. } => {
            is_stack_tool(tool_name) && is_stack_read_tool(tool_name)
        }
    }
}

/// The stack tools `role` is offered, in [`STACK_TOOL_NAMES`] order; none for a session with no
/// stack role.
pub fn stack_tools_offered_to(role: Option<&StackCallerRole>) -> Vec<ToolDef> {
    let Some(role) = role else {
        return Vec::new();
    };
    stack_tool_defs()
        .into_iter()
        .filter(|tool| is_stack_tool_offered_to(role, &tool.name))
        .collect()
}

/// The stack `caller`'s call of `tool_name` acts on, given the `stack_id` it `named` (if any):
/// `Ok(None)` for a tool that acts on no stack (`pr_stack_list`, `pr_stack_create`, and `pr_search`
/// or a `pull_number` read with none named or defaulted). An orchestrator's own stack is its
/// default and a stack it names wins over it; a node may name only its own stack; a stack-requiring tool with none named nor
/// defaulted is refused naming `stack_id` and `pr_stack_list`.
pub fn addressed_stack_id(
    caller: &StackCaller,
    tool_name: &str,
    named: Option<&str>,
) -> Result<Option<String>, String> {
    let named = named.map(str::trim).filter(|id| !id.is_empty());
    if matches!(tool_name, PR_STACK_LIST | PR_STACK_CREATE) {
        return Ok(None);
    }
    match &caller.role {
        StackCallerRole::StackNode { stack_id } => match named {
            Some(other) if other != stack_id => Err(format!(
                "{tool_name}: stack {other} is not this session's: a session bound to a node of a \
                 stack reads only that stack ({stack_id})"
            )),
            _ => Ok(Some(stack_id.clone())),
        },
        StackCallerRole::Orchestrator { own_stack_id } => {
            let addressed = named.or(own_stack_id.as_deref());
            match addressed {
                Some(stack_id) => Ok(Some(stack_id.to_string())),
                // These three need a stack only for a node address, which the arguments decide.
                None if matches!(tool_name, PR_SEARCH | PR_READ | PR_COMMENTS) => Ok(None),
                None => Err(no_stack_named(tool_name)),
            }
        }
    }
}

/// The refusal of a call that needs a stack and was given none to act on.
fn no_stack_named(tool_name: &str) -> String {
    format!(
        "{tool_name}: name the stack with stack_id — this session owns no stack to default to; \
         pr_stack_list lists the project's stacks"
    )
}

/// Which pull request a read names: the one a node of the stack records, or one by number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrAddress {
    Node(String),
    Number(u64),
}

/// Where a stack operation runs: the caller's project, and the stack it addressed, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackScope {
    pub project_id: String,
    pub stack_id: Option<String>,
}

/// One stack operation, its arguments parsed and checked. The port does it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StackOp {
    /// `pr_stack_status`: every node with its live GitHub state, derived statuses refreshed.
    Status,
    ReadPr {
        pr: PrAddress,
        include_files: bool,
    },
    SearchPrs {
        query: Option<String>,
        /// `open` unless named.
        state: String,
        author: Option<String>,
        base: Option<String>,
        /// `0` is "no preference".
        limit: u32,
    },
    PrComments {
        pr: PrAddress,
    },
    AddPlanned {
        title: String,
        description: String,
        branch_suggestion: Option<String>,
        parents: Vec<String>,
    },
    UpdatePlanned {
        node_id: String,
        title: Option<String>,
        description: Option<String>,
        branch_suggestion: Option<String>,
        sync_pr: bool,
    },
    DeletePlanned {
        node_id: String,
    },
    SetStatus {
        node_id: String,
        kind: String,
        note: Option<String>,
    },
    Adopt {
        pull_number: u64,
        parents: Vec<String>,
    },
}

/// What the tools need from the daemon: this host's stacks of a project, and the operations on one.
/// Every answer is about this host — a call naming another host was forwarded before it got here.
#[async_trait]
pub trait StackToolPort: Send + Sync {
    /// This host, as the eligible-daemon roster names it; every stack `pr_stack_list` answers
    /// carries it.
    fn daemon_instance_id(&self) -> String;

    /// Every stack of `project_id` on this host.
    async fn list_stacks(&self, project_id: &str) -> Result<Vec<StackRecord>, String>;

    /// A new, empty stack of `caller`'s project, created by `caller` and titled `title`. Creating a
    /// stack never makes it the caller's own stack.
    async fn create_stack(
        &self,
        caller: &StackCaller,
        title: Option<&str>,
    ) -> Result<StackRecord, String>;

    /// Run `op` in `scope`, answering the tool's JSON. An unknown stack, an unknown node and a
    /// GitHub refusal are the error.
    async fn run_stack_op(&self, scope: &StackScope, op: StackOp) -> Result<Value, String>;
}

/// The eleven tools' definitions. Every one takes [`TARGET_DAEMON_INSTANCE_ID`]; every one but the
/// two project-level tools takes [`STACK_ID`].
pub fn stack_tool_defs() -> Vec<ToolDef> {
    let node_id = |what: &str| json!({ "type": "string", "description": what });
    let parents = json!({
        "type": "array",
        "items": { "type": "string" },
        "description": "Parent node ids (chosen ancestors); empty for a root node."
    });
    vec![
        tool_def(
            PR_STACK_LIST,
            "List the PR stacks of this project on a host — this one unless daemon_instance_id names another — each with its stack_id, title, the host that holds it, the session that created it and its node count. Name a stack by its stack_id (and daemon_instance_id) in the other pr_ tools.",
            object(json!({}), &[]),
        ),
        tool_def(
            PR_STACK_CREATE,
            "Create a new, empty PR stack in this project, on this host or the one daemon_instance_id names, with an optional title saying what it is for. Answers its stack_id; add planned PRs to it with pr_add_planned.",
            object(
                json!({ "title": { "type": "string", "description": "What the stack is for; pr_stack_list shows it." } }),
                &[],
            ),
        ),
        tool_def(
            PR_STACK_STATUS,
            "List every PR node in a stack with its live GitHub state and computed internal status (needs-repoint / has-conflicts / ready-to-merge / merged / up-to-date). Refreshes and persists derived statuses; agent overrides are preserved.",
            stack_object(json!({}), &[]),
        ),
        tool_def(
            PR_READ,
            "Read one pull request in full: title, body, state, base/head, mergeability, size, the latest review state per reviewer, and the head commit's check runs. Address it by exactly one of node_id (a node of stack_id) or pull_number — naming neither or both is rejected, and a node that records no PR url cannot be addressed by node_id. Pass include_files for the changed-file list, which is otherwise neither fetched nor returned.",
            stack_object(
                json!({
                    "node_id": node_id("Stack node id whose PR to read; name this or pull_number, not both."),
                    "pull_number": { "type": "integer", "minimum": 1, "description": "Pull request number to read; name this or node_id, not both." },
                    "include_files": { "type": "boolean", "description": "Include the changed-file list (omitted by default)." }
                }),
                &[],
            ),
        ),
        tool_def(
            PR_SEARCH,
            "Search this project's repository's pull requests, including ones no stack tracks, by text, state (open/closed/merged/all, default open), author and base branch. The repository is always the project's own and cannot be chosen. Returns at most limit hits (default 20, hard cap 100), each with number, title, state, draft, author, url and updated_at — GitHub's search reports no head or base branch, so follow up with pr_read when you need them.",
            stack_object(
                json!({
                    "query": { "type": "string", "description": "Free text matched against PR titles and bodies." },
                    "state": { "type": "string", "enum": ["open", "closed", "merged", "all"], "description": "Default open." },
                    "author": { "type": "string", "description": "Restrict to PRs opened by this GitHub login." },
                    "base": { "type": "string", "description": "Restrict to PRs whose base branch is this one." },
                    "limit": { "type": "integer", "minimum": 0, "description": "Maximum hits to return (default 20, hard cap 100)." }
                }),
                &[],
            ),
        ),
        tool_def(
            PR_COMMENTS,
            "Read a pull request's review feedback as three separate sections: submitted reviews, diff-anchored comment threads in reply order, and conversation comments. Address it by exactly one of node_id (a node of stack_id) or pull_number — naming neither or both is rejected. A thread's resolved/unresolved state is not available over this API, so no thread reports one: read the replies to judge.",
            stack_object(
                json!({
                    "node_id": node_id("Stack node id whose PR feedback to read; name this or pull_number, not both."),
                    "pull_number": { "type": "integer", "minimum": 1, "description": "Pull request number to read; name this or node_id, not both." }
                }),
                &[],
            ),
        ),
        tool_def(
            PR_ADD_PLANNED,
            "Add a new planned PR node to a stack, choosing its ancestors from existing node ids. Returns the server-assigned node id.",
            stack_object(
                json!({
                    "title": { "type": "string", "description": "PR title." },
                    "description": { "type": "string", "description": "PR description / body." },
                    "branch_suggestion": { "type": "string", "description": "Optional suggested branch name (feature/<stack>/<node>)." },
                    "parents": parents.clone()
                }),
                &["title", "description"],
            ),
        ),
        tool_def(
            PR_UPDATE_PLANNED,
            "Edit a stack node's title, description and/or branch_suggestion. Title and description are editable at any time, including once the node owns a branch, a child session and an open PR; branch_suggestion only while the node owns no branch. A call naming none of the three is rejected. With sync_pr, the same title/description are also pushed to the node's pull request, as the project's GitHub account — rejected, before anything is written, when the node records no PR or when neither a title nor a description was given.",
            stack_object(
                json!({
                    "node_id": node_id("Stack node id to edit."),
                    "title": { "type": "string", "description": "New PR title; omit to leave it unchanged." },
                    "description": { "type": "string", "description": "New PR description / body; omit to leave it unchanged." },
                    "branch_suggestion": { "type": "string", "description": "New suggested branch name; editable only while the node owns no branch." },
                    "sync_pr": { "type": "boolean", "description": "Also push the new title/description to the node's pull request. Rejected when the node has no PR." }
                }),
                &["node_id"],
            ),
        ),
        tool_def(
            PR_DELETE_PLANNED,
            "Remove a node from a stack's plan, reparenting its children onto that node's parents so the DAG stays connected (a deleted root's children become roots). Refuses a node whose PR is open — merge or close it first. The node's branch, worktree and child session are left untouched and reported back as now unowned.",
            stack_object(
                json!({ "node_id": node_id("Stack node id to remove from the plan.") }),
                &["node_id"],
            ),
        ),
        tool_def(
            PR_SET_STATUS,
            "Record a manual internal-status override on a stack node (e.g. blocked) with an optional note. Overrides are not overwritten by automatic derivation.",
            stack_object(
                json!({
                    "node_id": node_id("Stack node id to annotate."),
                    "kind": { "type": "string", "description": "Internal status kind (e.g. blocked, needs-repoint, has-conflicts, ready-to-merge, up-to-date, merged)." },
                    "note": { "type": "string", "description": "Optional free-text note explaining the status." }
                }),
                &["node_id", "kind"],
            ),
        ),
        tool_def(
            PR_ADOPT,
            "Bring an existing pull request into a stack as a node bound to its head branch and PR reference, carrying the PR's title, body and live phase, choosing which existing nodes it stacks on (empty for a root). Refuses a PR whose head branch is already bound to a node, and an unknown or cycle-forming parent. The adopted node has no child session.",
            stack_object(
                json!({
                    "pull_number": { "type": "integer", "minimum": 1, "description": "Number of the existing pull request to bring into the stack." },
                    "parents": parents
                }),
                &["pull_number"],
            ),
        ),
    ]
}

/// A tool schema of `properties` plus the host argument.
fn object(properties: Value, required: &[&str]) -> Value {
    let mut properties = properties;
    properties[TARGET_DAEMON_INSTANCE_ID] = json!({
        "type": "string",
        "description": "The host that holds the stack, as pr_stack_list reports it in its daemon_instance_id; empty for this host."
    });
    json!({ "type": "object", "properties": properties, "required": required })
}

/// [`object`] plus [`STACK_ID`].
fn stack_object(properties: Value, required: &[&str]) -> Value {
    let mut properties = properties;
    properties[STACK_ID] = json!({
        "type": "string",
        "description": "The stack, as pr_stack_list or pr_stack_create answers its stack_id. The calling session's own stack (the one its changeset names) when omitted."
    });
    object(properties, required)
}

/// Run one of the eleven stack tools for `caller`.
///
/// The daemon has already refused a caller with no stack role and a tool its role is not offered;
/// bad arguments, an addressing refusal and a refusal of the port come back as an `is_error`
/// outcome naming what was refused.
pub async fn execute_stack_tool(
    port: &dyn StackToolPort,
    caller: &StackCaller,
    tool_name: &str,
    args_json: &str,
) -> ToolOutcome {
    match run(port, caller, tool_name, args_json).await {
        Ok(answer) => ToolOutcome::ok(answer.to_string()),
        Err(message) => ToolOutcome::err(message),
    }
}

/// Every argument any stack tool takes; each tool reads the ones it names and refuses the missing
/// ones it requires ([`required`]).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct StackToolArgs {
    stack_id: Option<String>,
    node_id: Option<String>,
    pull_number: Option<u64>,
    include_files: bool,
    query: Option<String>,
    state: Option<String>,
    author: Option<String>,
    base: Option<String>,
    limit: u32,
    title: Option<String>,
    description: Option<String>,
    branch_suggestion: Option<String>,
    parents: Vec<String>,
    sync_pr: bool,
    kind: Option<String>,
    note: Option<String>,
}

/// The state a `pr_search` that expressed no preference is run with: the PRs still in play.
const DEFAULT_SEARCH_STATE: &str = "open";

/// `value`, or the refusal naming the argument `tool_name` requires.
fn required<T>(tool_name: &str, value: Option<T>, argument: &str) -> Result<T, String> {
    value.ok_or_else(|| format!("{tool_name}: invalid arguments: missing field `{argument}`"))
}

/// The pull request a read names: exactly one of `node_id` and `pull_number`.
fn pr_address_of(tool_name: &str, args: &StackToolArgs) -> Result<PrAddress, String> {
    match (&args.node_id, args.pull_number) {
        (Some(node_id), None) => Ok(PrAddress::Node(node_id.clone())),
        (None, Some(number)) => Ok(PrAddress::Number(number)),
        _ => Err(format!(
            "{tool_name}: name exactly one of node_id and pull_number — naming neither or both is \
             rejected"
        )),
    }
}

/// The operation `tool_name` asks for, its arguments checked. The two project-level tools are not
/// operations on a stack and are answered by [`run`] itself.
fn stack_op_of(tool_name: &str, args: StackToolArgs) -> Result<StackOp, String> {
    Ok(match tool_name {
        PR_STACK_STATUS => StackOp::Status,
        PR_READ => StackOp::ReadPr {
            pr: pr_address_of(tool_name, &args)?,
            include_files: args.include_files,
        },
        PR_COMMENTS => StackOp::PrComments {
            pr: pr_address_of(tool_name, &args)?,
        },
        PR_SEARCH => StackOp::SearchPrs {
            query: args.query,
            state: args
                .state
                .unwrap_or_else(|| DEFAULT_SEARCH_STATE.to_string()),
            author: args.author,
            base: args.base,
            limit: args.limit,
        },
        PR_ADD_PLANNED => StackOp::AddPlanned {
            title: required(tool_name, args.title, "title")?,
            description: required(tool_name, args.description, "description")?,
            branch_suggestion: args.branch_suggestion,
            parents: args.parents,
        },
        PR_UPDATE_PLANNED => StackOp::UpdatePlanned {
            node_id: required(tool_name, args.node_id, "node_id")?,
            title: args.title,
            description: args.description,
            branch_suggestion: args.branch_suggestion,
            sync_pr: args.sync_pr,
        },
        PR_DELETE_PLANNED => StackOp::DeletePlanned {
            node_id: required(tool_name, args.node_id, "node_id")?,
        },
        PR_SET_STATUS => StackOp::SetStatus {
            node_id: required(tool_name, args.node_id, "node_id")?,
            kind: required(tool_name, args.kind, "kind")?,
            note: args.note,
        },
        PR_ADOPT => StackOp::Adopt {
            pull_number: required(tool_name, args.pull_number, "pull_number")?,
            parents: args.parents,
        },
        other => return Err(format!("unknown stack tool: {other}")),
    })
}

async fn run(
    port: &dyn StackToolPort,
    caller: &StackCaller,
    tool_name: &str,
    args_json: &str,
) -> Result<Value, String> {
    let args: StackToolArgs = if args_json.trim().is_empty() {
        StackToolArgs::default()
    } else {
        serde_json::from_str(args_json)
            .map_err(|e| format!("{tool_name}: invalid arguments: {e}"))?
    };
    let refused = |why: String| format!("{tool_name}: {why}");
    match tool_name {
        PR_STACK_LIST => {
            let stacks = port
                .list_stacks(&caller.project_id)
                .await
                .map_err(refused)?;
            let host = port.daemon_instance_id();
            Ok(json!({ "stacks": stacks.iter().map(|record| json!({
                "stack_id": record.stack_id,
                "title": record.title,
                "daemon_instance_id": host,
                "created_by_session_id": record.created_by_session_id,
                "created_at_unix_ms": record.created_at_unix_ms,
                "node_count": record.stack.nodes.len(),
            })).collect::<Vec<_>>() }))
        }
        PR_STACK_CREATE => {
            let title = args.title.as_deref().filter(|t| !t.trim().is_empty());
            let record = port.create_stack(caller, title).await.map_err(refused)?;
            Ok(json!({ "stack_id": record.stack_id }))
        }
        _ => {
            let named = args.stack_id.clone();
            let stack_id = addressed_stack_id(caller, tool_name, named.as_deref())?;
            let op = stack_op_of(tool_name, args)?;
            let reads_a_node = matches!(
                op,
                StackOp::ReadPr {
                    pr: PrAddress::Node(_),
                    ..
                } | StackOp::PrComments {
                    pr: PrAddress::Node(_)
                }
            );
            if stack_id.is_none() && reads_a_node {
                return Err(no_stack_named(tool_name));
            }
            let scope = StackScope {
                project_id: caller.project_id.clone(),
                stack_id,
            };
            port.run_stack_op(&scope, op).await.map_err(refused)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use pretty_assertions::assert_eq;
    use tddy_core::changeset::Stack;

    use super::*;

    const OWN_STACK: &str = "stack-own";
    const OTHER_STACK: &str = "stack-other";

    // ── classification ────────────────────────────────────────────────────────────────────────

    #[test]
    fn main_and_an_orchestrator_naming_no_stack_are_orchestrators_owning_none() {
        // Given main's role and an orchestrator's, neither changeset naming a stack
        let roles = ["project-home", "orchestrator"].map(|role| {
            stack_caller_role(&StackCallerFacts {
                session_role: Some(role),
                ..StackCallerFacts::default()
            })
        });

        // Then both are orchestrators with no own stack
        assert_eq!(
            roles,
            [
                Some(StackCallerRole::Orchestrator { own_stack_id: None }),
                Some(StackCallerRole::Orchestrator { own_stack_id: None })
            ]
        );
    }

    #[test]
    fn an_orchestrator_whose_changeset_names_a_stack_owns_it() {
        // Given an orchestrator whose changeset names its stack
        let facts = StackCallerFacts {
            session_role: Some("orchestrator"),
            own_stack_id: Some(OWN_STACK),
            ..StackCallerFacts::default()
        };

        // Then that stack is its own
        assert_eq!(
            stack_caller_role(&facts),
            Some(StackCallerRole::Orchestrator {
                own_stack_id: Some(OWN_STACK.to_string())
            })
        );
    }

    #[test]
    fn a_session_with_no_orchestrator_role_is_offered_nothing_for_the_stack_it_names() {
        // Given a session with no role whose changeset names a stack
        let facts = StackCallerFacts {
            own_stack_id: Some(OWN_STACK),
            ..StackCallerFacts::default()
        };

        // Then naming a stack gives it no stack role
        assert_eq!(stack_caller_role(&facts), None);
    }

    #[test]
    fn a_session_bound_to_a_node_of_its_orchestrators_stack_is_a_node_of_that_stack() {
        // Given a session whose orchestrator's stack binds a node to it
        let facts = StackCallerFacts {
            parent_stack: Some(ParentStack {
                stack_id: OWN_STACK,
                binds_this_session: true,
            }),
            ..StackCallerFacts::default()
        };

        // Then it is a node of that stack
        assert_eq!(
            stack_caller_role(&facts),
            Some(StackCallerRole::StackNode {
                stack_id: OWN_STACK.to_string()
            })
        );
    }

    #[test]
    fn a_session_its_orchestrators_stack_binds_no_node_to_has_no_stack_role() {
        // Given a session started by an orchestrator whose stack binds no node to it
        let facts = StackCallerFacts {
            parent_stack: Some(ParentStack {
                stack_id: OWN_STACK,
                binds_this_session: false,
            }),
            ..StackCallerFacts::default()
        };

        // Then it is offered no stack tool
        assert_eq!(stack_caller_role(&facts), None);
    }

    #[test]
    fn an_orchestrator_role_wins_over_a_node_binding() {
        // Given an orchestrator-role session owning a stack and bound to a node of another
        let facts = StackCallerFacts {
            session_role: Some("orchestrator"),
            own_stack_id: Some(OWN_STACK),
            parent_stack: Some(ParentStack {
                stack_id: OTHER_STACK,
                binds_this_session: true,
            }),
        };

        // Then its role is what decides
        assert_eq!(
            stack_caller_role(&facts),
            Some(StackCallerRole::Orchestrator {
                own_stack_id: Some(OWN_STACK.to_string())
            })
        );
    }

    #[test]
    fn an_ordinary_session_has_no_stack_role() {
        // Given a session with no role, no stack and no orchestrator
        let facts = StackCallerFacts::default();

        // Then it is offered no stack tool
        assert_eq!(stack_caller_role(&facts), None);
    }

    // ── offering ──────────────────────────────────────────────────────────────────────────────

    #[test]
    fn a_node_is_served_the_two_reads_and_no_writer() {
        // Given a node of a stack
        let node = StackCallerRole::StackNode {
            stack_id: OWN_STACK.to_string(),
        };

        // When each tool is checked
        let served: Vec<&str> = STACK_TOOL_NAMES
            .into_iter()
            .filter(|tool| is_stack_tool_offered_to(&node, tool))
            .collect();

        // Then only status and read are served
        assert_eq!(served, vec![PR_STACK_STATUS, PR_READ]);
    }

    #[test]
    fn a_tool_outside_the_family_is_offered_to_no_caller() {
        // Given an orchestrator, who is offered every stack tool
        // Then a project-home tool and a git tool n10 moves are not stack tools it is offered here
        assert!(!is_stack_tool_offered_to(
            &StackCallerRole::Orchestrator { own_stack_id: None },
            "ListProjectSessions"
        ));
        assert!(!is_stack_tool_offered_to(
            &StackCallerRole::Orchestrator { own_stack_id: None },
            "pr_merge"
        ));
    }

    // ── addressing ────────────────────────────────────────────────────────────────────────────

    fn a_caller(role: StackCallerRole) -> StackCaller {
        StackCaller {
            session_id: "caller".to_string(),
            project_id: "proj".to_string(),
            role,
        }
    }

    fn an_orchestrator_owning_a_stack() -> StackCaller {
        a_caller(StackCallerRole::Orchestrator {
            own_stack_id: Some(OWN_STACK.to_string()),
        })
    }

    fn an_orchestrator_owning_none() -> StackCaller {
        a_caller(StackCallerRole::Orchestrator { own_stack_id: None })
    }

    fn a_node() -> StackCaller {
        a_caller(StackCallerRole::StackNode {
            stack_id: OWN_STACK.to_string(),
        })
    }

    #[test]
    fn an_orchestrator_acts_on_the_stack_it_names() {
        // Given an orchestrator naming a stack
        let caller = an_orchestrator_owning_none();

        // When a planning tool is addressed
        let addressed = addressed_stack_id(&caller, PR_ADD_PLANNED, Some(OTHER_STACK));

        // Then it is that stack
        assert_eq!(addressed, Ok(Some(OTHER_STACK.to_string())));
    }

    #[test]
    fn an_orchestrator_naming_no_stack_is_refused_a_stack_tool_and_told_how_to_find_one() {
        // Given an orchestrator owning no stack, naming none
        let caller = an_orchestrator_owning_none();

        // When a stack-requiring tool is addressed
        let refusal = addressed_stack_id(&caller, PR_STACK_STATUS, None)
            .expect_err("an orchestrator has no default stack");

        // Then the refusal names the argument and the tool that lists the stacks
        assert!(refusal.contains("stack_id"), "{refusal}");
        assert!(refusal.contains("pr_stack_list"), "{refusal}");
    }

    #[test]
    fn an_orchestrator_naming_no_stack_acts_on_its_own() {
        // Given an orchestrator owning a stack, naming none
        let caller = an_orchestrator_owning_a_stack();

        // When a planning tool is addressed
        let addressed = addressed_stack_id(&caller, PR_ADD_PLANNED, None);

        // Then it is its own stack
        assert_eq!(addressed, Ok(Some(OWN_STACK.to_string())));
    }

    #[test]
    fn an_orchestrator_owning_a_stack_may_name_another_of_its_project() {
        // Given an orchestrator owning a stack, naming another
        let caller = an_orchestrator_owning_a_stack();

        // When a read is addressed
        let addressed = addressed_stack_id(&caller, PR_STACK_STATUS, Some(OTHER_STACK));

        // Then the named stack wins over its default
        assert_eq!(addressed, Ok(Some(OTHER_STACK.to_string())));
    }

    #[test]
    fn a_node_reads_its_own_stack_whether_or_not_it_names_it() {
        // Given a node of a stack
        let caller = a_node();

        // When it reads its stack, naming it and not
        let addressed = [
            addressed_stack_id(&caller, PR_STACK_STATUS, None),
            addressed_stack_id(&caller, PR_STACK_STATUS, Some(OWN_STACK)),
        ];

        // Then both are its own stack
        assert_eq!(
            addressed,
            [
                Ok(Some(OWN_STACK.to_string())),
                Ok(Some(OWN_STACK.to_string()))
            ]
        );
    }

    #[test]
    fn a_node_naming_another_stack_is_refused_naming_that_stack() {
        // Given a node of a stack naming another one
        let caller = a_node();

        // When it reads the other stack's status
        let refusal = addressed_stack_id(&caller, PR_STACK_STATUS, Some(OTHER_STACK))
            .expect_err("a node reads its own stack alone");

        // Then the refusal names the stack it may not read
        assert!(refusal.contains(OTHER_STACK), "{refusal}");
    }

    #[test]
    fn the_project_level_tools_address_no_stack() {
        // Given an orchestrator owning a stack, the default of every other tool
        let caller = an_orchestrator_owning_a_stack();

        // When the two project-level tools are addressed
        let addressed = [
            addressed_stack_id(&caller, PR_STACK_LIST, None),
            addressed_stack_id(&caller, PR_STACK_CREATE, None),
        ];

        // Then neither acts on a stack
        assert_eq!(addressed, [Ok(None), Ok(None)]);
    }

    #[test]
    fn an_orchestrator_searches_the_project_with_no_stack_named() {
        // Given an orchestrator naming no stack
        let caller = an_orchestrator_owning_none();

        // When it searches
        let addressed = addressed_stack_id(&caller, PR_SEARCH, None);

        // Then no stack is needed
        assert_eq!(addressed, Ok(None));
    }

    // ── execution over a port ─────────────────────────────────────────────────────────────────

    /// A port that records every operation it is asked and answers each with `{"ran": <tool>}`.
    #[derive(Default)]
    struct RecordingPort {
        ops: Mutex<Vec<(StackScope, StackOp)>>,
        created: Mutex<Vec<(StackCaller, Option<String>)>>,
        refusal: Option<String>,
    }

    impl RecordingPort {
        fn refusing(why: &str) -> Self {
            Self {
                refusal: Some(why.to_string()),
                ..Self::default()
            }
        }

        fn ops(&self) -> Vec<(StackScope, StackOp)> {
            self.ops.lock().unwrap().clone()
        }
    }

    fn a_record(stack_id: &str, nodes: usize) -> StackRecord {
        StackRecord {
            stack_id: stack_id.to_string(),
            project_id: "proj".to_string(),
            created_by_session_id: "main".to_string(),
            created_at_unix_ms: 1_700_000_000_000,
            title: (stack_id == OWN_STACK).then(|| "Orchestrator sessions".to_string()),
            stack: Stack {
                version: 1,
                nodes: (0..nodes)
                    .map(|i| tddy_core::changeset::StackNode {
                        node_id: format!("n{}", i + 1),
                        ..Default::default()
                    })
                    .collect(),
            },
        }
    }

    #[async_trait]
    impl StackToolPort for RecordingPort {
        fn daemon_instance_id(&self) -> String {
            "host-a".to_string()
        }

        async fn list_stacks(&self, _project_id: &str) -> Result<Vec<StackRecord>, String> {
            Ok(vec![a_record(OWN_STACK, 2), a_record(OTHER_STACK, 0)])
        }

        async fn create_stack(
            &self,
            caller: &StackCaller,
            title: Option<&str>,
        ) -> Result<StackRecord, String> {
            self.created
                .lock()
                .unwrap()
                .push((caller.clone(), title.map(str::to_string)));
            Ok(a_record("stack-new", 0))
        }

        async fn run_stack_op(&self, scope: &StackScope, op: StackOp) -> Result<Value, String> {
            if let Some(why) = &self.refusal {
                return Err(why.clone());
            }
            self.ops.lock().unwrap().push((scope.clone(), op));
            Ok(json!({ "ran": true }))
        }
    }

    fn answer_of(outcome: &ToolOutcome) -> Value {
        assert!(!outcome.is_error, "{}", outcome.error_message);
        serde_json::from_str(&outcome.result_json).expect("the answer is JSON")
    }

    fn own_scope() -> StackScope {
        StackScope {
            project_id: "proj".to_string(),
            stack_id: Some(OWN_STACK.to_string()),
        }
    }

    #[tokio::test]
    async fn pr_stack_create_answers_the_stack_the_port_created_for_the_caller() {
        // Given an orchestrator
        let port = RecordingPort::default();
        let caller = an_orchestrator_owning_none();

        // When it creates a stack with no title
        let outcome = execute_stack_tool(&port, &caller, PR_STACK_CREATE, "{}").await;

        // Then the answer is the new stack's id, created untitled for that caller
        assert_eq!(answer_of(&outcome), json!({ "stack_id": "stack-new" }));
        assert_eq!(*port.created.lock().unwrap(), vec![(caller, None)]);
    }

    #[tokio::test]
    async fn pr_stack_create_hands_the_port_the_title_it_names() {
        // Given an orchestrator
        let port = RecordingPort::default();
        let caller = an_orchestrator_owning_none();

        // When it creates a stack titled for what it is for
        execute_stack_tool(
            &port,
            &caller,
            PR_STACK_CREATE,
            &json!({ "title": "Orchestrator sessions" }).to_string(),
        )
        .await;

        // Then the port created it with that title
        assert_eq!(
            *port.created.lock().unwrap(),
            vec![(caller, Some("Orchestrator sessions".to_string()))]
        );
    }

    #[test]
    fn pr_stack_create_takes_an_optional_title() {
        // Given the tools' definitions
        let create = stack_tool_defs()
            .into_iter()
            .find(|tool| tool.name == PR_STACK_CREATE)
            .expect("pr_stack_create is defined");
        let schema: Value = serde_json::from_str(&create.input_schema_json).unwrap();

        // Then it takes a string title, and requires nothing
        assert_eq!(
            (&schema["properties"]["title"]["type"], &schema["required"]),
            (&json!("string"), &json!([]))
        );
    }

    #[tokio::test]
    async fn pr_stack_list_answers_every_stack_with_the_host_that_holds_it() {
        // Given an orchestrator, and a host holding two stacks
        let port = RecordingPort::default();
        let caller = an_orchestrator_owning_none();

        // When it lists the stacks
        let outcome = execute_stack_tool(&port, &caller, PR_STACK_LIST, "{}").await;

        // Then each is listed with its id, title, host, creator, creation time and node count
        assert_eq!(
            answer_of(&outcome),
            json!({ "stacks": [
                {
                    "stack_id": OWN_STACK,
                    "title": "Orchestrator sessions",
                    "daemon_instance_id": "host-a",
                    "created_by_session_id": "main",
                    "created_at_unix_ms": 1_700_000_000_000u64,
                    "node_count": 2,
                },
                {
                    "stack_id": OTHER_STACK,
                    "title": null,
                    "daemon_instance_id": "host-a",
                    "created_by_session_id": "main",
                    "created_at_unix_ms": 1_700_000_000_000u64,
                    "node_count": 0,
                },
            ] })
        );
    }

    #[tokio::test]
    async fn pr_add_planned_is_run_on_the_addressed_stack_with_its_arguments() {
        // Given an orchestrator owning a stack
        let port = RecordingPort::default();
        let caller = an_orchestrator_owning_a_stack();

        // When it plans a PR on its own stack
        let outcome = execute_stack_tool(
            &port,
            &caller,
            PR_ADD_PLANNED,
            &json!({ "title": "Tools", "description": "the tools", "parents": ["n1"] }).to_string(),
        )
        .await;

        // Then the port planned it there, and its answer is the tool's
        assert_eq!(answer_of(&outcome), json!({ "ran": true }));
        assert_eq!(
            port.ops(),
            vec![(
                own_scope(),
                StackOp::AddPlanned {
                    title: "Tools".to_string(),
                    description: "the tools".to_string(),
                    branch_suggestion: None,
                    parents: vec!["n1".to_string()],
                }
            )]
        );
    }

    #[tokio::test]
    async fn pr_search_names_open_when_no_state_is_given() {
        // Given an orchestrator
        let port = RecordingPort::default();
        let caller = an_orchestrator_owning_none();

        // When it searches by text alone
        execute_stack_tool(
            &port,
            &caller,
            PR_SEARCH,
            &json!({ "query": "store" }).to_string(),
        )
        .await;

        // Then the search is of open PRs in the project, with no stack and no limit preference
        assert_eq!(
            port.ops(),
            vec![(
                StackScope {
                    project_id: "proj".to_string(),
                    stack_id: None,
                },
                StackOp::SearchPrs {
                    query: Some("store".to_string()),
                    state: "open".to_string(),
                    author: None,
                    base: None,
                    limit: 0,
                }
            )]
        );
    }

    #[tokio::test]
    async fn pr_read_by_node_is_run_as_a_node_address() {
        // Given a node of a stack
        let port = RecordingPort::default();
        let caller = a_node();

        // When it reads a node's PR with its files
        execute_stack_tool(
            &port,
            &caller,
            PR_READ,
            &json!({ "node_id": "n2", "include_files": true }).to_string(),
        )
        .await;

        // Then the port reads that node's PR on its stack
        assert_eq!(
            port.ops(),
            vec![(
                own_scope(),
                StackOp::ReadPr {
                    pr: PrAddress::Node("n2".to_string()),
                    include_files: true,
                }
            )]
        );
    }

    #[tokio::test]
    async fn a_read_naming_both_a_node_and_a_pull_number_is_refused_before_the_port_is_asked() {
        // Given an orchestrator
        let port = RecordingPort::default();
        let caller = an_orchestrator_owning_none();

        // When it reads a PR naming both a node and a number
        let outcome = execute_stack_tool(
            &port,
            &caller,
            PR_COMMENTS,
            &json!({ "stack_id": OWN_STACK, "node_id": "n1", "pull_number": 7 }).to_string(),
        )
        .await;

        // Then it is refused, and nothing was asked
        assert!(outcome.is_error, "{}", outcome.result_json);
        assert!(
            outcome.error_message.contains("exactly one"),
            "{}",
            outcome.error_message
        );
        assert_eq!(port.ops(), Vec::new());
    }

    #[tokio::test]
    async fn a_planning_call_missing_its_title_is_refused_as_invalid_arguments() {
        // Given an orchestrator owning a stack
        let port = RecordingPort::default();
        let caller = an_orchestrator_owning_a_stack();

        // When it plans a PR with no title
        let outcome = execute_stack_tool(
            &port,
            &caller,
            PR_ADD_PLANNED,
            &json!({ "description": "the tools" }).to_string(),
        )
        .await;

        // Then the arguments are refused, naming the tool, and nothing was asked
        assert!(outcome.is_error, "{}", outcome.result_json);
        assert!(
            outcome
                .error_message
                .starts_with("pr_add_planned: invalid arguments"),
            "{}",
            outcome.error_message
        );
        assert_eq!(port.ops(), Vec::new());
    }

    #[tokio::test]
    async fn the_ports_refusal_is_the_tools_error() {
        // Given a port that refuses, naming why
        let port = RecordingPort::refusing("project proj holds no stack stack-own");
        let caller = an_orchestrator_owning_a_stack();

        // When a node is deleted from the stack
        let outcome = execute_stack_tool(
            &port,
            &caller,
            PR_DELETE_PLANNED,
            &json!({ "node_id": "n1" }).to_string(),
        )
        .await;

        // Then the tool's error is the port's reason, prefixed by the tool
        assert!(outcome.is_error, "{}", outcome.result_json);
        assert_eq!(
            outcome.error_message,
            "pr_delete_planned: project proj holds no stack stack-own"
        );
    }
}
