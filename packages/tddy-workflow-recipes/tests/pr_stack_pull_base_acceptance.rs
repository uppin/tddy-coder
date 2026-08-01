//! Acceptance: taking a base branch's commits into a stack node's branch, inside that node's own
//! worktree, and pushing the result.
//!
//! This is the operator's one-click "stay where I am and take what the base has", distinct from a
//! repoint (which answers "this node belongs somewhere else now" by dropping parent edges). Every
//! assertion here exists because the button may be pressed while a child session's agent is
//! mid-turn in the very worktree being touched:
//!
//! - The **order of the safety checks** is load-bearing, not cosmetic. A dirty worktree is refused
//!   *before* the fetch, so a refusal leaves not just the tree but the repository's remote-tracking
//!   refs exactly as they were — which is what `a_dirty_worktree_is_refused_before_anything_is_
//!   fetched_or_merged` asserts by watching `refs/remotes/origin/<base>` across the call.
//! - A **conflict aborts** (D33). Unlike the agent-facing `pr_resolve_conflicts`, which deliberately
//!   leaves markers in the tree for an agent that is about to be prompted to resolve them, nobody is
//!   in scope here — so `MERGE_HEAD`, `rebase-merge` and a modified `HEAD` are all failures.
//! - A **failed push is `Ok`, not `Err`** (D32). The local merge or rebase landed; reporting that
//!   truthfully is strictly better than rolling it back, and a rebase's force-push carries a lease
//!   so a remote that moved aborts the push rather than being clobbered.
//!
//! Real git throughout — a bare `origin`, a clone, a linked worktree per node, and a second clone
//! standing in for whoever else pushes. Fetch, lease and abort semantics are precisely what a fake
//! would get wrong. All plumbing lives in the builders below; no test body runs git itself.
//!
//! PRD: `docs/ft/coder/pr-stack-live-status.md § Panel UX` § C5 (D30-D33).

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::{a_planned_node, an_open_node, assert_rejected, write_stack};
use tddy_workflow_recipes::pr_stack::{
    pull_base_into_node_branch, BaseSyncStrategy, PullBaseReport,
};

const BASE_BRANCH: &str = "master";
const NODE: &str = "n1";
const BRANCH: &str = "feature/stack/n1";
/// The one file both the base and the branch edit, in the tests whose subject is a conflict.
const SHARED_FILE: &str = "file.txt";

/// `dirty_worktree_action`: the empty string is the default, and refuses.
const REFUSE_IF_DIRTY: &str = "";
const COMMIT_FIRST: &str = "commit";
/// `commit_message` is meaningless unless the action is `commit`.
const NO_COMMIT_MESSAGE: &str = "";

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

