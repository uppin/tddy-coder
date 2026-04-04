//! Acceptance tests: `--recipe tdd|bugfix` and config resolution (bugfix workflow recipe PRD).

use tddy_coder::WorkflowRecipeResolver;

/// CLI/config must resolve `bugfix` to BugfixRecipe (`name`, `start_goal` = reproduce).
#[test]
fn cli_recipe_bugfix_selects_bugfix_recipe() {
    let r = WorkflowRecipeResolver::from_cli_name("bugfix").expect("resolve bugfix recipe");
    assert_eq!(r.name(), "bugfix");
    assert_eq!(r.start_goal().as_str(), "reproduce");
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
