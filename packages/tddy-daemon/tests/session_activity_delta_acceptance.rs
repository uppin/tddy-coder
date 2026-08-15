//! Delta production in the session-room poll loop — AC6-AC14 of
//! `docs/ft/daemon/session-worktree-sync.md`.
//!
//! Real git repositories in temp directories, ticks driven explicitly rather than by a timer, and
//! no LiveKit at all: what a tick measures and what the store retains are decided entirely by the
//! checkout and the store's bounds.
//!
//! Every assertion is on the *effect* of applying a patch, never on the patch's text — asserting on
//! `git diff` output would pin git's formatting rather than this feature's behaviour.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_daemon::session_room::{
    changed_paths_between, delete_wip_ref, diff_between, publish_wip_ref, snapshot_worktree,
    wip_ref_name, write_wip_tree_within, ActivityDelta, DeltaLookupError, DeltaScope,
    SessionDeltaStore,
};

/// Pathspec meaning "no narrowing" — the whole diff.
const EVERY_PATH: &[String] = &[];

/// The measurement budget the poll loop uses; generous here because a temp repo is fast and a tight
/// budget would make this suite fail under load rather than on a defect.
const A_GENEROUS_BUDGET: Duration = Duration::from_secs(30);

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// A checkout with one commit, standing in for a session worktree.
fn a_session_worktree(root: &Path) -> String {
    git(root, &["init", "--initial-branch=main"]);
    git(root, &["config", "user.email", "agent@example.com"]);
    git(root, &["config", "user.name", "Agent"]);
    std::fs::write(root.join("README.md"), "one\n").expect("write README");
    git(root, &["add", "."]);
    git(root, &["commit", "-m", "initial"]);
    git(root, &["rev-parse", "HEAD"]).trim().to_string()
}

fn git(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| panic!("failed to run git {args:?}: {e}"));
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Apply `patch` to a fresh clone of `origin` at `base_commit` and return that clone, so a test
/// asserts on files rather than on diff text.
fn a_worktree_with_patch_applied(
    origin: &Path,
    base_commit: &str,
    patch: &[u8],
) -> tempfile::TempDir {
    let clone = tempfile::tempdir().expect("tempdir");
    git(clone.path(), &["clone", &origin.to_string_lossy(), "."]);
    git(clone.path(), &["checkout", base_commit]);
    let patch_file = clone.path().join("incoming.patch");
    std::fs::write(&patch_file, patch).expect("write patch");
    git(clone.path(), &["apply", &patch_file.to_string_lossy()]);
    std::fs::remove_file(&patch_file).expect("remove patch file");
    clone
}

fn a_delta(seq: u64, patch: Vec<u8>) -> ActivityDelta {
    ActivityDelta {
        seq,
        prev_seq: seq.saturating_sub(1),
        base_commit: "c0ffee".to_string(),
        patch,
        scoped_paths: Vec::new(),
    }
}

fn paths(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| n.to_string()).collect()
}

/// How many loose objects the repository holds, so a test can prove a measurement wrote none.
fn loose_object_count(root: &Path) -> usize {
    git(root, &["count-objects", "-v"])
        .lines()
        .find_map(|l| l.strip_prefix("count: "))
        .expect("count-objects must report a count")
        .trim()
        .parse()
        .expect("the count must be a number")
}

fn a_store() -> SessionDeltaStore {
    SessionDeltaStore::new(8, 1024 * 1024)
}

// ---------------------------------------------------------------------------
// The WIP tree
// ---------------------------------------------------------------------------

#[test]
fn writes_a_wip_tree_without_touching_the_agents_own_index() {
    // Given a worktree where the agent has staged one file and left another unstaged
    let repo = tempfile::tempdir().expect("tempdir");
    a_session_worktree(repo.path());
    std::fs::write(repo.path().join("staged.txt"), "staged\n").expect("write");
    git(repo.path(), &["add", "staged.txt"]);
    std::fs::write(repo.path().join("unstaged.txt"), "unstaged\n").expect("write");
    let index_before = git(repo.path(), &["diff", "--cached", "--name-only"]);

    // When the poll loop writes a WIP tree
    let tree = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);

    // Then a tree exists, and the agent's staging area is exactly as it was — rewriting it
    // mid-session is the one thing this measurement must never do.
    assert_eq!(tree.len(), 40, "expected a tree sha, got {tree:?}");
    assert_eq!(
        git(repo.path(), &["diff", "--cached", "--name-only"]),
        index_before
    );
}

