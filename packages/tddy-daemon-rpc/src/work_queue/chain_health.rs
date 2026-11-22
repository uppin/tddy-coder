//! Wire views of an item's chain health (`#workqueue` 12/14): the status stream's messages and the
//! `work_queue_status` tool's JSON. The reading itself is `tddy_work_queue::chain_health_reads`.

use serde_json::{json, Value};
use tddy_service::proto::work_queue::{LeakView, LineStateView};
use tddy_work_queue::chain_health::{Leak, LineState};
use tddy_work_queue::chain_health_reads::ItemChainHealth;

pub(crate) fn leak_views(leaks: &[Leak]) -> Vec<LeakView> {
    leaks
        .iter()
        .map(|l| LeakView {
            sha: l.sha.clone(),
            ancestor: l.ancestor.clone(),
        })
        .collect()
}

pub(crate) fn line_view(line: &LineState) -> LineStateView {
    let (state, by) = match line {
        LineState::Linear => ("linear", 0),
        LineState::Behind { by } => ("behind", *by),
        LineState::Conflicting { by } => ("conflicting", *by),
        LineState::Unknown => ("unknown", 0),
    };
    LineStateView {
        state: state.to_string(),
        by,
    }
}

/// The tool's answer for one item: `leaks` and `line`.
pub(crate) fn health_json(health: &ItemChainHealth) -> Value {
    let line = health.line.as_ref().map(line_view);
    json!({
        "leaks": health.leaks.iter().map(|l| json!({ "sha": l.sha, "ancestor": l.ancestor })).collect::<Vec<_>>(),
        "line": line.map(|l| json!({ "state": l.state, "by": l.by })),
    })
}
