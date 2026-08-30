//! PRD acceptance ([docs/ft/coder/pr-stack-docs.md] § The `write-stack-docs` goal): the pr-stack
//! recipe authors a PRD and a changeset for every planned PR, under
//! `artifacts/prs/<node_id>/`, and only then drops into the interactive `orchestrate` loop.
//!
//! The documents exist so a child agent knows where its PR stops and its neighbours' begin. That is
//! why an incomplete pass is refused rather than half-written (see
//! `pr_stack_docs_validation_acceptance.rs`) and why a refined plan sends the session back through
//! this goal.

use std::fs;
use std::path::Path;

use tddy_core::changeset::{read_changeset, write_changeset, Changeset, Stack, StackNode};
use tddy_core::workflow::context::Context;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::recipe::WorkflowRecipe;
use tddy_core::workflow::task::{NextAction, TaskResult};
use tddy_workflow_recipes::plan_pr_stack::write_stack_docs_system_prompt;
use tddy_workflow_recipes::pr_stack::{
    node_doc_paths, PrStackHooks, PrStackRecipe, STATE_STACK_DOCS_WRITTEN, STATE_STACK_PLANNED,
};

// ── Builders ────────────────────────────────────────────────────────────────────────────────

/// A planned node — no branch, no session: the state every node is in when the docs pass runs.
fn a_planned_node(node_id: &str, title: &str) -> StackNode {
    StackNode {
        node_id: node_id.to_string(),
        title: title.to_string(),
        description: format!("{title} description"),
        branch_suggestion: Some(format!("feature/auth/{node_id}")),
        branch: None,
        session_id: None,
        parents: vec![],
        pr_status: None,
        child_state: None,
        internal_status: None,
        display_order: None,
    }
}

fn with_parents(mut node: StackNode, parents: &[&str]) -> StackNode {
    node.parents = parents.iter().map(|p| p.to_string()).collect();
    node
}

/// An orchestrator session whose plan is already persisted — the state `write-stack-docs` runs in.
fn an_orchestrator_with_stack(nodes: Vec<StackNode>) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("temp session dir");
    let changeset = Changeset {
        stack: Some(Stack { version: 1, nodes }),
        ..Changeset::default()
    };
    write_changeset(tmp.path(), &changeset).expect("seed changeset with stack");
    tmp
}

/// A well-formed changeset body carrying all four required headings.
fn a_changeset_body(node_id: &str) -> String {
    format!(
        "# Changeset: {node_id}\n\n\
         ## Responsibility\n\
         Owns the token store.\n\n\
         ## Boundaries\n\
         Does not touch the HTTP middleware — that is n2.\n\n\
         ## Dependencies\n\
         None; this is a root node.\n\n\
         ## Draft PR contract\n\
         Land `trait TokenStore` plus its failing tests first.\n"
    )
}

fn a_docs_submit(node_ids: &[&str]) -> String {
    let mut yaml = String::from("version: 1\ndocs:\n");
    for node_id in node_ids {
        yaml.push_str(&format!(
            "  - node_id: {node_id}\n    prd: |\n      # {node_id} — PRD\n      What this PR delivers.\n"
        ));
        yaml.push_str("    changeset: |\n");
        for line in a_changeset_body(node_id).lines() {
            yaml.push_str(&format!("      {line}\n"));
        }
    }
    yaml
}

/// The example payload the `write-stack-docs` system prompt shows the agent, lifted from the
/// prompt's own fenced `json` block rather than restated here — so a prompt that starts describing
/// a shape the hook refuses fails this suite instead of a live session.
fn the_payload_the_prompt_shows() -> String {
    let prompt = write_stack_docs_system_prompt("pr-stack");
    let opening = "```json\n";
    let body_start = prompt
        .find(opening)
        .expect("the write-stack-docs prompt must show a fenced json example")
        + opening.len();
    let body = &prompt[body_start..];
    let body_end = body
        .find("```")
        .expect("the prompt's fenced json example must be closed");
    body[..body_end].trim().to_string()
}

/// The node the prompt's example documents. The hook refuses a pass that misses a planned node, so
/// the stack under test is the one that example is a complete pass over.
const NODE_THE_PROMPT_EXAMPLE_DOCUMENTS: &str = "token-store";

// ── Seams ───────────────────────────────────────────────────────────────────────────────────

