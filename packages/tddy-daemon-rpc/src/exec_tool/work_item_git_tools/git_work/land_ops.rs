//! Merge, propagate and land (`#workqueue` 14/14) as the daemon runs them: the engines of
//! `tddy_work_queue::engine` over [`DaemonQueueWorld`], with the merge journal kept beside them.
//!
//! `work_item_merge` is a land of one item, `work_item_repoint` a propagation of one item — one
//! path with one set of checks. The direct tools do not evaluate the schedule conditions of n13
//! (checks, reviews): they are explicit acts. A schedule's conditions are judged before it fires.

use std::collections::HashMap;

use serde_json::{json, Value};
use tddy_work_queue::engine::{
    land, propagate, EngineRefusal, LandItem, LandReport, PropagateRequest, Propagated,
};
use tddy_work_queue::lineage::{dependencies_of, planned_line_parent, LineEndpoint};
use tddy_work_queue::op_journal::{MergePhase, WorkQueueOpJournal};
use tddy_work_queue::waves::queue_waves;

use super::queue_ports::DaemonQueueWorld;
use super::{journal_path, node_of, write_journal, GitEnv};

/// Drive an engine future from the blocking thread the git work runs on.
fn drive<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Handle::current().block_on(future)
}

fn is_merged(item: &tddy_core::changeset::WorkItem) -> bool {
    item.pr_status
        .as_ref()
        .is_some_and(|status| status.phase == "merged")
}

fn pr_number_of(
    world: &DaemonQueueWorld<'_>,
    item: &tddy_core::changeset::WorkItem,
) -> Result<Option<u64>, String> {
    let Some(branch) = item.branch.as_deref() else {
        return Ok(None);
    };
    world
        .github()
        .get_open_pr(branch)
        .map(|pr| pr.map(|pr| pr.number))
        .map_err(|e| format!("get_open_pr failed: {e}"))
}

/// A propagation request for `item_id` onto `onto`, retargeting its PR (when it has one).
fn request_for(
    world: &DaemonQueueWorld<'_>,
    item_id: &str,
    onto: &str,
    force: bool,
) -> Result<PropagateRequest, String> {
    let item = node_of(world.queue(), item_id)?;
    let branch = item
        .branch
        .clone()
        .ok_or_else(|| format!("item '{item_id}' owns no branch yet"))?;
    let pr_number = pr_number_of(world, item)?;
    Ok(PropagateRequest {
        item_id: item_id.to_string(),
        branch,
        onto: onto.to_string(),
        pr_number,
        retarget_to: pr_number.map(|_| onto.to_string()),
        base_checks_green: true,
        force,
    })
}

/// Propagate `item_id` onto `onto` (its effective base when `None`).
pub(crate) fn propagate_item(
    env: &GitEnv,
    item_id: &str,
    onto: Option<&str>,
    force: bool,
) -> Result<Propagated, EngineRefusal> {
    let world = DaemonQueueWorld::new(env).map_err(EngineRefusal::Failed)?;
    let trunk = env.default_branch(world.project());
    let onto = match onto {
        Some(named) => named.to_string(),
        None => env.effective_base(world.queue(), item_id, world.remote(), &trunk).0,
    };
    let request = request_for(&world, item_id, &onto, force).map_err(EngineRefusal::Failed)?;
    drive(propagate(&world, &world, &request))
}

pub(crate) fn propagate_tool(env: &GitEnv, item_id: &str, force: bool) -> Result<Value, String> {
    let done = propagate_item(env, item_id, None, force).map_err(|refusal| {
        format!("{item_id} was not propagated — {refusal}")
    })?;
    Ok(json!({
        "propagated": true,
        "item_id": done.item_id,
        "onto": done.onto,
        "commits_taken": done.commits_taken,
    }))
}

/// What sits directly on `item_id` (its planned line children), not yet merged, as requests to
/// propagate onto `trunk` once it lands.
fn successors_of(
    world: &DaemonQueueWorld<'_>,
    item_id: &str,
    trunk: &str,
) -> Result<Vec<PropagateRequest>, String> {
    let queue = world.queue();
    let mut requests = Vec::new();
    for candidate in &queue.nodes {
        let on_it = matches!(
            planned_line_parent(queue, &candidate.node_id, trunk),
            Ok(LineEndpoint::Item(parent)) if parent == item_id
        );
        if on_it && candidate.branch.is_some() && !is_merged(candidate) {
            requests.push(request_for(world, &candidate.node_id, trunk, false)?);
        }
    }
    Ok(requests)
}

