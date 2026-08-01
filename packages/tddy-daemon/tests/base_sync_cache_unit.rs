//! Unit: the branch/base comparison cache re-probes exactly when the commits change, and never
//! otherwise.
//!
//! The PR-Stack panel polls `QueryBranch` every five seconds per rendered row, and each base-sync
//! answer costs a `git merge-tree`. What is pinned here is *when a probe runs*, isolated from git
//! entirely: the probe is a counting closure, so a test states how many times git would have been
//! asked rather than how long it took.
//!
//! The key is the two **commits**, which is why there is no expiry to test — an answer about a pair
//! of commits cannot become wrong, only unreachable. Failures are remembered on exactly the same
//! terms: a repository that cannot answer is the case that costs the most per probe, so re-asking it
//! twelve times a minute is the one thing the cache must not do.
//!
//! PRD: `docs/ft/coder/pr-stack-live-status.md § Panel UX` § C4.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use tddy_core::base_sync::{BaseSyncRefs, BranchBaseSync};
use tddy_daemon::base_sync_cache::{BaseSyncCache, BaseSyncKey};

const BASE: &str = "base-commit-aaa";
const HEAD: &str = "head-commit-bbb";
/// The branch a row is about, unless the test's subject is which branch it is about.
const HEAD_REF: &str = "feature/stack/n1";

// --- builders ---------------------------------------------------------------

fn a_repo() -> PathBuf {
    PathBuf::from("/repos/acme")
}

/// A comparison of one node's branch against the stack's base, at the two commits named.
fn a_key(repo_root: &Path, base_sha: &str, head_sha: &str) -> BaseSyncKey {
    a_key_for_branch(repo_root, HEAD_REF, base_sha, head_sha)
}

/// The same, for a stated branch — the two node branches that sit at the same commit right after
/// both are cut from the base are one key each, not one key.
fn a_key_for_branch(
    repo_root: &Path,
    head_ref: &str,
    base_sha: &str,
    head_sha: &str,
) -> BaseSyncKey {
    BaseSyncKey::new(
        repo_root,
        &BaseSyncRefs {
            base_ref: "origin/master".to_string(),
            base_sha: base_sha.to_string(),
            head_ref: head_ref.to_string(),
            head_sha: head_sha.to_string(),
        },
    )
}

/// A comparison that reads "behind by `behind`, no conflicts".
fn a_comparison(behind: u32) -> BranchBaseSync {
    BranchBaseSync {
        base_branch: "master".to_string(),
        base_ref: "origin/master".to_string(),
        head_ref: "feature/x".to_string(),
        behind_count: behind,
        ahead_count: 0,
        has_conflicts: false,
        conflicted_paths: Vec::new(),
    }
}

/// A probe that counts how many times git would have been asked, and answers `answer` every time.
struct CountingProbe {
    runs: AtomicUsize,
    answer: Result<BranchBaseSync, String>,
}

fn a_probe_answering(answer: Result<BranchBaseSync, String>) -> CountingProbe {
    CountingProbe {
        runs: AtomicUsize::new(0),
        answer,
    }
}

impl CountingProbe {
    fn run(&self) -> Result<BranchBaseSync, String> {
        self.runs.fetch_add(1, Ordering::SeqCst);
        self.answer.clone()
    }

    fn runs(&self) -> usize {
        self.runs.load(Ordering::SeqCst)
    }
}

/// Ask the cache for one comparison and discard the answer — most of these tests are about whether
/// the probe *ran*, not about what it said.
fn ask(cache: &BaseSyncCache, key: BaseSyncKey, probe: &CountingProbe) {
    let _ = cache.get_or_probe(key, || probe.run());
}

// --- tests ------------------------------------------------------------------

#[test]
fn the_same_pair_of_commits_is_probed_once_however_often_it_is_asked_for() {
    // Given — one comparison, and a panel that will ask for it on every poll tick
    let cache = BaseSyncCache::with_capacity(8);
    let probe = a_probe_answering(Ok(a_comparison(3)));

    // When — twelve ticks' worth of the same question
    for _ in 0..12 {
        ask(&cache, a_key(&a_repo(), BASE, HEAD), &probe);
    }

    // Then
    assert_eq!(
        probe.runs(),
        1,
        "an answer about a fixed pair of commits cannot change, so it must be computed once"
    );
}

#[test]
fn a_remembered_comparison_is_returned_verbatim() {
    // Given
    let cache = BaseSyncCache::with_capacity(8);
    let probe = a_probe_answering(Ok(a_comparison(3)));
    ask(&cache, a_key(&a_repo(), BASE, HEAD), &probe);

    // When — asked again, with a probe that would answer differently if it ran
    let second = cache.get_or_probe(a_key(&a_repo(), BASE, HEAD), || Ok(a_comparison(999)));

    // Then
    assert_eq!(second, Ok(a_comparison(3)));
}

