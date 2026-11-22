//! Land and propagate (`#workqueue` 14/14) against recording port doubles: what reached git and GitHub,
//! in order.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use pretty_assertions::assert_eq;
use tddy_work_queue::engine::{
    land, propagate, EngineError, EngineRefusal, LandItem, LocalBranchState, PropagateRequest,
    QueueGit, QueueGithub,
};

#[derive(Default)]
struct ARecordingWorld {
    calls: Mutex<Vec<String>>,
    states: Mutex<HashMap<String, LocalBranchState>>,
    conflicts: Mutex<HashMap<String, Vec<String>>>,
}

impl ARecordingWorld {
    fn with_state(self, branch: &str, state: LocalBranchState) -> Self {
        self.states
            .lock()
            .unwrap()
            .insert(branch.to_string(), state);
        self
    }

    fn with_conflict(self, branch: &str, file: &str) -> Self {
        self.conflicts
            .lock()
            .unwrap()
            .insert(branch.to_string(), vec![file.to_string()]);
        self
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    fn record(&self, call: String) {
        self.calls.lock().unwrap().push(call);
    }
}

#[async_trait]
impl QueueGit for ARecordingWorld {
    async fn fetch(&self) -> Result<(), EngineError> {
        self.record("fetch".into());
        Ok(())
    }
    async fn local_branch_state(&self, branch: &str) -> Result<LocalBranchState, EngineError> {
        Ok(*self
            .states
            .lock()
            .unwrap()
            .get(branch)
            .unwrap_or(&LocalBranchState::InSync))
    }
    async fn inbox_count(&self, _branch: &str, _onto: &str) -> Result<u32, EngineError> {
        Ok(2)
    }
    async fn predicted_conflicts(
        &self,
        branch: &str,
        _onto: &str,
    ) -> Result<Vec<String>, EngineError> {
        Ok(self
            .conflicts
            .lock()
            .unwrap()
            .get(branch)
            .cloned()
            .unwrap_or_default())
    }
    async fn tip(&self, branch: &str) -> Result<String, EngineError> {
        Ok(format!("tip-of-{branch}"))
    }
    async fn rebase_onto(&self, branch: &str, onto: &str) -> Result<(), EngineError> {
        self.record(format!("rebase {branch} onto {onto}"));
        Ok(())
    }
    async fn push_with_lease(&self, branch: &str, expected_sha: &str) -> Result<(), EngineError> {
        self.record(format!("push {branch} lease {expected_sha}"));
        Ok(())
    }
}

#[async_trait]
impl QueueGithub for ARecordingWorld {
    async fn merge(&self, number: u64) -> Result<String, EngineError> {
        self.record(format!("merge #{number}"));
        Ok(format!("merged-{number}"))
    }
    async fn retarget(&self, number: u64, base: &str) -> Result<(), EngineError> {
        self.record(format!("retarget #{number} to {base}"));
        Ok(())
    }
}

fn propagating(branch: &str, onto: &str) -> PropagateRequest {
    PropagateRequest {
        item_id: format!("item-{branch}"),
        branch: branch.to_string(),
        onto: onto.to_string(),
        pr_number: Some(8),
        retarget_to: None,
        base_checks_green: true,
        force: false,
    }
}

#[tokio::test]
async fn propagating_a_clean_inbox_rebases_onto_the_parent_tip_and_pushes_with_lease() {
    // Given
    let world = ARecordingWorld::default();

    // When
    let done = propagate(&world, &world, &propagating("feat/two", "feat/one")).await;

    // Then
    assert_eq!(done.map(|p| p.commits_taken), Ok(2));
    assert_eq!(
        world.calls(),
        vec![
            "fetch".to_string(),
            "rebase feat/two onto feat/one".to_string(),
            "push feat/two lease tip-of-feat/two".to_string(),
        ]
    );
}

#[tokio::test]
async fn a_dirty_unpushed_unpulled_or_diverged_branch_refuses_naming_state_and_actions() {
    for (state, actions) in [
        (LocalBranchState::Dirty, vec!["commit"]),
        (LocalBranchState::Unpushed, vec!["push"]),
        (LocalBranchState::Unpulled, vec!["pull"]),
        (LocalBranchState::Diverged, vec!["pull", "push"]),
    ] {
        // Given
        let world = ARecordingWorld::default().with_state("feat/two", state);

        // When
        let refused = propagate(&world, &world, &propagating("feat/two", "feat/one")).await;

        // Then
        assert_eq!(
            refused,
            Err(EngineRefusal::LocalBranch { state, actions }),
            "{state:?}"
        );
        assert!(
            !world.calls().iter().any(|c| c.starts_with("rebase")),
            "{state:?}: nothing rebased"
        );
    }
}

#[tokio::test]
async fn force_does_not_override_the_local_branch_gate() {
    // Given
    let world = ARecordingWorld::default().with_state("feat/two", LocalBranchState::Dirty);
    let forced = PropagateRequest {
        force: true,
        ..propagating("feat/two", "feat/one")
    };

    // When
    let refused = propagate(&world, &world, &forced).await;

    // Then
    assert!(
        matches!(refused, Err(EngineRefusal::LocalBranch { .. })),
        "{refused:?}"
    );
}

#[tokio::test]
async fn a_predicted_conflict_refuses_unless_forced() {
    // Given
    let world = ARecordingWorld::default().with_conflict("feat/two", "f.txt");

    // When
    let refused = propagate(&world, &world, &propagating("feat/two", "feat/one")).await;
    let forced = propagate(
        &world,
        &world,
        &PropagateRequest {
            force: true,
            ..propagating("feat/two", "feat/one")
        },
    )
    .await;

    // Then
    assert!(
        matches!(&refused, Err(EngineRefusal::Preflight { condition: "conflicts", detail }) if detail.contains("f.txt")),
        "{refused:?}"
    );
    assert!(forced.is_ok(), "{forced:?}");
}

#[tokio::test]
async fn a_changed_line_parent_retargets_the_pr_base() {
    // Given
    let world = ARecordingWorld::default();
    let retargeting = PropagateRequest {
        retarget_to: Some("master".to_string()),
        ..propagating("feat/two", "master")
    };

    // When
    propagate(&world, &world, &retargeting)
        .await
        .expect("propagated");

    // Then
    assert_eq!(
        world.calls().last(),
        Some(&"retarget #8 to master".to_string())
    );
}

fn a_land_item(id: &str, branch: &str, pr: u64, successors: Vec<PropagateRequest>) -> LandItem {
    LandItem {
        item_id: id.to_string(),
        branch: branch.to_string(),
        pr_number: pr,
        held_up_on: None,
        successors,
    }
}

#[tokio::test]
async fn landing_two_waves_merges_in_derived_line_order_and_propagates_successors_between_merges() {
    // Given — n1 then n2, each with the next one sitting on it
    let world = ARecordingWorld::default();
    let line = vec![
        a_land_item("n1", "feat/one", 7, vec![propagating("feat/two", "master")]),
        a_land_item(
            "n2",
            "feat/two",
            8,
            vec![propagating("feat/three", "master")],
        ),
    ];

    // When
    let report = land(&world, &world, &line).await;

    // Then
    let merges_and_rebases: Vec<String> = world
        .calls()
        .into_iter()
        .filter(|c| c.starts_with("merge") || c.starts_with("rebase"))
        .collect();
    assert_eq!(
        merges_and_rebases,
        vec![
            "merge #7".to_string(),
            "rebase feat/two onto master".to_string(),
            "merge #8".to_string(),
            "rebase feat/three onto master".to_string(),
        ]
    );
    assert_eq!((report.landed.len(), report.stopped_at), (2, None));
}

#[tokio::test]
async fn landing_stops_at_the_first_item_whose_conditions_are_not_met_and_names_it() {
    // Given
    let world = ARecordingWorld::default();
    let mut held = a_land_item("n2", "feat/two", 8, vec![]);
    held.held_up_on = Some(("checks".to_string(), "2 running".to_string()));
    let line = vec![
        a_land_item("n1", "feat/one", 7, vec![]),
        held,
        a_land_item("n3", "feat/three", 9, vec![]),
    ];

    // When
    let report = land(&world, &world, &line).await;

    // Then
    assert_eq!(
        (report.landed, report.stopped_at),
        (
            vec![("n1".to_string(), "merged-7".to_string())],
            Some(("n2".to_string(), "checks: 2 running".to_string()))
        )
    );
    assert!(!world.calls().contains(&"merge #9".to_string()));
}

#[tokio::test]
async fn merging_one_item_propagates_its_successors() {
    // Given — what work_item_merge does through the engine
    let world = ARecordingWorld::default();

    // When
    let report = land(
        &world,
        &world,
        &[a_land_item(
            "n1",
            "feat/one",
            7,
            vec![propagating("feat/two", "master")],
        )],
    )
    .await;

    // Then
    assert_eq!(
        report
            .propagated
            .iter()
            .map(|p| p.item_id.as_str())
            .collect::<Vec<_>>(),
        vec!["item-feat/two"]
    );
}