#[test]
fn a_wip_tree_includes_a_file_git_has_never_been_told_about() {
    // Given an untracked file — which is what a `Write` produces, for the moment between the file
    // appearing and anything staging it
    let repo = tempfile::tempdir().expect("tempdir");
    a_session_worktree(repo.path());
    std::fs::write(repo.path().join("brand-new.txt"), "hello\n").expect("write");

    // When
    let tree = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);

    // Then it is in the tree. `git diff HEAD` would not have seen it at all, which is the blind
    // spot this measurement exists to close.
    let listed = git(repo.path(), &["ls-tree", "-r", "--name-only", &tree]);
    assert!(
        listed.lines().any(|l| l == "brand-new.txt"),
        "expected brand-new.txt in the WIP tree, got:\n{listed}"
    );
}

// ---------------------------------------------------------------------------
// Tick deltas
// ---------------------------------------------------------------------------

#[test]
fn a_tick_delta_carries_a_newly_written_untracked_file() {
    // Given a tick before and after the agent writes a new file
    let repo = tempfile::tempdir().expect("tempdir");
    let head = a_session_worktree(repo.path());
    let before = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);
    std::fs::write(repo.path().join("brand-new.txt"), "hello\n").expect("write");
    let after = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);

    // When
    let patch = diff_between(repo.path(), &before, &after, EVERY_PATH)
        .expect("git must diff two trees it just wrote");

    // Then applying it reproduces the file
    let mirror = a_worktree_with_patch_applied(repo.path(), &head, &patch);
    assert_eq!(
        std::fs::read(mirror.path().join("brand-new.txt")).expect("must exist"),
        b"hello\n"
    );
}

#[test]
fn a_tick_delta_carries_a_deletion() {
    // Given a tick before and after the agent deletes a tracked file
    let repo = tempfile::tempdir().expect("tempdir");
    let head = a_session_worktree(repo.path());
    let before = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);
    std::fs::remove_file(repo.path().join("README.md")).expect("delete");
    let after = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);

    // When
    let patch = diff_between(repo.path(), &before, &after, EVERY_PATH)
        .expect("git must diff two trees it just wrote");

    // Then the file is gone from the mirror. A file-by-file pull could not express this at all.
    let mirror = a_worktree_with_patch_applied(repo.path(), &head, &patch);
    assert!(
        !mirror.path().join("README.md").exists(),
        "README.md should have been deleted by the patch"
    );
}

#[test]
fn a_tick_delta_carries_binary_content_byte_for_byte() {
    // Given a file that is not valid UTF-8 — the case ReadWorktreeFile cannot express
    let repo = tempfile::tempdir().expect("tempdir");
    let head = a_session_worktree(repo.path());
    let before = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);
    let bytes: Vec<u8> = vec![0x89, 0x50, 0x4E, 0x47, 0x00, 0x80, 0xFF, 0xFE, 0x0D, 0x0A];
    std::fs::write(repo.path().join("logo.png"), &bytes).expect("write");
    let after = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);

    // When
    let patch = diff_between(repo.path(), &before, &after, EVERY_PATH)
        .expect("git must diff two trees it just wrote");

    // Then
    let mirror = a_worktree_with_patch_applied(repo.path(), &head, &patch);
    assert_eq!(
        std::fs::read(mirror.path().join("logo.png")).expect("must exist"),
        bytes
    );
}

