//! Existing pull requests as stack nodes: adoption, and pushing a node's edits back to its PR.

use std::path::Path;

use tddy_core::changeset::StackNode;

use super::{
    append_node_atomic, next_free_node_id, read_stack, reject_if_cyclic, validate_parents,
};

/// The facts about an existing pull request that a stack node is built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdoptedPrFacts {
    pub pull_number: u64,
    pub title: String,
    pub body: String,
    pub head_branch: String,
    pub url: String,
    /// `StackNode.pr_status.phase` vocabulary: `open` / `merged` / `closed`.
    pub phase: String,
}

/// Push a node's title and/or body to its pull request, returning the PR number written to.
///
/// Takes the values explicitly rather than re-reading the node, so only the fields the operator
/// actually edited are sent — a `PATCH` that restates an unchanged body would overwrite whatever
/// was edited on GitHub in the meantime.
///
/// A node that records no PR is a rejection, not a skip: the caller asked for something that cannot
/// happen, and quietly doing nothing would be a fallback.
pub fn sync_node_to_github_pr(
    session_dir: &Path,
    node_id: &str,
    title: Option<&str>,
    body: Option<&str>,
    gh: &dyn tddy_github::pr_api::GithubPrInsightApi,
) -> Result<u64, String> {
    let stack = tddy_core::changeset::read_changeset(session_dir)
        .map_err(|e| format!("sync_node_to_github_pr: failed to read changeset: {e}"))?
        .stack
        .unwrap_or_default();
    // Resolving the number is also the "does this node have a PR at all" check: an unknown node and a
    // node that records no PR url both fail here, naming the node, before anything is sent.
    let number = crate::pr_insight::pull_number_for_node(&stack, node_id)
        .map_err(|e| format!("sync_node_to_github_pr: {e}"))?;

    gh.patch_pr_title_body(number, title, body).map_err(|e| {
        format!("sync_node_to_github_pr: patching PR #{number} for node '{node_id}' failed: {e}")
    })?;
    Ok(number)
}

/// Append a stack node built from an existing pull request's facts.
///
/// Pure: the PR has already been read, so the DAG rules are testable without GitHub. Parents are
/// validated exactly as [`add_planned_pr_node`] validates them, and a PR already reachable through
/// some node — by its head branch or by the pull number recorded in its `pr_status.url` — is a
/// rejection: a PR must not be adopted twice.
///
/// The new node owns a `branch` and a `pr_status` from the start, but no `session_id`: an adopted PR
/// has a branch and a pull request, and no child session in this orchestrator. `internal_status` is
/// left unset for `pr_stack_status` to derive.
///
/// [`add_planned_pr_node`]: super::add_planned_pr_node
pub fn adopt_pr_as_stack_node(
    session_dir: &Path,
    facts: AdoptedPrFacts,
    parents: Vec<String>,
) -> Result<StackNode, String> {
    use tddy_core::changeset::GithubPrStatus;
    const OP: &str = "adopt_pr_as_stack_node";

    let existing = read_stack(session_dir, OP)?;

    validate_parents(&existing, None, &parents, OP)?;
    reject_if_branch_bound(&existing, &facts)?;
    reject_if_pull_number_tracked(&existing, &facts)?;

    let new_node = StackNode {
        node_id: next_free_node_id(&existing),
        title: facts.title,
        description: facts.body,
        // The branch is real from the start — the PR is already built on it. There is no suggestion
        // to record, since nothing here is going to choose a name.
        branch: Some(facts.head_branch),
        branch_suggestion: None,
        // An adopted PR has no child session in *this* orchestrator, and `internal_status` is
        // `pr_stack_status`'s to derive from the live PR.
        session_id: None,
        parents,
        pr_status: Some(GithubPrStatus {
            phase: facts.phase,
            // The url is how every existing caller recovers the node's PR number.
            url: Some(facts.url),
            error: None,
        }),
        child_state: None,
        internal_status: None,
        // Chosen by `append_node_atomic` below, against the stack that is actually about to be
        // written.
        display_order: None,
    };

    let mut candidate_nodes = existing.nodes.clone();
    candidate_nodes.push(new_node.clone());
    reject_if_cyclic(
        &tddy_core::changeset::Stack {
            version: existing.version,
            nodes: candidate_nodes,
        },
        OP,
    )?;

    append_node_atomic(session_dir, new_node, OP)
}

