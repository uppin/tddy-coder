//! Acceptance: what a `pr-stack` session does at startup when it was told to seed its stack.
//!
//! The session that owns a `changeset.yaml` is the process that writes it, so the seeding happens
//! here rather than in the daemon that spawned it.
//!
//! The workflow state it lands in is not a preference. A seeded node owns a branch from the moment
//! it exists, and `reseed_stack_from_plan_if_unspawned` refuses a plan once any node owns a branch or
//! a session — so a seeded orchestrator that ran its planning phase would reach `write-stack-plan`
//! and have the plan **rejected**. `StackPlanned` is the state whose goal is `orchestrate`, the
//! free-prompting operator loop, which is where an operator who authored the stack themselves wants
//! to be anyway.
//!
//! A failed seed fails startup. There is no fallback to an unseeded orchestrator: that is a
//! different session from the one the operator asked for, and it would come up looking successful.
//!
//! The `--stack-seed-base-session` gate is exercised through the CLI args a spawned `tddy-coder`
//! actually receives, parsed by the real parser: the flag's spelling is the daemon's contract with the
//! coder, and a test that called the seeding function directly would pass with the flag renamed,
//! dropped from `Args`, or never consulted at startup.
//!
//! Feature: `docs/ft/coder/pr-stacking.md#seeding-the-stack-from-an-existing-session-added-2026-08-13`

use std::path::Path;

use clap::Parser;
use tddy_coder::run::{
    seed_pr_stack_from_base_session_if_requested, seed_stack_and_enter_orchestrate, Args, CoderArgs,
};
use tddy_core::changeset::{
    read_changeset, start_goal_for_session_continue, write_changeset, Changeset, Stack,
};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_workflow_recipes::recipe_resolve::resolve_workflow_recipe_from_cli_name;

const ORCHESTRATOR: &str = "orchestrator-1";
const BASE_SESSION: &str = "session-auth-store";
const BASE_BRANCH: &str = "feat/auth-store";

// --- fixtures ---------------------------------------------------------------

fn a_session(sessions_base: &Path, session_id: &str, changeset: Changeset) -> std::path::PathBuf {
    let dir = unified_session_dir_path(sessions_base, session_id);
    std::fs::create_dir_all(&dir).unwrap();
    write_changeset(&dir, &changeset).unwrap();
    dir
}

fn a_session_on_branch(sessions_base: &Path, session_id: &str, branch: &str) {
    a_session(
        sessions_base,
        session_id,
        Changeset {
            branch: Some(branch.to_string()),
            ..Changeset::default()
        },
    );
}

/// A freshly written `pr-stack` orchestrator changeset, as a tool session's startup leaves it:
/// `ensure_changeset_recipe` names the recipe on an otherwise default changeset, so the state is
/// `Changeset::default()`'s `Init` and there is no stack. A tool session's `changeset.yaml` is
/// written by its own `tddy-coder` process, not by the daemon — the daemon's
/// `update_state(recipe.start_goal())` writes belong to the claude-cli / cursor-cli spawn paths.
fn a_fresh_orchestrator(sessions_base: &Path, session_id: &str) -> std::path::PathBuf {
    a_session(
        sessions_base,
        session_id,
        Changeset {
            recipe: Some("pr-stack".to_string()),
            ..Changeset::default()
        },
    )
}

