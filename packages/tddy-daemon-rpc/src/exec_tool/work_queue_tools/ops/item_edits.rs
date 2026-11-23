use tddy_work_queue::internal_status::reconcile_internal_status;

use tddy_core::changeset::PrInternalStatus;

use tddy_work_queue::item_ops::sync_node_to_github_pr;

use tddy_work_queue::item_ops::UpdateWorkItemInput;

use tddy_work_queue::item_ops::update_work_item;

use tddy_tool_engine::work_queue_tools::PrAddress;

use serde_json::{json, Value};

use super::super::github::GithubSource;

use super::OpEnv;
use tddy_work_queue::item_ops::WorkQueueHome;

/// Edit a node's metadata and, when asked, push the same title/body to its pull request.
///
/// Everything a `sync_pr` push can refuse without touching the network is checked *before* the
/// local edit, so a refused call leaves the plan exactly as it was. Only a failure GitHub itself
/// returns can leave the node edited and its pull request not, and that is reported as such.
pub(crate) fn update_planned(
    env: &OpEnv,
    github: &GithubSource,
    node_id: String,
    title: Option<String>,
    description: Option<String>,
    branch_suggestion: Option<String>,
    sync_pr: bool,
) -> Result<Value, String> {
    let home = env.home()?;
    // Resolved *before* the local edit: each of these can refuse without touching the network, and
    // a refusal that has already rewritten the plan is a partial application of a call the caller
    // was told had failed.
    let sync_client = if sync_pr {
        if title.is_none() && description.is_none() {
            return Err(format!(
                "node '{node_id}' was asked to sync its pull request, but neither a title nor a \
                 description was given — GitHub refuses an empty edit, so there is nothing to \
                 push and nothing was changed"
            ));
        }
        env.pull_number(&PrAddress::Node(node_id.clone()))?;
        Some(github.client()?)
    } else {
        None
    };

    let node = update_work_item(
        &home,
        UpdateWorkItemInput {
            node_id: node_id.clone(),
            title: title.clone(),
            description: description.clone(),
            branch_suggestion,
        },
    )?;
    let mut out = json!({
        "node_id": node.node_id,
        "title": node.title,
        "description": node.description,
        "branch_suggestion": node.branch_suggestion,
        "branch": node.branch,
        "session_id": node.session_id,
        "parents": node.parents,
    });
    if let Some(gh) = sync_client {
        // The preconditions held, so a failure here is GitHub itself refusing the write — the plan
        // is already edited and the pull request is not, which the caller has to be told.
        let number = sync_node_to_github_pr(
            &home,
            &node_id,
            title.as_deref(),
            description.as_deref(),
            &*gh,
        )
        .map_err(|e| format!("{e} (the node was updated; its pull request was not)"))?;
        out["pr_synced"] = json!(number);
    }
    Ok(out)
}

/// Record a manual internal-status override on a node, which derivation never overwrites.
pub(crate) fn set_status(
    env: &OpEnv,
    node_id: &str,
    kind: String,
    note: Option<String>,
) -> Result<Value, String> {
    let new_status = PrInternalStatus {
        kind,
        note,
        source: "override".to_string(),
    };
    let mut found = false;
    env.home()?
        .update_stack(|stack| {
            if let Some(node) = stack.nodes.iter_mut().find(|n| n.node_id == node_id) {
                found = true;
                node.internal_status = Some(reconcile_internal_status(
                    node.internal_status.as_ref(),
                    new_status.clone(),
                ));
            }
        })
        .map_err(|e| format!("failed to write stack: {e}"))?;
    if found {
        Ok(json!({ "ok": true }))
    } else {
        Err(format!("node '{node_id}' not found"))
    }
}