fn a_task_result(task_id: &str) -> TaskResult {
    TaskResult {
        response: String::new(),
        next_action: NextAction::Continue,
        task_id: task_id.to_string(),
        status_message: None,
    }
}

fn goal_completes(
    session_dir: &Path,
    goal: &str,
    submit_yaml: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let context = Context::new();
    context.set_sync("session_dir", session_dir.to_path_buf());
    context.set_sync("output", submit_yaml);
    PrStackHooks::new(None).after_task(goal, &context, &a_task_result(goal))
}

fn write_stack_docs_completes(session_dir: &Path, submit_yaml: String) {
    goal_completes(session_dir, "write-stack-docs", submit_yaml)
        .expect("write-stack-docs after_task");
}

// ── Assertions ──────────────────────────────────────────────────────────────────────────────

trait NodeDocAssertions {
    fn assert_documents_for(&self, node_id: &str) -> &Self;
    fn assert_changeset_for(&self, node_id: &str, contains: &str) -> &Self;
    fn assert_state(&self, expected: &str) -> &Self;
}

impl NodeDocAssertions for Path {
    fn assert_documents_for(&self, node_id: &str) -> &Self {
        let paths = node_doc_paths(self, node_id);
        assert!(
            paths.prd.is_file(),
            "expected a PRD for '{node_id}' at {}",
            paths.prd.display()
        );
        assert!(
            paths.changeset.is_file(),
            "expected a changeset for '{node_id}' at {}",
            paths.changeset.display()
        );
        self
    }

    fn assert_changeset_for(&self, node_id: &str, contains: &str) -> &Self {
        let path = node_doc_paths(self, node_id).changeset;
        let body = fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!("read changeset for '{node_id}' at {}: {e}", path.display())
        });
        assert!(
            body.contains(contains),
            "changeset for '{node_id}' must contain '{contains}'; got:\n{body}"
        );
        self
    }

    fn assert_state(&self, expected: &str) -> &Self {
        let changeset = read_changeset(self).expect("read changeset");
        assert_eq!(
            changeset.state.current.as_str(),
            expected,
            "session state mismatch"
        );
        self
    }
}

// ── The docs pass writes a document pair per node ────────────────────────────────────────────

#[test]
fn a_prd_and_a_changeset_are_written_for_every_planned_node() {
    // Given — a two-node stack whose plan is already persisted
    let session = an_orchestrator_with_stack(vec![
        a_planned_node("n1", "Token store"),
        with_parents(a_planned_node("n2", "Auth middleware"), &["n1"]),
    ]);

    // When — the agent submits documents for both nodes
    write_stack_docs_completes(session.path(), a_docs_submit(&["n1", "n2"]));

    // Then
    session
        .path()
        .assert_documents_for("n1")
        .assert_documents_for("n2");
}

#[test]
fn each_node_owns_its_documents_under_its_own_directory() {
    // Given
    let session = an_orchestrator_with_stack(vec![
        a_planned_node("n1", "Token store"),
        a_planned_node("n2", "Auth middleware"),
    ]);

    // When
    write_stack_docs_completes(session.path(), a_docs_submit(&["n1", "n2"]));

    // Then — a node's boundaries must not be readable as another node's
    session
        .path()
        .assert_changeset_for("n1", "# Changeset: n1")
        .assert_changeset_for("n2", "# Changeset: n2");
}

#[test]
fn node_documents_live_under_the_artifacts_root() {
    // Given
    let session = an_orchestrator_with_stack(vec![a_planned_node("n1", "Token store")]);

    // When
    write_stack_docs_completes(session.path(), a_docs_submit(&["n1"]));

    // Then — the exact path a SESSION_ARTIFACT attachment names
    let expected = session
        .path()
        .join("artifacts")
        .join("prs")
        .join("n1")
        .join("PRD.md");
    assert!(
        expected.is_file(),
        "a node PRD must be reachable at artifacts/prs/<node_id>/PRD.md; looked at {}",
        expected.display()
    );
}

#[test]
fn rewriting_the_documents_replaces_the_previous_pass() {
    // Given — a stack whose documents have already been written once
    let session = an_orchestrator_with_stack(vec![a_planned_node("n1", "Token store")]);
    write_stack_docs_completes(session.path(), a_docs_submit(&["n1"]));

    // When — the agent re-runs the pass with a different body
    let revised =
        a_docs_submit(&["n1"]).replace("Owns the token store.", "Owns the refresh store.");
    write_stack_docs_completes(session.path(), revised);

    // Then — the pass overwrites rather than appending a second copy
    session
        .path()
        .assert_changeset_for("n1", "Owns the refresh store.");
    let body = fs::read_to_string(node_doc_paths(session.path(), "n1").changeset).unwrap();
    assert!(
        !body.contains("Owns the token store."),
        "a re-run must replace the previous changeset, not append to it; got:\n{body}"
    );
}

