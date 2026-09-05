//! Acceptance: reading and searching pull requests from the orchestrator.
//!
//! Until now the orchestrator's only read was `pr_stack_status` — a per-node roll-up of phase plus
//! internal status, which also writes derived statuses back to the changeset. There was no way to
//! read one PR's body, its review state, its checks or its changed files, and no way to find a PR the
//! agent held no reference to. These three reads close that.
//!
//! Two limitations are pinned here as behaviour rather than papered over: a search hit carries no
//! head or base branch (`GET /search/issues` does not report them), and a thread carries no resolved
//! flag (REST does not expose one — it is GraphQL-only).
//!
//! PRD: docs/ft/coder/pr-stacking.md § Full control over the plan.
//! Changeset: docs/dev/changesets/2026-07-30-pr-stack-full-control.md.

mod common;

use common::{
    a_check_run, a_conversation_comment, a_merged_node, a_planned_node, a_pr, a_reply_to, a_review,
    a_review_comment, a_search_hit, an_insight_github, an_open_node, assert_rejected, stack_of,
    write_stack, written_at, REPO,
};
use tddy_workflow_recipes::orchestrate_pr_stack::github::{PrFile, PrSearchQuery, PrState};
use tddy_workflow_recipes::orchestrate_pr_stack::pr_insight::{
    pull_number_for_node, read_pr, read_pr_comments, search_repository_prs, PrSearchInput,
    PrThreadComment, ReviewerState,
};

const BRANCH_N1: &str = "feature/stack/n1";
const BASE: &str = "master";
const PR: u64 = 42;

fn a_search_for(text: &str, limit: u32) -> PrSearchInput {
    PrSearchInput {
        text: Some(text.to_string()),
        state: "open".to_string(),
        author: None,
        base: None,
        limit,
    }
}

/// One comment as a thread reports it — the anchor (path, line, hunk) lives on the thread, so a
/// thread's comments are compared on author, body and time alone.
fn a_thread_comment(author: &str, body: &str, created_at: &str) -> PrThreadComment {
    PrThreadComment {
        author: author.to_string(),
        body: body.to_string(),
        created_at: created_at.to_string(),
    }
}

// --- pr_read ----------------------------------------------------------------

#[test]
fn reading_a_pr_returns_its_body_state_base_and_head() {
    // Given — one open PR, nothing else seeded
    let gh = an_insight_github().with_pr(a_pr(PR, BRANCH_N1, BASE));

    // When
    let view = read_pr(&gh, PR, false).expect("reading a PR the fake holds should succeed");

    // Then
    assert_eq!(view.number, PR);
    assert_eq!(view.title, format!("PR {PR}"));
    assert_eq!(view.body, format!("body of PR {PR}"));
    assert_eq!(view.state, PrState::Open);
    assert_eq!(view.base_branch, BASE);
    assert_eq!(view.head_branch, BRANCH_N1);
    assert_eq!(view.head_sha, format!("sha-{PR}"));
    assert_eq!(view.mergeable, Some(true));
    assert_eq!(view.mergeable_state, "clean");
    assert_eq!(view.additions, 10);
    assert_eq!(view.deletions, 2);
    assert_eq!(view.changed_files, 3);
}

#[test]
fn reading_a_pr_reports_one_latest_review_state_per_reviewer() {
    // Given — alice reviewed twice, and her later verdict is the one that stands. Her reviews are
    // seeded newest-first: GitHub promises no order, so only comparing `submitted_at` can produce the
    // expected rows — "whichever came last in the list" would report her earlier COMMENTED.
    let gh = an_insight_github()
        .with_pr(a_pr(PR, BRANCH_N1, BASE))
        .with_reviews(
            PR,
            vec![
                a_review("alice", "CHANGES_REQUESTED", "2026-07-30T11:00:00Z"),
                a_review("bob", "APPROVED", "2026-07-30T10:00:00Z"),
                a_review("alice", "COMMENTED", "2026-07-30T09:00:00Z"),
            ],
        );

    // When
    let view = read_pr(&gh, PR, false).expect("reading a PR should succeed");

    // Then — one row per reviewer, ordered by author so the output is stable
    assert_eq!(
        view.reviews,
        vec![
            ReviewerState {
                author: "alice".to_string(),
                state: "CHANGES_REQUESTED".to_string()
            },
            ReviewerState {
                author: "bob".to_string(),
                state: "APPROVED".to_string()
            },
        ]
    );
}

