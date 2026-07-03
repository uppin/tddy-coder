//! Derivation of a stack node's [`PrInternalStatus`] (the action-needed signal) from the
//! assembled [`NodeView`]s + live GitHub state, plus the agent-override reconciliation rule.

use tddy_core::changeset::PrInternalStatus;

use super::assess::{effective_base_ref, NodeView, PrLiveStatus};

const SOURCE_DERIVED: &str = "derived";
const SOURCE_OVERRIDE: &str = "override";

/// Strip a leading `origin/` so a GitHub PR base (`"master"`) compares equal to an effective
/// base ref (`"origin/master"`).
fn base_branch(reference: &str) -> &str {
    reference.strip_prefix("origin/").unwrap_or(reference)
}

/// True when every parent dependency of `view` has a merged PR (vacuously true for a root).
fn all_parents_merged(view: &NodeView, views: &[NodeView]) -> bool {
    view.parent_dep_ids.iter().all(|parent_id| {
        views
            .iter()
            .find(|other| &other.node_id == parent_id)
            .is_some_and(|parent| matches!(parent.pr, PrLiveStatus::Merged))
    })
}

/// Compute the derived internal status for every node view.
///
/// - merged PR → `merged`
/// - open PR whose base no longer matches its effective base → `needs-repoint`
/// - open PR with all dependencies merged and base up to date → `ready-to-merge`
/// - otherwise → `up-to-date`
///
/// Every result is stamped `source = "derived"`; conflict detection (`has-conflicts`) and manual
/// states (`blocked`) are produced by the tools, not here.
pub fn derive_internal_status(
    views: &[NodeView],
    default_branch: &str,
) -> Vec<(String, PrInternalStatus)> {
    views
        .iter()
        .map(|view| {
            let kind = match &view.pr {
                PrLiveStatus::Merged => "merged",
                PrLiveStatus::Open { base, .. } => {
                    let effective = effective_base_ref(&view.node_id, views, default_branch);
                    if base_branch(base) != base_branch(&effective) {
                        "needs-repoint"
                    } else if all_parents_merged(view, views) {
                        "ready-to-merge"
                    } else {
                        "up-to-date"
                    }
                }
                PrLiveStatus::None | PrLiveStatus::Queued | PrLiveStatus::Closed => "up-to-date",
            };
            (
                view.node_id.clone(),
                PrInternalStatus {
                    kind: kind.to_string(),
                    note: None,
                    source: SOURCE_DERIVED.to_string(),
                },
            )
        })
        .collect()
}

/// Override-wins reconciliation: an agent-set status (`source == "override"`) is preserved;
/// otherwise the freshly derived status is used.
pub fn reconcile_internal_status(
    existing: Option<&PrInternalStatus>,
    derived: PrInternalStatus,
) -> PrInternalStatus {
    match existing {
        Some(current) if current.source == SOURCE_OVERRIDE => current.clone(),
        _ => derived,
    }
}
