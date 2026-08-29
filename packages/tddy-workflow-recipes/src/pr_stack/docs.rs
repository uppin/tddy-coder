//! Per-PR documents: the payload the `write-stack-docs` goal submits, its validator, and where the
//! documents live on disk.
//!
//! PRD: `docs/ft/coder/pr-stack-docs.md`.
//!
//! Each planned PR owns two documents under `artifacts/prs/<node_id>/`: a `PRD.md` saying what the
//! PR delivers, and a `changeset.md` saying where its edges are. The changeset is the one that stops
//! two children building the same abstraction, so its four sections are required and their presence
//! is checked here. Whether the boundaries they describe are *correct* is not checkable — that needs
//! the diff the node has yet to produce — and stays prompt-carried, exactly as the PR boundary
//! contract does.
//!
//! Paths are derived from `node_id` by convention rather than recorded on `StackNode`: the docs pass
//! writes every node at once, so they are predictable by construction.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tddy_core::changeset::Stack;

/// Subdirectory of `artifacts/` holding the per-PR document directories.
pub const NODE_DOCS_SUBDIR: &str = "prs";

/// The high-level document: what this PR delivers.
pub const NODE_PRD_BASENAME: &str = "PRD.md";

/// The technical document: responsibility, boundaries, dependencies, draft-PR contract.
pub const NODE_CHANGESET_BASENAME: &str = "changeset.md";

/// Sections every per-PR changeset must carry.
///
/// `Dependencies` is what prevents duplicate development — it names, per parent, what that PR
/// delivers that this one consumes. `Draft PR contract` is what stops a dependent waiting for a full
/// implementation: the API surface plus its failing tests, enough to open a draft PR against.
pub const REQUIRED_CHANGESET_HEADINGS: &[&str] = &[
    "## Responsibility",
    "## Boundaries",
    "## Dependencies",
    "## Draft PR contract",
];

/// Where a node's two documents live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeDocPaths {
    pub prd: PathBuf,
    pub changeset: PathBuf,
}

/// One node's submitted pair.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodeDocs {
    pub node_id: String,
    #[serde(default)]
    pub prd: String,
    #[serde(default)]
    pub changeset: String,
}

/// The `write-stack-docs` submit payload, mirroring `StackPlanOutput`.
///
/// The goal is registered (`goals.json` → `write-stack-docs.schema.json`), so `tddy-tools submit`
/// has already checked the shape — including the `goal` discriminator every registered payload
/// carries — before the hook sees it. Neither struct declares `goal`: it names the task the
/// submission is routed on, not part of the plan or the documents, and serde ignores it on the way
/// in exactly as it does for the plan.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StackDocsOutput {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub docs: Vec<NodeDocs>,
}

/// `session_dir/artifacts/prs/<node_id>/{PRD.md,changeset.md}` — pure path math, touches no
/// filesystem and enforces no policy.
pub fn node_doc_paths(session_dir: &Path, node_id: &str) -> NodeDocPaths {
    let node_dir = tddy_workflow::session_artifacts_root(session_dir)
        .join(NODE_DOCS_SUBDIR)
        .join(node_id);
    NodeDocPaths {
        prd: node_dir.join(NODE_PRD_BASENAME),
        changeset: node_dir.join(NODE_CHANGESET_BASENAME),
    }
}

/// Whether `body` carries `heading` as a heading rather than as prose.
///
/// A heading starts a line; a mention of `## Boundaries` inside a sentence is a reference to the
/// section, not the section. Trailing whitespace is tolerated because a markdown editor routinely
/// leaves it and rejecting it would be a formatting trap rather than a boundary check.
fn carries_heading(body: &str, heading: &str) -> bool {
    body.lines()
        .any(|line| line.trim_end().starts_with(heading))
}

/// Validate a submitted pass against the persisted stack. `Err` means nothing is written.
///
/// Every planned node must be covered and no unknown node named: a node with no boundaries document
/// is precisely the hazard the documents exist to prevent, so a partial pass is refused rather than
/// half-written.
pub fn validate_stack_docs(stack: &Stack, output: &StackDocsOutput) -> Result<(), String> {
    let mut seen: Vec<&str> = Vec::with_capacity(output.docs.len());
    for entry in &output.docs {
        let node_id = entry.node_id.as_str();
        if !stack.nodes.iter().any(|node| node.node_id == node_id) {
            return Err(format!(
                "node '{node_id}' is not in the stack — document only planned nodes"
            ));
        }
        if seen.contains(&node_id) {
            return Err(format!(
                "node '{node_id}' has more than one entry — submit exactly one document pair per node"
            ));
        }
        seen.push(node_id);

        if entry.prd.trim().is_empty() {
            return Err(format!(
                "node '{node_id}' has a blank PRD — an empty document reads as 'no boundaries', \
                 not 'not written yet'"
            ));
        }
        if entry.changeset.trim().is_empty() {
            return Err(format!(
                "node '{node_id}' has a blank changeset — an empty document reads as \
                 'no boundaries', not 'not written yet'"
            ));
        }
        for heading in REQUIRED_CHANGESET_HEADINGS {
            if !carries_heading(&entry.changeset, heading) {
                return Err(format!(
                    "the changeset for node '{node_id}' is missing the '{heading}' section"
                ));
            }
        }
    }

    for node in &stack.nodes {
        if !seen.contains(&node.node_id.as_str()) {
            return Err(format!(
                "node '{}' has no documents — every planned node must be documented before the \
                 stack is driven",
                node.node_id
            ));
        }
    }

    Ok(())
}

