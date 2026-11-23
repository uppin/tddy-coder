use crate::catalog::ToolDef;
use crate::project_home_tools::{tool_def, TARGET_DAEMON_INSTANCE_ID};
use crate::work_queue_tools::PR_ADD_PLANNED;
use crate::work_queue_tools::PR_ADOPT;
use crate::work_queue_tools::PR_COMMENTS;
use crate::work_queue_tools::PR_DELETE_PLANNED;
use crate::work_queue_tools::PR_READ;
use crate::work_queue_tools::PR_SEARCH;
use crate::work_queue_tools::PR_SET_STATUS;
use crate::work_queue_tools::PR_STACK_CREATE;
use crate::work_queue_tools::PR_STACK_LIST;
use crate::work_queue_tools::PR_STACK_STATUS;
use crate::work_queue_tools::PR_UPDATE_PLANNED;
use crate::work_queue_tools::STACK_ID;
use serde_json::{json, Value};

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
            "List the PR stacks of this project on a host — this one unless daemon_instance_id names another — each with its queue_id, title, the host that holds it, the session that created it and its node count. Name a stack by its queue_id (and daemon_instance_id) in the other pr_ tools.",
            object(json!({}), &[]),
        ),
        tool_def(
            PR_STACK_CREATE,
            "Create a new, empty PR stack in this project, on this host or the one daemon_instance_id names, with an optional title saying what it is for. Answers its queue_id; add planned PRs to it with work_item_add.",
            object(
                json!({ "title": { "type": "string", "description": "What the stack is for; work_queue_list shows it." } }),
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
            "Read one pull request in full: title, body, state, base/head, mergeability, size, the latest review state per reviewer, and the head commit's check runs. Address it by exactly one of node_id (a node of queue_id) or pull_number — naming neither or both is rejected, and a node that records no PR url cannot be addressed by node_id. Pass include_files for the changed-file list, which is otherwise neither fetched nor returned.",
            stack_object(
                json!({
                    "node_id": node_id("WorkQueue node id whose PR to read; name this or pull_number, not both."),
                    "pull_number": { "type": "integer", "minimum": 1, "description": "Pull request number to read; name this or node_id, not both." },
                    "include_files": { "type": "boolean", "description": "Include the changed-file list (omitted by default)." }
                }),
                &[],
            ),
        ),
        tool_def(
            PR_SEARCH,
            "Search this project's repository's pull requests, including ones no stack tracks, by text, state (open/closed/merged/all, default open), author and base branch. The repository is always the project's own and cannot be chosen. Returns at most limit hits (default 20, hard cap 100), each with number, title, state, draft, author, url and updated_at — GitHub's search reports no head or base branch, so follow up with work_pr_read when you need them.",
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
            "Read a pull request's review feedback as three separate sections: submitted reviews, diff-anchored comment threads in reply order, and conversation comments. Address it by exactly one of node_id (a node of queue_id) or pull_number — naming neither or both is rejected. A thread's resolved/unresolved state is not available over this API, so no thread reports one: read the replies to judge.",
            stack_object(
                json!({
                    "node_id": node_id("WorkQueue node id whose PR feedback to read; name this or pull_number, not both."),
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
                    "node_id": node_id("WorkQueue node id to edit."),
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
                json!({ "node_id": node_id("WorkQueue node id to remove from the plan.") }),
                &["node_id"],
            ),
        ),
        tool_def(
            PR_SET_STATUS,
            "Record a manual internal-status override on a stack node (e.g. blocked) with an optional note. Overrides are not overwritten by automatic derivation.",
            stack_object(
                json!({
                    "node_id": node_id("WorkQueue node id to annotate."),
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
pub(super) fn object(properties: Value, required: &[&str]) -> Value {
    let mut properties = properties;
    properties[TARGET_DAEMON_INSTANCE_ID] = json!({
        "type": "string",
        "description": "The host that holds the stack, as work_queue_list reports it in its daemon_instance_id; empty for this host."
    });
    json!({ "type": "object", "properties": properties, "required": required })
}

/// [`object`] plus [`STACK_ID`].
pub(super) fn stack_object(properties: Value, required: &[&str]) -> Value {
    let mut properties = properties;
    properties[STACK_ID] = json!({
        "type": "string",
        "description": "The stack, as work_queue_list or work_queue_create answers its queue_id. The calling session's own stack (the one its changeset names) when omitted."
    });
    object(properties, required)
}
