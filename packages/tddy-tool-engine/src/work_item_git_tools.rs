//! The PR-stack git tools, run by the daemon in each node's own worktree (`#orchestrators` 10/11).
//!
//! The siblings of the plan and read tools ([`crate::work_queue_tools`], `#orchestrators` 7/11): the
//! same `queue_id` addressing, the same access to the addressed queue and the same forwarding of
//! a call naming another host with [`TARGET_DAEMON_INSTANCE_ID`] — but every one of these six
//! changes git or a pull request, so they are offered to the **owner** of a queue alone, never to a
//! session it merely addresses.
//!
//! | Tool | Git, on the daemon | GitHub, as the project's account |
//! |---|---|---|
//! | `work_item_merge` | — | merges the node's PR; the node reads `merged` |
//! | `work_item_close` | — | closes the node's PR; the node reads `closed` |
//! | `work_item_repoint` | rebases the node's branch onto its effective base (or `new_base`) in the node's worktree, force-pushes with lease to the project's remote | retargets the PR's base |
//! | `work_item_set_parents` | as `work_item_repoint`, after the parents change | as `work_item_repoint` |
//! | `work_item_resolve_conflicts` | tries the base merge against the node's branch, leaves the worktree as it was | — ; `delegate` asks the node's session to resolve the paths |
//! | `work_item_spawn_session` | the node's worktree is created by the daemon's start path | — |
//!
//! Git never runs in the caller's checkout: a read-only orchestrator has none to run it in. A node
//! whose worktree is dirty or whose session is running is refused a rebase, naming why, and left
//! untouched.
//!
//! Feature doc: docs/ft/coder/pr-stacking.md § Merge and repoint.

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::catalog::ToolDef;
use crate::project_home_tools::{tool_def, TARGET_DAEMON_INSTANCE_ID};
use crate::work_queue_access::WorkQueueAccess;
use crate::work_queue_land_tools::{
    work_queue_land_tool_defs, WORK_ITEM_PROPAGATE, WORK_QUEUE_LAND,
};
use crate::work_queue_tools::{WorkQueueCaller, STACK_ID};
use crate::ToolOutcome;

pub const PR_MERGE: &str = "work_item_merge";
pub const PR_CLOSE: &str = "work_item_close";
pub const PR_REPOINT: &str = "work_item_repoint";
pub const PR_SET_PARENTS: &str = "work_item_set_parents";
pub const PR_RESOLVE_CONFLICTS: &str = "work_item_resolve_conflicts";
pub const PR_SPAWN_CHILD: &str = "work_item_spawn_session";

/// The eight tools, in the order they are advertised.
pub const WORK_ITEM_GIT_TOOL_NAMES: [&str; 8] = [
    PR_MERGE,
    PR_CLOSE,
    PR_REPOINT,
    PR_SET_PARENTS,
    PR_RESOLVE_CONFLICTS,
    PR_SPAWN_CHILD,
    WORK_ITEM_PROPAGATE,
    WORK_QUEUE_LAND,
];

/// Whether `name` is one of the eight stack git tools.
pub fn is_stack_git_tool(name: &str) -> bool {
    WORK_ITEM_GIT_TOOL_NAMES.contains(&name)
}

/// Whether a session with `access` is offered (and served) the stack git tool `tool_name`: the
/// owner every one, an addressed session none.
pub fn is_stack_git_tool_offered_to(access: WorkQueueAccess, tool_name: &str) -> bool {
    access == WorkQueueAccess::Owner && is_stack_git_tool(tool_name)
}

/// One stack git operation on a node of the addressed stack, its arguments parsed and checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkItemGitOp {
    /// `work_item_merge`: merge the node's open PR; the node reads `merged`.
    Merge { node_id: String },
    /// `work_item_close`: close the node's open PR without merging; the node reads `closed`.
    Close { node_id: String },
    /// `work_item_repoint`: rebase the node's branch onto `new_base` — its effective base when `None` — in
    /// its worktree, force-push it with lease, and retarget its PR.
    Repoint {
        node_id: String,
        new_base: Option<String>,
    },
    /// `work_item_set_parents`: give the node a whole new parent list, then realign it as `Repoint` does
    /// when it owns a branch.
    SetParents {
        node_id: String,
        parents: Vec<String>,
    },
    /// `work_item_resolve_conflicts`: the paths the node's branch conflicts on against its base; the node
    /// reads `has-conflicts` while there are any. With `delegate`, the node's session is asked to
    /// resolve them — a running one by a new turn, a stopped one resumed with that message.
    ResolveConflicts { node_id: String, delegate: bool },
    /// `work_item_spawn_session`: start the node's session through the daemon's start path, its stack
    /// parent the caller, on the node's suggested branch. `session_type` and `model` default to the
    /// caller's own.
    SpawnChild {
        node_id: String,
        session_type: Option<String>,
        model: Option<String>,
    },
    /// `work_item_propagate` (`#workqueue` 14/14): rebase the item onto its line parent's tip and push
    /// with lease. `force` skips a predicted conflict only — never the local-branch gate.
    Propagate { item_id: String, force: bool },
    /// `work_queue_land` (`#workqueue` 14/14): land waves `from_wave..=to_wave` in derived line order,
    /// propagating each landed item's successors before the next.
    Land { from_wave: u32, to_wave: u32 },
}

