//! Acceptance: a stack seeded, at creation time, from a session the operator already has.
//!
//! Every existing way to populate a stack starts from something the operator does not have yet: a
//! plan the agent has to write, a planned node with no branch behind it, or a pull request that must
//! already exist. The one case that has no path is the common one — "I am working on
//! `feat/auth-store`, and the next PR goes on top of it".
//!
//! [`seed_stack_with_base_session`] is that path. It appends exactly one root node, bound to a
//! session that already owns a branch, and it is a *creation-time* act: it refuses a stack that
//! already has nodes rather than growing one, because the thing that grows a stack is
//! `add_planned_pr_node`.
//!
//! What the node records is as much the point as what it does not. `branch` is real from the start,
//! so `branch_suggestion` stays `None` — nothing here is going to choose a name. `pr_status` stays
//! `None`: whether that branch has a pull request is the live-status poll's to discover, and
//! asserting `planned` for a branch that may already have an open PR would misreport it until the
//! first tick.
//!
//! The payoff is not in this module at all, which is why A11 exists: once the base node is seeded,
//! the *existing* spawn plumbing bases a node parented on it off `origin/<base branch>`. That is
//! `Stack::base_ref_for_spawn` doing what it has always done, over a stack that could not previously
//! be built this way.
//!
//! Feature: `docs/ft/coder/pr-stacking.md#seeding-the-stack-from-an-existing-session-added-2026-08-13`

mod common;

use std::path::Path;

use common::{a_planned_node, assert_rejected, stack_of, write_stack, DEFAULT_BRANCH};
use tddy_core::changeset::{write_changeset, Changeset, Stack, StackNode};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_workflow_recipes::pr_stack::seed_stack_with_base_session;

const ORCHESTRATOR: &str = "orchestrator-1";
const BASE_SESSION: &str = "session-auth-store";
const BASE_BRANCH: &str = "feat/auth-store";

// --- fixtures ---------------------------------------------------------------

/// A sessions root holding one session directory per id, the layout
/// `unified_session_dir_path` resolves and every stack reader expects.
struct SessionsRoot {
    _tmp: tempfile::TempDir,
    root: std::path::PathBuf,
}

fn a_sessions_root() -> SessionsRoot {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    SessionsRoot { _tmp: tmp, root }
}

impl SessionsRoot {
    fn path(&self) -> &Path {
        &self.root
    }

    /// A session directory carrying `changeset.yaml`, created the way a real session's is.
    fn with_session(&self, session_id: &str, changeset: Changeset) -> std::path::PathBuf {
        let dir = unified_session_dir_path(&self.root, session_id);
        std::fs::create_dir_all(&dir).unwrap();
        write_changeset(&dir, &changeset).unwrap();
        dir
    }

    /// A code session that owns a branch — the kind the picker offers as a stack base.
    fn with_session_on_branch(&self, session_id: &str, branch: &str) -> std::path::PathBuf {
        self.with_session(
            session_id,
            Changeset {
                branch: Some(branch.to_string()),
                ..Changeset::default()
            },
        )
    }

    /// A freshly created `pr-stack` orchestrator: a recipe, and no stack yet.
    fn with_fresh_orchestrator(&self, session_id: &str) -> std::path::PathBuf {
        self.with_session(
            session_id,
            Changeset {
                recipe: Some("pr-stack".to_string()),
                ..Changeset::default()
            },
        )
    }
}

/// The named session's changeset, with a feature name on it.
fn named(name: &str, branch: &str) -> Changeset {
    Changeset {
        name: Some(name.to_string()),
        branch: Some(branch.to_string()),
        ..Changeset::default()
    }
}

// --- A1–A6: what the seeded node records ------------------------------------

