//! What a complexity cache is for: an unchanged file is not scored twice, a changed one always is.
//!
//! Scoring is pure over the source text, so a cache cannot be observed through what it returns —
//! a reused score and a recomputed one are the same value. What distinguishes them is *how many
//! times the source was scored*, so every test here counts that, through a cache that records what
//! it was asked to remember.
//!
//! Keyed by the hash of the content and never by the path, which is the whole reason the daemon can
//! hold one of these across requests: a file it has already scored is answered from the score, and
//! a file whose bytes changed is scored again without anyone having to invalidate anything.

use std::sync::Mutex;

use pretty_assertions::assert_eq;
use tddy_code_analysis::complexity::FunctionComplexity;
use tddy_code_analysis::complexity_cache::{
    cached_file_complexity, ComplexityCache, ContentHash, InMemoryComplexityCache,
    PassThroughComplexityCache,
};

/// One branching function — the smallest source whose score is something other than 1.
const A_BRANCHING_FUNCTION: &str = r#"pub fn classify(x: i32) -> &'static str {
    if x < 0 {
        "neg"
    } else {
        "pos"
    }
}
"#;

/// The same function with one more branch, so an edit changes both the bytes and the score.
const THE_SAME_FUNCTION_WITH_ANOTHER_BRANCH: &str = r#"pub fn classify(x: i32) -> &'static str {
    if x < 0 {
        "neg"
    } else if x == 0 {
        "zero"
    } else {
        "pos"
    }
}
"#;

fn classify_scoring(complexity: u32) -> Vec<FunctionComplexity> {
    vec![FunctionComplexity {
        name: "classify".to_string(),
        line: 1,
        complexity,
    }]
}

/// A cache that records every score it was asked to remember, wrapped around a real one.
///
/// A remembered score is by definition a score that had just been computed, so the count of those
/// *is* the count of scorings — the one fact these tests are about and the one no return value
/// carries.
struct RecordingCache<C: ComplexityCache> {
    inner: C,
    remembered: Mutex<Vec<ContentHash>>,
}

impl<C: ComplexityCache> RecordingCache<C> {
    fn over(inner: C) -> Self {
        Self {
            inner,
            remembered: Mutex::new(Vec::new()),
        }
    }

    /// How many times a source had to be scored rather than answered from the cache.
    fn scorings(&self) -> usize {
        self.remembered
            .lock()
            .expect("the record is readable")
            .len()
    }
}

impl<C: ComplexityCache> ComplexityCache for RecordingCache<C> {
    fn scored(&self, content: &ContentHash) -> Option<Vec<FunctionComplexity>> {
        self.inner.scored(content)
    }

    fn remember(&self, content: ContentHash, functions: Vec<FunctionComplexity>) {
        self.remembered
            .lock()
            .expect("the record is writable")
            .push(content);
        self.inner.remember(content, functions);
    }
}

#[test]
fn reuses_a_cached_score_for_unchanged_source() {
    // Given a source already scored once by a cache that keeps what it is told
    let cache = RecordingCache::over(InMemoryComplexityCache::default());
    let first = cached_file_complexity(&cache, A_BRANCHING_FUNCTION).expect("the source scores");

    // When the same source is scored again
    let second = cached_file_complexity(&cache, A_BRANCHING_FUNCTION).expect("the source scores");

    // Then it was scored once and answered twice
    assert_eq!(cache.scorings(), 1);
    assert_eq!(first, classify_scoring(2));
    assert_eq!(second, classify_scoring(2));
}

#[test]
fn rescores_a_file_whose_content_changed() {
    // Given a source already scored once
    let cache = RecordingCache::over(InMemoryComplexityCache::default());
    cached_file_complexity(&cache, A_BRANCHING_FUNCTION).expect("the source scores");

    // When an edit adds a branch and the file is scored again
    let edited = cached_file_complexity(&cache, THE_SAME_FUNCTION_WITH_ANOTHER_BRANCH)
        .expect("the edited source scores");

    // Then the new content was scored rather than answered with the old score
    assert_eq!(cache.scorings(), 2);
    assert_eq!(edited, classify_scoring(3));
}

#[test]
fn scores_two_files_with_identical_content_once() {
    // Given two files, at two paths, holding the same source
    let tree = tempfile::tempdir().expect("a temporary tree");
    let one = tree.path().join("one.rs");
    let other = tree.path().join("other.rs");
    std::fs::write(&one, A_BRANCHING_FUNCTION).expect("the first file");
    std::fs::write(&other, A_BRANCHING_FUNCTION).expect("the second file");
    let cache = RecordingCache::over(InMemoryComplexityCache::default());

    // When each is read from disk and scored
    let first = cached_file_complexity(&cache, &std::fs::read_to_string(&one).expect("read one"))
        .expect("the first file scores");
    let second = cached_file_complexity(
        &cache,
        &std::fs::read_to_string(&other).expect("read other"),
    )
    .expect("the second file scores");

    // Then the content was scored once for both, because the key is the content and not the path
    assert_eq!(cache.scorings(), 1);
    assert_eq!(first, classify_scoring(2));
    assert_eq!(second, classify_scoring(2));
}

#[test]
fn a_pass_through_cache_scores_every_time() {
    // Given the cache the command line installs, which keeps nothing
    let cache = RecordingCache::over(PassThroughComplexityCache);
    let first = cached_file_complexity(&cache, A_BRANCHING_FUNCTION).expect("the source scores");

    // When the same source is scored again
    let second = cached_file_complexity(&cache, A_BRANCHING_FUNCTION).expect("the source scores");

    // Then it was scored both times — exactly what `tddy-tools analyze` does today
    assert_eq!(cache.scorings(), 2);
    assert_eq!(first, classify_scoring(2));
    assert_eq!(second, classify_scoring(2));
}