#[test]
fn two_reviews_submitted_at_the_same_instant_report_the_one_listed_last() {
    // Given — alice's two reviews carry the same `submitted_at`, which GitHub does allow, so the
    // timestamps cannot decide between them
    let gh = an_insight_github()
        .with_pr(a_pr(PR, BRANCH_N1, BASE))
        .with_reviews(
            PR,
            vec![
                a_review("alice", "CHANGES_REQUESTED", "2026-07-30T11:00:00Z"),
                a_review("alice", "APPROVED", "2026-07-30T11:00:00Z"),
            ],
        );

    // When
    let view = read_pr(&gh, PR, false).expect("reading a PR should succeed");

    // Then — the later entry in the list stands. A deliberate choice, not an accident of the fold:
    // with nothing left to order the two by, GitHub's own list order is the only signal there is.
    assert_eq!(
        view.reviews,
        vec![ReviewerState {
            author: "alice".to_string(),
            state: "APPROVED".to_string()
        }]
    );
}

#[test]
fn reading_a_pr_summarises_each_check_run_on_its_head_commit() {
    // Given — three check runs against the PR's head sha
    let gh = an_insight_github()
        .with_pr(a_pr(PR, BRANCH_N1, BASE))
        .with_check_runs(
            &format!("sha-{PR}"),
            vec![
                a_check_run("build", "success"),
                a_check_run("test", "failure"),
                a_check_run("lint", ""),
            ],
        );

    // When
    let view = read_pr(&gh, PR, false).expect("reading a PR should succeed");

    // Then — reported in the order GitHub gave them, an in-progress run keeping its empty conclusion
    assert_eq!(
        view.checks,
        vec![
            a_check_run("build", "success"),
            a_check_run("test", "failure"),
            a_check_run("lint", ""),
        ]
    );
}

#[test]
fn changed_files_are_omitted_and_never_fetched_unless_they_are_requested() {
    // Given — a PR whose file list the fake could serve
    let gh = an_insight_github()
        .with_pr(a_pr(PR, BRANCH_N1, BASE))
        .with_files(
            PR,
            vec![PrFile {
                path: "src/lib.rs".to_string(),
                status: "modified".to_string(),
            }],
        );

    // When — the caller does not ask for files
    let view = read_pr(&gh, PR, false).expect("reading a PR should succeed");

    // Then — none are reported, and the extra request was never made
    assert_eq!(view.files, None);
    assert_eq!(gh.files_requested_for(), Vec::<u64>::new());
}

#[test]
fn changed_files_are_returned_with_a_path_and_a_status_when_requested() {
    // Given
    let gh = an_insight_github()
        .with_pr(a_pr(PR, BRANCH_N1, BASE))
        .with_files(
            PR,
            vec![
                PrFile {
                    path: "src/lib.rs".to_string(),
                    status: "modified".to_string(),
                },
                PrFile {
                    path: "src/new.rs".to_string(),
                    status: "added".to_string(),
                },
            ],
        );

    // When
    let view = read_pr(&gh, PR, true).expect("reading a PR should succeed");

    // Then
    assert_eq!(
        view.files,
        Some(vec![
            PrFile {
                path: "src/lib.rs".to_string(),
                status: "modified".to_string()
            },
            PrFile {
                path: "src/new.rs".to_string(),
                status: "added".to_string()
            },
        ])
    );
    assert_eq!(gh.files_requested_for(), vec![PR]);
}

// --- addressing a PR by node id ---------------------------------------------

#[test]
fn a_node_id_resolves_to_the_pull_number_recorded_in_its_pr_status_url() {
    // Given — n1 records PR #1234, so a number derived from anything else would show
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(dir, vec![an_open_node("n1", BRANCH_N1, 1234, &[])]);

    // When
    let number = pull_number_for_node(&stack_of(dir), "n1");

    // Then
    assert_eq!(number, Ok(1234));
}