/// What the git tools need from the daemon: one operation on a node of one of this host's stacks.
/// A call naming another host was forwarded before it got here.
#[async_trait]
pub trait WorkItemGitToolPort: Send + Sync {
    /// What `caller` may do with the queue `queue_id` (a session id) on this host, or the refusal
    /// naming why it has none (see [`crate::work_queue_tools::WorkQueueToolPort::access_to`]).
    async fn access_to(
        &self,
        caller: &WorkQueueCaller,
        queue_id: &str,
    ) -> Result<WorkQueueAccess, String>;

    /// Run `op` on `queue_id` of `caller`'s project for `caller`, answering the tool's JSON. An
    /// unknown stack or node, a node refused a rebase (dirty worktree, running session) and a git or
    /// GitHub failure are the error, each naming what was refused.
    async fn run_stack_git_op(
        &self,
        caller: &WorkQueueCaller,
        queue_id: &str,
        op: WorkItemGitOp,
    ) -> Result<Value, String>;
}

/// The six tools' definitions. Every one takes [`STACK_ID`] and [`TARGET_DAEMON_INSTANCE_ID`] and
/// names the node it acts on by `node_id`.
pub fn stack_git_tool_defs() -> Vec<ToolDef> {
    let node_id = |what: &str| json!({ "type": "string", "description": what });
    vec![
        tool_def(
            PR_MERGE,
            "Merge a stack node's open pull request into its base, as the project's GitHub account, and mark the node merged. Then work_item_repoint its dependents.",
            node_object(json!({ "node_id": node_id("WorkQueue node id whose PR to merge.") }), &["node_id"]),
        ),
        tool_def(
            PR_CLOSE,
            "Close a stack node's open pull request without merging, as the project's GitHub account, and mark the node closed.",
            node_object(json!({ "node_id": node_id("WorkQueue node id whose PR to close.") }), &["node_id"]),
        ),
        tool_def(
            PR_REPOINT,
            "Rebase a stack node's branch onto its effective base (the next unmerged ancestor, else the project's default branch) or onto new_base, in the node's own worktree on the daemon, force-push it with lease to the project's remote, and retarget its pull request's base. Refused, with nothing touched, while the node's worktree has uncommitted changes or its session is running.",
            node_object(
                json!({
                    "node_id": node_id("WorkQueue node id to repoint."),
                    "new_base": { "type": "string", "description": "Base branch to repoint onto; the node's effective base when omitted." }
                }),
                &["node_id"],
            ),
        ),
        tool_def(
            PR_SET_PARENTS,
            "Move a node in the stack by giving it a whole new parent list. parents is required: pass [] to make the node a root off the stack bottom. Rejects an unknown parent, self-parenthood, a duplicate entry, and any change that would close a cycle, writing nothing. When the node owns a branch, it is realigned as work_item_repoint does: rebased in its own worktree, force-pushed with lease to the project's remote, its pull request retargeted — refused, with nothing written, while its worktree is dirty or its session is running.",
            node_object(
                json!({
                    "node_id": node_id("WorkQueue node id to move."),
                    "parents": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "The node's whole new parent list (chosen ancestors); pass [] to make it a root off the stack bottom. Required — there is no default."
                    }
                }),
                &["node_id", "parents"],
            ),
        ),
        tool_def(
            PR_RESOLVE_CONFLICTS,
            "Find the files a stack node's branch conflicts on against its base, on the daemon, leaving the node's worktree as it was. Marks the node has-conflicts while there are any and clears that once its branch merges cleanly. With delegate, the node's session is asked to resolve the named files: a running session is given the request as a new turn, a stopped one is resumed with it.",
            node_object(
                json!({
                    "node_id": node_id("WorkQueue node id whose branch to check."),
                    "delegate": { "type": "boolean", "description": "Ask the node's session to resolve the conflicting files (resuming it when stopped)." }
                }),
                &["node_id"],
            ),
        ),
        tool_def(
            PR_SPAWN_CHILD,
            "Start the coding session for a planned stack node, on its suggested branch, with the calling session recorded as its orchestrator. session_type and model default to the caller's own. Answers the new session's session_id.",
            node_object(
                json!({
                    "node_id": node_id("WorkQueue node id to start a session for."),
                    "session_type": { "type": "string", "description": "Session type to start; the caller's own when omitted." },
                    "model": { "type": "string", "description": "Model to start it with; the caller's own when omitted." }
                }),
                &["node_id"],
            ),
        ),
    ]
    .into_iter()
    .chain(work_queue_land_tool_defs())
    .collect()
}