/// File modes are a Unix concept; on a platform without them there is no bit to carry.
#[cfg(unix)]
#[test]
fn a_tick_delta_carries_a_mode_change() {
    use std::os::unix::fs::PermissionsExt as _;

    // Given a committed script that the agent then makes executable **on disk**. Not
    // `git update-index --chmod=+x`, which writes only the index: a WIP tree is `git add -A`, and
    // `add -A` takes each mode from the filesystem, so an index-only chmod is invisible to it and
    // both ticks would measure the same tree.
    let repo = tempfile::tempdir().expect("tempdir");
    a_session_worktree(repo.path());
    let script = repo.path().join("run.sh");
    std::fs::write(&script, "#!/bin/sh\necho hi\n").expect("write");
    git(repo.path(), &["add", "run.sh"]);
    git(repo.path(), &["commit", "-m", "add script"]);
    let head = git(repo.path(), &["rev-parse", "HEAD"]).trim().to_string();
    let before = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let after = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);

    // When
    let patch = diff_between(repo.path(), &before, &after, EVERY_PATH)
        .expect("git must diff two trees it just wrote");

    // Then the mirror's file is executable **on disk**, which is the half a mirror exists to
    // reproduce — `git apply` updates the working tree, so asserting on the mirror's index would
    // read the one side the patch never touched.
    let mirror = a_worktree_with_patch_applied(repo.path(), &head, &patch);
    let mode = std::fs::metadata(mirror.path().join("run.sh"))
        .expect("the mirrored script must exist")
        .permissions()
        .mode();
    assert_eq!(
        mode & 0o111,
        0o111,
        "expected an executable file, got {mode:o}"
    );
}

#[test]
fn an_idle_tick_produces_an_empty_patch() {
    // Given two ticks with nothing between them
    let repo = tempfile::tempdir().expect("tempdir");
    a_session_worktree(repo.path());
    let before = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);
    let after = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);

    // When
    let patch = diff_between(repo.path(), &before, &after, EVERY_PATH)
        .expect("git must diff two trees it just wrote");

    // Then — AC7: nothing changed is an empty patch, not an absent one.
    assert_eq!(patch, Vec::<u8>::new());
}

#[test]
fn publishes_the_uncommitted_state_as_a_ref_an_ordinary_git_fetch_can_reach() {
    // Given a worktree dirtied several ticks deep
    let repo = tempfile::tempdir().expect("tempdir");
    let head = a_session_worktree(repo.path());
    std::fs::write(repo.path().join("one.txt"), "1\n").expect("write");
    std::fs::write(repo.path().join("two.txt"), "2\n").expect("write");
    let tree = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);

    // When the tick publishes it
    let commit = publish_wip_ref(repo.path(), "sess-1", &head, &tree).expect("must publish");

    // Then a clone reaches the whole uncommitted state by fetching that ref — no patch, no
    // whole-worktree diff, and git moves only the objects the clone is missing.
    let mirror = tempfile::tempdir().expect("tempdir");
    git(
        mirror.path(),
        &["clone", &repo.path().to_string_lossy(), "."],
    );
    git(
        mirror.path(),
        &[
            "fetch",
            "origin",
            &format!("+{}:refs/tddy/wip", wip_ref_name("sess-1")),
        ],
    );
    git(mirror.path(), &["reset", "--hard", "refs/tddy/wip"]);

    assert_eq!(
        std::fs::read(mirror.path().join("one.txt")).expect("must exist"),
        b"1\n"
    );
    assert_eq!(
        std::fs::read(mirror.path().join("two.txt")).expect("must exist"),
        b"2\n"
    );
    assert_eq!(
        git(mirror.path(), &["rev-parse", "HEAD"]).trim(),
        commit,
        "the mirror must land on the published commit"
    );
}

#[test]
fn parents_the_published_commit_on_the_head_it_was_taken_from() {
    // Given a published WIP state
    let repo = tempfile::tempdir().expect("tempdir");
    let head = a_session_worktree(repo.path());
    std::fs::write(repo.path().join("dirty.txt"), "x\n").expect("write");
    let tree = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);

    // When
    let commit = publish_wip_ref(repo.path(), "sess-1", &head, &tree).expect("must publish");

    // Then its parent is the commit the work sits on, so "which commit does this apply to" is
    // answered by the object graph rather than by a field anyone has to keep in step.
    assert_eq!(
        git(repo.path(), &["rev-parse", &format!("{commit}^")]).trim(),
        head
    );
}

