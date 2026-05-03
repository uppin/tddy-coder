//! Acceptance tests: `--recipe tdd|bugfix` and config resolution (bugfix workflow recipe PRD).

use tddy_coder::WorkflowRecipeResolver;
use tddy_core::workflow::ids::WorkflowState;

/// CLI/config must resolve `bugfix` to BugfixRecipe (`name` = bugfix).
#[test]
fn cli_recipe_bugfix_selects_bugfix_recipe() {
    let r = WorkflowRecipeResolver::from_cli_name("bugfix").expect("resolve bugfix recipe");
    assert_eq!(r.name(), "bugfix");
}

/// Acceptance: bugfix recipe starts at `interview`, then `analyze` / `reproduce`.
#[test]
fn cli_recipe_bugfix_start_goal_is_interview() {
    let r = WorkflowRecipeResolver::from_cli_name("bugfix").expect("resolve bugfix recipe");
    assert_eq!(r.start_goal().as_str(), "interview");
    assert_eq!(r.plan_refinement_goal().as_str(), "analyze");
    assert_eq!(
        r.status_for_state(&WorkflowState::new("Analyzing")),
        "Analyzing",
        "CLI/presenter must agree on analyzing status label"
    );
    assert_eq!(
        r.status_for_state(&WorkflowState::new("Interviewing")),
        "Interviewing",
        "bugfix interview phase exposes Interviewing status"
    );
}

/// CLI default `tdd` uses **interview** as the first workflow goal (then **plan**).
#[test]
fn cli_recipe_tdd_defaults_match_legacy() {
    let r = WorkflowRecipeResolver::from_cli_name("tdd").expect("resolve tdd recipe");
    assert_eq!(r.name(), "tdd");
    assert_eq!(r.start_goal().as_str(), "interview");
}

/// `--recipe grill-me` resolves to GrillMeRecipe (`grill-me` name, `grill` start goal).
#[test]
fn cli_recipe_grill_me_selects_grill_me_recipe() {
    let r = WorkflowRecipeResolver::from_cli_name("grill-me").expect("resolve grill-me recipe");
    assert_eq!(r.name(), "grill-me");
    assert_eq!(r.start_goal().as_str(), "grill");
}

/// `--recipe review` resolves to ReviewRecipe (`review` name).
#[test]
fn cli_recipe_review_selects_review_recipe() {
    let r = WorkflowRecipeResolver::from_cli_name("review").expect("resolve review recipe");
    assert_eq!(r.name(), "review");
    assert_eq!(
        r.start_goal().as_str(),
        "inspect",
        "review workflow starts at inspect (branch diff + elicitation)"
    );
    assert_eq!(
        r.plan_refinement_goal().as_str(),
        "branch-review",
        "after inspect, structured submit step is branch-review → review.md"
    );
}