fn write_file(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

// --- the fixture ------------------------------------------------------------

/// A bare `origin`, a clone with `master` checked out, a linked worktree holding the node's branch,
/// and a session dir whose `changeset.yaml` records the node.
///
/// The base commits it makes land on both the clone's local `master` and on `origin/master`, so a
/// test says nothing about which of the two refs the pull resolves — except where that is precisely
/// the subject, in which case the commit is made from [`StackedRepo::another_clone`] and the local
/// remote-tracking ref is deliberately left stale.
struct StackedRepo {
    tmp: tempfile::TempDir,
}

fn a_stacked_repo() -> StackedRepo {
    let tmp = tempfile::tempdir().unwrap();
    let repo = StackedRepo { tmp };

    fs::create_dir_all(repo.session_dir()).unwrap();
    fs::create_dir_all(repo.worktrees_dir()).unwrap();

    fs::create_dir_all(repo.origin()).unwrap();
    git(
        &repo.origin(),
        &["init", "--quiet", "--bare", "-b", BASE_BRANCH],
    );

    fs::create_dir_all(repo.repo_root()).unwrap();
    let root = repo.repo_root();
    git(&root, &["init", "--quiet", "-b", BASE_BRANCH]);
    identify(&root);
    write_file(&root.join(SHARED_FILE), "initial\n");
    git(&root, &["add", "."]);
    git(&root, &["commit", "--quiet", "-m", "initial"]);
    git(&root, &["remote", "add", "origin", &repo.origin_url()]);
    git(&root, &["push", "--quiet", "-u", "origin", BASE_BRANCH]);

    git(&root, &["branch", BRANCH]);
    git(
        &root,
        &[
            "worktree",
            "add",
            "--quiet",
            repo.worktree().to_str().unwrap(),
            BRANCH,
        ],
    );
    git(
        &repo.worktree(),
        &["push", "--quiet", "-u", "origin", BRANCH],
    );

    write_stack(
        &repo.session_dir(),
        vec![an_open_node(NODE, BRANCH, 1, &[])],
    );
    repo
}

impl StackedRepo {
    // --- layout ---

    fn origin(&self) -> PathBuf {
        self.tmp.path().join("origin.git")
    }

    /// The bare repo's path, as the remote url `git` is handed.
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
        self.worktrees_dir().join(NODE)
    }

    fn session_dir(&self) -> PathBuf {
        self.tmp.path().join("session")
    }

    /// A linked worktree's git directory is `<clone>/.git/worktrees/<name>`, not `<worktree>/.git` —
    /// which is where `MERGE_HEAD` and `rebase-merge` actually live.
    fn worktree_git_dir(&self) -> PathBuf {
        PathBuf::from(git(&self.worktree(), &["rev-parse", "--absolute-git-dir"]))
    }

    // --- seeding the base and the branch ---

    /// A commit on the base branch, pushed — so the clone's `master` and `origin/master` both hold it.
    fn commit_on_the_base(&self, path: &str, contents: &str, subject: &str) -> &Self {
        let root = self.repo_root();
        write_file(&root.join(path), contents);
        git(&root, &["add", "."]);
        git(&root, &["commit", "--quiet", "-m", subject]);
        git(&root, &["push", "--quiet", "origin", BASE_BRANCH]);
        self
    }

    /// A commit on the node's branch, in its own worktree, pushed.
    fn commit_on_the_branch(&self, path: &str, contents: &str, subject: &str) -> &Self {
        let wt = self.worktree();
        write_file(&wt.join(path), contents);
        git(&wt, &["add", "."]);
        git(&wt, &["commit", "--quiet", "-m", subject]);
        git(&wt, &["push", "--quiet", "origin", BRANCH]);
        self
    }

    /// An edit to a tracked file, left uncommitted — the worktree is now dirty.
    fn edit_in_the_worktree(&self, path: &str, contents: &str) -> &Self {
        write_file(&self.worktree().join(path), contents);
        self
    }

    /// A file git has never seen. Untracked is not dirty: it blocks nothing.
    fn add_an_untracked_file(&self, path: &str, contents: &str) -> &Self {
        write_file(&self.worktree().join(path), contents);
        self
    }

    /// Someone else's clone of the same origin — whoever else pushes while the operator is looking at
    /// the panel. Its pushes deliberately leave *this* clone's remote-tracking refs stale.
    fn another_clone(&self) -> PathBuf {
        let other = self.tmp.path().join("other-clone");
        if !other.exists() {
            git(
                self.tmp.path(),
                &[
                    "clone",
                    "--quiet",
                    &self.origin_url(),
                    other.to_str().unwrap(),
                ],
            );
            identify(&other);
        }
        other
    }

    /// A base commit pushed by somebody else: on `origin`, but not yet on any ref this clone holds.
    fn commit_on_the_base_from_another_clone(
        &self,
        path: &str,
        contents: &str,
        subject: &str,
    ) -> &Self {
        let other = self.another_clone();
        write_file(&other.join(path), contents);
        git(&other, &["add", "."]);
        git(&other, &["commit", "--quiet", "-m", subject]);
        git(&other, &["push", "--quiet", "origin", BASE_BRANCH]);
        self
    }

    /// A commit somebody else pushed onto the *node's* branch after this clone last saw it — the
    /// remote that moved under a rebase.
    fn commit_on_the_branch_from_another_clone(
        &self,
        path: &str,
        contents: &str,
        subject: &str,
    ) -> &Self {
        let other = self.another_clone();
        git(&other, &["checkout", "--quiet", BRANCH]);
        write_file(&other.join(path), contents);
        git(&other, &["add", "."]);
        git(&other, &["commit", "--quiet", "-m", subject]);
        git(&other, &["push", "--quiet", "origin", BRANCH]);
        self
    }

    // --- the stack the session records ---

    /// A second node that was never started, so it owns no branch of its own.
    fn with_a_node_that_owns_no_branch(&self, node_id: &str) -> &Self {
        write_stack(
            &self.session_dir(),
            vec![
                an_open_node(NODE, BRANCH, 1, &[]),
                a_planned_node(node_id, &[]),
            ],
        );
        self
    }

    /// A sibling node's worktree, created **under `.worktrees/`** off the node's branch and not
    /// committed to — so its `HEAD` is the very commit the node's branch points at.
    ///
    /// That is the normal state moments after a descendant is spawned, and it is what
    /// `find_existing_worktree_for_branch_ref`'s by-commit tier answers with once the node's own
    /// worktree stops matching by name.
    fn with_a_sibling_worktree_branched_off_the_node(&self, sibling_branch: &str) -> PathBuf {
        let sibling = self.repo_root().join(".worktrees").join("n2");
        git(
            &self.repo_root(),
            &[
                "worktree",
                "add",
                "--quiet",
                "-b",
                sibling_branch,
                sibling.to_str().unwrap(),
                BRANCH,
            ],
        );
        sibling
    }

    /// The node's own worktree, taken away — a checkout the operator removed by hand, or one whose
    /// session was cleaned up. The branch itself survives.
    fn without_the_nodes_own_worktree(&self) -> &Self {
        git(
            &self.repo_root(),
            &[
                "worktree",
                "remove",
                "--force",
                self.worktree().to_str().unwrap(),
            ],
        );
        self
    }

    /// `origin` renamed to `upstream`, so the repository's own default remote is no longer the one a
    /// hardcoded `"origin"` would reach. Renaming carries the remote-tracking refs and the branches'
    /// upstream configuration with it, which is what makes `upstream` the *detected* default.
    fn with_upstream_as_the_default_remote(&self) -> &Self {
        git(
            &self.repo_root(),
            &["remote", "rename", "origin", "upstream"],
        );
        self
    }

    /// This clone's remote-tracking ref for the node's branch, deleted — what a clone that has never
    /// fetched the branch holds, and the state a lease cannot be taken against.
    fn forgetting_what_the_remote_holds_for_the_branch(&self) -> &Self {
        git(
            &self.repo_root(),
            &["update-ref", "-d", &format!("refs/remotes/origin/{BRANCH}")],
        );
        self
    }

    /// A second node whose branch is real but which nobody ever checked out anywhere.
    fn with_a_node_whose_branch_has_no_worktree(&self, node_id: &str, branch: &str) -> &Self {
        git(&self.repo_root(), &["branch", branch]);
        write_stack(
            &self.session_dir(),
            vec![
                an_open_node(NODE, BRANCH, 1, &[]),
                an_open_node(node_id, branch, 2, &[]),
            ],
        );
        self
    }

    // --- reading the result back ---

    fn worktree_head(&self) -> String {
        git(&self.worktree(), &["rev-parse", "HEAD"])
    }

    /// `git status --porcelain` in the node's worktree: empty means clean.
    fn worktree_status(&self) -> String {
        git(&self.worktree(), &["status", "--porcelain"])
    }

    fn file_in_the_worktree(&self, path: &str) -> String {
        fs::read_to_string(self.worktree().join(path))
            .unwrap_or_else(|e| panic!("reading {path} from the worktree: {e}"))
    }

    /// The commit subjects on the node's branch, newest first.
    fn branch_subjects(&self) -> Vec<String> {
        git(&self.worktree(), &["log", "--format=%s"])
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// The subject of the newest commit that touched `path` on the node's branch.
    fn newest_commit_touching(&self, path: &str) -> String {
        git(&self.worktree(), &["log", "-1", "--format=%s", "--", path])
    }

    /// What `origin` itself holds for a branch — the truth a reviewer sees.
    fn sha_on_origin(&self, branch: &str) -> String {
        git(
            &self.origin(),
            &["rev-parse", &format!("refs/heads/{branch}")],
        )
    }

    /// What *this* clone believes `origin` holds — the ref a fetch would move, and the ref a
    /// force-with-lease is taken against.
    fn remote_tracking_sha(&self, branch: &str) -> String {
        git(
            &self.repo_root(),
            &["rev-parse", &format!("refs/remotes/origin/{branch}")],
        )
    }

    /// The commit any worktree's `HEAD` points at — for reading a *sibling*'s tip, which the node's
    /// own [`StackedRepo::worktree_head`] cannot.
    fn head_of(&self, worktree: &Path) -> String {
        git(worktree, &["rev-parse", "HEAD"])
    }
}

// --- the call under test ----------------------------------------------------

/// The pull the operator's one click sends: **merge** [`BASE_BRANCH`] into [`NODE`]'s branch, and
/// **refuse** rather than touch a worktree with outstanding tracked changes.
///
/// Every test varies at most one of those, so the `with`-style methods below name that one thing and
/// the rest stays out of the way.
struct Pull<'a> {
    repo: &'a StackedRepo,
    node_id: &'a str,
    strategy: BaseSyncStrategy,
    dirty_worktree_action: &'a str,
    commit_message: &'a str,
}

