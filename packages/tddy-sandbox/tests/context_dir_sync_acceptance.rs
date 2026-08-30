//! Acceptance tests: building an agent's context directory from a glob allow-list — AC7-AC14.
//!
//! The context directory is the agent's working directory when the codebase is managed. It carries
//! the target repo's own guidance — whatever that backend's allow-list names — with the
//! managed-codebase preamble **prepended** to `CLAUDE.md` and `AGENTS.md` so the rule that the
//! codebase lives elsewhere is read before the project's own thousands of words.
//!
//! Deliberately not under test here: where the globs come from (tddy-core's table) and how a split
//! session fetches the bytes over the peer link (tddy-daemon). This file pins matching, copying,
//! the preamble and the file modes.
//!
//! PRD: docs/ft/daemon/agent-context-sync.md § Acceptance Criteria.

use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_sandbox::{
    copy_context_from_repo, managed_codebase_preamble, SandboxContextDir, SubagentReplacement,
};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// A target repo with the shape an arbitrary project has: agent config for several tools, the
/// project's own prose, and source that is none of the sync's business.
struct ATargetRepo {
    dir: tempfile::TempDir,
}

fn a_target_repo() -> ATargetRepo {
    let repo = ATargetRepo {
        dir: tempfile::tempdir().expect("tempdir"),
    };
    repo.with_file("CLAUDE.md", "# Project rules\n\nAlways run ./test.\n")
        .with_file("AGENTS.md", "# Agents\n\nUse the TDD flow.\n")
        .with_file(".claude/settings.json", "{\"model\":\"opus\"}\n")
        .with_file(".claude/skills/tdd/SKILL.md", "# TDD skill\n")
        .with_file(".cursor/rules/style.mdc", "# Cursor style\n")
        .with_file(".agents/skills/plan/SKILL.md", "# Plan skill\n")
        .with_file(".mcp.json", "{\"mcpServers\":{}}\n")
        .with_file("README.md", "# The project\n")
        .with_file("src/main.rs", "fn main() {}\n")
        .with_file("docs/architecture/overview.md", "# Overview\n")
}

impl ATargetRepo {
    fn with_file(self, rel_path: &str, contents: &str) -> Self {
        let path = self.path().join(rel_path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, contents).expect("write");
        self
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }
}

/// The globs a Claude session syncs, spelled here rather than imported so this crate's matcher is
/// pinned independently of tddy-core's table.
const CLAUDE_GLOBS: &[&str] = &[
    "CLAUDE.md",
    "AGENTS.md",
    ".claude/**",
    ".mcp.json",
    ".agents/**",
];

const CURSOR_GLOBS: &[&str] = &[
    "CLAUDE.md",
    "AGENTS.md",
    ".claude/**",
    ".cursor/**",
    ".mcp.json",
    ".agents/**",
];

/// Copy `repo` into a fresh destination under `globs` and hand back the destination.
fn a_context_dir_of(repo: &ATargetRepo, globs: &[&str]) -> (tempfile::TempDir, PathBuf) {
    let dest = tempfile::tempdir().expect("tempdir");
    let path = dest.path().to_path_buf();
    copy_context_from_repo(repo.path(), &path, globs).expect("copy_context_from_repo must succeed");
    (dest, path)
}

fn exists_in(dir: &Path, rel_path: &str) -> bool {
    dir.join(rel_path).exists()
}

fn read(dir: &Path, rel_path: &str) -> String {
    std::fs::read_to_string(dir.join(rel_path))
        .unwrap_or_else(|e| panic!("{rel_path} must be readable: {e}"))
}

// ---------------------------------------------------------------------------
// AC7 — the allow-list decides
// ---------------------------------------------------------------------------

/// AC7. A file the backend's globs name is copied; one they do not name is not — including the
/// project's own `README.md` and `docs/`, which the old hardcoded list dragged in wholesale.
#[test]
fn a_file_the_globs_name_is_copied_and_one_they_do_not_is_left_behind() {
    // Given
    let repo = a_target_repo();

    // When
    let (_guard, ctx) = a_context_dir_of(&repo, CLAUDE_GLOBS);

    // Then
    for synced in [
        "CLAUDE.md",
        "AGENTS.md",
        ".claude/settings.json",
        ".mcp.json",
    ] {
        assert!(
            exists_in(&ctx, synced),
            "{synced} matches the allow-list and must be copied"
        );
    }
    for skipped in ["README.md", "src/main.rs", "docs/architecture/overview.md"] {
        assert!(
            !exists_in(&ctx, skipped),
            "{skipped} matches no glob and must not reach the context dir"
        );
    }
}