/// Refuse to adopt a PR whose head branch some node already owns — that node already *is* this PR in
/// the stack, and a second one would have `pr_merge` and `pr_stack_status` act on it twice.
fn reject_if_branch_bound(
    stack: &tddy_core::changeset::Stack,
    facts: &AdoptedPrFacts,
) -> Result<(), String> {
    const OP: &str = "adopt_pr_as_stack_node";
    let Some(bound) = stack
        .nodes
        .iter()
        .find(|n| n.branch.as_deref() == Some(facts.head_branch.as_str()))
    else {
        return Ok(());
    };
    Err(format!(
        "{OP}: branch '{}' is already bound to node '{}', so PR #{} is already tracked by this \
         stack",
        facts.head_branch, bound.node_id, facts.pull_number
    ))
}

/// Refuse to adopt a PR some node already records by number.
///
/// The recorded url is the system's only statement of "which pull request is this node" (see
/// `pr_number_from_status_url`), and a node can record one without ever recording a branch — a node
/// whose child session never ran, or one adopted before this check existed. Checking the branch alone
/// would let PR #42 be reached through two nodes, and `pr_merge` / `pr_stack_status` would then act on
/// it twice.
fn reject_if_pull_number_tracked(
    stack: &tddy_core::changeset::Stack,
    facts: &AdoptedPrFacts,
) -> Result<(), String> {
    const OP: &str = "adopt_pr_as_stack_node";
    let Some(tracking) = stack.nodes.iter().find(|n| {
        crate::pr_insight::pr_number_from_status_url(n.pr_status.as_ref())
            == Some(facts.pull_number)
    }) else {
        return Ok(());
    };
    Err(format!(
        "{OP}: PR #{} is already recorded on node '{}', so it is already tracked by this stack",
        facts.pull_number, tracking.node_id
    ))
}