#[test]
fn a_base_that_moved_is_probed_again() {
    // Given — the predecessor landed a commit, so the base ref now points somewhere else
    let cache = BaseSyncCache::with_capacity(8);
    let probe = a_probe_answering(Ok(a_comparison(1)));
    ask(&cache, a_key(&a_repo(), BASE, HEAD), &probe);

    // When
    ask(&cache, a_key(&a_repo(), "base-commit-ccc", HEAD), &probe);

    // Then
    assert_eq!(
        probe.runs(),
        2,
        "a comparison against a different base commit is a different question"
    );
}

#[test]
fn a_head_that_moved_is_probed_again() {
    // Given — the child session committed, so the branch tip moved
    let cache = BaseSyncCache::with_capacity(8);
    let probe = a_probe_answering(Ok(a_comparison(1)));
    ask(&cache, a_key(&a_repo(), BASE, HEAD), &probe);

    // When
    ask(&cache, a_key(&a_repo(), BASE, "head-commit-ddd"), &probe);

    // Then
    assert_eq!(probe.runs(), 2);
}

#[test]
fn two_branches_sitting_at_the_same_commit_are_probed_one_each() {
    // Given — two node branches cut from the same base and not yet committed to, which is what every
    // row of a freshly planned stack looks like: same base commit, same tip commit, different branch
    let cache = BaseSyncCache::with_capacity(8);
    let probe = a_probe_answering(Ok(a_comparison(1)));
    ask(
        &cache,
        a_key_for_branch(&a_repo(), "feature/stack/n1", BASE, HEAD),
        &probe,
    );

    // When — the second row asks
    ask(
        &cache,
        a_key_for_branch(&a_repo(), "feature/stack/n2", BASE, HEAD),
        &probe,
    );

    // Then — the answer carries the refs it was made between, and a row renders them: serving the
    // second branch the first branch's answer would put another branch's name beside its counts
    assert_eq!(probe.runs(), 2);
}

#[test]
fn the_same_commits_in_a_different_repository_are_probed_again() {
    // Given — two checkouts of the same project, only one of which holds the objects
    let cache = BaseSyncCache::with_capacity(8);
    let probe = a_probe_answering(Ok(a_comparison(1)));
    ask(&cache, a_key(&a_repo(), BASE, HEAD), &probe);

    // When
    ask(&cache, a_key(Path::new("/repos/other"), BASE, HEAD), &probe);

    // Then
    assert_eq!(probe.runs(), 2);
}

#[test]
fn a_comparison_that_failed_is_remembered_rather_than_retried_every_tick() {
    // Given — a repository that cannot answer at all. This is the case that costs the most per
    // probe, so it is the one that must not run twelve times a minute.
    let cache = BaseSyncCache::with_capacity(8);
    let probe = a_probe_answering(Err("unrelated histories".to_string()));

    // When
    for _ in 0..12 {
        ask(&cache, a_key(&a_repo(), BASE, HEAD), &probe);
    }

    // Then
    assert_eq!(probe.runs(), 1);
}

#[test]
fn a_remembered_failure_is_reported_as_the_failure_it_was() {
    // Given
    let cache = BaseSyncCache::with_capacity(8);
    let probe = a_probe_answering(Err("unrelated histories".to_string()));
    ask(&cache, a_key(&a_repo(), BASE, HEAD), &probe);

    // When — a failure must never come back out of the cache as a zeroed success
    let second = cache.get_or_probe(a_key(&a_repo(), BASE, HEAD), || Ok(a_comparison(0)));

    // Then
    assert_eq!(second, Err("unrelated histories".to_string()));
}

#[test]
fn the_oldest_entry_is_evicted_once_the_cache_is_full() {
    // Given — a cache holding two comparisons, filled to its capacity
    let cache = BaseSyncCache::with_capacity(2);
    let probe = a_probe_answering(Ok(a_comparison(1)));
    ask(&cache, a_key(&a_repo(), BASE, "head-1"), &probe);
    ask(&cache, a_key(&a_repo(), BASE, "head-2"), &probe);

    // When — a third comparison arrives, then the first is asked for again
    ask(&cache, a_key(&a_repo(), BASE, "head-3"), &probe);
    ask(&cache, a_key(&a_repo(), BASE, "head-1"), &probe);

    // Then — three cold probes plus the re-probe of the evicted one
    assert_eq!(probe.runs(), 4);
}

#[test]
fn an_entry_that_survived_eviction_is_still_remembered() {
    // Given — the same three comparisons through a cache that holds two
    let cache = BaseSyncCache::with_capacity(2);
    let probe = a_probe_answering(Ok(a_comparison(1)));
    ask(&cache, a_key(&a_repo(), BASE, "head-1"), &probe);
    ask(&cache, a_key(&a_repo(), BASE, "head-2"), &probe);
    ask(&cache, a_key(&a_repo(), BASE, "head-3"), &probe);

    // When — the newest is asked for again
    ask(&cache, a_key(&a_repo(), BASE, "head-3"), &probe);

    // Then — eviction took the oldest, not the whole cache
    assert_eq!(probe.runs(), 3);
}