#[test]
fn a_node_that_records_no_pr_url_cannot_be_addressed_and_says_so() {
    // Given — n1 was never started, so no PR url was ever recorded on it
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(dir, vec![a_planned_node("n1", &[])]);

    // When
    let result = pull_number_for_node(&stack_of(dir), "n1");

    // Then — an explicit refusal, never a guessed number
    assert_rejected(result).with_reason_containing("n1");
}

#[test]
fn a_node_whose_recorded_pr_status_carries_no_url_cannot_be_addressed_and_says_so() {
    // Given — n1's PR is recorded as merged, but no url was ever recorded with it: a pr_status is
    // present, and it still names no pull request
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(dir, vec![a_merged_node("n1", BRANCH_N1, &[])]);

    // When
    let result = pull_number_for_node(&stack_of(dir), "n1");

    // Then — as unaddressable as no pr_status at all, and refused the same way
    assert_rejected(result).with_reason_containing("records no pull request url");
}

#[test]
fn an_unknown_node_id_cannot_be_addressed_and_says_so() {
    // Given
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(dir, vec![an_open_node("n1", BRANCH_N1, PR, &[])]);

    // When
    let result = pull_number_for_node(&stack_of(dir), "n9");

    // Then
    assert_rejected(result).with_reason_containing("n9");
}

// --- pr_search --------------------------------------------------------------

#[test]
fn a_search_asks_github_for_the_callers_repository_and_the_agents_own_text_state_and_limit() {
    // Given — a fake that records the query it is handed
    let gh = an_insight_github();

    // When — the agent supplies only free text; the repository is the caller's to set
    search_repository_prs(&gh, REPO, a_search_for("token store", 20))
        .expect("searching should succeed");

    // Then — exactly one search, carrying every field the caller and the agent named. What keeps a
    // search inside this repository is `search_qualifiers`/`scoped_value`, tested where they live; this
    // is the propagation those rely on.
    assert_eq!(
        gh.searched(),
        vec![PrSearchQuery {
            repo: REPO.to_string(),
            text: Some("token store".to_string()),
            state: "open".to_string(),
            author: None,
            base: None,
            limit: 20,
        }]
    );
}

#[test]
fn a_search_returns_at_most_the_requested_number_of_hits() {
    // Given — four matches available but a limit of two
    let gh = an_insight_github().with_search_hits(vec![
        a_search_hit(1, "Add the token store"),
        a_search_hit(2, "Split out the parser"),
        a_search_hit(3, "Rotate the signing key"),
        a_search_hit(4, "Drop the legacy shim"),
    ]);

    // When
    let hits = search_repository_prs(&gh, REPO, a_search_for("anything", 2))
        .expect("searching should succeed");

    // Then — the cap is honoured on the way out, not merely requested on the way in
    assert_eq!(
        hits,
        vec![
            a_search_hit(1, "Add the token store"),
            a_search_hit(2, "Split out the parser"),
        ],
        "a limit must bound what the agent is handed"
    );
}

// --- pr_comments ------------------------------------------------------------

#[test]
fn review_comments_are_grouped_into_one_thread_per_root_comment() {
    // Given — two threads on one file plus a third on another, interleaved as GitHub returns them
    let root_a = a_review_comment(1, "src/a.rs", 10, "why here?");
    let root_b = a_review_comment(3, "src/b.rs", 20, "typo");
    let root_c = a_review_comment(5, "src/a.rs", 99, "nit");
    let gh = an_insight_github()
        .with_pr(a_pr(PR, BRANCH_N1, BASE))
        .with_review_comments(
            PR,
            vec![
                root_a.clone(),
                a_reply_to(2, &root_a, "because of the lock"),
                root_b.clone(),
                a_reply_to(4, &root_a, "agreed, moving it"),
                root_c.clone(),
            ],
        );

    // When
    let view = read_pr_comments(&gh, PR).expect("reading comments should succeed");

    // Then — one thread per root, in root order, each replying under the root it answers rather than
    // starting a thread of its own
    assert_eq!(view.threads.len(), 3);
    assert_eq!(view.threads[0].path, "src/a.rs");
    assert_eq!(view.threads[0].line, Some(10));
    assert_eq!(
        view.threads[0].comments,
        vec![
            a_thread_comment("author-1", "why here?", "2026-07-30T00:00:01Z"),
            a_thread_comment("author-2", "because of the lock", "2026-07-30T00:00:02Z"),
            a_thread_comment("author-4", "agreed, moving it", "2026-07-30T00:00:04Z"),
        ]
    );
    assert_eq!(view.threads[1].path, "src/b.rs");
    assert_eq!(view.threads[1].line, Some(20));
    assert_eq!(
        view.threads[1].comments,
        vec![a_thread_comment("author-3", "typo", "2026-07-30T00:00:03Z")]
    );
    assert_eq!(view.threads[2].path, "src/a.rs");
    assert_eq!(view.threads[2].line, Some(99));
    assert_eq!(
        view.threads[2].comments,
        vec![a_thread_comment("author-5", "nit", "2026-07-30T00:00:05Z")]
    );
}

