//! Complexity scores kept against the content that produced them.
//!
//! [`crate::complexity::file_complexity`] is pure over the source text and costs a full `syn`
//! parse plus a walk of every expression in the file. A command line pays that once per run and
//! exits, so it never mattered; a process that outlives its requests pays it again for every file
//! of every report, which on this workspace is thousands of files that did not change between two
//! requests seconds apart.
//!
//! The key is the **hash of the content**, never the path. That is what makes the cache correct
//! without an invalidation protocol: nothing has to notice an edit, because an edited file hashes
//! differently and is therefore a miss, and an unchanged file hashes identically however it was
//! reached. It also means two paths holding the same bytes — the same file in two worktrees, or a
//! generated file emitted twice — are scored once between them.
//!
//! Two implementations, because the two hosts want opposite things. A daemon wants
//! [`InMemoryComplexityCache`], which is the whole point. A command line wants
//! [`PassThroughComplexityCache`], which keeps nothing and therefore behaves exactly as the
//! uncached code did: one process, one pass over the tree, no state to be wrong about.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::complexity::{file_complexity, FunctionComplexity};
use crate::error::Result;

/// The identity of a piece of source, as far as scoring is concerned.
///
/// md5, which this crate already carries and already uses to address per-test artifacts
/// (`coverage::test_artifact_id`). Nothing here is a security boundary — the only question asked
/// of the digest is whether two sources this same process read are the same source — so the
/// stronger digest a signature would need is not worth a new dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ContentHash([u8; 16]);

impl ContentHash {
    /// The hash of `source`, which is the only way one is made.
    #[must_use]
    pub fn of(source: &str) -> Self {
        Self(md5::compute(source.as_bytes()).0)
    }
}

/// Somewhere complexity scores are kept between the calls that need them.
///
/// Storage only: what to compute on a miss is [`cached_file_complexity`]'s business, so an
/// implementation of this trait cannot get the scoring wrong, only the keeping.
pub trait ComplexityCache: Send + Sync {
    /// The scores already computed for this content, if this cache still holds them.
    fn scored(&self, content: &ContentHash) -> Option<Vec<FunctionComplexity>>;

    /// Keep these scores against the content they were computed from.
    fn remember(&self, content: ContentHash, functions: Vec<FunctionComplexity>);
}

/// The scores for `source`, from `cache` if it holds them and computed and kept otherwise.
///
/// The one place a miss turns into a scoring, so every host reaches the cache the same way and no
/// caller decides for itself what a key is.
pub fn cached_file_complexity(
    cache: &dyn ComplexityCache,
    source: &str,
) -> Result<Vec<FunctionComplexity>> {
    let content = ContentHash::of(source);
    if let Some(scored) = cache.scored(&content) {
        return Ok(scored);
    }
    let measured = file_complexity(source)?;
    cache.remember(content, measured.clone());
    Ok(measured)
}

/// How many scored file versions a daemon's cache keeps.
///
/// A count of **versions**, not files: the key is the hash of the content, so a file edited twenty
/// times between two reports occupies twenty entries. The bound therefore has to clear a full pass
/// over the tree with room to spare, or two successive reports over one workspace would evict each
/// other's entries and the cache would cost a lookup per file and save nothing.
///
/// 8192 is about five passes over the largest tree this repo holds (~1,500 Rust files), which
/// leaves a working set of files under active edit — tens of files, a few versions each — held
/// comfortably alongside the last report's scores, while a day of editing evicts its own oldest
/// versions instead of accumulating them. The entries are small (a name, a line and a count per
/// function), so the ceiling is single-digit megabytes rather than anything a process would notice.
pub const SCORED_VERSIONS_KEPT: usize = 8192;

/// Scores held in this process's memory, keyed by content — the cache a daemon owns.
///
/// Bounded, and least-recently-used: a file read again is the one a process must not have to
/// rescore, however long ago it was first seen, so a hit renews an entry rather than only
/// answering from it. Hand-rolled over a map and a use counter because there is no `lru` crate
/// here and the entries are small enough that a counter per entry is cheaper than a dependency.
///
/// Interior mutability so it can be shared as `&dyn ComplexityCache` across concurrent requests:
/// a cache that needed `&mut` would have to be locked by every caller, which is the same lock one
/// level further from the data.
pub struct InMemoryComplexityCache {
    kept: Mutex<Lru>,
}

impl Default for InMemoryComplexityCache {
    fn default() -> Self {
        Self::holding(SCORED_VERSIONS_KEPT)
    }
}

impl ComplexityCache for InMemoryComplexityCache {
    fn scored(&self, content: &ContentHash) -> Option<Vec<FunctionComplexity>> {
        self.held().renewed(content)
    }

