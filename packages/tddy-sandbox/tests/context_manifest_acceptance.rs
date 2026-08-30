//! Acceptance tests: the context manifest and the diff that drives re-sync — AC15, AC17, AC19,
//! AC26-AC29.
//!
//! Re-sync is a manifest diff, not a re-transfer. On each `worktree.activity` broadcast the syncer
//! fetches `(rel_path, sha256, size_bytes)` for every allow-listed path and applies the minimum:
//! read what moved, delete what vanished, transfer nothing when nothing changed. The same decision
//! procedure serves both halves — the split path builds the remote manifest over RPC, the
//! co-located path reads it straight off the worktree beside it.
//!
//! Deliberately not under test here: the RPC that carries the manifest across the peer link
//! (tddy-daemon) and the broadcast that triggers a tick (tddy-daemon).
//!
//! PRD: docs/ft/daemon/agent-context-sync.md § Acceptance Criteria.

use std::path::Path;

use pretty_assertions::assert_eq;
use tddy_sandbox::{
    clear_context_stale, diff_manifests, mark_context_stale, ContextEntry, ContextManifest,
};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const CLAUDE_GLOBS: &[&str] = &[
    "CLAUDE.md",
    "AGENTS.md",
    ".claude/**",
    ".mcp.json",
    ".agents/**",
];

/// The per-file cap a manifest is built under, wide enough that no fixture here brushes it. A test
/// that is *about* the cap names its own, so the two concerns never quietly swap places.
const A_GENEROUS_CAP: u64 = 64 * 1024 * 1024;

struct ATargetRepo {
    dir: tempfile::TempDir,
}

fn a_target_repo() -> ATargetRepo {
    ATargetRepo {
        dir: tempfile::tempdir().expect("tempdir"),
    }
}

impl ATargetRepo {
    fn with_file(self, rel_path: &str, contents: &str) -> Self {
        let path = self.dir.path().join(rel_path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, contents).expect("write");
        self
    }