#[test]
fn keeps_the_wip_ref_out_of_the_branch_listing_an_agent_sees() {
    // Given a published WIP state in the repository an agent is working in
    let repo = tempfile::tempdir().expect("tempdir");
    let head = a_session_worktree(repo.path());
    std::fs::write(repo.path().join("dirty.txt"), "x\n").expect("write");
    let tree = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);
    publish_wip_ref(repo.path(), "sess-1", &head, &tree).expect("must publish");

    // When the agent lists branches
    let branches = git(repo.path(), &["branch", "--list"]);

    // Then the ref is not among them: it lives under refs/tddy/, so it is never a checkout
    // target, a push target, or a name the agent has to reason about.
    assert!(
        !branches.contains("wip"),
        "the WIP ref must not appear as a branch, got:\n{branches}"
    );
}

#[test]
fn drops_the_wip_ref_when_the_session_ends_so_its_objects_stop_being_pinned() {
    // Given a published WIP state
    let repo = tempfile::tempdir().expect("tempdir");
    let head = a_session_worktree(repo.path());
    std::fs::write(repo.path().join("dirty.txt"), "x\n").expect("write");
    let tree = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);
    publish_wip_ref(repo.path(), "sess-1", &head, &tree).expect("must publish");

    // When the session ends
    delete_wip_ref(repo.path(), "sess-1").expect("must delete");

    // Then nothing pins those objects any more. Left behind, every deleted session would hold a
    // whole worktree's worth of blobs in the project repository forever.
    let listed = git(
        repo.path(),
        &["for-each-ref", "--format=%(refname)", "refs/tddy/"],
    );
    assert_eq!(listed.trim(), "");
}

#[test]
fn writes_a_wip_tree_that_is_a_real_object_holding_the_dirty_file() {
    // Given a dirty worktree
    let repo = tempfile::tempdir().expect("tempdir");
    a_session_worktree(repo.path());
    std::fs::write(repo.path().join("dirty.txt"), "x\n").expect("write");

    // When a tree is written for it
    let tree = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);

    // Then git itself calls it a tree and it holds the dirty file. Comparing the sha against
    // another call of the same function would assert nothing — and would pass on the empty string
    // this returns when the whole measurement failed.
    assert_eq!(git(repo.path(), &["cat-file", "-t", &tree]).trim(), "tree");
    assert_eq!(
        git(repo.path(), &["ls-tree", "-r", "--name-only", &tree]),
        "README.md\ndirty.txt\n"
    );
}

#[test]
fn a_snapshot_does_not_write_a_tree_because_measuring_is_not_a_side_effect() {
    // Given a dirty worktree
    let repo = tempfile::tempdir().expect("tempdir");
    a_session_worktree(repo.path());
    std::fs::write(repo.path().join("dirty.txt"), "x\n").expect("write");
    let objects_before = loose_object_count(repo.path());

    // When the poll loop measures it
    let snapshot = snapshot_worktree(repo.path());

    // Then no tree was written and no object was created. `git add -A` materialises blobs and
    // trees in the project's SHARED object database; doing that on every poll of every room would
    // leave unreferenced objects behind twice a second, for git's two-week prune grace period.
    assert_eq!(snapshot.wip_tree, "");
    assert_eq!(loose_object_count(repo.path()), objects_before);
}

// ---------------------------------------------------------------------------
// The delta store
// ---------------------------------------------------------------------------

#[test]
fn scopes_a_calls_delta_to_the_files_that_call_touched() {
    // Given one poll window in which two calls each edited their own file
    let repo = tempfile::tempdir().expect("tempdir");
    let head = a_session_worktree(repo.path());
    let before = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);
    std::fs::write(repo.path().join("alpha.txt"), "from call a\n").expect("write");
    std::fs::write(repo.path().join("beta.txt"), "from call b\n").expect("write");
    let after = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);

    let mut store = a_store();
    store.record(ActivityDelta {
        seq: 1,
        prev_seq: 0,
        base_commit: head.clone(),
        patch: diff_between(repo.path(), &before, &after, EVERY_PATH)
            .expect("git must diff two trees it just wrote"),
        scoped_paths: Vec::new(),
    });
    store.attribute("call-a", 1, &paths(&["alpha.txt"]));
    store.attribute("call-b", 1, &paths(&["beta.txt"]));

    // When the delta for one of them is asked for
    let for_a = store
        .delta_for_call("call-a", DeltaScope::Call)
        .expect("must resolve");

    // Then applying it produces that call's file and NOT its neighbour's — two calls in one
    // window get two patches, not one shared whole-tree diff.
    let mirror = a_worktree_with_patch_applied(repo.path(), &head, &for_a.patch);
    assert_eq!(
        std::fs::read(mirror.path().join("alpha.txt")).expect("must exist"),
        b"from call a\n"
    );
    assert!(
        !mirror.path().join("beta.txt").exists(),
        "beta.txt belongs to call-b and must not appear in call-a's delta"
    );
    assert_eq!(for_a.scoped_paths, paths(&["alpha.txt"]));
}