#[test]
fn a_threads_replies_are_ordered_by_when_they_were_written_not_by_their_id() {
    // Given — one thread whose earlier-id reply was written *after* the later-id one. A comment id
    // orders nothing in time (ids are handed out across the whole repository, and a review's pending
    // comments are all created when it is submitted), so id order and reply order really do differ.
    let root = a_review_comment(1, "src/a.rs", 10, "why here?");
    let gh = an_insight_github()
        .with_pr(a_pr(PR, BRANCH_N1, BASE))
        .with_review_comments(
            PR,
            vec![
                root.clone(),
                written_at(
                    a_reply_to(2, &root, "because of the lock"),
                    "2026-07-30T00:00:09Z",
                ),
                a_reply_to(4, &root, "agreed, moving it"),
            ],
        );

    // When
    let view = read_pr_comments(&gh, PR).expect("reading comments should succeed");

    // Then — the reply written first is read first, so the thread reads as the conversation happened
    assert_eq!(view.threads.len(), 1);
    assert_eq!(
        view.threads[0].comments,
        vec![
            a_thread_comment("author-1", "why here?", "2026-07-30T00:00:01Z"),
            a_thread_comment("author-4", "agreed, moving it", "2026-07-30T00:00:04Z"),
            a_thread_comment("author-2", "because of the lock", "2026-07-30T00:00:09Z"),
        ]
    );
}

#[test]
fn reviews_and_conversation_comments_are_returned_as_separate_sections() {
    // Given — a PR with submitted reviews and issue-level comments, and no diff-anchored comments
    let gh = an_insight_github()
        .with_pr(a_pr(PR, BRANCH_N1, BASE))
        .with_reviews(
            PR,
            vec![
                a_review("alice", "CHANGES_REQUESTED", "2026-07-30T11:00:00Z"),
                a_review("bob", "APPROVED", "2026-07-30T12:00:00Z"),
            ],
        )
        .with_conversation(
            PR,
            vec![
                a_conversation_comment("carol", "rebase needed", "2026-07-30T13:00:00Z"),
                a_conversation_comment("dave", "ping", "2026-07-30T14:00:00Z"),
            ],
        );

    // When
    let view = read_pr_comments(&gh, PR).expect("reading comments should succeed");

    // Then — a review verdict is never mixed into the conversation, and neither becomes a thread
    assert_eq!(
        view.reviews,
        vec![
            a_review("alice", "CHANGES_REQUESTED", "2026-07-30T11:00:00Z"),
            a_review("bob", "APPROVED", "2026-07-30T12:00:00Z"),
        ]
    );
    assert_eq!(
        view.conversation,
        vec![
            a_conversation_comment("carol", "rebase needed", "2026-07-30T13:00:00Z"),
            a_conversation_comment("dave", "ping", "2026-07-30T14:00:00Z"),
        ]
    );
    assert_eq!(view.threads.len(), 0);
}

#[test]
fn reading_the_comments_of_a_pull_request_that_does_not_exist_fails_rather_than_reporting_none() {
    // Given — a repository that holds no PR #42 at all
    let gh = an_insight_github();

    // When
    let result = read_pr_comments(&gh, PR);

    // Then — GitHub answers an unknown number with a 404, not with three empty lists, and "this PR has
    // no feedback" is a different answer from "there is no such PR"
    assert_eq!(
        result.map_err(|e| e.to_string()),
        Err("artifact write failed: fake holds no PR #42".to_string())
    );
}