fn a_pull(repo: &StackedRepo) -> Pull<'_> {
    Pull {
        repo,
        node_id: NODE,
        strategy: BaseSyncStrategy::Merge,
        dirty_worktree_action: REFUSE_IF_DIRTY,
        commit_message: NO_COMMIT_MESSAGE,
    }
}

impl<'a> Pull<'a> {
    /// A node other than the one the fixture's worktree belongs to.
    fn of_the_node(mut self, node_id: &'a str) -> Self {
        self.node_id = node_id;
        self
    }

    /// Replay the branch on its base instead of merging the base into it.
    fn rebasing(mut self) -> Self {
        self.strategy = BaseSyncStrategy::Rebase;
        self
    }

    /// The operator confirmed the prompt: commit and push what is outstanding, then pull.
    fn committing_outstanding_work_as(mut self, commit_message: &'a str) -> Self {
        self.dirty_worktree_action = COMMIT_FIRST;
        self.commit_message = commit_message;
        self
    }

    /// The same confirmation with the message field left blank — a prompt submitted before it was
    /// filled in, or a caller that sent the action without the message that gives it meaning.
    fn committing_outstanding_work_without_a_message(self) -> Self {
        self.committing_outstanding_work_as(NO_COMMIT_MESSAGE)
    }

    fn run(self) -> Result<PullBaseReport, String> {
        pull_base_into_node_branch(
            &self.repo.session_dir(),
            &self.repo.repo_root(),
            self.node_id,
            BASE_BRANCH,
            self.strategy,
            self.dirty_worktree_action,
            self.commit_message,
        )
    }
}

