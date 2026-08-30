//! Unit tests: manifest construction, ordering, hashing and the diff.
//!
//! The diff between two manifests is the whole of what re-sync decides. It runs on every
//! `worktree.activity` broadcast — roughly every two seconds while an agent works — so a manifest
//! whose order wobbled, or a diff that returned a path twice, would turn a free tick into a
//! transfer and a stable directory into a churning one.
//!
//! The filesystem-facing half (`ContextManifest::of_worktree`) is covered by
//! `context_manifest_acceptance.rs`; this file pins the pure structure.
//!
//! PRD: docs/ft/daemon/agent-context-sync.md § Design.

use pretty_assertions::assert_eq;
use tddy_sandbox::{diff_manifests, sha256_hex, ContextEntry, ContextManifest};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn an_entry(rel_path: &str, sha256: &str) -> ContextEntry {
    ContextEntry {
        rel_path: rel_path.to_string(),
        sha256: sha256.to_string(),
        size_bytes: 1,
    }
}

fn a_manifest_of(entries: &[(&str, &str)]) -> ContextManifest {
    ContextManifest::from_entries(
        entries
            .iter()
            .map(|(path, sha)| an_entry(path, sha))
            .collect(),
    )
}

fn paths_of(manifest: &ContextManifest) -> Vec<&str> {
    manifest
        .entries()
        .iter()
        .map(|e| e.rel_path.as_str())
        .collect()
}

// ---------------------------------------------------------------------------
// Hashing
// ---------------------------------------------------------------------------