#[test]
fn seeds_a_single_root_node_bound_to_the_base_sessions_branch() {
    // Given a fresh orchestrator and a session working on `feat/auth-store`
    let sessions = a_sessions_root();
    sessions.with_session_on_branch(BASE_SESSION, BASE_BRANCH);
    let orchestrator = sessions.with_fresh_orchestrator(ORCHESTRATOR);

    // When the stack is seeded on that session
    seed_stack_with_base_session(&orchestrator, sessions.path(), BASE_SESSION).unwrap();

    // Then the stack is exactly one root node owning that branch, with no suggestion to resolve
    let stack = stack_of(&orchestrator);
    assert_eq!(stack.nodes.len(), 1, "seeding appends exactly one node");
    let node = &stack.nodes[0];
    assert_eq!(node.branch.as_deref(), Some(BASE_BRANCH));
    assert_eq!(node.branch_suggestion, None);
    assert_eq!(node.parents, Vec::<String>::new());
}

#[test]
fn titles_the_seeded_node_after_the_base_sessions_name() {
    // Given a base session with a feature name
    let sessions = a_sessions_root();
    sessions.with_session(BASE_SESSION, named("Auth token store", BASE_BRANCH));
    let orchestrator = sessions.with_fresh_orchestrator(ORCHESTRATOR);

    // When the stack is seeded on it
    let node = seed_stack_with_base_session(&orchestrator, sessions.path(), BASE_SESSION).unwrap();

    // Then the row reads as the operator named the work
    assert_eq!(node.title, "Auth token store");
}

#[test]
fn titles_the_seeded_node_after_its_branch_when_the_base_session_has_no_name() {
    // Given a base session that was never named
    let sessions = a_sessions_root();
    sessions.with_session_on_branch(BASE_SESSION, BASE_BRANCH);
    let orchestrator = sessions.with_fresh_orchestrator(ORCHESTRATOR);

    // When the stack is seeded on it
    let node = seed_stack_with_base_session(&orchestrator, sessions.path(), BASE_SESSION).unwrap();

    // Then the branch is the title — a node with no legible title is the one thing the panel
    // cannot render usefully
    assert_eq!(node.title, BASE_BRANCH);
}

#[test]
fn titles_the_seeded_node_after_its_branch_when_the_base_sessions_name_is_blank() {
    // Given a base session whose name is present but blank — a hand-edited changeset, or a form that
    // submitted whitespace
    let sessions = a_sessions_root();
    sessions.with_session(BASE_SESSION, named("   ", BASE_BRANCH));
    let orchestrator = sessions.with_fresh_orchestrator(ORCHESTRATOR);

    // When the stack is seeded on it
    let node = seed_stack_with_base_session(&orchestrator, sessions.path(), BASE_SESSION).unwrap();

    // Then the branch is the title: a blank name is no more legible in the panel than no name at all
    assert_eq!(node.title, BASE_BRANCH);
}

#[test]
fn records_the_base_sessions_id_on_the_seeded_node() {
    // Given a base session
    let sessions = a_sessions_root();
    sessions.with_session_on_branch(BASE_SESSION, BASE_BRANCH);
    let orchestrator = sessions.with_fresh_orchestrator(ORCHESTRATOR);

    // When the stack is seeded on it
    let node = seed_stack_with_base_session(&orchestrator, sessions.path(), BASE_SESSION).unwrap();

    // Then the row can open the session it is bound to
    assert_eq!(node.session_id.as_deref(), Some(BASE_SESSION));
}

#[test]
fn leaves_the_seeded_nodes_pr_status_unset() {
    // Given a base session whose branch may or may not have a pull request
    let sessions = a_sessions_root();
    sessions.with_session_on_branch(BASE_SESSION, BASE_BRANCH);
    let orchestrator = sessions.with_fresh_orchestrator(ORCHESTRATOR);

    // When the stack is seeded on it
    let node = seed_stack_with_base_session(&orchestrator, sessions.path(), BASE_SESSION).unwrap();

    // Then seeding asserts nothing about GitHub — the live-status poll is what discovers a PR
    assert_eq!(node.pr_status, None);
}