// --- tests ------------------------------------------------------------------

#[test]
fn merging_the_base_brings_the_branch_up_to_date_and_pushes_it() {
    // Given — the base has landed a commit the branch does not have, and the branch has one of its own
    let repo = a_stacked_repo();
    repo.commit_on_the_branch("feature.txt", "feature\n", "feature commit");
    repo.commit_on_the_base("base.txt", "from the base\n", "base commit");

    // When
    let report = a_pull(&repo)
        .run()
        .expect("merging a clean base into a clean worktree should succeed");

    // Then — the base's work is in the branch, and origin agrees with what landed locally
    assert_eq!(report.strategy, "merge");
    assert!(
        report.changed,
        "the branch was behind, so the pull changed it"
    );
    assert!(report.pushed, "a successful merge must be pushed");
    assert_eq!(report.push_error, None);
    assert_eq!(repo.file_in_the_worktree("base.txt"), "from the base\n");
    assert_eq!(repo.file_in_the_worktree("feature.txt"), "feature\n");
    assert_eq!(repo.worktree_head(), report.head_sha);
    assert_eq!(repo.sha_on_origin(BRANCH), report.head_sha);
}

#[test]
fn merging_a_branch_that_is_already_up_to_date_changes_nothing_and_pushes_nothing() {
    // Given — the branch already contains every commit on its base
    let repo = a_stacked_repo();
    let head_before = repo.worktree_head();
    let origin_before = repo.sha_on_origin(BRANCH);

    // When
    let report = a_pull(&repo)
        .run()
        .expect("a no-op pull is a success, not a refusal");

    // Then — no commit, and no push: a push here would be a round trip that changes nothing
    assert!(!report.changed);
    assert!(!report.pushed);
    assert_eq!(report.push_error, None);
    assert_eq!(report.head_sha, head_before);
    assert_eq!(repo.worktree_head(), head_before);
    assert_eq!(repo.sha_on_origin(BRANCH), origin_before);
}

