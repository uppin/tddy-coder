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

/// Scores held in this process's memory, keyed by content — the cache a daemon owns.
///
/// Interior mutability so it can be shared as `&dyn ComplexityCache` across concurrent requests:
/// a cache that needed `&mut` would have to be locked by every caller, which is the same lock one
/// level further from the data.
#[derive(Default)]
pub struct InMemoryComplexityCache {
    // TODO(tddy-index-daemon): unbounded. Every distinct source this process scores stays for the
    // lifetime of the process, which for a long-running daemon over a tree under active edit grows
    // with the number of *versions* of a file rather than the number of files. Bounding it needs an
    // eviction policy — an LRU over insertion order, or dropping a root's entries when its index is
    // reaped — and the daemon has no idle-reap hook for analysis state yet.
    scores: Mutex<HashMap<ContentHash, Vec<FunctionComplexity>>>,
}

impl ComplexityCache for InMemoryComplexityCache {
    fn scored(&self, content: &ContentHash) -> Option<Vec<FunctionComplexity>> {
        self.held().get(content).cloned()
    }

    fn remember(&self, content: ContentHash, functions: Vec<FunctionComplexity>) {
        self.held().insert(content, functions);
    }
}

impl InMemoryComplexityCache {
    /// The held scores.
    ///
    /// The lock covers a map lookup, a clone and an insert and nothing else — no scoring and no
    /// caller's code runs under it — so it is never held across a panic, and a poisoned one would
    /// mean this module is not what it says it is rather than something to recover from.
    fn held(&self) -> std::sync::MutexGuard<'_, HashMap<ContentHash, Vec<FunctionComplexity>>> {
        self.scores
            .lock()
            .expect("nothing that can panic runs while the complexity cache is locked")
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
