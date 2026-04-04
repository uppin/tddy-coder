//! Lower-level Red tests for interview relay helpers (fail until persistence + merge are implemented).

use std::fs;

use tddy_core::workflow::context::Context;
use tddy_workflow_recipes::tdd::interview::{
    apply_staged_interview_handoff_to_plan_context, interview_handoff_path,
    persist_interview_handoff_for_plan,
};

#[test]
fn persist_interview_handoff_writes_relay_file() {
    let tmp = std::env::temp_dir().join(format!("tdd-interview-relay-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(tmp.join(".workflow")).unwrap();
    let marker = "TDD_INTERVIEW_RELAY_UNIT_XYZZY";
    persist_interview_handoff_for_plan(&tmp, marker).unwrap();
    let p = interview_handoff_path(&tmp);
    assert!(
        p.exists(),
        "persist_interview_handoff_for_plan must write {:?}",
        p
    );
    assert_eq!(fs::read_to_string(&p).unwrap(), marker);
}

#[test]
fn apply_staged_interview_handoff_sets_answers_on_context() {
    let tmp = std::env::temp_dir().join(format!("tdd-interview-apply-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(tmp.join(".workflow")).unwrap();
    let marker = "HANDOFF_APPLY_UNIT_XYZZY";
    fs::write(interview_handoff_path(&tmp), marker).unwrap();

    let ctx = Context::new();
    apply_staged_interview_handoff_to_plan_context(&tmp, &ctx).unwrap();
    assert_eq!(
        ctx.get_sync::<String>("answers").as_deref(),
        Some(marker),
        "plan step must see interview handoff as answers (or merged prompt) before PlanTask runs"
    );
}

#[test]
fn apply_staged_when_relay_missing_leaves_context_unchanged() {
    let tmp = std::env::temp_dir().join(format!("tdd-interview-missing-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();
    // No .workflow/ and no relay file
    let ctx = Context::new();
    apply_staged_interview_handoff_to_plan_context(&tmp, &ctx).unwrap();
    assert!(
        ctx.get_sync::<String>("answers").is_none(),
        "missing relay must not set answers"
    );
}

#[test]
fn apply_staged_when_relay_empty_does_not_set_answers() {
    let tmp = std::env::temp_dir().join(format!("tdd-interview-empty-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(tmp.join(".workflow")).unwrap();
    fs::write(interview_handoff_path(&tmp), "   \n\t  ").unwrap();
    let ctx = Context::new();
    apply_staged_interview_handoff_to_plan_context(&tmp, &ctx).unwrap();
    assert!(
        ctx.get_sync::<String>("answers").is_none(),
        "whitespace-only relay must not set answers"
    );
}

#[test]
fn relay_paths_are_independent_per_session_dir() {
    let a = std::env::temp_dir().join(format!("tdd-interlay-a-{}", std::process::id()));
    let b = std::env::temp_dir().join(format!("tdd-interlay-b-{}", std::process::id()));
    let _ = fs::remove_dir_all(&a);
    let _ = fs::remove_dir_all(&b);
    fs::create_dir_all(a.join(".workflow")).unwrap();
    fs::create_dir_all(b.join(".workflow")).unwrap();
    fs::write(interview_handoff_path(&a), "marker-a").unwrap();
    fs::write(interview_handoff_path(&b), "marker-b").unwrap();
    let ctx_a = Context::new();
    let ctx_b = Context::new();
    apply_staged_interview_handoff_to_plan_context(&a, &ctx_a).unwrap();
    apply_staged_interview_handoff_to_plan_context(&b, &ctx_b).unwrap();
    assert_eq!(
        ctx_a.get_sync::<String>("answers").as_deref(),
        Some("marker-a")
    );
    assert_eq!(
        ctx_b.get_sync::<String>("answers").as_deref(),
        Some("marker-b")
    );
}