#[test]
fn a_conflicting_merge_is_refused_and_leaves_the_worktree_exactly_as_it_was() {
    // Given — the base and the branch have each rewritten the same file
    let repo = a_stacked_repo();
    repo.commit_on_the_branch(SHARED_FILE, "the branch's version\n", "branch edit");
    repo.commit_on_the_base(SHARED_FILE, "the base's version\n", "base edit");
    let head_before = repo.worktree_head();

    // When
    let result = a_pull(&repo).run();

    // Then — the conflicting path is named, and nothing is left half-merged for an agent that may be
    // mid-turn in this worktree to trip over
    assert_rejected(result).with_reason_containing(SHARED_FILE);
    assert_eq!(repo.worktree_status(), "", "the worktree must be clean");
    assert_eq!(repo.worktree_head(), head_before);
    assert_eq!(
        repo.file_in_the_worktree(SHARED_FILE),
        "the branch's version\n",
        "no conflict markers may be left in the tree"
    );
    assert!(
        !repo.worktree_git_dir().join("MERGE_HEAD").exists(),
        "an aborted merge must leave no MERGE_HEAD behind"
    );
}

#[test]
fn a_dirty_worktree_is_refused_before_anything_is_fetched_or_merged() {
    // Given — somebody else pushed a base commit this clone has not seen, and the operator has
    // uncommitted work in the node's worktree
    let repo = a_stacked_repo();
    repo.commit_on_the_base_from_another_clone("base.txt", "from the base\n", "base commit");
    repo.edit_in_the_worktree(SHARED_FILE, "the operator's unsaved edit\n");
    let head_before = repo.worktree_head();
    let base_tracking_before = repo.remote_tracking_sha(BASE_BRANCH);

    // When
    let result = a_pull(&repo).run();

    // Then — refused by path, and the refusal came first: the fetch never ran, so this clone's idea
    // of the base has not moved either
    assert_rejected(result).with_reason_containing(SHARED_FILE);
    assert_eq!(
        repo.remote_tracking_sha(BASE_BRANCH),
        base_tracking_before,
        "the dirty check must run before the fetch"
    );
    assert_eq!(repo.worktree_head(), head_before);
    assert_eq!(
        repo.file_in_the_worktree(SHARED_FILE),
        "the operator's unsaved edit\n",
        "the uncommitted edit must survive byte-for-byte"
    );
}

#[test]
fn an_untracked_file_does_not_block_the_pull() {
    // Given — the base is ahead, and the worktree holds a file git has never tracked
    let repo = a_stacked_repo();
    repo.commit_on_the_base("base.txt", "from the base\n", "base commit");
    repo.add_an_untracked_file("scratch.md", "notes to self\n");

    // When — the caller did not opt into committing anything
    let report = a_pull(&repo)
        .run()
        .expect("an untracked file is not outstanding work — it must not block the pull");

    // Then — the pull landed and the untracked file was left alone
    assert!(report.changed);
    assert!(report.pushed);
    assert_eq!(repo.file_in_the_worktree("base.txt"), "from the base\n");
    assert_eq!(repo.file_in_the_worktree("scratch.md"), "notes to self\n");
}

#[test]
fn committing_the_outstanding_work_first_lets_the_pull_proceed() {
    // Given — the base is ahead and the worktree has an uncommitted edit
    let repo = a_stacked_repo();
    repo.commit_on_the_base("base.txt", "from the base\n", "base commit");
    repo.edit_in_the_worktree(SHARED_FILE, "the operator's edit\n");

    // When — the operator confirms the prompt: commit and push what is outstanding, then pull
    let report = a_pull(&repo)
        .committing_outstanding_work_as("wip: save the operator's edit")
        .run()
        .expect("committing first should let the pull proceed");

    // Then — the edit is a commit under the message the caller gave, and the base was merged in
    assert_eq!(
        repo.newest_commit_touching(SHARED_FILE),
        "wip: save the operator's edit"
    );
    assert_eq!(
        repo.worktree_status(),
        "",
        "nothing may be left uncommitted"
    );
    assert_eq!(
        repo.file_in_the_worktree(SHARED_FILE),
        "the operator's edit\n"
    );
    assert_eq!(repo.file_in_the_worktree("base.txt"), "from the base\n");
    assert!(report.changed);
    assert!(report.pushed);
    assert_eq!(repo.sha_on_origin(BRANCH), report.head_sha);
}