#[test]
fn scopes_a_rename_by_either_the_name_it_left_or_the_name_it_took() {
    // Given a window in which the agent renamed a tracked file
    let repo = tempfile::tempdir().expect("tempdir");
    a_session_worktree(repo.path());
    std::fs::write(repo.path().join("before.txt"), "content worth renaming\n").expect("write");
    git(repo.path(), &["add", "before.txt"]);
    git(repo.path(), &["commit", "-m", "add the file"]);
    let head_with_file = git(repo.path(), &["rev-parse", "HEAD"]).trim().to_string();
    let before = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);
    std::fs::rename(
        repo.path().join("before.txt"),
        repo.path().join("after.txt"),
    )
    .expect("rename");
    let after = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);

    let mut store = a_store();
    store.record(ActivityDelta {
        seq: 1,
        prev_seq: 0,
        base_commit: head_with_file.clone(),
        patch: diff_between(repo.path(), &before, &after, EVERY_PATH)
            .expect("git must diff two trees it just wrote"),
        scoped_paths: Vec::new(),
    });
    // The call declared only the name it wrote to; git may describe the change under either.
    store.attribute("call-a", 1, &paths(&["after.txt"]));

    // When that call's delta is applied
    let delta = store
        .delta_for_call("call-a", DeltaScope::Call)
        .expect("must resolve");
    let mirror = a_worktree_with_patch_applied(repo.path(), &head_with_file, &delta.patch);

    // Then the whole rename travels: claiming either end of it claims the one change, because a
    // slice that carried the new name without the old would leave the mirror holding both.
    assert_eq!(
        std::fs::read(mirror.path().join("after.txt")).expect("must exist"),
        b"content worth renaming\n"
    );
    assert!(
        !mirror.path().join("before.txt").exists(),
        "the old name must be gone, or the mirror holds the file twice"
    );
}

#[test]
fn scopes_a_calls_delta_to_a_file_whose_name_contains_a_space() {
    // Given a window in which two calls each edited a file, one of them spaced — git terminates a
    // `---`/`+++` name with a tab exactly when it leaves a spaced name unquoted, and a reader that
    // keeps the tab matches no declared path at all
    let repo = tempfile::tempdir().expect("tempdir");
    let head = a_session_worktree(repo.path());
    let before = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);
    std::fs::write(repo.path().join("sp ace.txt"), "spaced\n").expect("write");
    std::fs::write(repo.path().join("plain.txt"), "plain\n").expect("write");
    let after = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);

    let mut store = a_store();
    store.record(ActivityDelta {
        seq: 1,
        prev_seq: 0,
        base_commit: head.clone(),
        patch: diff_between(repo.path(), &before, &after, EVERY_PATH)
            .expect("git must diff two trees it just wrote"),
        scoped_paths: Vec::new(),
    });
    store.attribute("call-spaced", 1, &paths(&["sp ace.txt"]));

    // When that call's delta is applied
    let delta = store
        .delta_for_call("call-spaced", DeltaScope::Call)
        .expect("must resolve");
    let mirror = a_worktree_with_patch_applied(repo.path(), &head, &delta.patch);

    // Then the spaced file is there and its neighbour is not. An empty patch here would look
    // exactly like a call that declared nothing, so the failure would be silent.
    assert_eq!(
        std::fs::read(mirror.path().join("sp ace.txt")).expect("must exist"),
        b"spaced\n"
    );
    assert!(
        !mirror.path().join("plain.txt").exists(),
        "plain.txt belongs to another call and must not ride along"
    );
    assert_eq!(delta.scoped_paths, paths(&["sp ace.txt"]));
}

