//! PRD acceptance: orchestrate-pr-stack crash recovery — write_stack_op_journal persists journal,
//! recover_in_flight_stack_op reads it back and returns a repoint action (no re-merge).

use std::fs;
use std::path::PathBuf;

use tddy_workflow_recipes::orchestrate_pr_stack::{
    MergePhase, StackOpJournal, recover_in_flight_stack_op, write_stack_op_journal,
};

fn scratch(label: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "tddy-orch-resume-acc-{}-{}",
        label,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn recover_in_flight_stack_op_resumes_repointing_not_remerges() {
    // Given — a stack-op.json in PrMerged state (merge happened, repoint not yet done)
    let dir = scratch("resume-repoint");
    // write_stack_op_journal persists the journal (temp+rename)
    let journal = StackOpJournal {
        op_id: "op-001".into(),
        merged_node_id: "n1".into(),
        merge_phase: MergePhase::PrMerged { sha: "abc123".into() },
        dependents: vec!["n2".into()],
    };
    write_stack_op_journal(&dir, &journal).expect("write_stack_op_journal must succeed");

    // Verify the file landed
    let journal_path = dir.join(".workflow").join("stack-op.json");
    assert!(journal_path.exists(), "stack-op.json must be written by write_stack_op_journal");

    // When — recover reads the journal and determines the action to take
    // Stub GithubPrApi: for now, recovery just reads the journal (no real GitHub call needed for
    // a unit-level accept test — actual calls are gated in integration tests).
    // recover_in_flight_stack_op returns Some(repoint_action) when PrMerged, None when Done.
    let result = recover_in_flight_stack_op(&dir);
    let _ = fs::remove_dir_all(&dir);

    // Then — must return Some action to repoint (not re-merge)
    let action = result.expect("recover_in_flight_stack_op must succeed when journal is present");
    assert!(
        action.is_some(),
        "recover must return Some(action) when journal phase is PrMerged (repointing incomplete)"
    );
    let recovered = action.unwrap();
    // The action should be a repoint targeting n2, not another merge of n1
    assert_eq!(
        recovered.merged_node_id, "n1",
        "recovered action must reference the already-merged node n1"
    );
    assert!(
        !matches!(recovered.merge_phase, MergePhase::PrMerged { .. }),
        "recovered journal phase must have advanced past PrMerged (no re-merge)"
    );
}