// ── The docs pass sits between planning and the interactive loop ─────────────────────────────

#[test]
fn writing_the_documents_moves_the_session_into_the_orchestrate_loop() {
    // Given
    let session = an_orchestrator_with_stack(vec![a_planned_node("n1", "Token store")]);

    // When
    write_stack_docs_completes(session.path(), a_docs_submit(&["n1"]));

    // Then
    session.path().assert_state(STATE_STACK_DOCS_WRITTEN);
}

#[test]
fn a_planned_stack_resolves_to_the_docs_goal() {
    // Given — a stack that has been planned but not yet documented
    let recipe = PrStackRecipe;

    // When
    let goal = recipe.next_goal_for_state(&WorkflowState::new(STATE_STACK_PLANNED));

    // Then
    assert_eq!(
        goal.as_ref().map(|g| g.as_str()),
        Some("write-stack-docs"),
        "a planned stack must document itself before the operator starts driving it"
    );
}

/// Weaker than it reads, and deliberately kept anyway: `next_goal_for_state` ends in a catch-all
/// sending every unrecognised state to `orchestrate`, so this passes whether or not
/// `StackDocsWritten` is named in the table. It guards against a *wrong explicit* mapping, not a
/// missing one — the load-bearing assertion of the pair is
/// `a_planned_stack_resolves_to_the_docs_goal` above, which the catch-all cannot satisfy.
#[test]
fn a_documented_stack_resolves_to_the_orchestrate_goal() {
    // Given
    let recipe = PrStackRecipe;

    // When
    let goal = recipe.next_goal_for_state(&WorkflowState::new(STATE_STACK_DOCS_WRITTEN));

    // Then
    assert_eq!(
        goal.as_ref().map(|g| g.as_str()),
        Some("orchestrate"),
        "once documented, the session drops into the free-prompting operator loop"
    );
}

#[test]
fn a_refined_plan_returns_the_session_to_the_docs_pass() {
    // Given — a documented stack the operator then re-plans through chat
    let session = an_orchestrator_with_stack(vec![a_planned_node("n1", "Token store")]);
    write_stack_docs_completes(session.path(), a_docs_submit(&["n1"]));

    // When — a refinement turn re-runs write-stack-plan on the same session
    let replan = r#"version: 1
prs:
  - node_id: n1
    title: Token store
    description: persist and read auth tokens
    branch_suggestion: feature/auth/token-store
    parents: []
  - node_id: n2
    title: Auth middleware
    description: verify tokens on each request
    branch_suggestion: feature/auth/middleware
    parents: [n1]
"#;
    goal_completes(session.path(), "write-stack-plan", replan.to_string())
        .expect("write-stack-plan after_task");

    // Then — documents describing a superseded plan must be regenerated before driving resumes
    session.path().assert_state(STATE_STACK_PLANNED);
}

// ── The prompt's example is a payload the hook accepts ──────────────────────────────────────

/// The agent copies this payload out of its system prompt; `tddy-tools submit` hands it to the hook
/// as-is. A prompt whose own example the validator refuses teaches a shape that can never land, and
/// no test that hand-writes its input can see that.
#[test]
fn the_payload_the_prompt_shows_is_persisted_as_a_document_pair() {
    // Given — a stack planned with exactly the node the prompt's example documents
    let session = an_orchestrator_with_stack(vec![a_planned_node(
        NODE_THE_PROMPT_EXAMPLE_DOCUMENTS,
        "Auth token store",
    )]);

    // When
    write_stack_docs_completes(session.path(), the_payload_the_prompt_shows());

    // Then — both documents land, and the changeset carries the section the boundaries rest on
    session
        .path()
        .assert_documents_for(NODE_THE_PROMPT_EXAMPLE_DOCUMENTS)
        .assert_changeset_for(NODE_THE_PROMPT_EXAMPLE_DOCUMENTS, "## Draft PR contract")
        .assert_state(STATE_STACK_DOCS_WRITTEN);
}