#[test]
fn serves_a_change_no_call_declared_as_the_ticks_residual() {
    // Given a window in which a declared edit and an undeclared write both landed — the
    // undeclared one being what a `Bash` running a formatter produces
    let repo = tempfile::tempdir().expect("tempdir");
    let head = a_session_worktree(repo.path());
    let before = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);
    std::fs::write(repo.path().join("declared.txt"), "an Edit said so\n").expect("write");
    std::fs::write(repo.path().join("undeclared.txt"), "a formatter did this\n").expect("write");
    let after = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);

    let mut store = a_store();
    store.record(ActivityDelta {
        seq: 1,
        prev_seq: 0,
        base_commit: head.clone(),
        patch: diff_between(repo.path(), &before, &after, EVERY_PATH)
            .expect("git must diff two trees it just wrote"),
        scoped_paths: Vec::new(),
    });
    store.attribute("call-a", 1, &paths(&["declared.txt"]));

    // When the residual is asked for
    let residual = store
        .delta_for_call("call-a", DeltaScope::Residual)
        .expect("must resolve");

    // Then the undeclared change is delivered rather than attributed away into silence — the
    // property that keeps scoping from making the tick lossy.
    let mirror = a_worktree_with_patch_applied(repo.path(), &head, &residual.patch);
    assert_eq!(
        std::fs::read(mirror.path().join("undeclared.txt")).expect("must exist"),
        b"a formatter did this\n"
    );
    assert_eq!(residual.scoped_paths, paths(&["undeclared.txt"]));
}

#[test]
fn every_call_scope_plus_the_residual_reconstructs_the_whole_tick() {
    // Given a window with one declared and one undeclared change
    let repo = tempfile::tempdir().expect("tempdir");
    let head = a_session_worktree(repo.path());
    let before = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);
    std::fs::write(repo.path().join("declared.txt"), "one\n").expect("write");
    std::fs::write(repo.path().join("undeclared.txt"), "two\n").expect("write");
    let after = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);

    let mut store = a_store();
    store.record(ActivityDelta {
        seq: 1,
        prev_seq: 0,
        base_commit: head.clone(),
        patch: diff_between(repo.path(), &before, &after, EVERY_PATH)
            .expect("git must diff two trees it just wrote"),
        scoped_paths: Vec::new(),
    });
    store.attribute("call-a", 1, &paths(&["declared.txt"]));

    // When the scopes are unioned
    let mut union = store
        .delta_for_call("call-a", DeltaScope::Call)
        .expect("must resolve")
        .scoped_paths;
    union.extend(
        store
            .delta_for_call("call-a", DeltaScope::Residual)
            .expect("must resolve")
            .scoped_paths,
    );
    union.sort();

    // Then they cover exactly what the tick touched. Scoping partitions a tick; it never drops
    // part of one.
    let mut whole = changed_paths_between(repo.path(), &before, &after)
        .expect("git must list the paths between two trees it just wrote");
    whole.sort();
    assert_eq!(union, whole);
}

#[test]
fn gives_a_call_that_declared_nothing_an_empty_delta_rather_than_its_neighbours_changes() {
    // Given a window where one call declared a path and another (a Bash) declared none
    let repo = tempfile::tempdir().expect("tempdir");
    let head = a_session_worktree(repo.path());
    let before = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);
    std::fs::write(repo.path().join("declared.txt"), "one\n").expect("write");
    let after = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);

    let mut store = a_store();
    store.record(ActivityDelta {
        seq: 1,
        prev_seq: 0,
        base_commit: head.clone(),
        patch: diff_between(repo.path(), &before, &after, EVERY_PATH)
            .expect("git must diff two trees it just wrote"),
        scoped_paths: Vec::new(),
    });
    store.attribute("call-a", 1, &paths(&["declared.txt"]));
    store.attribute("call-silent", 1, &[]);

    // When the silent call's delta is asked for
    let delta = store
        .delta_for_call("call-silent", DeltaScope::Call)
        .expect("must resolve");

    // Then it is empty. Falling back to the whole tick would credit this call with another's
    // work and apply the same change twice.
    assert_eq!(delta.patch, Vec::<u8>::new());
    assert_eq!(delta.scoped_paths, Vec::<String>::new());
}