fn land_item_of(
    world: &DaemonQueueWorld<'_>,
    item_id: &str,
    trunk: &str,
    held_up: Option<&(String, String)>,
) -> Result<LandItem, String> {
    let item = node_of(world.queue(), item_id)?;
    let branch = item
        .branch
        .clone()
        .ok_or_else(|| format!("item '{item_id}' owns no branch yet"))?;
    let number = pr_number_of(world, item)?
        .ok_or_else(|| format!("item '{item_id}' has no open pull request to merge"))?;
    Ok(LandItem {
        item_id: item_id.to_string(),
        branch,
        pr_number: number,
        held_up_on: held_up.cloned(),
        successors: successors_of(world, item_id, trunk)?,
    })
}

/// Land `line` and keep the merge journal and the items' phases in step with what landed.
fn run_land(
    env: &GitEnv,
    world: &DaemonQueueWorld<'_>,
    line: &[LandItem],
) -> Result<LandReport, String> {
    let Some(first) = line.first() else {
        return Ok(LandReport::default());
    };
    let mut journal = WorkQueueOpJournal {
        op_id: uuid::Uuid::new_v4().to_string(),
        merged_node_id: first.item_id.clone(),
        merge_phase: MergePhase::Planned,
        dependents: line
            .iter()
            .flat_map(|item| item.successors.iter().map(|s| s.item_id.clone()))
            .collect(),
    };
    write_journal(env, &journal)?;
    let report = drive(land(world, world, line));
    if let Some((_, sha)) = report.landed.last() {
        journal.merge_phase = MergePhase::PrMerged { sha: sha.clone() };
        write_journal(env, &journal)?;
    }
    for (item_id, _) in &report.landed {
        env.set_phase(item_id, "merged")?;
    }
    if report.stopped_at.is_none() {
        if let Ok(path) = journal_path(env) {
            let _ = std::fs::remove_file(path);
        }
    }
    Ok(report)
}

fn report_json(report: &LandReport) -> Value {
    json!({
        "landed": report.landed.iter().map(|(id, sha)| json!({"item_id": id, "sha": sha})).collect::<Vec<_>>(),
        "propagated": report.propagated.iter().map(|p| json!({
            "item_id": p.item_id, "onto": p.onto, "commits_taken": p.commits_taken,
        })).collect::<Vec<_>>(),
        "stopped_at": report.stopped_at.as_ref().map(|(id, why)| json!({"item_id": id, "reason": why})),
    })
}

/// `work_item_merge`: land one item and propagate what sits on it.
pub(crate) fn merge_item(env: &GitEnv, item_id: &str) -> Result<Value, String> {
    let world = DaemonQueueWorld::new(env)?;
    let trunk = env.default_branch(world.project());
    let line = [land_item_of(&world, item_id, &trunk, None)?];
    let report = run_land(env, &world, &line)?;
    match report.landed.first() {
        None => Err(match &report.stopped_at {
            Some((_, why)) => format!("merge pr: {why}"),
            None => "merge pr: nothing was merged".to_string(),
        }),
        Some((_, sha)) => {
            let mut answer = report_json(&report);
            answer["merged"] = json!(true);
            answer["sha"] = json!(sha);
            Ok(answer)
        }
    }
}

/// The items of waves `from..=to` in derived line order, merged ones left out.
fn items_in_waves(
    world: &DaemonQueueWorld<'_>,
    from: u32,
    to: u32,
) -> Result<Vec<String>, String> {
    let queue = world.queue();
    let ids: Vec<String> = queue.nodes.iter().map(|n| n.node_id.clone()).collect();
    let derived = queue_waves(&ids, &dependencies_of(queue)).map_err(|e| e.to_string())?;
    if to as usize > derived.waves.len() {
        return Err(format!(
            "wave {to} does not exist (the queue has {})",
            derived.waves.len()
        ));
    }
    let wanted: Vec<&String> = derived.waves[(from as usize - 1)..to as usize]
        .iter()
        .flatten()
        .collect();
    Ok(derived
        .derived_line
        .iter()
        .filter(|id| wanted.contains(id))
        .filter(|id| queue.node(id).is_some_and(|item| !is_merged(item)))
        .cloned()
        .collect())
}

/// `work_queue_land`: land waves `from..=to`. `held_up` names, per item, the merge condition that is
/// not met (judged by the caller from the live facts); the run stops at the first of those.
pub(crate) fn land_waves(
    env: &GitEnv,
    from: u32,
    to: u32,
    held_up: &HashMap<String, (String, String)>,
) -> Result<Value, String> {
    let world = DaemonQueueWorld::new(env)?;
    let trunk = env.default_branch(world.project());
    let mut line = Vec::new();
    for id in items_in_waves(&world, from, to)? {
        line.push(land_item_of(&world, &id, &trunk, held_up.get(&id))?);
    }
    let report = run_land(env, &world, &line)?;
    Ok(report_json(&report))
}