    /// Symlink `link_rel` at `target_rel`, both spelled relative to the repo root.
    #[cfg(unix)]
    fn with_symlink(self, link_rel: &str, target_rel: &str) -> Self {
        let link = self.dir.path().join(link_rel);
        std::fs::create_dir_all(link.parent().expect("parent")).expect("mkdir");
        std::os::unix::fs::symlink(self.dir.path().join(target_rel), link).expect("symlink");
        self
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn manifest(&self) -> ContextManifest {
        self.manifest_capped_at(A_GENEROUS_CAP)
    }

    fn manifest_capped_at(&self, max_bytes: u64) -> ContextManifest {
        ContextManifest::of_worktree(self.path(), CLAUDE_GLOBS, max_bytes)
            .expect("manifest must build")
    }
}

/// A manifest assembled by hand, for the diff tests where no filesystem is involved.
fn a_manifest_of(entries: &[(&str, &str)]) -> ContextManifest {
    ContextManifest::from_entries(
        entries
            .iter()
            .map(|(rel_path, sha256)| ContextEntry {
                rel_path: (*rel_path).to_string(),
                sha256: (*sha256).to_string(),
                size_bytes: 1,
            })
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
// AC15, AC17, AC19 — what the manifest lists
// ---------------------------------------------------------------------------

/// AC15. Every allow-listed path is listed once, with the hash that decides whether it moved and
/// the size that lets a client refuse an over-cap file before spending a read on it.
#[test]
fn the_manifest_lists_every_allow_listed_path_with_its_hash_and_size() {
    // Given
    let repo = a_target_repo()
        .with_file("CLAUDE.md", "# rules\n")
        .with_file(".claude/settings.json", "{}\n");

    // When
    let manifest = repo.manifest();

    // Then
    assert_eq!(
        paths_of(&manifest),
        vec![".claude/settings.json", "CLAUDE.md"]
    );
    for entry in manifest.entries() {
        assert_eq!(
            entry.sha256.len(),
            64,
            "{} must carry a hex sha256, got {:?}",
            entry.rel_path,
            entry.sha256
        );
        assert!(
            entry.size_bytes > 0,
            "{} must carry its on-disk size",
            entry.rel_path
        );
    }
}

/// AC15. The hash is of the file's content, so two identical files hash alike and an edit changes
/// the hash — that is the whole basis on which re-sync decides to transfer anything.
#[test]
fn editing_a_file_changes_its_hash_in_the_manifest() {
    // Given
    let repo = a_target_repo().with_file("CLAUDE.md", "# rules\n");
    let before = repo.manifest();

    // When
    let repo = repo.with_file("CLAUDE.md", "# rules\n\nAnd one more.\n");
    let after = repo.manifest();

    // Then
    assert_ne!(
        before.entries()[0].sha256,
        after.entries()[0].sha256,
        "an edited file must hash differently, or re-sync never notices it"
    );
}

/// AC17. A file the globs do not name is absent from the manifest, so it is never even considered
/// for transfer — the target repo's source and prose stay where they are.
#[test]
fn a_path_matching_no_glob_is_absent_from_the_manifest() {
    // Given
    let repo = a_target_repo()
        .with_file("CLAUDE.md", "# rules\n")
        .with_file("README.md", "# project\n")
        .with_file("src/main.rs", "fn main() {}\n")
        .with_file("docs/overview.md", "# overview\n");

    // When
    let manifest = repo.manifest();

    // Then
    assert_eq!(paths_of(&manifest), vec!["CLAUDE.md"]);
}

/// AC19. A symlink under an allow-listed path that resolves outside the worktree is not listed —
/// the manifest must never advertise a path the reader would then refuse, nor one that would leak
/// a file from outside the repo.
#[cfg(unix)]
#[test]
fn a_symlink_escaping_the_worktree_is_not_listed_in_the_manifest() {
    // Given
    let outside = tempfile::tempdir().expect("tempdir");
    std::fs::write(outside.path().join("secret.md"), "not yours\n").expect("write");
    let repo = a_target_repo().with_file(".claude/settings.json", "{}\n");
    std::os::unix::fs::symlink(
        outside.path().join("secret.md"),
        repo.path().join(".claude/escape.md"),
    )
    .expect("symlink");

    // When
    let manifest = repo.manifest();

    // Then
    assert_eq!(paths_of(&manifest), vec![".claude/settings.json"]);
}

/// AC19, the case containment alone never asked about. `.claude/creds -> ../.env` resolves *inside*
/// the worktree, so a guard that only checks `starts_with(root)` publishes `.env`'s bytes under a
/// name every `.claude/**` consumer happily accepts — and on the split path those bytes cross to
/// another host and land in the agent's readable working directory. The whole security argument for
/// replacing the git-listing gate with an allow-list is that `.env` is unreachable, so the link's
/// *target* has to be allow-listed too.
#[cfg(unix)]
#[test]
fn a_symlink_to_a_sibling_the_allow_list_does_not_name_is_not_listed_in_the_manifest() {
    // Given
    let repo = a_target_repo()
        .with_file(".env", "AWS_SECRET_ACCESS_KEY=hunter2\n")
        .with_file(".claude/settings.json", "{}\n")
        .with_symlink(".claude/creds", ".env");

    // When
    let manifest = repo.manifest();

    // Then
    assert_eq!(
        paths_of(&manifest),
        vec![".claude/settings.json"],
        "a link whose target the allow-list does not name must not be advertised"
    );
}

/// AC19's other side: the guard rejects the link's *target*, not links as such. A tree that keeps
/// one copy of a skill and reaches it from both conventional places is still listed under both
/// names, because both ends of that link are allow-listed.
#[cfg(unix)]
#[test]
fn a_symlink_between_two_allow_listed_places_is_still_followed() {
    // Given
    let repo = a_target_repo()
        .with_file(".agents/skills/tdd/SKILL.md", "# TDD skill\n")
        .with_symlink(".claude/tdd-SKILL.md", ".agents/skills/tdd/SKILL.md");

    // When
    let manifest = repo.manifest();

    // Then
    assert_eq!(
        paths_of(&manifest),
        vec![".agents/skills/tdd/SKILL.md", ".claude/tdd-SKILL.md"],
        "both ends are allow-listed, so the link is guidance and must sync"
    );
}

/// A file over the cap is left out rather than advertised: the reader that serves the bytes refuses
/// over the same cap before its first frame, so a manifest that named it would describe a fetch
/// that cannot complete — and at setup that is a session which cannot start at all.
#[test]
fn a_file_over_the_cap_is_left_out_of_the_manifest_the_reader_would_refuse_it_from() {
    // Given
    let repo = a_target_repo()
        .with_file("CLAUDE.md", "# rules\n")
        .with_file(".claude/huge.bin", &"x".repeat(4096));

    // When
    let manifest = repo.manifest_capped_at(1024);

    // Then
    assert_eq!(
        paths_of(&manifest),
        vec!["CLAUDE.md"],
        "an over-cap path must not be promised to a reader that will refuse it"
    );
}

// ---------------------------------------------------------------------------
// AC26-AC27 — the diff transfers the minimum
// ---------------------------------------------------------------------------

/// AC26. Nothing moved, so nothing is fetched. Steady state is one manifest round trip carrying no
/// file content at all.
#[test]
fn an_unchanged_manifest_asks_for_no_transfer_and_no_deletion() {
    // Given
    let held = a_manifest_of(&[("CLAUDE.md", "aaa"), (".claude/settings.json", "bbb")]);
    let served = a_manifest_of(&[("CLAUDE.md", "aaa"), (".claude/settings.json", "bbb")]);

    // When
    let diff = diff_manifests(&held, &served);

    // Then
    assert_eq!(diff.fetch, Vec::<String>::new());
    assert_eq!(diff.delete, Vec::<String>::new());
}

/// AC26. Only the path whose hash moved is read back; the one that did not is left alone.
#[test]
fn only_the_path_whose_hash_changed_is_fetched() {
    // Given
    let held = a_manifest_of(&[("CLAUDE.md", "aaa"), (".claude/settings.json", "bbb")]);
    let served = a_manifest_of(&[("CLAUDE.md", "zzz"), (".claude/settings.json", "bbb")]);

    // When
    let diff = diff_manifests(&held, &served);

    // Then
    assert_eq!(diff.fetch, vec!["CLAUDE.md".to_string()]);
    assert_eq!(diff.delete, Vec::<String>::new());
}

/// AC26. A path the context dir has never seen is fetched, not mistaken for unchanged.
#[test]
fn a_path_that_is_new_to_the_context_dir_is_fetched() {
    // Given
    let held = a_manifest_of(&[("CLAUDE.md", "aaa")]);
    let served = a_manifest_of(&[("CLAUDE.md", "aaa"), (".claude/skills/tdd/SKILL.md", "ccc")]);

    // When
    let diff = diff_manifests(&held, &served);

    // Then
    assert_eq!(diff.fetch, vec![".claude/skills/tdd/SKILL.md".to_string()]);
}

/// AC27. A file deleted from the target repo is deleted from the context dir. Leaving it behind
/// would have the agent obeying a rule the project has retracted — the failure mode this feature
/// exists to close, arriving by the back door.
#[test]
fn a_path_that_vanished_from_the_repo_is_deleted_from_the_context_dir() {
    // Given
    let held = a_manifest_of(&[("CLAUDE.md", "aaa"), (".claude/settings.json", "bbb")]);
    let served = a_manifest_of(&[("CLAUDE.md", "aaa")]);

    // When
    let diff = diff_manifests(&held, &served);

    // Then
    assert_eq!(diff.fetch, Vec::<String>::new());
    assert_eq!(diff.delete, vec![".claude/settings.json".to_string()]);
}

// ---------------------------------------------------------------------------
// AC28-AC29 — staleness is visible to the agent
// ---------------------------------------------------------------------------

/// AC28. A re-sync that fails leaves the session running — dropping a working session over a
/// transient link failure would be worse than the staleness — but the agent is told, rather than
/// trusting guidance that may have drifted.
#[test]
fn a_failed_re_sync_marks_the_context_stale_in_the_preamble() {
    // Given
    let ctx = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        ctx.path().join("CLAUDE.md"),
        "## Managed Codebase\n\nThe real codebase is elsewhere.\n\n# Project rules\n",
    )
    .expect("write");

    // When
    mark_context_stale(ctx.path()).expect("mark_context_stale must succeed");

    // Then
    let claude_md = std::fs::read_to_string(ctx.path().join("CLAUDE.md")).expect("read");
    assert!(
        claude_md.contains("STALE"),
        "the preamble must carry a staleness line: {claude_md}"
    );
    assert!(
        claude_md.contains("# Project rules"),
        "marking stale must not disturb the project's own content: {claude_md}"
    );
}

/// AC29. The next successful re-sync clears the line, so a session that recovers stops warning
/// about guidance that is now current.
#[test]
fn a_successful_re_sync_clears_the_staleness_line() {
    // Given
    let ctx = tempfile::tempdir().expect("tempdir");
    let original = "## Managed Codebase\n\nThe real codebase is elsewhere.\n\n# Project rules\n";
    std::fs::write(ctx.path().join("CLAUDE.md"), original).expect("write");
    mark_context_stale(ctx.path()).expect("mark");

    // When
    clear_context_stale(ctx.path()).expect("clear_context_stale must succeed");

    // Then
    let claude_md = std::fs::read_to_string(ctx.path().join("CLAUDE.md")).expect("read");
    assert_eq!(claude_md, original);
}

/// AC28. Marking twice does not stack two warnings — a link that is down for ten ticks must not
/// bury the project's guidance under ten identical lines.
#[test]
fn marking_stale_twice_leaves_exactly_one_warning() {
    // Given
    let ctx = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        ctx.path().join("CLAUDE.md"),
        "## Managed Codebase\n\nThe real codebase is elsewhere.\n",
    )
    .expect("write");

    // When
    mark_context_stale(ctx.path()).expect("mark");
    mark_context_stale(ctx.path()).expect("mark again");

    // Then
    let claude_md = std::fs::read_to_string(ctx.path().join("CLAUDE.md")).expect("read");
    assert_eq!(
        claude_md.matches("STALE").count(),
        1,
        "repeated failures must not stack warnings: {claude_md}"
    );
}
