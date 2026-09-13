//! A cache for the branch/base comparison behind the PR-Stack panel's "behind base / conflicts"
//! badge.
//!
//! The panel polls `QueryBranch` every five seconds, once per rendered row, so a five-row stack asks
//! for twelve comparisons a minute per row. Each one runs `git merge-tree`, which is the expensive
//! half of the probe.
//!
//! The key is `(repo_root, base_ref, base_sha, head_ref, head_sha)` — the two resolved refs and the
//! two **commits** they point at. So there is **no TTL**, and there does not need to be one: the
//! answer for a given pair of commits is a fact about those commits and can never change. An entry
//! does not go stale, it only becomes unreachable, which happens the moment either ref moves and the
//! caller asks under a different key.
//!
//! The ref *names* are in the key because the cached answer carries them, and a row renders them
//! beside the counts. Two node branches sitting at the same commit is the normal state right after
//! both branch off the same base, and keying on the commits alone would serve the second row the
//! first row's ref names — counts right, identity wrong.
//!
//! **Failures are cached too.** A repository that cannot answer — an unresolvable ref, a corrupt
//! object, unrelated histories — would otherwise be re-probed on every single tick, which is the
//! case that costs the most and can least afford it. Note that a failure to *resolve* the refs
//! never reaches this cache at all: without two SHAs there is no key.
//!
//! Eviction is by oldest insert. There is no recency tracking because there is nothing to protect
//! against: entries are tiny, and a stack the operator is actively looking at re-inserts itself on
//! the next tick after being evicted.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use tddy_core::base_sync::{BaseSyncRefs, BranchBaseSync};

/// A comparison, keyed by the two refs it was made between — names and commits alike — inside one
/// repository.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BaseSyncKey {
    pub repo_root: PathBuf,
    pub base_ref: String,
    pub base_sha: String,
    pub head_ref: String,
    pub head_sha: String,
}

impl BaseSyncKey {
    pub fn new(repo_root: &Path, refs: &BaseSyncRefs) -> Self {
        Self {
            repo_root: repo_root.to_path_buf(),
            base_ref: refs.base_ref.clone(),
            base_sha: refs.base_sha.clone(),
            head_ref: refs.head_ref.clone(),
            head_sha: refs.head_sha.clone(),
        }
    }
}

/// The number of comparisons the process-wide cache keeps. Generous relative to the number of rows
/// an operator has open, and each entry is a handful of strings.
const DEFAULT_CAPACITY: usize = 512;

/// A bounded, content-keyed store of branch/base comparisons, successes and failures alike.
pub struct BaseSyncCache {
    capacity: usize,
    state: Mutex<CacheState>,
}

#[derive(Default)]
struct CacheState {
    answers: HashMap<BaseSyncKey, Result<BranchBaseSync, String>>,
    /// Insertion order, oldest first — what eviction walks.
    inserted: VecDeque<BaseSyncKey>,
}

impl BaseSyncCache {
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            state: Mutex::new(CacheState::default()),
        }
    }

    /// The remembered answer for `key`, or `probe`'s answer — remembered, whichever way it went.
    ///
    /// `probe` runs with no lock held, so a slow `git merge-tree` never blocks another branch's
    /// lookup. Two callers racing on the same cold key both probe and agree; the alternative is
    /// holding the lock across a subprocess.
    pub fn get_or_probe(
        &self,
        key: BaseSyncKey,
        probe: impl FnOnce() -> Result<BranchBaseSync, String>,
    ) -> Result<BranchBaseSync, String> {
        if let Some(remembered) = self.remembered(&key) {
            return remembered;
        }
        let answer = probe();
        self.remember(key, answer.clone());
        answer
    }

    fn remembered(&self, key: &BaseSyncKey) -> Option<Result<BranchBaseSync, String>> {
        let state = self.state.lock().ok()?;
        state.answers.get(key).cloned()
    }

    fn remember(&self, key: BaseSyncKey, answer: Result<BranchBaseSync, String>) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if state.answers.insert(key.clone(), answer).is_none() {
            state.inserted.push_back(key);
        }
        while state.inserted.len() > self.capacity {
            let Some(oldest) = state.inserted.pop_front() else {
                break;
            };
            state.answers.remove(&oldest);
        }
    }
}

impl Default for BaseSyncCache {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }
}

/// The process-wide cache the `QueryBranch` base-sync leg reads through.
pub fn shared() -> &'static BaseSyncCache {
    static SHARED: OnceLock<BaseSyncCache> = OnceLock::new();
    SHARED.get_or_init(BaseSyncCache::default)
}
