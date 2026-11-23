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

use tddy_session_activity::work_queue_activity::WorkQueueChange;

use super::queue_ports::DaemonQueueWorld;
use super::{journal_path, node_of, write_journal, GitEnv};
use crate::work_queue::move_activity::MovedQueue;

/// Drive an engine future from the blocking thread the git work runs on.
fn drive<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Handle::current().block_on(future)
}

/// Tell the queue's owner about `changes` (`#workqueue` 14/14), each item titled from the queue as it
/// is now. Nothing without a sink; a queue that cannot be read again is announced untitled.
fn announce(env: &GitEnv, changes: Vec<(String, WorkQueueChange)>) {
    let Some(sink) = &env.activity else { return };
    let stack = env.stack().unwrap_or_default();
    let queue = MovedQueue {
        queue_session_id: &env.queue_id,
        project_id: &env.project_id,
        os_user: &env.os_user,
    };
    drive(async {
        for (item_id, change) in changes {
            sink.publish(&queue, &item_id, &stack, change).await;
        }
    });
}

/// Tell the queue's owner that `item_id` could not even be lined up to land (its project, PR or
/// branch could not be read); a land that ran announces through [`land_changes`].
fn refused_land(env: &GitEnv, item_id: &str, why: &str) {
    announce(
        env,
        vec![(
            item_id.to_string(),
            WorkQueueChange::Refused {
                action: "land".to_string(),
                reason: why.to_string(),
            },
        )],
    );
}

/// What a land announces: each landing, each propagation between them, and where it stopped.
fn land_changes(report: &LandReport) -> Vec<(String, WorkQueueChange)> {
    let landed = report
        .landed
        .iter()
        .map(|(id, sha)| (id.clone(), WorkQueueChange::Landed { sha: sha.clone() }));
    let propagated = report.propagated.iter().map(|p| {
        (
            p.item_id.clone(),
            WorkQueueChange::Propagated {
                onto: p.onto.clone(),
            },
        )
    });
    let stopped = report.stopped_at.iter().map(|(id, why)| {
        (
            id.clone(),
            WorkQueueChange::Refused {
                action: "land".to_string(),
                reason: why.clone(),
            },
        )
    });
    landed.chain(propagated).chain(stopped).collect()
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
    // Every outcome is announced, a failure to reach the item's worktree or PR included.
    let outcome = propagate_unannounced(env, item_id, onto, force);
    announce(env, vec![(item_id.to_string(), propagate_change(&outcome))]);
    outcome
}

fn propagate_unannounced(
    env: &GitEnv,
    item_id: &str,
    onto: Option<&str>,
    force: bool,
) -> Result<Propagated, EngineRefusal> {
    let world = DaemonQueueWorld::new(env).map_err(EngineRefusal::Failed)?;
    let trunk = env.default_branch(world.project());
    let onto = match onto {
        Some(named) => named.to_string(),
        None => {
            env.effective_base(world.queue(), item_id, world.remote(), &trunk)
                .0
        }
    };
    let request = request_for(&world, item_id, &onto, force).map_err(EngineRefusal::Failed)?;
    drive(propagate(&world, &world, &request))
}

/// What a propagation announces: where it went, or why it was refused.
fn propagate_change(outcome: &Result<Propagated, EngineRefusal>) -> WorkQueueChange {
    match outcome {
        Ok(done) => WorkQueueChange::Propagated {
            onto: done.onto.clone(),
        },
        Err(refusal) => WorkQueueChange::Refused {
            action: "propagate".to_string(),
            reason: refusal.to_string(),
        },
    }
}

pub(crate) fn propagate_tool(env: &GitEnv, item_id: &str, force: bool) -> Result<Value, String> {
    let done = propagate_item(env, item_id, None, force)
        .map_err(|refusal| format!("{item_id} was not propagated — {refusal}"))?;
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
    announce(env, land_changes(&report));
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
    let world = DaemonQueueWorld::new(env).inspect_err(|why| refused_land(env, item_id, why))?;
    let trunk = env.default_branch(world.project());
    let line = [land_item_of(&world, item_id, &trunk, None)
        .inspect_err(|why| refused_land(env, item_id, why))?];
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
fn items_in_waves(world: &DaemonQueueWorld<'_>, from: u32, to: u32) -> Result<Vec<String>, String> {
    let queue = world.queue();
    let ids: Vec<String> = queue.nodes.iter().map(|n| n.node_id.clone()).collect();
    let derived = queue_waves(&ids, &dependencies_of(queue), &[]).map_err(|e| e.to_string())?;
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

/// Land waves `from..=to`. `held_up` names, per item, the merge condition that is not met (judged by
/// the caller from the live facts); the run stops at the first of those.
pub(crate) fn land_range(
    env: &GitEnv,
    from: u32,
    to: u32,
    held_up: &HashMap<String, (String, String)>,
) -> Result<LandReport, String> {
    let world = DaemonQueueWorld::new(env)?;
    let trunk = env.default_branch(world.project());
    let mut line = Vec::new();
    for id in items_in_waves(&world, from, to)? {
        line.push(land_item_of(&world, &id, &trunk, held_up.get(&id))?);
    }
    run_land(env, &world, &line)
}

/// `work_queue_land`: [`land_range`], answered as the tool's JSON.
pub(crate) fn land_waves(
    env: &GitEnv,
    from: u32,
    to: u32,
    held_up: &HashMap<String, (String, String)>,
) -> Result<Value, String> {
    land_range(env, from, to, held_up).map(|report| report_json(&report))
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn a_propagation(item_id: &str, onto: &str) -> Propagated {
        Propagated {
            item_id: item_id.to_string(),
            onto: onto.to_string(),
            commits_taken: 1,
        }
    }

    #[test]
    fn a_land_announces_each_landing_each_propagation_and_where_it_stopped() {
        // Given a land that merged n1, rebased n2 onto the trunk, and stopped at n3
        let report = LandReport {
            landed: vec![("n1".to_string(), "abc1234".to_string())],
            propagated: vec![a_propagation("n2", "master")],
            stopped_at: Some(("n3".to_string(), "checks are failing".to_string())),
        };

        // When
        let changes = land_changes(&report);

        // Then
        assert_eq!(
            changes,
            vec![
                (
                    "n1".to_string(),
                    WorkQueueChange::Landed {
                        sha: "abc1234".to_string()
                    }
                ),
                (
                    "n2".to_string(),
                    WorkQueueChange::Propagated {
                        onto: "master".to_string()
                    }
                ),
                (
                    "n3".to_string(),
                    WorkQueueChange::Refused {
                        action: "land".to_string(),
                        reason: "checks are failing".to_string()
                    }
                ),
            ]
        );
    }

    #[test]
    fn a_propagation_announces_where_it_went_or_why_it_was_refused() {
        // When
        let went = propagate_change(&Ok(a_propagation("n2", "feature/n1")));
        let refused = propagate_change(&Err(EngineRefusal::Failed("no worktree".to_string())));

        // Then
        assert_eq!(
            went,
            WorkQueueChange::Propagated {
                onto: "feature/n1".to_string()
            }
        );
        assert!(
            matches!(&refused, WorkQueueChange::Refused { action, reason }
                if action == "propagate" && reason.contains("no worktree")),
            "{refused:?}"
        );
    }
}