/// A session directory with nothing in it — a tool session before its own workflow has written a
/// `changeset.yaml`, which is when the daemon's spawn actually reaches the seeding gate.
fn a_session_dir_with_no_changeset(sessions_base: &Path, session_id: &str) -> std::path::PathBuf {
    let dir = unified_session_dir_path(sessions_base, session_id);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The command line the daemon spawns a coder with: a recipe, this session's directory, the data root
/// the sessions live under, and whichever stack flags were requested.
fn coder_args(
    sessions_base: &Path,
    session_dir: &Path,
    recipe: &str,
    stack_flags: &[&str],
) -> Args {
    let mut argv = vec![
        "tddy-coder".to_string(),
        "--recipe".to_string(),
        recipe.to_string(),
        "--session-dir".to_string(),
        session_dir.display().to_string(),
        "--tddy-data-dir".to_string(),
        sessions_base.display().to_string(),
    ];
    argv.extend(stack_flags.iter().map(|flag| (*flag).to_string()));
    Args::from(CoderArgs::parse_from(argv))
}

/// The goal this session would run next, resolved the way a start or a continue resolves it.
fn next_goal(session_dir: &Path) -> String {
    let cs = read_changeset(session_dir).unwrap();
    let recipe = resolve_workflow_recipe_from_cli_name(cs.recipe.as_deref().unwrap()).unwrap();
    start_goal_for_session_continue(recipe.as_ref(), &cs)
        .as_str()
        .to_string()
}

/// The persisted stack, or `None` for a changeset that records none.
fn stack_of(session_dir: &Path) -> Option<Stack> {
    read_changeset(session_dir).unwrap().stack
}

// --- rejection assertions ---------------------------------------------------

/// A refused startup, so a test reads as one assertion instead of unwrapping an error by hand.
struct Refusal(String);

fn assert_refused<T: std::fmt::Debug, E: std::fmt::Display>(result: Result<T, E>) -> Refusal {
    match result {
        Err(reason) => Refusal(format!("{reason:#}")),
        Ok(value) => panic!("expected startup to be refused, but it succeeded with {value:?}"),
    }
}

impl Refusal {
    fn with_reason_containing(self, fragment: &str) -> Self {
        assert!(
            self.0.contains(fragment),
            "expected the refusal to mention '{fragment}', was '{}'",
            self.0
        );
        self
    }
}

// --- C1, C2, C4: seeding through the coder's own writer ----------------------

#[test]
fn a_coder_told_to_seed_its_stack_enters_the_operator_loop() {
    // Given a fresh pr-stack orchestrator and a session working on a branch
    let sessions = tempfile::tempdir().unwrap();
    a_session_on_branch(sessions.path(), BASE_SESSION, BASE_BRANCH);
    let orchestrator = a_fresh_orchestrator(sessions.path(), ORCHESTRATOR);

    // When it seeds its stack on that session at startup
    seed_stack_and_enter_orchestrate(&orchestrator, sessions.path(), BASE_SESSION).unwrap();

    // Then it runs `orchestrate`, not the planning phase whose plan its own seeded node would refuse
    assert_eq!(next_goal(&orchestrator), "orchestrate");
}

#[test]
fn a_coder_told_to_seed_its_stack_records_the_base_node() {
    // Given a fresh pr-stack orchestrator and a session working on a branch
    let sessions = tempfile::tempdir().unwrap();
    a_session_on_branch(sessions.path(), BASE_SESSION, BASE_BRANCH);
    let orchestrator = a_fresh_orchestrator(sessions.path(), ORCHESTRATOR);

    // When it seeds its stack on that session at startup
    seed_stack_and_enter_orchestrate(&orchestrator, sessions.path(), BASE_SESSION).unwrap();

    // Then the panel has one row, bound to that branch
    let stack = read_changeset(&orchestrator).unwrap().stack.unwrap();
    assert_eq!(stack.nodes.len(), 1);
    assert_eq!(stack.nodes[0].branch.as_deref(), Some(BASE_BRANCH));
}

#[test]
fn a_failed_seed_fails_startup_rather_than_starting_unseeded() {
    // Given a base session that owns no branch
    let sessions = tempfile::tempdir().unwrap();
    a_session(sessions.path(), "session-unstarted", Changeset::default());
    let orchestrator = a_fresh_orchestrator(sessions.path(), ORCHESTRATOR);

    // When the seed is attempted
    let result =
        seed_stack_and_enter_orchestrate(&orchestrator, sessions.path(), "session-unstarted");

    // Then startup fails, and the session is not left in the operator loop with an empty stack
    assert_refused(result).with_reason_containing("owns no branch");
    assert_eq!(stack_of(&orchestrator), None);
    assert_eq!(next_goal(&orchestrator), "analyze-stack");
}

// --- C3, C5–C7: the startup gate, driven by the flags the daemon passes ------

#[test]
fn an_orchestrator_told_to_seed_nothing_writes_nothing_and_starts_in_the_planning_phase() {
    // Given a fresh pr-stack orchestrator spawned with no stack flags at all
    let sessions = tempfile::tempdir().unwrap();
    let orchestrator = a_fresh_orchestrator(sessions.path(), ORCHESTRATOR);
    let args = coder_args(sessions.path(), &orchestrator, "pr-stack", &[]);

    // When startup runs the seeding gate
    seed_pr_stack_from_base_session_if_requested(&args).unwrap();

    // Then nothing was written and the session plans its own stack — the behaviour every existing
    // caller gets, untouched
    assert_eq!(stack_of(&orchestrator), None);
    assert_eq!(next_goal(&orchestrator), "analyze-stack");
}

#[test]
fn a_blank_stack_base_session_is_read_as_no_seed_at_all() {
    // Given an orchestrator whose flag arrived blank — the daemon fills it from a proto3 string
    // field, which carries "unset" as the empty string
    let sessions = tempfile::tempdir().unwrap();
    let orchestrator = a_fresh_orchestrator(sessions.path(), ORCHESTRATOR);
    let args = coder_args(
        sessions.path(),
        &orchestrator,
        "pr-stack",
        &["--stack-seed-base-session", "   "],
    );

    // When startup runs the seeding gate
    seed_pr_stack_from_base_session_if_requested(&args).unwrap();

    // Then it is the unseeded session, not a seed attempt on a session named "   "
    assert_eq!(stack_of(&orchestrator), None);
    assert_eq!(next_goal(&orchestrator), "analyze-stack");
}

#[test]
fn refuses_a_stack_base_session_named_beside_a_recipe_that_owns_no_stack() {
    // Given a resolvable base session, and a `tdd` session told to seed a stack it has none of
    let sessions = tempfile::tempdir().unwrap();
    a_session_on_branch(sessions.path(), BASE_SESSION, BASE_BRANCH);
    let session_dir = a_session_dir_with_no_changeset(sessions.path(), ORCHESTRATOR);
    let args = coder_args(
        sessions.path(),
        &session_dir,
        "tdd",
        &["--stack-seed-base-session", BASE_SESSION],
    );

    // When startup runs the seeding gate
    let result = seed_pr_stack_from_base_session_if_requested(&args);

    // Then start fails rather than ignoring the flag: a session that looks seeded and is not is worse
    // than one that never started
    assert_refused(result).with_reason_containing("pr-stack");
    assert!(
        read_changeset(&session_dir).is_err(),
        "a refused gate must not write a changeset"
    );
}

/// A refused seed must leave the session directory as it found it.
///
/// The gate has to *create* `changeset.yaml` to seed into it — a tool session has none when startup
/// runs. If it creates it before checking whether the seed can happen at all, a refused start leaves a
/// directory holding `{recipe: pr-stack, state: Init}` behind, and resuming that session brings up an
/// ordinary **unseeded** orchestrator: the recipe the operator asked for, without the stack that was
/// the whole point. Nothing downstream can tell that apart from a stack the agent has yet to plan.
#[test]
fn a_refused_seed_leaves_no_changeset_behind() {
    // Given a session directory as a tool session's startup really finds it — no `changeset.yaml` yet
    // — told to seed on a session that owns no branch
    let sessions = tempfile::tempdir().unwrap();
    a_session(sessions.path(), "session-unstarted", Changeset::default());
    let orchestrator = a_session_dir_with_no_changeset(sessions.path(), ORCHESTRATOR);
    let args = coder_args(
        sessions.path(),
        &orchestrator,
        "pr-stack",
        &["--stack-seed-base-session", "session-unstarted"],
    );

    // When startup runs the seeding gate
    let result = seed_pr_stack_from_base_session_if_requested(&args);

    // Then startup fails and the directory is untouched — there is no half-made orchestrator to resume
    assert_refused(result).with_reason_containing("owns no branch");
    assert!(
        !orchestrator.join("changeset.yaml").exists(),
        "a refused seed must not leave a changeset behind for a later resume to read as unseeded"
    );
}

/// The same, for the other precondition the gate checks ahead of the write: a stack cannot contain the
/// session that owns it.
#[test]
fn a_seed_refused_for_naming_its_own_session_leaves_no_changeset_behind() {
    // Given an orchestrator with no changeset yet, told to base its stack on itself
    let sessions = tempfile::tempdir().unwrap();
    let orchestrator = a_session_dir_with_no_changeset(sessions.path(), ORCHESTRATOR);
    let args = coder_args(
        sessions.path(),
        &orchestrator,
        "pr-stack",
        &["--stack-seed-base-session", ORCHESTRATOR],
    );

    // When startup runs the seeding gate
    let result = seed_pr_stack_from_base_session_if_requested(&args);

    // Then it is refused before anything is written
    assert_refused(result).with_reason_containing("itself");
    assert!(
        !orchestrator.join("changeset.yaml").exists(),
        "a refused seed must not leave a changeset behind for a later resume to read as unseeded"
    );
}

#[test]
fn seeds_a_session_that_has_no_changeset_yet() {
    // Given a session directory as a tool session's startup really finds it — the daemon creates the
    // directory, and the coder's own workflow has not written `changeset.yaml` yet
    let sessions = tempfile::tempdir().unwrap();
    a_session_on_branch(sessions.path(), BASE_SESSION, BASE_BRANCH);
    let orchestrator = a_session_dir_with_no_changeset(sessions.path(), ORCHESTRATOR);
    let args = coder_args(
        sessions.path(),
        &orchestrator,
        "pr-stack",
        &["--stack-seed-base-session", BASE_SESSION],
    );

    // When startup runs the seeding gate
    seed_pr_stack_from_base_session_if_requested(&args).unwrap();

    // Then the changeset was created, seeded and left in the operator loop
    let stack = stack_of(&orchestrator).expect("the gate must create the changeset it seeds");
    assert_eq!(stack.nodes.len(), 1);
    assert_eq!(stack.nodes[0].branch.as_deref(), Some(BASE_BRANCH));
    assert_eq!(next_goal(&orchestrator), "orchestrate");
}