#[test]
fn numbers_the_seeded_node_first_in_display_order() {
    // Given a fresh orchestrator
    let sessions = a_sessions_root();
    sessions.with_session_on_branch(BASE_SESSION, BASE_BRANCH);
    let orchestrator = sessions.with_fresh_orchestrator(ORCHESTRATOR);

    // When the stack is seeded on a base session
    let node = seed_stack_with_base_session(&orchestrator, sessions.path(), BASE_SESSION).unwrap();

    // Then it is the first row, so every node added on top of it reads below it
    assert_eq!(node.display_order, Some(0));
}

// --- A7–A10: refusals, none of which write ----------------------------------

#[test]
fn refuses_a_base_session_that_owns_no_branch() {
    // Given a session that has not created its branch yet
    let sessions = a_sessions_root();
    sessions.with_session("session-unstarted", Changeset::default());
    let orchestrator = sessions.with_fresh_orchestrator(ORCHESTRATOR);

    // When the stack is asked to base on it
    let result = seed_stack_with_base_session(&orchestrator, sessions.path(), "session-unstarted");

    // Then it is refused: the branch is the node's whole purpose, and a branchless base would fail
    // the spawn gate for every descendant
    assert_rejected(result).with_reason_containing("owns no branch");
    assert_eq!(
        tddy_core::read_changeset(&orchestrator).unwrap().stack,
        None,
        "a refused seed writes nothing"
    );
}

#[test]
fn refuses_a_base_session_that_does_not_exist() {
    // Given a sessions root with no such session
    let sessions = a_sessions_root();
    let orchestrator = sessions.with_fresh_orchestrator(ORCHESTRATOR);

    // When the stack is asked to base on it
    let result = seed_stack_with_base_session(&orchestrator, sessions.path(), "session-ghost");

    // Then it is refused, naming what could not be resolved
    assert_rejected(result).with_reason_containing("session-ghost");
    assert_eq!(
        tddy_core::read_changeset(&orchestrator).unwrap().stack,
        None,
        "a refused seed writes nothing"
    );
}

#[test]
fn refuses_to_seed_an_orchestrator_whose_stack_already_has_nodes() {
    // Given an orchestrator that already has a plan
    let sessions = a_sessions_root();
    sessions.with_session_on_branch(BASE_SESSION, BASE_BRANCH);
    let orchestrator = sessions.with_fresh_orchestrator(ORCHESTRATOR);
    write_stack(&orchestrator, vec![a_planned_node("n1", &[])]);

    // When it is asked to seed a base node
    let result = seed_stack_with_base_session(&orchestrator, sessions.path(), BASE_SESSION);

    // Then it is refused: seeding is a creation-time act, and growing a stack is
    // `add_planned_pr_node`'s job
    assert_rejected(result)
        .with_reason_containing("already has 1 node(s)")
        .with_reason_containing("nothing to seed");
    assert_eq!(
        stack_of(&orchestrator).nodes.len(),
        1,
        "the existing plan is left exactly as it was"
    );
}

#[test]
fn refuses_an_orchestrator_asked_to_base_the_stack_on_itself() {
    // Given an orchestrator that somehow records a branch of its own
    let sessions = a_sessions_root();
    let orchestrator = sessions.with_session(
        ORCHESTRATOR,
        Changeset {
            recipe: Some("pr-stack".to_string()),
            branch: Some("feat/orchestrator".to_string()),
            ..Changeset::default()
        },
    );

    // When it is asked to base its stack on itself
    let result = seed_stack_with_base_session(&orchestrator, sessions.path(), ORCHESTRATOR);

    // Then it is refused: a stack cannot contain the session that owns it
    assert_rejected(result).with_reason_containing("itself");
    assert_eq!(
        tddy_core::read_changeset(&orchestrator).unwrap().stack,
        None,
        "a refused seed writes nothing"
    );
}

