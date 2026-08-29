//! PRD acceptance ([docs/ft/coder/pr-stack-docs.md] § Validation): a `write-stack-docs` submit is
//! validated before anything is written, and a rejected submit writes **nothing** — the same
//! contract every other stack writer holds.
//!
//! Two rules carry the design. **A partial pass is refused**: a node with no boundaries document is
//! precisely the duplicate-development hazard the feature exists to prevent, and a half-written pass
//! leaves an operator unable to tell "not written yet" from "deliberately empty". **The four
//! required headings are checked structurally**: whether the boundaries are *correct* needs the diff
//! the node has yet to produce, so that judgement stays prompt-carried and human-reviewed, but
//! whether the section is present at all needs no semantics.

use std::fs;
use std::path::Path;

use tddy_core::changeset::{write_changeset, Changeset, Stack, StackNode};
use tddy_core::workflow::context::Context;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::task::{NextAction, TaskResult};
use tddy_workflow_recipes::pr_stack::{node_doc_paths, PrStackHooks};

// ── Builders ────────────────────────────────────────────────────────────────────────────────

fn a_planned_node(node_id: &str) -> StackNode {
    StackNode {
        node_id: node_id.to_string(),
        title: format!("{node_id} title"),
        description: format!("{node_id} description"),
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

fn an_orchestrator_with_stack(node_ids: &[&str]) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("temp session dir");
    let changeset = Changeset {
        stack: Some(Stack {
            version: 1,
            nodes: node_ids.iter().map(|id| a_planned_node(id)).collect(),
        }),
        ..Changeset::default()
    };
    write_changeset(tmp.path(), &changeset).expect("seed changeset with stack");
    tmp
}

/// A changeset body carrying all four required headings. Callers drop one to test its absence.
fn a_changeset_body_with_headings(headings: &[&str]) -> String {
    let mut body = String::from("# Changeset\n\n");
    for heading in headings {
        body.push_str(heading);
        body.push_str("\nSome content for this section.\n\n");
    }
    body
}

const ALL_HEADINGS: &[&str] = &[
    "## Responsibility",
    "## Boundaries",
    "## Dependencies",
    "## Draft PR contract",
];

fn a_docs_submit_with(entries: &[(&str, &str, &str)]) -> String {
    let mut yaml = String::from("version: 1\ndocs:\n");
    for (node_id, prd, changeset) in entries {
        yaml.push_str(&format!("  - node_id: {node_id}\n"));
        yaml.push_str("    prd: |\n");
        for line in prd.lines() {
            yaml.push_str(&format!("      {line}\n"));
        }
        yaml.push_str("    changeset: |\n");
        for line in changeset.lines() {
            yaml.push_str(&format!("      {line}\n"));
        }
    }
    yaml
}

/// The happy-path entry for one node, so a test names only the field it is breaking.
fn a_complete_entry(node_id: &str) -> (String, String, String) {
    (
        node_id.to_string(),
        format!("# {node_id} — PRD\nWhat this PR delivers.\n"),
        a_changeset_body_with_headings(ALL_HEADINGS),
    )
}

fn a_complete_submit(node_ids: &[&str]) -> String {
    let owned: Vec<(String, String, String)> =
        node_ids.iter().map(|id| a_complete_entry(id)).collect();
    let borrowed: Vec<(&str, &str, &str)> = owned
        .iter()
        .map(|(a, b, c)| (a.as_str(), b.as_str(), c.as_str()))
        .collect();
    a_docs_submit_with(&borrowed)
}

// ── Seams ───────────────────────────────────────────────────────────────────────────────────

fn write_stack_docs(
    session_dir: &Path,
    submit_yaml: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let context = Context::new();
    context.set_sync("session_dir", session_dir.to_path_buf());
    context.set_sync("output", submit_yaml);
    let result = TaskResult {
        response: String::new(),
        next_action: NextAction::Continue,
        task_id: "write-stack-docs".to_string(),
        status_message: None,
    };
    PrStackHooks::new(None).after_task("write-stack-docs", &context, &result)
}

// ── Assertions ──────────────────────────────────────────────────────────────────────────────

struct RefusalAssert(String);

fn assert_refused(result: Result<(), Box<dyn std::error::Error + Send + Sync>>) -> RefusalAssert {
    match result {
        Err(e) => RefusalAssert(e.to_string()),
        Ok(()) => panic!("expected the submit to be refused, but it was accepted"),
    }
}

impl RefusalAssert {
    fn naming(self, fragment: &str) -> Self {
        assert!(
            self.0.contains(fragment),
            "the refusal must name '{fragment}' so the agent can fix it; message was '{}'",
            self.0
        );
        self
    }
}

trait NoWriteAssertions {
    fn assert_no_documents_at_all(&self) -> &Self;
    fn assert_changeset_for(&self, node_id: &str, contains: &str) -> &Self;
}

impl NoWriteAssertions for Path {
    fn assert_no_documents_at_all(&self) -> &Self {
        let prs_root = self.join("artifacts").join("prs");
        assert!(
            !prs_root.exists(),
            "a refused submit must write nothing; found {}",
            prs_root.display()
        );
        self
    }

    fn assert_changeset_for(&self, node_id: &str, contains: &str) -> &Self {
        let path = node_doc_paths(self, node_id).changeset;
        let body = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read changeset for '{node_id}': {e}"));
        assert!(
            body.contains(contains),
            "changeset for '{node_id}' must still contain '{contains}'; got:\n{body}"
        );
        self
    }
}

// ── The pass must cover the whole stack ──────────────────────────────────────────────────────

#[test]
fn a_submit_naming_a_node_outside_the_stack_is_refused() {
    // Given — a stack holding only n1
    let session = an_orchestrator_with_stack(&["n1"]);

    // When — the agent documents a node that was never planned
    let result = write_stack_docs(session.path(), a_complete_submit(&["n1", "n7"]));

    // Then
    assert_refused(result).naming("n7");
}

#[test]
fn a_submit_naming_a_node_outside_the_stack_writes_nothing() {
    // Given
    let session = an_orchestrator_with_stack(&["n1"]);

    // When
    let _ = write_stack_docs(session.path(), a_complete_submit(&["n1", "n7"]));

    // Then — the valid half of a rejected submit must not land either
    session.path().assert_no_documents_at_all();
}

#[test]
fn a_submit_omitting_a_planned_node_is_refused() {
    // Given — a two-node stack
    let session = an_orchestrator_with_stack(&["n1", "n2"]);

    // When — the agent documents only one of them
    let result = write_stack_docs(session.path(), a_complete_submit(&["n1"]));

    // Then — an undocumented node is the duplicate-development hazard itself
    assert_refused(result).naming("n2");
}

#[test]
fn a_submit_omitting_a_planned_node_writes_nothing() {
    // Given
    let session = an_orchestrator_with_stack(&["n1", "n2"]);

    // When
    let _ = write_stack_docs(session.path(), a_complete_submit(&["n1"]));

    // Then
    session.path().assert_no_documents_at_all();
}

// ── The four required headings ───────────────────────────────────────────────────────────────

#[test]
fn a_changeset_without_a_draft_pr_contract_heading_is_refused() {
    // Given — a changeset missing the section dependents rely on to start early
    let session = an_orchestrator_with_stack(&["n1"]);
    let without =
        a_changeset_body_with_headings(&["## Responsibility", "## Boundaries", "## Dependencies"]);
    let submit = a_docs_submit_with(&[("n1", "# n1 — PRD\nWhat this delivers.\n", &without)]);

    // When
    let result = write_stack_docs(session.path(), submit);

    // Then
    assert_refused(result).naming("Draft PR contract");
}

#[test]
fn a_changeset_without_a_dependencies_heading_is_refused() {
    // Given — a changeset that never says what its parents deliver
    let session = an_orchestrator_with_stack(&["n1"]);
    let without = a_changeset_body_with_headings(&[
        "## Responsibility",
        "## Boundaries",
        "## Draft PR contract",
    ]);
    let submit = a_docs_submit_with(&[("n1", "# n1 — PRD\nWhat this delivers.\n", &without)]);

    // When
    let result = write_stack_docs(session.path(), submit);

    // Then
    assert_refused(result).naming("Dependencies");
}

#[test]
fn a_changeset_without_a_boundaries_heading_is_refused() {
    // Given
    let session = an_orchestrator_with_stack(&["n1"]);
    let without = a_changeset_body_with_headings(&[
        "## Responsibility",
        "## Dependencies",
        "## Draft PR contract",
    ]);
    let submit = a_docs_submit_with(&[("n1", "# n1 — PRD\nWhat this delivers.\n", &without)]);

    // When
    let result = write_stack_docs(session.path(), submit);

    // Then
    assert_refused(result).naming("Boundaries");
}

#[test]
fn a_changeset_without_a_responsibility_heading_is_refused() {
    // Given
    let session = an_orchestrator_with_stack(&["n1"]);
    let without = a_changeset_body_with_headings(&[
        "## Boundaries",
        "## Dependencies",
        "## Draft PR contract",
    ]);
    let submit = a_docs_submit_with(&[("n1", "# n1 — PRD\nWhat this delivers.\n", &without)]);

    // When
    let result = write_stack_docs(session.path(), submit);

    // Then
    assert_refused(result).naming("Responsibility");
}

#[test]
fn a_heading_mentioned_only_in_prose_does_not_satisfy_the_check() {
    // Given — a changeset that talks about boundaries without carrying the section
    let session = an_orchestrator_with_stack(&["n1"]);
    let prose = "# Changeset\n\n\
         ## Responsibility\nOwns the store.\n\n\
         See the ## Boundaries discussion in the epic for why this is scoped so.\n\n\
         ## Dependencies\nNone.\n\n\
         ## Draft PR contract\nAPI plus failing tests.\n";
    let submit = a_docs_submit_with(&[("n1", "# n1 — PRD\nWhat this delivers.\n", prose)]);

    // When
    let result = write_stack_docs(session.path(), submit);

    // Then — a heading is a line, not a substring
    assert_refused(result).naming("Boundaries");
}

// ── Blank bodies ─────────────────────────────────────────────────────────────────────────────

#[test]
fn a_blank_prd_is_refused() {
    // Given — an empty document reads as "no boundaries", not "not written yet"
    let session = an_orchestrator_with_stack(&["n1"]);
    let submit =
        a_docs_submit_with(&[("n1", "   ", &a_changeset_body_with_headings(ALL_HEADINGS))]);

    // When
    let result = write_stack_docs(session.path(), submit);

    // Then
    assert_refused(result).naming("n1");
}

// ── A rejected submit never disturbs what is already there ──────────────────────────────────

#[test]
fn a_rejected_submit_leaves_the_previously_written_documents_untouched() {
    // Given — a stack documented once, then re-planned to add a node
    let session = an_orchestrator_with_stack(&["n1"]);
    write_stack_docs(session.path(), a_complete_submit(&["n1"])).expect("first pass");

    // When — a later pass is refused for omitting a node
    let two_node = an_orchestrator_with_stack(&["n1", "n2"]);
    fs::rename(
        session.path().join("artifacts"),
        two_node.path().join("artifacts"),
    )
    .expect("carry the first pass over to the re-planned stack");
    let _ = write_stack_docs(two_node.path(), a_complete_submit(&["n1"]));

    // Then — the good documents from the previous pass survive
    two_node
        .path()
        .assert_changeset_for("n1", "## Draft PR contract");
}