#[test]
fn resolves_two_calls_in_one_window_to_the_same_tick() {
    // Given a tick with two calls attributed to it
    let mut store = a_store();
    store.record(a_delta(1, b"patch-one".to_vec()));
    store.attribute("call-a", 1, &paths(&["a.txt"]));
    store.attribute("call-b", 1, &paths(&["b.txt"]));

    // When each is looked up
    let for_a = store
        .delta_for_call("call-a", DeltaScope::Call)
        .expect("must resolve");
    let for_b = store
        .delta_for_call("call-b", DeltaScope::Call)
        .expect("must resolve");

    // Then both name the same tick, which is what a client de-duplicates on, while their
    // patches are scoped apart.
    assert_eq!(for_a.seq, 1);
    assert_eq!(for_b.seq, 1);
    assert_eq!(for_a.scoped_paths, paths(&["a.txt"]));
    assert_eq!(for_b.scoped_paths, paths(&["b.txt"]));
}

#[test]
fn reports_a_call_it_has_never_seen_as_unknown() {
    // Given a store that knows one call
    let mut store = a_store();
    store.record(a_delta(1, b"patch-one".to_vec()));
    store.attribute("call-a", 1, &paths(&["a.txt"]));

    // When another is asked for
    let error = store
        .delta_for_call("call-zzz", DeltaScope::Call)
        .expect_err("must not resolve");

    // Then
    assert_eq!(
        error,
        DeltaLookupError::UnknownCall {
            call_id: "call-zzz".to_string(),
        }
    );
}

#[test]
fn distinguishes_an_unknown_call_from_a_delta_that_aged_out() {
    // Given a store holding two ticks, with a call attributed to the first
    let mut store = SessionDeltaStore::new(2, 1024 * 1024);
    store.record(a_delta(1, b"patch-one".to_vec()));
    store.attribute("call-old", 1, &paths(&["a.txt"]));
    store.record(a_delta(2, b"patch-two".to_vec()));

    // When a third tick evicts the first
    store.record(a_delta(3, b"patch-three".to_vec()));
    let error = store
        .delta_for_call("call-old", DeltaScope::Call)
        .expect_err("must not resolve");

    // Then it is reported as aged out, not as unknown: the client reconciles for one and reports a
    // defect for the other, so collapsing them would hide a bug behind a routine recovery.
    assert_eq!(
        error,
        DeltaLookupError::AgedOut {
            call_id: "call-old".to_string(),
            seq: 1,
        }
    );
}

#[test]
fn evicts_the_oldest_delta_once_the_ring_is_full() {
    // Given a store bounded to two ticks
    let mut store = SessionDeltaStore::new(2, 1024 * 1024);

    // When three arrive
    store.record(a_delta(1, b"patch-one".to_vec()));
    store.record(a_delta(2, b"patch-two".to_vec()));
    store.record(a_delta(3, b"patch-three".to_vec()));

    // Then the ring stayed bounded and kept the newest.
    assert_eq!(store.len(), 2);
    store.attribute("call-newest", 3, &paths(&["c.txt"]));
    assert_eq!(
        store
            .delta_for_call("call-newest", DeltaScope::Call)
            .expect("must resolve")
            .seq,
        3
    );
}

#[test]
fn evicts_by_total_bytes_as_well_as_by_tick_count() {
    // Given a store whose byte budget admits one of these patches but not two
    let mut store = SessionDeltaStore::new(100, 16);

    // When two eight-byte patches and then a third arrive
    store.record(a_delta(1, vec![b'a'; 8]));
    store.record(a_delta(2, vec![b'b'; 8]));
    store.record(a_delta(3, vec![b'c'; 8]));

    // Then the tick bound alone would have kept all three — a session that makes one enormous
    // change must be bounded too, not only one that makes many.
    assert_eq!(store.len(), 2);
}