#[test]
fn committing_the_outstanding_work_under_no_message_at_all_is_refused() {
    // Given — the base is ahead and the worktree has an uncommitted edit
    let repo = a_stacked_repo();
    repo.commit_on_the_base("base.txt", "from the base\n", "base commit");
    repo.edit_in_the_worktree(SHARED_FILE, "the operator's edit\n");
    let head_before = repo.worktree_head();
    let base_tracking_before = repo.remote_tracking_sha(BASE_BRANCH);

    // When — the action says "commit it first" but names nothing to commit it under
    let result = a_pull(&repo)
        .committing_outstanding_work_without_a_message()
        .run();

    // Then — refused rather than committed under an invented subject, and the refusal came before
    // the fetch, so the operator's edit and this clone's idea of the base are both where they were
    assert_rejected(result).with_reason_containing("commit message");
    assert_eq!(
        repo.file_in_the_worktree(SHARED_FILE),
        "the operator's edit\n",
        "the uncommitted edit must survive byte-for-byte"
    );
    assert_eq!(repo.worktree_head(), head_before);
    assert_eq!(repo.remote_tracking_sha(BASE_BRANCH), base_tracking_before);
}

#[test]
fn rebasing_replays_the_branch_on_the_base_and_force_pushes_with_a_lease() {
    // Given — one commit on the branch, one on the base, touching different files
    let repo = a_stacked_repo();
    repo.commit_on_the_branch("feature.txt", "feature\n", "feature commit");
    repo.commit_on_the_base("base.txt", "from the base\n", "base commit");

    // When
    let report = a_pull(&repo)
        .rebasing()
        .run()
        .expect("rebasing a clean branch onto its base should succeed");

    // Then — the branch's own commit sits on top of the base's, with no merge commit anywhere
    assert_eq!(report.strategy, "rebase");
    assert!(report.changed);
    assert_eq!(
        repo.branch_subjects(),
        vec![
            "feature commit".to_string(),
            "base commit".to_string(),
            "initial".to_string()
        ]
    );
    // …and the rewritten history reached origin, which only a force-push can do
    assert!(report.pushed);
    assert_eq!(report.push_error, None);
    assert_eq!(repo.sha_on_origin(BRANCH), report.head_sha);
}

#[test]
fn a_conflicting_rebase_is_aborted_and_leaves_the_branch_where_it_was() {
    // Given — the base and the branch have each rewritten the same file
    let repo = a_stacked_repo();
    repo.commit_on_the_branch(SHARED_FILE, "the branch's version\n", "branch edit");
    repo.commit_on_the_base(SHARED_FILE, "the base's version\n", "base edit");
    let head_before = repo.worktree_head();

    // When
    let result = a_pull(&repo).rebasing().run();

    // Then — a half-finished rebase is the one state that would strand the branch on a detached HEAD
    assert_rejected(result).with_reason_containing(SHARED_FILE);
    assert_eq!(repo.worktree_head(), head_before);
    assert_eq!(repo.worktree_status(), "");
    assert!(
        !repo.worktree_git_dir().join("rebase-merge").exists()
            && !repo.worktree_git_dir().join("rebase-apply").exists(),
        "an aborted rebase must leave no rebase state directory behind"
    );
}

#[test]
fn a_remote_that_moved_under_a_rebase_reports_the_failed_push_rather_than_overwriting_it() {
    // Given — a rebase-able branch, and then somebody else pushes onto that same branch. The lease is
    // taken against what *this* clone believes origin holds, and a fetch of the base does not move a
    // remote-tracking ref for the branch — so the lease is stale by the time the push runs.
    let repo = a_stacked_repo();
    repo.commit_on_the_branch("feature.txt", "feature\n", "feature commit");
    repo.commit_on_the_base("base.txt", "from the base\n", "base commit");
    repo.commit_on_the_branch_from_another_clone(
        "theirs.txt",
        "somebody else's work\n",
        "their commit",
    );
    let their_sha = repo.sha_on_origin(BRANCH);

    // When
    let report = a_pull(&repo)
        .rebasing()
        .run()
        .expect("a failed push is reported, not raised — the local rebase landed");

    // Then — the local work is real and reported as such…
    assert!(report.changed);
    assert_eq!(repo.worktree_head(), report.head_sha);
    // …the push is reported as failed rather than swallowed…
    assert!(!report.pushed);
    assert!(
        report.push_error.is_some(),
        "a push that failed must say why, not report a bare false"
    );
    // …and the lease did its job: somebody else's commit is still what origin holds
    assert_eq!(repo.sha_on_origin(BRANCH), their_sha);
}