/// AC7. The allow-list is per backend: `.cursor/` reaches a Cursor session and not a Claude one.
#[test]
fn the_cursor_tree_reaches_a_cursor_session_and_not_a_claude_one() {
    // Given
    let repo = a_target_repo();

    // When
    let (_claude_guard, claude_ctx) = a_context_dir_of(&repo, CLAUDE_GLOBS);
    let (_cursor_guard, cursor_ctx) = a_context_dir_of(&repo, CURSOR_GLOBS);

    // Then
    assert!(
        !exists_in(&claude_ctx, ".cursor/rules/style.mdc"),
        "claude's allow-list does not name .cursor/, so it must not be copied"
    );
    assert!(
        exists_in(&cursor_ctx, ".cursor/rules/style.mdc"),
        "cursor's allow-list names .cursor/, so it must be copied"
    );
}

// ---------------------------------------------------------------------------
// AC8 — `**` reaches all the way down
// ---------------------------------------------------------------------------

/// AC8. A skill three levels below `.claude/` is as much Claude configuration as one directly under
/// it; `**` must not stop at the first directory level.
#[test]
fn a_double_star_glob_matches_a_file_nested_several_directories_deep() {
    // Given
    let repo = a_target_repo().with_file(
        ".claude/skills/fluent-tests/references/rust/std-test.md",
        "# Rust fluent tests\n",
    );

    // When
    let (_guard, ctx) = a_context_dir_of(&repo, CLAUDE_GLOBS);

    // Then
    assert!(
        exists_in(
            &ctx,
            ".claude/skills/fluent-tests/references/rust/std-test.md"
        ),
        ".claude/** must match at any depth, not just the first level"
    );
}

// ---------------------------------------------------------------------------
// AC9 — the containment guard survives the rewrite
// ---------------------------------------------------------------------------

/// AC9. A symlink under an allow-listed path that resolves outside the worktree is skipped — the
/// guard that keeps a `.claude` link to `node_modules` (or to `/etc`) out of the context dir must
/// not be lost when the copier moves from fixed names to globs.
#[cfg(unix)]
#[test]
fn a_symlink_under_an_allow_listed_path_that_escapes_the_repo_is_skipped() {
    // Given
    let outside = tempfile::tempdir().expect("tempdir");
    std::fs::write(outside.path().join("secret.md"), "not yours\n").expect("write");
    let repo = a_target_repo();
    std::os::unix::fs::symlink(
        outside.path().join("secret.md"),
        repo.path().join(".claude/escape.md"),
    )
    .expect("symlink");

    // When
    let (_guard, ctx) = a_context_dir_of(&repo, CLAUDE_GLOBS);

    // Then
    assert!(
        !exists_in(&ctx, ".claude/escape.md"),
        "a symlink resolving outside the worktree root must not be copied"
    );
    assert!(
        exists_in(&ctx, ".claude/settings.json"),
        "the escaping link must not abort the rest of the copy"
    );
}

/// AC9, the half containment alone never covered. `.claude/creds -> ../.env` resolves *inside* the
/// worktree, so a guard that asks only "does the target start with the root?" copies `.env`'s bytes
/// into the agent's working directory under a name `.claude/**` happily matches. The copier feeds on
/// the same walk the manifest does, so the guard has to hold here too — this is the path that puts
/// the file where a co-located agent can read it.
#[cfg(unix)]
#[test]
fn a_symlink_to_a_sibling_the_allow_list_does_not_name_is_not_copied() {
    // Given
    let repo = a_target_repo().with_file(".env", "AWS_SECRET_ACCESS_KEY=hunter2\n");
    std::os::unix::fs::symlink(repo.path().join(".env"), repo.path().join(".claude/creds"))
        .expect("symlink");

    // When
    let (_guard, ctx) = a_context_dir_of(&repo, CLAUDE_GLOBS);

    // Then
    assert!(
        !exists_in(&ctx, ".claude/creds"),
        "a link to a path the allow-list does not name must not be copied, however it is spelled"
    );
    assert!(
        exists_in(&ctx, ".claude/settings.json"),
        "and the rest of the allow-listed tree must still arrive"
    );
}

/// The guard rejects the link's *target*, not links as such: a repo that keeps one copy of a skill
/// and reaches it from both conventional places still syncs it under both names, because both ends
/// are allow-listed.
#[cfg(unix)]
#[test]
fn a_symlink_between_two_allow_listed_places_is_still_copied() {
    // Given
    let repo = a_target_repo();
    std::os::unix::fs::symlink(
        repo.path().join(".agents/skills/plan/SKILL.md"),
        repo.path().join(".claude/plan-SKILL.md"),
    )
    .expect("symlink");

    // When
    let (_guard, ctx) = a_context_dir_of(&repo, CLAUDE_GLOBS);

    // Then
    assert_eq!(
        read(&ctx, ".claude/plan-SKILL.md"),
        "# Plan skill\n",
        "both ends are allow-listed, so the link is guidance and must be copied"
    );
}

// ---------------------------------------------------------------------------
// AC10 — the directory is writable
// ---------------------------------------------------------------------------