/// Read a pull request and adopt it as a stack node — [`adopt_pr_as_stack_node`] with the fetch in
/// front of it.
///
/// Maps GitHub's live state onto `pr_status.phase`'s vocabulary. A **draft** is recorded as `open`:
/// the phase says whether the PR is still in play, and every consumer of `phase` (merge readiness,
/// delete's refusal, internal-status derivation) treats a draft as an open PR.
pub fn adopt_pr_into_stack(
    session_dir: &Path,
    pull_number: u64,
    parents: Vec<String>,
    gh: &dyn tddy_github::pr_api::GithubPrInsightApi,
) -> Result<StackNode, String> {
    use tddy_github::pr_api::PrState;

    let pr = gh
        .get_pr(pull_number)
        .map_err(|e| format!("adopt_pr_into_stack: reading PR #{pull_number} failed: {e}"))?;
    let phase = match pr.state {
        PrState::Open | PrState::Draft => "open",
        PrState::Merged => "merged",
        PrState::Closed => "closed",
    };

    adopt_pr_as_stack_node(
        session_dir,
        AdoptedPrFacts {
            pull_number,
            title: pr.title,
            body: pr.body,
            head_branch: pr.head_branch,
            url: pr.url,
            phase: phase.to_string(),
        },
        parents,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stack_ops::test_support::{
        a_node, a_started_node, assert_rejected, node_ids, stack_on_disk, write_stack,
    };
    use tddy_core::changeset::{GithubPrStatus, StackNode};

    // -----------------------------------------------------------------------
    // adopt_pr_as_stack_node
    // -----------------------------------------------------------------------

    fn the_facts_of_pr(pull_number: u64, head_branch: &str) -> AdoptedPrFacts {
        AdoptedPrFacts {
            pull_number,
            title: format!("PR {pull_number}"),
            body: format!("body of PR {pull_number}"),
            head_branch: head_branch.to_string(),
            url: format!("https://github.com/acme/repo/pull/{pull_number}"),
            phase: "open".to_string(),
        }
    }

    #[test]
    fn adopting_a_pr_creates_a_node_carrying_its_head_branch_title_body_and_url() {
        // Given — an empty plan
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(dir, vec![]);

        // When
        let node = adopt_pr_as_stack_node(dir, the_facts_of_pr(77, "feature/elsewhere"), vec![])
            .expect("adopting a PR should succeed");

        // Then
        assert_eq!(node.node_id, "n1");
        assert_eq!(node.title, "PR 77");
        assert_eq!(node.description, "body of PR 77");
        assert_eq!(node.branch.as_deref(), Some("feature/elsewhere"));
        let status = node.pr_status.as_ref().unwrap();
        assert_eq!(status.phase, "open");
        assert_eq!(
            status.url.as_deref(),
            Some("https://github.com/acme/repo/pull/77")
        );
        assert_eq!(stack_on_disk(dir).node("n1").unwrap().branch, node.branch);
    }

    #[test]
    fn an_adopted_node_starts_with_no_child_session_and_no_internal_status() {
        // Given
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(dir, vec![]);

        // When
        adopt_pr_as_stack_node(dir, the_facts_of_pr(77, "feature/elsewhere"), vec![])
            .expect("adopting should succeed");

        // Then — as persisted: an adopted PR has a branch and a PR, and no child session in this
        // orchestrator. Read from disk rather than from the returned value, which a node built but
        // never written would satisfy just as well.
        let adopted = stack_on_disk(dir).node("n1").unwrap().clone();
        assert_eq!(adopted.session_id, None);
        assert_eq!(adopted.internal_status, None);
        assert_eq!(adopted.branch_suggestion, None);
    }

    #[test]
    fn adopting_a_pr_whose_head_branch_is_already_bound_to_a_node_is_rejected() {
        // Given — n1 already owns the branch this PR is built on
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(
            dir,
            vec![a_started_node("n1", "feature/elsewhere", "open", vec![])],
        );

        // When
        let result = adopt_pr_as_stack_node(dir, the_facts_of_pr(77, "feature/elsewhere"), vec![]);

        // Then — a PR must not be tracked twice
        assert_rejected(result).with_reason_containing("feature/elsewhere");
        assert_eq!(node_ids(dir), vec!["n1"]);
    }

    #[test]
    fn adopting_a_pr_a_branchless_node_already_records_the_url_of_is_rejected() {
        // Given — n1 records PR #77 in its pr_status url but never recorded a branch of its own
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(
            dir,
            vec![StackNode {
                pr_status: Some(GithubPrStatus {
                    phase: "open".to_string(),
                    url: Some("https://github.com/acme/repo/pull/77".to_string()),
                    error: None,
                }),
                ..a_node("n1", "one", vec![])
            }],
        );

        // When — the same pull request is adopted through a second node
        let result = adopt_pr_as_stack_node(dir, the_facts_of_pr(77, "feature/elsewhere"), vec![]);

        // Then — two nodes resolving to PR #77 would make pr_merge and pr_stack_status act on it twice
        assert_rejected(result).with_reason_containing("n1");
        assert_eq!(node_ids(dir), vec!["n1"]);
    }

    #[test]
    fn adopting_a_pr_with_a_dangling_parent_ref_is_rejected_and_the_stack_on_disk_is_unchanged() {
        // Given — a plan with no node called n9
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(dir, vec![a_node("n1", "one", vec![])]);

        // When
        let result = adopt_pr_as_stack_node(
            dir,
            the_facts_of_pr(77, "feature/elsewhere"),
            vec!["n9".to_string()],
        );

        // Then
        assert_rejected(result).with_reason_containing("n9");
        assert_eq!(node_ids(dir), vec!["n1"]);
    }
}