    fn remember(&self, content: ContentHash, functions: Vec<FunctionComplexity>) {
        self.held().keep(content, functions);
    }
}

impl InMemoryComplexityCache {
    /// A cache bounded at `capacity` scored versions. [`Self::default`] is the bound a daemon
    /// wants, [`SCORED_VERSIONS_KEPT`]; a host that knows its own working set can say so.
    #[must_use]
    pub fn holding(capacity: usize) -> Self {
        Self {
            kept: Mutex::new(Lru::holding(capacity)),
        }
    }

    /// The held scores.
    ///
    /// The lock covers a map lookup, a clone, an insert and at most one eviction and nothing else
    /// — no scoring and no caller's code runs under it — so it is never held across a panic, and a
    /// poisoned one would mean this module is not what it says it is rather than something to
    /// recover from.
    fn held(&self) -> std::sync::MutexGuard<'_, Lru> {
        self.kept
            .lock()
            .expect("nothing that can panic runs while the complexity cache is locked")
    }
}

/// The bound itself: scores by content, each stamped with when it was last used.
///
/// The stamp is a counter rather than a clock. It only ever answers "which of these two was used
/// longer ago", so wall time would add a syscall per lookup and a question about monotonicity for
/// no gain — and at one increment per cached score, a `u64` outlives any process by a margin
/// nothing here has to reason about.
struct Lru {
    capacity: usize,
    scores: HashMap<ContentHash, (u64, Vec<FunctionComplexity>)>,
    /// The next use stamp, so every access orders strictly after every earlier one.
    used: u64,
}

impl Lru {
    fn holding(capacity: usize) -> Self {
        Self {
            capacity,
            scores: HashMap::new(),
            used: 0,
        }
    }

    /// The scores for this content, marked as just used.
    ///
    /// A miss spends no stamp, so the counter measures accesses to kept scores and nothing else.
    fn renewed(&mut self, content: &ContentHash) -> Option<Vec<FunctionComplexity>> {
        let stamp = self.used + 1;
        let (last_used, functions) = self.scores.get_mut(content)?;
        *last_used = stamp;
        let scored = functions.clone();
        self.used = stamp;
        Some(scored)
    }

    /// Keep these scores, dropping the least recently used entry if that puts it over its bound.
    ///
    /// A cache bounded at nothing therefore evicts what it has just kept, which is the honest
    /// reading of a zero bound rather than a case of its own.
    fn keep(&mut self, content: ContentHash, functions: Vec<FunctionComplexity>) {
        let stamp = self.next_stamp();
        self.scores.insert(content, (stamp, functions));
        while self.scores.len() > self.capacity {
            let Some(coldest) = self.least_recently_used() else {
                return;
            };
            self.scores.remove(&coldest);
        }
    }

    fn next_stamp(&mut self) -> u64 {
        self.used += 1;
        self.used
    }

    /// The content whose score has gone longest without being asked for.
    ///
    /// A scan of the map rather than a second index ordered by stamp. It runs on every insert once
    /// the cache is full, so it is a walk of the bound — tens of microseconds at 8192 entries —
    /// against the milliseconds of `syn` parsing that a hit saves, which is the whole point of the
    /// cache. An ordering structure would make it logarithmic and would also have to be kept in
    /// step with the map on every renewal, which is a second thing to get wrong for a saving
    /// nothing here can measure.
    fn least_recently_used(&self) -> Option<ContentHash> {
        self.scores
            .iter()
            .min_by_key(|(_, (last_used, _))| *last_used)
            .map(|(content, _)| *content)
    }
}

/// A cache that keeps nothing, so every score is computed — what a one-shot command line wants.
///
/// Not a disabled cache but the honest description of a process that scores a tree once and exits:
/// there is no second request to answer, so keeping the scores would only hold memory until the
/// process ended.
pub struct PassThroughComplexityCache;

impl ComplexityCache for PassThroughComplexityCache {
    fn scored(&self, _content: &ContentHash) -> Option<Vec<FunctionComplexity>> {
        None
    }

    fn remember(&self, _content: ContentHash, _functions: Vec<FunctionComplexity>) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_identical_source_to_one_key_and_different_source_to_another() {
        // Given two readings of the same source, and one of an edited copy
        let source = "pub fn f() {}\n";

        // When each is keyed
        // Then the key follows the content: same bytes, same key; changed bytes, changed key
        assert_eq!(ContentHash::of(source), ContentHash::of("pub fn f() {}\n"));
        assert_ne!(ContentHash::of(source), ContentHash::of("pub fn g() {}\n"));
    }
}