/// A known vector, so a future change of hash function is a deliberate act rather than a silent
/// one that makes every context dir re-transfer once.
#[test]
fn the_hash_of_the_empty_input_is_the_known_sha256_of_nothing() {
    // Then
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn the_hash_is_lowercase_hex_of_the_full_digest() {
    // When
    let hash = sha256_hex(b"# Project rules\n");

    // Then
    assert_eq!(hash.len(), 64);
    assert!(hash
        .chars()
        .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
}

#[test]
fn identical_bytes_hash_identically_and_a_single_changed_byte_does_not() {
    // Then
    assert_eq!(sha256_hex(b"abc"), sha256_hex(b"abc"));
    assert_ne!(sha256_hex(b"abc"), sha256_hex(b"abd"));
}

// ---------------------------------------------------------------------------
// Ordering and construction
// ---------------------------------------------------------------------------

/// Entries come back sorted by path whatever order they were supplied in. Two manifests of the same
/// tree must compare equal, and a manifest is compared on every tick.
#[test]
fn entries_are_sorted_by_path_regardless_of_the_order_they_were_supplied_in() {
    // When
    let manifest = a_manifest_of(&[
        ("CLAUDE.md", "aaa"),
        (".claude/settings.json", "bbb"),
        ("AGENTS.md", "ccc"),
        (".agents/skills/plan/SKILL.md", "ddd"),
    ]);

    // Then
    assert_eq!(
        paths_of(&manifest),
        vec![
            ".agents/skills/plan/SKILL.md",
            ".claude/settings.json",
            "AGENTS.md",
            "CLAUDE.md",
        ]
    );
}

#[test]
fn two_manifests_of_the_same_entries_supplied_in_different_orders_are_equal() {
    // When
    let one = a_manifest_of(&[("CLAUDE.md", "aaa"), (".mcp.json", "bbb")]);
    let other = a_manifest_of(&[(".mcp.json", "bbb"), ("CLAUDE.md", "aaa")]);

    // Then
    assert_eq!(one.entries(), other.entries());
}

#[test]
fn an_empty_manifest_holds_no_entries() {
    // When
    let manifest = ContextManifest::from_entries(vec![]);

    // Then
    assert_eq!(manifest.entries(), &[] as &[ContextEntry]);
}

// ---------------------------------------------------------------------------
// The diff
// ---------------------------------------------------------------------------

/// Both halves of a change in one pass — a project that renames a skill produces exactly one fetch
/// and one delete, not two ticks' worth of work.
#[test]
fn a_rename_produces_one_fetch_and_one_delete_in_a_single_diff() {
    // Given
    let held = a_manifest_of(&[("CLAUDE.md", "aaa"), (".claude/skills/old/SKILL.md", "bbb")]);
    let served = a_manifest_of(&[("CLAUDE.md", "aaa"), (".claude/skills/new/SKILL.md", "bbb")]);

    // When
    let diff = diff_manifests(&held, &served);

    // Then
    assert_eq!(diff.fetch, vec![".claude/skills/new/SKILL.md".to_string()]);
    assert_eq!(diff.delete, vec![".claude/skills/old/SKILL.md".to_string()]);
}

/// The diff is directional: what the context dir holds versus what the repo serves. Swapping the
/// arguments swaps fetch and delete, and a caller that got them the wrong way round would delete
/// the guidance it meant to install.
#[test]
fn swapping_the_arguments_swaps_fetch_and_delete() {
    // Given
    let held = a_manifest_of(&[("CLAUDE.md", "aaa")]);
    let served = a_manifest_of(&[("CLAUDE.md", "aaa"), (".mcp.json", "bbb")]);

    // When
    let forward = diff_manifests(&held, &served);
    let backward = diff_manifests(&served, &held);

    // Then
    assert_eq!(forward.fetch, vec![".mcp.json".to_string()]);
    assert_eq!(forward.delete, Vec::<String>::new());
    assert_eq!(backward.fetch, Vec::<String>::new());
    assert_eq!(backward.delete, vec![".mcp.json".to_string()]);
}

/// A first sync, where the context dir holds nothing, fetches everything and deletes nothing.
#[test]
fn diffing_against_an_empty_local_manifest_fetches_everything() {
    // Given
    let held = ContextManifest::from_entries(vec![]);
    let served = a_manifest_of(&[("CLAUDE.md", "aaa"), (".mcp.json", "bbb")]);

    // When
    let diff = diff_manifests(&held, &served);

    // Then
    assert_eq!(
        diff.fetch,
        vec![".mcp.json".to_string(), "CLAUDE.md".to_string()]
    );
    assert_eq!(diff.delete, Vec::<String>::new());
}

/// A repo that removed its agent config entirely deletes everything and fetches nothing — the
/// context dir empties rather than freezing on the last state it saw.
#[test]
fn diffing_against_an_empty_remote_manifest_deletes_everything() {
    // Given
    let held = a_manifest_of(&[("CLAUDE.md", "aaa"), (".mcp.json", "bbb")]);
    let served = ContextManifest::from_entries(vec![]);

    // When
    let diff = diff_manifests(&held, &served);

    // Then
    assert_eq!(diff.fetch, Vec::<String>::new());
    assert_eq!(
        diff.delete,
        vec![".mcp.json".to_string(), "CLAUDE.md".to_string()]
    );
}

/// Two empty manifests are the degenerate steady state and must not be a special case.
#[test]
fn diffing_two_empty_manifests_asks_for_nothing() {
    // When
    let diff = diff_manifests(
        &ContextManifest::from_entries(vec![]),
        &ContextManifest::from_entries(vec![]),
    );

    // Then
    assert_eq!(diff.fetch, Vec::<String>::new());
    assert_eq!(diff.delete, Vec::<String>::new());
}

/// Both lists come back sorted, so a tick's work is applied in a stable order and two runs over the
/// same change produce identical logs.
#[test]
fn both_sides_of_the_diff_come_back_sorted() {
    // Given
    let held = a_manifest_of(&[("z.md", "1"), ("a.md", "1")]);
    let served = a_manifest_of(&[("y.md", "2"), ("b.md", "2")]);

    // When
    let diff = diff_manifests(&held, &served);

    // Then
    assert_eq!(diff.fetch, vec!["b.md".to_string(), "y.md".to_string()]);
    assert_eq!(diff.delete, vec!["a.md".to_string(), "z.md".to_string()]);
}

/// A path is never in both lists. Fetching and deleting the same path in one tick would race on the
/// order the caller happened to apply them in.
#[test]
fn no_path_appears_in_both_fetch_and_delete() {
    // Given
    let held = a_manifest_of(&[("CLAUDE.md", "aaa"), (".mcp.json", "bbb")]);
    let served = a_manifest_of(&[("CLAUDE.md", "zzz"), ("AGENTS.md", "ccc")]);

    // When
    let diff = diff_manifests(&held, &served);

    // Then
    for path in &diff.fetch {
        assert!(
            !diff.delete.contains(path),
            "{path} is in both fetch and delete"
        );
    }
    assert_eq!(
        diff.fetch,
        vec!["AGENTS.md".to_string(), "CLAUDE.md".to_string()]
    );
    assert_eq!(diff.delete, vec![".mcp.json".to_string()]);
}

/// Size alone does not trigger a fetch — the hash is the authority. Two files of different lengths
/// cannot share a hash, so a size that disagrees with a matching hash is a bug upstream, and
/// re-reading on it would mean a manifest with a stale size churned the directory forever.
#[test]
fn a_matching_hash_is_not_re_fetched_even_when_the_recorded_size_differs() {
    // Given
    let held = ContextManifest::from_entries(vec![ContextEntry {
        rel_path: "CLAUDE.md".to_string(),
        sha256: "aaa".to_string(),
        size_bytes: 10,
    }]);
    let served = ContextManifest::from_entries(vec![ContextEntry {
        rel_path: "CLAUDE.md".to_string(),
        sha256: "aaa".to_string(),
        size_bytes: 999,
    }]);

    // When
    let diff = diff_manifests(&held, &served);

    // Then
    assert_eq!(diff.fetch, Vec::<String>::new());
}