/// Persist a validated pass, replacing any previous one. Callers validate first.
pub fn write_stack_docs(session_dir: &Path, output: &StackDocsOutput) -> Result<(), String> {
    for entry in &output.docs {
        let paths = node_doc_paths(session_dir, &entry.node_id);
        // `write_atomic` creates the node directory and renames over any previous pass, so a
        // re-run replaces a document rather than appending to it.
        tddy_core::atomic_file::write_atomic(&paths.prd, &entry.prd)
            .map_err(|e| format!("write {}: {e}", paths.prd.display()))?;
        tddy_core::atomic_file::write_atomic(&paths.changeset, &entry.changeset)
            .map_err(|e| format!("write {}: {e}", paths.changeset.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use tddy_core::changeset::StackNode;

    // ── Builders ─────────────────────────────────────────────────────────────────────────────

    fn a_planned_node(node_id: &str) -> StackNode {
        StackNode {
            node_id: node_id.to_string(),
            title: format!("{node_id} title"),
            description: String::new(),
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

    fn a_stack_of(node_ids: &[&str]) -> Stack {
        Stack {
            version: 1,
            nodes: node_ids.iter().map(|id| a_planned_node(id)).collect(),
        }
    }

    fn a_changeset_with(headings: &[&str]) -> String {
        let mut body = String::from("# Changeset\n\n");
        for heading in headings {
            body.push_str(heading);
            body.push_str("\nContent.\n\n");
        }
        body
    }

    fn a_complete_changeset() -> String {
        a_changeset_with(REQUIRED_CHANGESET_HEADINGS)
    }

    fn docs_for(node_ids: &[&str]) -> StackDocsOutput {
        StackDocsOutput {
            version: 1,
            docs: node_ids
                .iter()
                .map(|id| NodeDocs {
                    node_id: id.to_string(),
                    prd: format!("# {id} — PRD\nWhat this delivers.\n"),
                    changeset: a_complete_changeset(),
                })
                .collect(),
        }
    }

    // ── Assertions ───────────────────────────────────────────────────────────────────────────

    fn assert_refused(result: Result<(), String>) -> String {
        match result {
            Err(message) => message,
            Ok(()) => panic!("expected the pass to be refused, but it was accepted"),
        }
    }

    fn assert_refusal_names(result: Result<(), String>, fragment: &str) {
        let message = assert_refused(result);
        assert!(
            message.contains(fragment),
            "the refusal must name '{fragment}' so the agent can fix it; message was '{message}'"
        );
    }

    // ── Paths ────────────────────────────────────────────────────────────────────────────────

    #[test]
    fn a_nodes_documents_resolve_under_its_own_directory() {
        // Given
        let session_dir = Path::new("/sessions/abc");

        // When
        let paths = node_doc_paths(session_dir, "n1");

        // Then
        assert_eq!(
            paths,
            NodeDocPaths {
                prd: PathBuf::from("/sessions/abc/artifacts/prs/n1/PRD.md"),
                changeset: PathBuf::from("/sessions/abc/artifacts/prs/n1/changeset.md"),
            }
        );
    }

    #[test]
    fn two_nodes_resolve_to_separate_directories() {
        // Given
        let session_dir = Path::new("/sessions/abc");

        // When
        let first = node_doc_paths(session_dir, "n1");
        let second = node_doc_paths(session_dir, "n2");

        // Then — a node reading its neighbour's boundaries as its own is the failure mode
        assert_ne!(first.changeset, second.changeset);
    }

    // ── A pass must cover the stack exactly ──────────────────────────────────────────────────

    #[test]
    fn a_complete_pass_over_a_two_node_stack_is_accepted() {
        // Given
        let stack = a_stack_of(&["n1", "n2"]);

        // When
        let result = validate_stack_docs(&stack, &docs_for(&["n1", "n2"]));

        // Then
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn a_pass_omitting_a_planned_node_names_it() {
        // Given
        let stack = a_stack_of(&["n1", "n2"]);

        // When
        let result = validate_stack_docs(&stack, &docs_for(&["n1"]));

        // Then
        assert_refusal_names(result, "n2");
    }

    #[test]
    fn a_pass_naming_an_unplanned_node_names_it() {
        // Given
        let stack = a_stack_of(&["n1"]);

        // When
        let result = validate_stack_docs(&stack, &docs_for(&["n1", "n7"]));

        // Then
        assert_refusal_names(result, "n7");
    }

    #[test]
    fn a_pass_naming_the_same_node_twice_is_refused() {
        // Given — two entries for one node leave it ambiguous which body wins
        let stack = a_stack_of(&["n1"]);
        let mut output = docs_for(&["n1"]);
        output.docs.push(output.docs[0].clone());

        // When
        let result = validate_stack_docs(&stack, &output);

        // Then
        assert_refusal_names(result, "n1");
    }

    #[test]
    fn an_empty_pass_over_an_empty_stack_is_accepted() {
        // Given — a stack with no nodes has nothing to document
        let stack = a_stack_of(&[]);

        // When
        let result = validate_stack_docs(&stack, &docs_for(&[]));

        // Then
        assert_eq!(result, Ok(()));
    }

    // ── The four required headings ───────────────────────────────────────────────────────────

    #[rstest]
    #[case::responsibility("## Responsibility")]
    #[case::boundaries("## Boundaries")]
    #[case::dependencies("## Dependencies")]
    #[case::draft_pr_contract("## Draft PR contract")]
    fn a_changeset_missing_a_required_heading_names_it(#[case] missing: &str) {
        // Given — a changeset carrying every required section but one
        let stack = a_stack_of(&["n1"]);
        let kept: Vec<&str> = REQUIRED_CHANGESET_HEADINGS
            .iter()
            .copied()
            .filter(|h| *h != missing)
            .collect();
        let output = StackDocsOutput {
            version: 1,
            docs: vec![NodeDocs {
                node_id: "n1".to_string(),
                prd: "# n1 — PRD\nWhat this delivers.\n".to_string(),
                changeset: a_changeset_with(&kept),
            }],
        };

        // When
        let result = validate_stack_docs(&stack, &output);

        // Then — the refusal names the section so the agent can add it
        assert_refusal_names(result, missing.trim_start_matches("## "));
    }

    #[test]
    fn a_heading_appearing_only_mid_sentence_does_not_satisfy_the_check() {
        // Given — prose mentioning a section is not the section
        let stack = a_stack_of(&["n1"]);
        let output = StackDocsOutput {
            version: 1,
            docs: vec![NodeDocs {
                node_id: "n1".to_string(),
                prd: "# n1 — PRD\nWhat this delivers.\n".to_string(),
                changeset: "# Changeset\n\n\
                     ## Responsibility\nOwns it.\n\n\
                     See the ## Boundaries note in the epic.\n\n\
                     ## Dependencies\nNone.\n\n\
                     ## Draft PR contract\nAPI plus failing tests.\n"
                    .to_string(),
            }],
        };

        // When
        let result = validate_stack_docs(&stack, &output);

        // Then
        assert_refusal_names(result, "Boundaries");
    }

    #[test]
    fn a_heading_with_trailing_whitespace_satisfies_the_check() {
        // Given — markdown editors routinely leave a trailing space on a heading line
        let stack = a_stack_of(&["n1"]);
        let output = StackDocsOutput {
            version: 1,
            docs: vec![NodeDocs {
                node_id: "n1".to_string(),
                prd: "# n1 — PRD\nWhat this delivers.\n".to_string(),
                changeset: "# Changeset\n\n\
                     ## Responsibility  \nOwns it.\n\n\
                     ## Boundaries\t\nNot the middleware.\n\n\
                     ## Dependencies \nNone.\n\n\
                     ## Draft PR contract  \nAPI plus failing tests.\n"
                    .to_string(),
            }],
        };

        // When
        let result = validate_stack_docs(&stack, &output);

        // Then — rejecting this would be a formatting trap, not a boundary check
        assert_eq!(result, Ok(()));
    }

    // ── Blank bodies ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn a_blank_prd_is_refused() {
        // Given — an empty document reads as "no boundaries", not "not written yet"
        let stack = a_stack_of(&["n1"]);
        let mut output = docs_for(&["n1"]);
        output.docs[0].prd = "   \n\t\n".to_string();

        // When
        let result = validate_stack_docs(&stack, &output);

        // Then
        assert_refusal_names(result, "n1");
    }

    #[test]
    fn a_blank_changeset_is_refused() {
        // Given
        let stack = a_stack_of(&["n1"]);
        let mut output = docs_for(&["n1"]);
        output.docs[0].changeset = String::new();

        // When
        let result = validate_stack_docs(&stack, &output);

        // Then
        assert_refusal_names(result, "n1");
    }
}