/// AC10. Continuous re-sync writes into the live context directory for the life of the session, so
/// nothing in it is frozen at 0444. The agent is kept out by the jail's read-only mount and the
/// native-tool disallowlist, neither of which depends on the file mode.
#[cfg(unix)]
#[test]
fn nothing_in_the_built_context_dir_is_left_read_only() {
    use std::os::unix::fs::PermissionsExt;

    // Given
    let repo = a_target_repo();

    // When
    let ctx = SandboxContextDir::create(repo.path(), CLAUDE_GLOBS).expect("create must succeed");

    // Then
    for entry in walkdir::WalkDir::new(ctx.path()).follow_links(false) {
        let entry = entry.expect("walk");
        if !entry.file_type().is_file() {
            continue;
        }
        let mode = std::fs::metadata(entry.path())
            .expect("metadata")
            .permissions()
            .mode();
        assert!(
            mode & 0o200 != 0,
            "{} is not owner-writable (mode {:o}); re-sync could not update it",
            entry.path().display(),
            mode
        );
    }
}

// ---------------------------------------------------------------------------
// AC11-AC13 — the preamble comes first
// ---------------------------------------------------------------------------

/// AC11. The managed-codebase rule is read before the project's own instructions, not after them.
#[test]
fn claude_md_begins_with_the_managed_codebase_preamble_and_keeps_the_projects_content() {
    // Given
    let repo = a_target_repo();

    // When
    let ctx = SandboxContextDir::create(repo.path(), CLAUDE_GLOBS).expect("create must succeed");
    let claude_md = read(ctx.path(), "CLAUDE.md");

    // Then
    let preamble = managed_codebase_preamble(&[]);
    assert!(
        claude_md.starts_with(preamble.trim_start()),
        "CLAUDE.md must open with the preamble, not close with it; it opens:\n{}",
        &claude_md[..claude_md.len().min(200)]
    );
    assert!(
        claude_md.contains("Always run ./test."),
        "the project's own content must survive intact: {claude_md}"
    );
}

/// AC12. `AGENTS.md` gets the same treatment — a backend that reads only `AGENTS.md` must not be
/// the one backend that misses the rule.
#[test]
fn agents_md_begins_with_the_managed_codebase_preamble_too() {
    // Given
    let repo = a_target_repo();

    // When
    let ctx = SandboxContextDir::create(repo.path(), CLAUDE_GLOBS).expect("create must succeed");
    let agents_md = read(ctx.path(), "AGENTS.md");

    // Then
    assert!(
        agents_md.starts_with(managed_codebase_preamble(&[]).trim_start()),
        "AGENTS.md must open with the preamble; it opens:\n{}",
        &agents_md[..agents_md.len().min(200)]
    );
    assert!(
        agents_md.contains("Use the TDD flow."),
        "the project's own content must survive intact: {agents_md}"
    );
}

/// AC13. A target repo with no `CLAUDE.md` still leaves the agent told where its codebase is. Today
/// the co-located path appends only to files that already exist, so such a repo yields a context
/// directory with no notice at all.
#[test]
fn a_repo_without_a_claude_md_still_gets_one_holding_the_preamble_alone() {
    // Given
    let repo = a_target_repo();
    std::fs::remove_file(repo.path().join("CLAUDE.md")).expect("remove");

    // When
    let ctx = SandboxContextDir::create(repo.path(), CLAUDE_GLOBS).expect("create must succeed");

    // Then
    let claude_md = read(ctx.path(), "CLAUDE.md");
    assert_eq!(claude_md.trim_end(), managed_codebase_preamble(&[]).trim());
}

/// AC14. The context directory is writable and, on a split session, is the agent's only scratch
/// space — so the preamble has to say which paths the sync owns and will replace, rather than
/// letting the agent discover it by losing an edit.
#[test]
fn the_preamble_names_the_synced_paths_as_owned_by_the_sync() {
    // When
    let preamble = managed_codebase_preamble(&[]);

    // Then
    assert!(
        preamble.contains("replaced"),
        "the preamble must say edits under the synced paths are replaced: {preamble}"
    );
    for owned in ["CLAUDE.md", "AGENTS.md"] {
        assert!(
            preamble.contains(owned),
            "the preamble must name {owned} as sync-owned: {preamble}"
        );
    }
}

/// AC14. Naming a replacing subagent still works — the preamble keeps every job the appendix did,
/// it only changes where it sits.
#[test]
fn the_preamble_still_names_the_subagent_a_withdrawn_tool_went_to() {
    // When
    let preamble = managed_codebase_preamble(&[SubagentReplacement {
        name: "explorer",
        replaced: &["Grep", "Glob"],
    }]);

    // Then
    assert!(
        preamble.contains("explorer"),
        "the preamble must name the replacing subagent: {preamble}"
    );
}