#[test]
fn refuses_a_base_session_that_is_already_a_node_of_another_stack() {
    // Given a session on a branch that another orchestrator already tracks as one of its nodes
    let sessions = a_sessions_root();
    sessions.with_session(
        BASE_SESSION,
        Changeset {
            branch: Some(BASE_BRANCH.to_string()),
            orchestrator_session_id: Some("orchestrator-elsewhere".to_string()),
            ..Changeset::default()
        },
    );
    let orchestrator = sessions.with_fresh_orchestrator(ORCHESTRATOR);

    // When this orchestrator is asked to base its stack on it
    let result = seed_stack_with_base_session(&orchestrator, sessions.path(), BASE_SESSION);

    // Then it is refused: two orchestrators holding repoint and pull authority over one branch is
    // ambiguous ownership, and refusing is the recoverable direction — the operator can still stack on
    // the branch by adding a node to the stack that already owns it
    assert_rejected(result)
        .with_reason_containing("already a node")
        .with_reason_containing("orchestrator-elsewhere");
    assert_eq!(
        tddy_core::read_changeset(&orchestrator).unwrap().stack,
        None,
        "a refused seed writes nothing"
    );
}

/// The same refusal when the two paths are spelled differently.
///
/// An orchestrator reaches [`seed_stack_with_base_session`] as the path *its own process* was given
/// (`--session-dir`), which need not be spelled the way `sessions_root` spells it: a symlinked or
/// relative data root produces two names for one directory. Comparing the literal paths alone would let
/// a stack be seeded on the session that owns it, and every descendant would then be stacked on the
/// orchestrator's own branch.
#[cfg(unix)]
#[test]
fn refuses_to_base_the_stack_on_itself_when_the_sessions_root_is_reached_through_a_symlink() {
    // Given an orchestrator whose own directory is spelled through a symlinked sessions root
    let sessions = a_sessions_root();
    sessions.with_session(
        ORCHESTRATOR,
        Changeset {
            recipe: Some("pr-stack".to_string()),
            branch: Some("feat/orchestrator".to_string()),
            ..Changeset::default()
        },
    );
    let elsewhere = tempfile::tempdir().unwrap();
    let linked_root = elsewhere.path().join("data-root-link");
    std::os::unix::fs::symlink(sessions.path(), &linked_root).unwrap();
    let orchestrator_via_link = unified_session_dir_path(&linked_root, ORCHESTRATOR);
    assert_ne!(
        orchestrator_via_link,
        unified_session_dir_path(sessions.path(), ORCHESTRATOR),
        "the two spellings must differ literally, or this test proves nothing"
    );

    // When it is asked to base its stack on itself, named through the root it was resolved from
    let result =
        seed_stack_with_base_session(&orchestrator_via_link, sessions.path(), ORCHESTRATOR);

    // Then it is refused all the same — the same directory under a second name is the same session
    assert_rejected(result).with_reason_containing("itself");
    assert_eq!(
        tddy_core::read_changeset(&orchestrator_via_link)
            .unwrap()
            .stack,
        None,
        "a refused seed writes nothing"
    );
}

// --- A11: the payoff, over existing plumbing --------------------------------

#[test]
fn a_planned_node_parented_on_the_seeded_base_node_spawns_off_the_base_sessions_branch() {
    // Given a stack seeded on a session working on `feat/auth-store`
    let sessions = a_sessions_root();
    sessions.with_session_on_branch(BASE_SESSION, BASE_BRANCH);
    let orchestrator = sessions.with_fresh_orchestrator(ORCHESTRATOR);
    let base = seed_stack_with_base_session(&orchestrator, sessions.path(), BASE_SESSION).unwrap();

    // And a planned PR stacked on top of it
    let stack = Stack {
        version: 1,
        nodes: vec![
            base.clone(),
            StackNode {
                parents: vec![base.node_id.clone()],
                ..a_planned_node("n2", &[])
            },
        ],
    };

    // When the spawn base for the planned node is resolved
    let spawn_base = stack
        .base_ref_for_spawn("n2", &format!("origin/{DEFAULT_BRANCH}"))
        .unwrap();

    // Then it is the base session's branch, not the project default — the stacking this feature
    // exists for, delivered by plumbing that already shipped
    assert_eq!(spawn_base, format!("origin/{BASE_BRANCH}"));
}