#[test]
fn a_rebase_force_pushes_to_the_repositorys_own_default_remote() {
    // Given — a clone whose default remote is `upstream`, not `origin`, and a rebase-able branch
    let repo = a_stacked_repo();
    repo.commit_on_the_branch("feature.txt", "feature\n", "feature commit");
    repo.commit_on_the_base("base.txt", "from the base\n", "base commit");
    repo.with_upstream_as_the_default_remote();

    // When
    let report = a_pull(&repo)
        .rebasing()
        .run()
        .expect("rebasing a clean branch onto its base should succeed");

    // Then — the rewritten history reached the remote the fetch and the lease were taken against. A
    // push aimed at a literal `origin` would have had no remote of that name to reach at all.
    assert!(report.changed);
    assert_eq!(report.push_error, None);
    assert!(report.pushed);
    assert_eq!(repo.sha_on_origin(BRANCH), report.head_sha);
}

#[test]
fn a_rebase_pushes_a_branch_this_clone_holds_no_remote_tracking_ref_for() {
    // Given — a branch with no commits of its own, so the rebase fast-forwards it onto the base, and
    // a clone that holds no `refs/remotes/origin/<branch>` — the state a clone that has never
    // fetched this branch is in, and one no lease can be taken against.
    let repo = a_stacked_repo();
    repo.commit_on_the_base("base.txt", "from the base\n", "base commit");
    repo.forgetting_what_the_remote_holds_for_the_branch();

    // When
    let report = a_pull(&repo)
        .rebasing()
        .run()
        .expect("a missing remote-tracking ref is not a reason to refuse the pull");

    // Then — the push went out as a plain one. An empty lease would have meant "this branch must not
    // exist on the remote", and every such push would be refused as stale info.
    assert!(report.changed);
    assert_eq!(report.push_error, None);
    assert!(report.pushed);
    assert_eq!(repo.sha_on_origin(BRANCH), report.head_sha);
}

#[test]
fn a_worktree_that_merely_shares_the_branch_head_commit_is_refused() {
    // Given — a sibling node branched off this node's tip and has not committed yet, so its worktree
    // sits on the same commit; and this node's own worktree is gone, so nothing matches by name
    let repo = a_stacked_repo();
    repo.commit_on_the_base("base.txt", "from the base\n", "base commit");
    let sibling = repo.with_a_sibling_worktree_branched_off_the_node("feature/stack/n2");
    repo.without_the_nodes_own_worktree();
    let sibling_head_before = repo.head_of(&sibling);
    let origin_before = repo.sha_on_origin(BRANCH);

    // When
    let result = a_pull(&repo).run();

    // Then — refused, naming both branches. Merging there would have landed the base on the
    // sibling's branch and then pushed this one, which never moved: "everything up-to-date", exit
    // zero, and a report that says the pull succeeded.
    assert_rejected(result)
        .with_reason_containing(BRANCH)
        .with_reason_containing("feature/stack/n2");
    assert_eq!(
        repo.head_of(&sibling),
        sibling_head_before,
        "the sibling's branch must not have moved"
    );
    assert_eq!(repo.sha_on_origin(BRANCH), origin_before);
}

#[test]
fn a_node_that_owns_no_branch_is_refused() {
    // Given — a node that was never started
    let repo = a_stacked_repo();
    repo.with_a_node_that_owns_no_branch("n2");

    // When
    let result = a_pull(&repo).of_the_node("n2").run();

    // Then — there is no branch to pull into, and saying so beats reporting a successful no-op
    assert_rejected(result).with_reason_containing("n2");
}

#[test]
fn a_branch_with_no_worktree_is_refused() {
    // Given — a node whose branch exists but is checked out nowhere
    let repo = a_stacked_repo();
    repo.with_a_node_whose_branch_has_no_worktree("n3", "feature/stack/n3");

    // When
    let result = a_pull(&repo).of_the_node("n3").run();

    // Then — the pull happens inside the node's *own* worktree, so the missing one is named
    assert_rejected(result).with_reason_containing("feature/stack/n3");
}
