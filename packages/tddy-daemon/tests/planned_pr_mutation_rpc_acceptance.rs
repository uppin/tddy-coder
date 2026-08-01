//! Acceptance: the daemon's two planned-PR mutation RPCs — `ReorderPlannedPr` and
//! `PullBaseIntoBranch`.
//!
//! Both rewrite something on the operator's behalf — a plan on disk, or a child session's branch and
//! the remote — so what they **refuse** is as much of the contract as what they do:
//!
//! - Neither touches a session that is not a pr-stack orchestrator. Without that guard any
//!   `session_id` belonging to the same user would have its `changeset.yaml` rewritten with a stack
//!   it never had, which is why the refusal is pinned for both legs rather than once.
//! - A missing `node_id`, a missing `base_branch` and a `direction` that is neither `up` nor `down`
//!   are `invalid_argument`: the caller sent something unusable, and guessing a node or a direction
//!   would move a row the operator never pointed at.
//! - A session with no recorded checkout is `failed_precondition` for the pull, not a degraded leg.
//!   `QueryBranch` can answer "unknown" for a repository it cannot find because it only displays; a
//!   mutation has nowhere to happen and must say so.
//!
//! And what they answer with: the plan — and the branch — **as they stand after the write**, so the
//! row repaints without waiting for the next five-second poll tick. The pull re-resolves the branch
//! *uncached* on purpose, because the two refs it compares are precisely the ones it just moved; a
//! resolution taken from the poll's cache would report the state the operator pressed the button to
//! change.
//!
//! Real git throughout — a bare `origin`, a clone, and a linked worktree for the node's branch —
//! because the pull fetches, merges and pushes, and the whole point of the returned resolution is
//! that it reads refs that moved.
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-08-01-pr-stack-panel-ux.md` § C3, C5.
//! Changeset: `docs/dev/1-WIP/CS-2026-08-01-pr-stack-panel-ux.md`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use tddy_core::changeset::{Changeset, Stack, StackNode};
use tddy_core::output::SESSIONS_SUBDIR;
use tddy_daemon::cli_session_manager::CliSessionManager;
use tddy_daemon::connection_service::{
    ConnectionServiceImpl, SessionUserResolver, SessionsBaseResolver,
};
use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::connection::{
    BranchResolution, ConnectionService as ConnectionServiceTrait, PullBaseIntoBranchRequest,
    PullBaseIntoBranchResponse, ReorderPlannedPrRequest, ReorderPlannedPrResponse,
};

const ORCHESTRATOR: &str = "orchestrator-1";
/// A child PR session of the same stack: a `tdd` recipe, not an orchestrator. The session a stale or
/// mistyped `session_id` most plausibly names.
const CHILD: &str = "child-1";
const TOKEN: &str = "valid-session-token";

const BASE_BRANCH: &str = "master";
const FIRST_NODE: &str = "n1";
const SECOND_NODE: &str = "n2";
const FIRST_BRANCH: &str = "feature/stack/n1";

const UP: &str = "up";
/// `node_id` and `base_branch` the caller left out — what a client bound to an unselected row sends.
const NO_NODE: &str = "";
const NO_BASE_BRANCH: &str = "";

// --- git plumbing -----------------------------------------------------------

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "could not run `git {}` in {}: {e}",
                args.join(" "),
                dir.display()
            )
        });
    assert!(
        out.status.success(),
        "`git {}` in {} failed: {}",
        args.join(" "),
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn identify(repo: &Path) {
    git(repo, &["config", "user.email", "test@test.com"]);
    git(repo, &["config", "user.name", "Test"]);
    git(repo, &["config", "commit.gpgsign", "false"]);
}

// --- the fixture ------------------------------------------------------------

/// A pr-stack orchestrator session over a real clone: a bare `origin`, a clone on `master`, a linked
/// worktree holding the first node's branch, and a plan of two nodes numbered 0 and 1.
struct Orchestrator {
    tmp: tempfile::TempDir,
}

fn a_pr_stack_orchestrator() -> Orchestrator {
    let tmp = tempfile::tempdir().unwrap();
    let repo = Orchestrator { tmp };

    fs::create_dir_all(repo.origin()).unwrap();
    git(
        &repo.origin(),
        &["init", "--quiet", "--bare", "-b", BASE_BRANCH],
    );

    let root = repo.repo_root();
    fs::create_dir_all(&root).unwrap();
    git(&root, &["init", "--quiet", "-b", BASE_BRANCH]);
    identify(&root);
    fs::write(root.join("README.md"), "the project\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "--quiet", "-m", "initial"]);
    git(&root, &["remote", "add", "origin", &repo.origin_url()]);
    git(&root, &["push", "--quiet", "-u", "origin", BASE_BRANCH]);

    fs::create_dir_all(repo.worktrees_dir()).unwrap();
    git(&root, &["branch", FIRST_BRANCH]);
    git(
        &root,
        &[
            "worktree",
            "add",
            "--quiet",
            repo.worktree().to_str().unwrap(),
            FIRST_BRANCH,
        ],
    );
    git(
        &repo.worktree(),
        &["push", "--quiet", "-u", "origin", FIRST_BRANCH],
    );

    repo.record_the_plan(Some(repo.repo_root()));
    repo
}

impl Orchestrator {
    // --- layout ---

    fn origin(&self) -> PathBuf {
        self.tmp.path().join("origin.git")
    }

    fn origin_url(&self) -> String {
        self.origin().to_string_lossy().into_owned()
    }

    fn repo_root(&self) -> PathBuf {
        self.tmp.path().join("clone")
    }

    fn worktrees_dir(&self) -> PathBuf {
        self.tmp.path().join("worktrees")
    }

    fn worktree(&self) -> PathBuf {
        self.worktrees_dir().join(FIRST_NODE)
    }

    fn sessions_base(&self) -> PathBuf {
        self.tmp.path().join("data")
    }

    fn session_dir(&self, session_id: &str) -> PathBuf {
        self.sessions_base().join(SESSIONS_SUBDIR).join(session_id)
    }

    // --- what the session directory records ---

    /// The orchestrator's `changeset.yaml`: the pr-stack recipe, the plan of two nodes, and — when
    /// given — the checkout the stack works on.
    fn record_the_plan(&self, repo_path: Option<PathBuf>) -> &Self {
        let dir = self.session_dir(ORCHESTRATOR);
        fs::create_dir_all(&dir).unwrap();
        tddy_core::write_changeset(
            &dir,
            &Changeset {
                recipe: Some("pr-stack".to_string()),
                repo_path: repo_path.map(|p| p.to_string_lossy().into_owned()),
                stack: Some(Stack {
                    version: 1,
                    nodes: vec![
                        a_started_node(FIRST_NODE, FIRST_BRANCH, 0),
                        a_planned_node(SECOND_NODE, 1),
                    ],
                }),
                ..Changeset::default()
            },
        )
        .unwrap();
        self
    }

    /// The same orchestrator with nothing in its session directory naming a checkout — a plan
    /// restored from elsewhere, or a session whose `repo_path` was never recorded.
    fn without_a_recorded_checkout(&self) -> &Self {
        self.record_the_plan(None)
    }

    /// A second session of the same user, running the `tdd` recipe — one of the stack's own child PR
    /// sessions, and not an orchestrator.
    fn with_a_child_session_that_is_not_an_orchestrator(&self) -> &Self {
        let dir = self.session_dir(CHILD);
        fs::create_dir_all(&dir).unwrap();
        tddy_core::write_changeset(
            &dir,
            &Changeset {
                recipe: Some("tdd".to_string()),
                repo_path: Some(self.repo_root().to_string_lossy().into_owned()),
                branch: Some(FIRST_BRANCH.to_string()),
                ..Changeset::default()
            },
        )
        .unwrap();
        self
    }

    // --- seeding the base ---

    /// A commit on the base branch, pushed — so the clone's `master` and `origin/master` both hold
    /// it and the node's branch is one commit behind.
    fn with_a_commit_on_the_base(&self, path: &str, contents: &str, subject: &str) -> &Self {
        let root = self.repo_root();
        fs::write(root.join(path), contents).unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "--quiet", "-m", subject]);
        git(&root, &["push", "--quiet", "origin", BASE_BRANCH]);
        self
    }

    // --- the service under test ---

    /// A service rooted at this fixture's data directory, holding no GitHub token store — so the PR
    /// leg of any resolution is unavailable and nothing ever reaches the network.
    fn service(&self) -> ConnectionServiceImpl {
        let config_dir = tempfile::tempdir().unwrap();
        let config_path = config_dir.path().join("config.yaml");
        fs::write(
            &config_path,
            "users:\n  - github_user: \"u\"\n    os_user: \"u\"\n",
        )
        .unwrap();
        let config = tddy_daemon::config::DaemonConfig::load(&config_path).unwrap();

        let sessions_base = self.sessions_base();
        let resolved = sessions_base.clone();
        let sessions_base_resolver: SessionsBaseResolver =
            Arc::new(move |_| Some(resolved.clone()));
        let user_resolver: SessionUserResolver =
            Arc::new(|token| (token == TOKEN).then(|| "u".to_string()));
        ConnectionServiceImpl::new(
            config,
            sessions_base_resolver,
            sessions_base,
            user_resolver,
            None,
            None,
            None,
            Arc::new(CliSessionManager::new()),
        )
    }

    // --- reading the result back ---

    fn worktree_head(&self) -> String {
        git(&self.worktree(), &["rev-parse", "HEAD"])
    }
}

/// A node that owns a branch and a child session, at a stated position in the reading order.
fn a_started_node(node_id: &str, branch: &str, display_order: u32) -> StackNode {
    StackNode {
        node_id: node_id.to_string(),
        title: format!("{node_id} title"),
        branch: Some(branch.to_string()),
        session_id: Some(format!("session-for-{node_id}")),
        display_order: Some(display_order),
        ..StackNode::default()
    }
}

/// A node that was never started: a suggested branch name, no branch and no session.
fn a_planned_node(node_id: &str, display_order: u32) -> StackNode {
    StackNode {
        node_id: node_id.to_string(),
        title: format!("{node_id} title"),
        branch_suggestion: Some(format!("feature/stack/{node_id}")),
        display_order: Some(display_order),
        ..StackNode::default()
    }
}

// --- calling the RPCs -------------------------------------------------------

async fn reorder(
    service: &ConnectionServiceImpl,
    session_id: &str,
    node_id: &str,
    direction: &str,
) -> Result<ReorderPlannedPrResponse, Status> {
    service
        .reorder_planned_pr(Request::new(ReorderPlannedPrRequest {
            session_token: TOKEN.to_string(),
            session_id: session_id.to_string(),
            node_id: node_id.to_string(),
            direction: direction.to_string(),
        }))
        .await
        .map(|response| response.into_inner())
}

/// Merge `base_branch` into `node_id`'s branch, refusing if the worktree is dirty — the defaults an
/// operator's one click sends.
async fn pull(
    service: &ConnectionServiceImpl,
    session_id: &str,
    node_id: &str,
    base_branch: &str,
) -> Result<PullBaseIntoBranchResponse, Status> {
    service
        .pull_base_into_branch(Request::new(PullBaseIntoBranchRequest {
            session_token: TOKEN.to_string(),
            session_id: session_id.to_string(),
            node_id: node_id.to_string(),
            base_branch: base_branch.to_string(),
            strategy: String::new(),
            dirty_worktree_action: String::new(),
            commit_message: String::new(),
        }))
        .await
        .map(|response| response.into_inner())
}

// --- reading the answers back -----------------------------------------------

/// The persisted position of each named node in a `stack_plan_json` payload, in the order named — so
/// an assertion states the whole reading order as one literal.
fn positions_in(stack_plan_json: &str, node_ids: &[&str]) -> Vec<Option<u32>> {
    let stack: Stack = serde_json::from_str(stack_plan_json).unwrap_or_else(|e| {
        panic!("stack_plan_json is not a Stack: {e} — was '{stack_plan_json}'")
    });
    node_ids
        .iter()
        .map(|id| {
            stack
                .nodes
                .iter()
                .find(|node| node.node_id == *id)
                .unwrap_or_else(|| panic!("the answered plan holds no node '{id}'"))
                .display_order
        })
        .collect()
}

fn resolution_of(response: &PullBaseIntoBranchResponse) -> &BranchResolution {
    response
        .resolution
        .as_ref()
        .expect("a pull must answer with the branch it moved")
}

// --- refusals ---------------------------------------------------------------

/// A refused RPC, so a test reads `assert_refused(result).with_code(…).naming(…)` instead of
/// unwrapping an `Err` by hand.
struct Refusal(Status);

fn assert_refused<T: std::fmt::Debug>(result: Result<T, Status>) -> Refusal {
    match result {
        Err(status) => Refusal(status),
        Ok(value) => panic!("expected the call to be refused, but it succeeded with {value:?}"),
    }
}

impl Refusal {
    fn with_code(self, expected: Code) -> Self {
        assert_eq!(
            self.0.code, expected,
            "wrong status code for refusal '{}'",
            self.0.message
        );
        self
    }

    fn naming(self, fragment: &str) -> Self {
        assert!(
            self.0.message.contains(fragment),
            "expected the refusal to mention '{fragment}', was '{}'",
            self.0.message
        );
        self
    }
}

// --- ReorderPlannedPr -------------------------------------------------------

#[tokio::test]
async fn reordering_a_session_that_is_not_a_pr_stack_orchestrator_is_refused() {
    // Given — a child PR session addressed as though it held the plan
    let repo = a_pr_stack_orchestrator();
    repo.with_a_child_session_that_is_not_an_orchestrator();

    // When
    let result = reorder(&repo.service(), CHILD, FIRST_NODE, UP).await;

    // Then — a session that owns no stack must not be given one by a reorder
    assert_refused(result)
        .with_code(Code::FailedPrecondition)
        .naming("pr-stack orchestrator");
}

#[tokio::test]
async fn reordering_without_naming_a_node_is_refused() {
    // Given — a client bound to a row it never resolved
    let repo = a_pr_stack_orchestrator();

    // When
    let result = reorder(&repo.service(), ORCHESTRATOR, NO_NODE, UP).await;

    // Then — there is no row to move, and picking one would move a row nobody pointed at
    assert_refused(result)
        .with_code(Code::InvalidArgument)
        .naming("node_id");
}

#[tokio::test]
async fn reordering_in_a_direction_that_is_neither_up_nor_down_is_refused() {
    // Given
    let repo = a_pr_stack_orchestrator();

    // When — a direction no control on the panel sends
    let result = reorder(&repo.service(), ORCHESTRATOR, SECOND_NODE, "sideways").await;

    // Then — the caller is wrong, which is `invalid_argument`, and the direction is named back
    assert_refused(result)
        .with_code(Code::InvalidArgument)
        .naming("sideways");
}

#[tokio::test]
async fn a_successful_reorder_answers_with_the_reordered_plan() {
    // Given — two rows, n1 above n2
    let repo = a_pr_stack_orchestrator();

    // When — the operator moves the lower row up
    let response = reorder(&repo.service(), ORCHESTRATOR, SECOND_NODE, UP)
        .await
        .expect("moving a planned row up should succeed");

    // Then — the answer carries the order the list must now render, so the row repaints without
    // waiting for the next poll tick
    assert_eq!(
        positions_in(&response.stack_plan_json, &[FIRST_NODE, SECOND_NODE]),
        vec![Some(1), Some(0)],
        "the response must carry the plan as it stands after the move"
    );
}

// --- PullBaseIntoBranch -----------------------------------------------------

#[tokio::test]
async fn pulling_into_a_session_that_is_not_a_pr_stack_orchestrator_is_refused() {
    // Given — a child PR session addressed as though it held the plan
    let repo = a_pr_stack_orchestrator();
    repo.with_a_child_session_that_is_not_an_orchestrator();

    // When
    let result = pull(&repo.service(), CHILD, FIRST_NODE, BASE_BRANCH).await;

    // Then — the pull reads a node out of a stack, and a session that has none has no branch to move
    assert_refused(result)
        .with_code(Code::FailedPrecondition)
        .naming("pr-stack orchestrator");
}

#[tokio::test]
async fn pulling_without_naming_a_node_is_refused() {
    // Given
    let repo = a_pr_stack_orchestrator();

    // When
    let result = pull(&repo.service(), ORCHESTRATOR, NO_NODE, BASE_BRANCH).await;

    // Then
    assert_refused(result)
        .with_code(Code::InvalidArgument)
        .naming("node_id");
}

#[tokio::test]
async fn pulling_without_naming_a_base_branch_is_refused() {
    // Given
    let repo = a_pr_stack_orchestrator();

    // When — the row's base-sync badge named no base, e.g. a project that stores no default branch
    let result = pull(&repo.service(), ORCHESTRATOR, FIRST_NODE, NO_BASE_BRANCH).await;

    // Then — substituting a default would take commits the operator's control never named
    assert_refused(result)
        .with_code(Code::InvalidArgument)
        .naming("base_branch");
}

#[tokio::test]
async fn pulling_in_a_session_with_no_recorded_checkout_is_refused() {
    // Given — an orchestrator whose session directory names no repository
    let repo = a_pr_stack_orchestrator();
    repo.without_a_recorded_checkout();

    // When
    let result = pull(&repo.service(), ORCHESTRATOR, FIRST_NODE, BASE_BRANCH).await;

    // Then — a mutation with nowhere to happen is a precondition failure, not a degraded leg the way
    // `QueryBranch`'s display legs are
    assert_refused(result)
        .with_code(Code::FailedPrecondition)
        .naming("no checkout is recorded");
}

#[tokio::test]
async fn a_successful_pull_answers_with_the_branch_as_it_stands_after_the_pull() {
    // Given — the base has landed a commit the node's branch does not have, so the branch is behind
    // by exactly one
    let repo = a_pr_stack_orchestrator();
    repo.with_a_commit_on_the_base("base.txt", "from the base\n", "base commit");

    // When
    let response = pull(&repo.service(), ORCHESTRATOR, FIRST_NODE, BASE_BRANCH)
        .await
        .expect("merging a clean base into a clean worktree should succeed");

    // Then — the pull happened…
    assert_eq!(
        (
            response.strategy.as_str(),
            response.changed,
            response.pushed,
            response.push_error.as_str()
        ),
        ("merge", true, true, "")
    );
    // …and the resolution describes the refs the pull just moved, not the ones the last poll saw: a
    // cached or pre-pull answer would still report the branch one commit behind, at its old tip
    let resolution = resolution_of(&response);
    let base_sync = resolution
        .base_sync
        .as_ref()
        .expect("the base-sync leg must be present");
    let remote = resolution
        .remote
        .as_ref()
        .expect("the remote leg must be present");
    assert_eq!(
        (
            resolution.branch.as_str(),
            base_sync.behind_count,
            base_sync.unavailable,
            remote.exists,
            remote.sha.as_str()
        ),
        (FIRST_BRANCH, 0, false, true, repo.worktree_head().as_str()),
        "the answered resolution must describe the branch after the pull"
    );
}