/// A tool schema of `properties` plus the stack and the host arguments.
fn node_object(properties: Value, required: &[&str]) -> Value {
    let mut properties = properties;
    properties[STACK_ID] = json!({
        "type": "string",
        "description": "The work queue, as work_queue_list answers its queue_id — the id of the session that owns it. The calling session's own queue when omitted."
    });
    properties[TARGET_DAEMON_INSTANCE_ID] = json!({
        "type": "string",
        "description": "The host that holds the stack, as work_queue_list reports it in its daemon_instance_id; empty for this host."
    });
    json!({ "type": "object", "properties": properties, "required": required })
}

/// Run one of the six stack git tools for `caller`.
///
/// The daemon has already refused a caller its role does not entitle. The arguments are parsed
/// first — bad arguments are refused before the stack is resolved — then the stack is resolved as
/// the plan tools resolve it ([`crate::work_queue_tools::addressed_stack_id`]), then the port runs the
/// operation. Each refusal comes back as an `is_error` outcome naming what was refused.
pub async fn execute_stack_git_tool(
    port: &dyn WorkItemGitToolPort,
    caller: &WorkQueueCaller,
    tool_name: &str,
    args_json: &str,
) -> ToolOutcome {
    match run_stack_git(port, caller, tool_name, args_json).await {
        Ok(answer) => ToolOutcome::ok(answer.to_string()),
        Err(message) => ToolOutcome::err(message),
    }
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct StackGitToolArgs {
    queue_id: Option<String>,
    node_id: Option<String>,
    new_base: Option<String>,
    parents: Option<Vec<String>>,
    delegate: bool,
    session_type: Option<String>,
    model: Option<String>,
    item_id: Option<String>,
    force: bool,
    from_wave: Option<u32>,
    to_wave: Option<u32>,
}

fn required<T>(tool_name: &str, value: Option<T>, argument: &str) -> Result<T, String> {
    value.ok_or_else(|| format!("{tool_name}: invalid arguments: missing field `{argument}`"))
}

fn work_item_git_op_of(tool_name: &str, args: StackGitToolArgs) -> Result<WorkItemGitOp, String> {
    match tool_name {
        WORK_ITEM_PROPAGATE => {
            return Ok(WorkItemGitOp::Propagate {
                item_id: required(tool_name, args.item_id, "item_id")?,
                force: args.force,
            })
        }
        WORK_QUEUE_LAND => {
            let from_wave = required(tool_name, args.from_wave, "from_wave")?;
            let to_wave = required(tool_name, args.to_wave, "to_wave")?;
            if from_wave == 0 || to_wave < from_wave {
                return Err(format!(
                    "{tool_name}: waves are numbered from 1 and from_wave must not exceed to_wave"
                ));
            }
            return Ok(WorkItemGitOp::Land { from_wave, to_wave });
        }
        _ => {}
    }
    let node_id = required(tool_name, args.node_id, "node_id")?;
    Ok(match tool_name {
        PR_MERGE => WorkItemGitOp::Merge { node_id },
        PR_CLOSE => WorkItemGitOp::Close { node_id },
        PR_REPOINT => WorkItemGitOp::Repoint {
            node_id,
            new_base: args.new_base.filter(|base| !base.trim().is_empty()),
        },
        PR_SET_PARENTS => WorkItemGitOp::SetParents {
            node_id,
            parents: required(tool_name, args.parents, "parents")?,
        },
        PR_RESOLVE_CONFLICTS => WorkItemGitOp::ResolveConflicts {
            node_id,
            delegate: args.delegate,
        },
        PR_SPAWN_CHILD => WorkItemGitOp::SpawnChild {
            node_id,
            session_type: args.session_type.filter(|value| !value.trim().is_empty()),
            model: args.model.filter(|value| !value.trim().is_empty()),
        },
        other => return Err(format!("unknown stack git tool: {other}")),
    })
}

async fn run_stack_git(
    port: &dyn WorkItemGitToolPort,
    caller: &WorkQueueCaller,
    tool_name: &str,
    args_json: &str,
) -> Result<Value, String> {
    if !is_stack_git_tool(tool_name) {
        return Err(format!("unknown stack git tool: {tool_name}"));
    }
    let args: StackGitToolArgs = if args_json.trim().is_empty() {
        StackGitToolArgs::default()
    } else {
        serde_json::from_str(args_json)
            .map_err(|e| format!("{tool_name}: invalid arguments: {e}"))?
    };
    let named = args.queue_id.clone();
    let op = work_item_git_op_of(tool_name, args)?;
    let queue_id =
        crate::work_queue_tools::addressed_stack_id(caller, tool_name, named.as_deref())?
            .ok_or_else(|| format!("{tool_name}: name the work queue with queue_id"))?;
    let access = port
        .access_to(caller, &queue_id)
        .await
        .map_err(|why| format!("{tool_name}: {why}"))?;
    if !is_stack_git_tool_offered_to(access, tool_name) {
        return Err(format!(
            "{tool_name}: only the session that owns work queue {queue_id} may change it"
        ));
    }
    port.run_stack_git_op(caller, &queue_id, op)
        .await
        .map_err(|why| format!("{tool_name}: {why}"))
}

#[cfg(test)]
mod tests;
