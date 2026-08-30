//! Acceptance tests: the allow-list-gated context reader — AC16, AC18, AC20-AC22.
//!
//! This reader is deliberately **not** `worktree_files`. That one gates on git's listing, to keep a
//! `.gitignore`d `.env` or private key unreadable — and agent config is routinely gitignored (a
//! work-in-progress skill under `.claude/skills/`, `**/.cursor/mcp.json`). So this reader replaces
//! the git gate with the compiled-in per-backend allow-list, and keeps every traversal and
//! containment guard its sibling applies.
//!
//! One path the allow-list names is withheld anyway — `.claude/settings.local.json`, which the
//! daemon writes its Claude Code hooks into — and that exclusion is pinned here too.
//!
//! What makes swapping the gate safe is that **no caller supplies the globs**: they are compiled in
//! and selected by the session's agent, so no request can widen the readable set to reach `.env`.
//!
//! Deliberately not under test here: the RPC plumbing that carries these bytes across the peer link
//! (that is `remote_managed_worktree_cross_host_acceptance.rs`) and the glob table itself (that is
//! tddy-core).
//!
//! PRD: docs/ft/daemon/agent-context-sync.md § Acceptance Criteria.

use std::path::Path;
use std::process::Command;

use pretty_assertions::assert_eq;
use tddy_daemon::context_files::{
    context_manifest, read_context_file_bytes, read_context_files_bytes,
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

const A_GENEROUS_CAP: u64 = 64 * 1024 * 1024;

/// A real git worktree, because the point of this reader is what it does *differently* from the
/// git-gated one — a fake filesystem could not show that.
struct AWorktree {
    dir: tempfile::TempDir,
}

fn a_worktree() -> AWorktree {
    let worktree = AWorktree {
        dir: tempfile::tempdir().expect("tempdir"),
    };
    worktree.git(&["init", "--initial-branch=main"]);
    worktree.git(&["config", "user.email", "test@example.invalid"]);
    worktree.git(&["config", "user.name", "Test"]);
    worktree
}

impl AWorktree {
    fn git(&self, args: &[&str]) -> &Self {
        let out = Command::new("git")
            .args(args)
            .current_dir(self.path())
            .output()
            .expect("git must run");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        self
    }

    fn with_file(&self, rel_path: &str, contents: &[u8]) -> &Self {
        let path = self.path().join(rel_path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, contents).expect("write");
        self
    }

    /// A file git deliberately does not track — the case the sibling reader refuses outright.
    fn with_gitignored_file(&self, rel_path: &str, contents: &[u8]) -> &Self {
        self.with_file(rel_path, contents);
        let gitignore = self.path().join(".gitignore");
        let mut existing = std::fs::read_to_string(&gitignore).unwrap_or_default();
        existing.push_str(rel_path);
        existing.push('\n');
        std::fs::write(&gitignore, existing).expect("write .gitignore");
        self
    }

    fn committed(&self) -> &Self {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-m", "fixture"]);
        self
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }
}

fn listed_paths(worktree: &AWorktree) -> Vec<String> {
    context_manifest(worktree.path(), CLAUDE_GLOBS, A_GENEROUS_CAP)
        .expect("manifest must build")
        .into_iter()
        .map(|e| e.rel_path)
        .collect()
}

// ---------------------------------------------------------------------------
// AC16 — gitignored agent config is served
// ---------------------------------------------------------------------------

/// AC16. The whole reason this reader exists. A locally-developed skill under `.claude/skills/` is
/// gitignored as often as not — it is work in progress, or machine-specific — and the git-gated
/// reader refuses every such path, so a synced context dir built on that reader would omit exactly
/// the guidance a developer is actively writing.
///
/// The path used to be `.claude/settings.local.json`; it moved because that one file is now
/// deliberately withheld (see `the_settings_file_the_daemon_owns_is_never_served`). What AC16 is
/// about — a *gitignored* allow-listed path being reachable — is unchanged and still pinned here.
#[test]
fn a_gitignored_path_the_allow_list_names_is_served() {
    // Given
    let worktree = a_worktree();
    worktree
        .with_file("CLAUDE.md", b"# rules\n")
        .with_gitignored_file(".claude/skills/local/SKILL.md", b"# work in progress\n")
        .committed();

    // When
    let paths = listed_paths(&worktree);

    // Then
    assert!(
        paths.contains(&".claude/skills/local/SKILL.md".to_string()),
        "a gitignored allow-listed path must be served; got {paths:?}"
    );
}

/// AC16. And its bytes come back, not merely its name.
#[test]
fn the_bytes_of_a_gitignored_allow_listed_path_come_back() {
    // Given
    let worktree = a_worktree();
    worktree
        .with_gitignored_file(".claude/skills/local/SKILL.md", b"# work in progress\n")
        .committed();

    // When
    let bytes = read_context_file_bytes(
        worktree.path(),
        ".claude/skills/local/SKILL.md",
        CLAUDE_GLOBS,
        A_GENEROUS_CAP,
    )
    .expect("read must succeed");

    // Then
    assert_eq!(bytes, b"# work in progress\n".to_vec());
}

/// `.claude/settings.local.json` is named by `.claude/**` and is nevertheless **not** served, for a
/// reason that has nothing to do with git: on a managed session the daemon owns that file.
/// `write_claude_hooks_settings` renders the six Claude Code hooks that report the session's status
/// and writes them to exactly that path in the agent's working directory, as a whole-file atomic
/// replace with no merge. Serving the repository's copy only decides which write lands last — and
/// when the repo's copy lands last, status reporting stops with nothing saying so.
#[test]
fn the_settings_file_the_daemon_owns_is_never_served() {
    // Given
    let worktree = a_worktree();
    worktree
        .with_gitignored_file(".claude/settings.local.json", b"{\"model\":\"opus\"}\n")
        .with_file(".claude/settings.json", b"{}\n")
        .committed();

    // When
    let paths = listed_paths(&worktree);
    let refusal = read_context_file_bytes(
        worktree.path(),
        ".claude/settings.local.json",
        CLAUDE_GLOBS,
        A_GENEROUS_CAP,
    );

    // Then
    assert!(
        !paths.contains(&".claude/settings.local.json".to_string()),
        "the file the daemon writes its hooks into must not be advertised; got {paths:?}"
    );
    assert_eq!(
        refusal
            .expect_err("the daemon-owned settings file must not be readable")
            .code(),
        tddy_rpc::Code::PermissionDenied
    );
    assert!(
        paths.contains(&".claude/settings.json".to_string()),
        "the exclusion must cost the project nothing else under .claude/; got {paths:?}"
    );
}

/// AC16. A gitignored path the allow-list does **not** name stays unreadable. Replacing the git
/// gate must not become a way to read `.env`.
#[test]
fn a_gitignored_path_the_allow_list_does_not_name_stays_refused() {
    // Given
    let worktree = a_worktree();
    worktree
        .with_gitignored_file(".env", b"SECRET=hunter2\n")
        .with_file("CLAUDE.md", b"# rules\n")
        .committed();

    // When
    let refusal = read_context_file_bytes(worktree.path(), ".env", CLAUDE_GLOBS, A_GENEROUS_CAP);

    // Then
    assert_eq!(
        refusal.expect_err("reading .env must be refused").code(),
        tddy_rpc::Code::PermissionDenied
    );
    assert!(
        !listed_paths(&worktree).contains(&".env".to_string()),
        "the manifest must not advertise .env either"
    );
}

// ---------------------------------------------------------------------------
// AC17 — a tracked path matching nothing is still refused
// ---------------------------------------------------------------------------

/// AC17. Being tracked by git buys a path nothing here: the allow-list is the only gate, and it
/// names agent configuration, not source.
#[test]
fn a_tracked_path_matching_no_glob_is_refused() {
    // Given
    let worktree = a_worktree();
    worktree
        .with_file("src/main.rs", b"fn main() {}\n")
        .committed();

    // When
    let refusal =
        read_context_file_bytes(worktree.path(), "src/main.rs", CLAUDE_GLOBS, A_GENEROUS_CAP);

    // Then
    assert_eq!(
        refusal
            .expect_err("a tracked non-config file must be refused")
            .code(),
        tddy_rpc::Code::PermissionDenied
    );
}

// ---------------------------------------------------------------------------
// AC18-AC19 — the containment guards
// ---------------------------------------------------------------------------

/// AC18. Traversal and absolute paths are refused before anything touches the filesystem.
#[rstest::rstest]
#[case("../../../etc/passwd")]
#[case(".claude/../../escape.md")]
#[case("/etc/passwd")]
#[case(".claude/./../../outside.md")]
fn a_traversing_or_absolute_path_is_refused(#[case] rel_path: &str) {
    // Given
    let worktree = a_worktree();
    worktree.with_file("CLAUDE.md", b"# rules\n").committed();

    // When
    let refusal = read_context_file_bytes(worktree.path(), rel_path, CLAUDE_GLOBS, A_GENEROUS_CAP);

    // Then
    assert!(refusal.is_err(), "{rel_path} must be refused, not resolved");
}

/// AC19. A symlink that matches a glob but resolves outside the worktree is not served — matching
/// the pattern is not the same as living in the repo.
#[cfg(unix)]
#[test]
fn a_symlink_matching_a_glob_but_escaping_the_worktree_is_not_served() {
    // Given
    let outside = tempfile::tempdir().expect("tempdir");
    std::fs::write(outside.path().join("secret.md"), b"not yours\n").expect("write");
    let worktree = a_worktree();
    worktree.with_file(".claude/settings.json", b"{}\n");
    std::os::unix::fs::symlink(
        outside.path().join("secret.md"),
        worktree.path().join(".claude/escape.md"),
    )
    .expect("symlink");
    worktree.committed();

    // When
    let refusal = read_context_file_bytes(
        worktree.path(),
        ".claude/escape.md",
        CLAUDE_GLOBS,
        A_GENEROUS_CAP,
    );

    // Then
    assert!(
        refusal.is_err(),
        "a symlink resolving outside the worktree must not be served"
    );
    assert!(
        !listed_paths(&worktree).contains(&".claude/escape.md".to_string()),
        "nor advertised in the manifest"
    );
}

/// A worktree whose `.claude/` holds two symlinks, each pointing at a file *inside* the repository
/// that the allow-list does not name: `.claude/creds -> ../.env`, the gitignored secret, and
/// `.claude/alias.json -> settings.local.json`, the hooks file `CONTEXT_EXCLUDE_GLOBS` withholds.
///
/// Both resolve squarely under the worktree root, so containment — the guard the escaping-symlink
/// test above pins — has nothing to say about either of them.
#[cfg(unix)]
fn a_worktree_whose_symlinks_alias_files_the_allow_list_withholds() -> AWorktree {
    let worktree = a_worktree();
    worktree
        .with_file("CLAUDE.md", b"# rules\n")
        .with_file(".claude/settings.json", b"{}\n")
        .with_gitignored_file(".env", b"SECRET=hunter2\n")
        .with_file(
            ".claude/settings.local.json",
            b"{\"hooks\":\"the daemon's\"}\n",
        );
    std::os::unix::fs::symlink("../.env", worktree.path().join(".claude/creds")).expect("symlink");
    std::os::unix::fs::symlink(
        "settings.local.json",
        worktree.path().join(".claude/alias.json"),
    )
    .expect("symlink");
    worktree.committed();
    worktree
}

/// AC19, at the **reader**. A symlink is followed only when both ends are allow-listed: the name it
/// is reached by *and* the place its target sits in the tree. Checking the requested name alone and
/// then only containment lets an attacker publish any file in the checkout under a name every glob
/// happily matches — `.claude/creds` hands back `.env`, and `.claude/alias.json` hands back the
/// `settings.local.json` the daemon owns, defeating the exclusion list outright. On the split path
/// those bytes then cross to another host into the agent's readable working directory.
///
/// The walk enforced this from the start; the reader did not, which is why it is pinned here as
/// well as in `context_manifest_acceptance`. The escaping-symlink test above covers only a target
/// *outside* the root, and neither of these leaves it.
#[cfg(unix)]
#[rstest::rstest]
#[case::a_link_to_the_gitignored_secret(".claude/creds", "SECRET")]
#[case::a_link_to_the_settings_file_the_daemon_owns(".claude/alias.json", "hooks")]
fn a_symlink_to_a_file_the_allow_list_does_not_name_is_not_served(
    #[case] rel_path: &str,
    #[case] giveaway: &str,
) {
    // Given
    let worktree = a_worktree_whose_symlinks_alias_files_the_allow_list_withholds();

    // When
    let refusal = read_context_file_bytes(worktree.path(), rel_path, CLAUDE_GLOBS, A_GENEROUS_CAP);

    // Then
    let served = refusal.map(|bytes| String::from_utf8_lossy(&bytes).to_string());
    assert_eq!(
        served
            .expect_err(&format!(
                "{rel_path} resolves to a file the allow-list does not name, whose contents carry \
                 {giveaway:?}; it must be refused rather than served"
            ))
            .code(),
        tddy_rpc::Code::PermissionDenied
    );
}

/// The batch reader is the single reader's loop body, not a second reader with its own idea of what
/// may be read — so the same alias is refused there, and the refusal fails the *whole* batch rather
/// than serving the paths around it. This is the path a split session's setup sync actually takes.
#[cfg(unix)]
#[rstest::rstest]
#[case::a_link_to_the_gitignored_secret(".claude/creds")]
#[case::a_link_to_the_settings_file_the_daemon_owns(".claude/alias.json")]
fn a_batch_naming_a_symlink_to_a_file_the_allow_list_does_not_name_is_refused(
    #[case] rel_path: &str,
) {
    // Given
    let worktree = a_worktree_whose_symlinks_alias_files_the_allow_list_withholds();

    // When
    let refusal = read_context_files_bytes(
        worktree.path(),
        &["CLAUDE.md".to_string(), rel_path.to_string()],
        CLAUDE_GLOBS,
        A_GENEROUS_CAP,
    );

    // Then
    assert_eq!(
        refusal
            .map(|files| files.len())
            .expect_err("a batch naming an aliased path must be refused whole")
            .code(),
        tddy_rpc::Code::PermissionDenied,
        "{rel_path} resolves to a path the allow-list does not name"
    );
}

/// And the manifest never advertised either of them, so the two halves agree: nothing is listed
/// that the reader would refuse, and nothing is served that the manifest omits.
#[cfg(unix)]
#[test]
fn the_manifest_omits_the_symlinks_whose_targets_the_allow_list_does_not_name() {
    // Given
    let worktree = a_worktree_whose_symlinks_alias_files_the_allow_list_withholds();

    // When
    let paths = listed_paths(&worktree);

    // Then
    assert!(
        !paths.contains(&".claude/creds".to_string())
            && !paths.contains(&".claude/alias.json".to_string()),
        "an aliased path must not be advertised; got {paths:?}"
    );
}

// ---------------------------------------------------------------------------
// AC20 — byte-exact reads
// ---------------------------------------------------------------------------

/// AC20. Content comes back byte for byte, with no encoding applied at any point — a `.claude/`
/// tree may hold a PNG, or a file in an encoding that is not UTF-8, and a mangled config is worse
/// than a missing one.
#[test]
fn a_non_utf8_file_round_trips_byte_for_byte() {
    // Given
    let raw: Vec<u8> = vec![0xff, 0xfe, 0x00, 0x01, 0x80, 0x7f];
    let worktree = a_worktree();
    worktree.with_file(".claude/logo.png", &raw).committed();

    // When
    let bytes = read_context_file_bytes(
        worktree.path(),
        ".claude/logo.png",
        CLAUDE_GLOBS,
        A_GENEROUS_CAP,
    )
    .expect("read must succeed");

    // Then
    assert_eq!(bytes, raw);
}

/// AC20. An empty allow-listed file reads as empty rather than as an error, so "the file is empty"
/// stays distinguishable from "the read failed".
#[test]
fn an_empty_allow_listed_file_reads_as_empty_rather_than_failing() {
    // Given
    let worktree = a_worktree();
    worktree.with_file(".claude/empty.json", b"").committed();

    // When
    let bytes = read_context_file_bytes(
        worktree.path(),
        ".claude/empty.json",
        CLAUDE_GLOBS,
        A_GENEROUS_CAP,
    )
    .expect("read must succeed");

    // Then
    assert_eq!(bytes, Vec::<u8>::new());
}

// ---------------------------------------------------------------------------
// AC21 — refusal leaks no existence map
// ---------------------------------------------------------------------------

/// AC21. A path matching no glob is refused with the same code whether or not a file sits there.
/// Answering "does it exist?" before "may you see it?" would keep contents secret while handing out
/// the existence map — the property `resolve_listed_worktree_file` protects, preserved here.
#[test]
fn a_path_outside_the_allow_list_is_refused_identically_whether_or_not_it_exists() {
    // Given
    let worktree = a_worktree();
    worktree.with_file("secrets.txt", b"hunter2\n").committed();

    // When
    let present =
        read_context_file_bytes(worktree.path(), "secrets.txt", CLAUDE_GLOBS, A_GENEROUS_CAP);
    let absent = read_context_file_bytes(
        worktree.path(),
        "no-such-file.txt",
        CLAUDE_GLOBS,
        A_GENEROUS_CAP,
    );

    // Then
    let present = present.expect_err("an unlisted present file must be refused");
    let absent = absent.expect_err("an unlisted absent file must be refused");
    assert_eq!(present.code(), absent.code());
    assert_eq!(present.message(), absent.message());
}

// ---------------------------------------------------------------------------
// AC22 — over-cap is refused, never truncated
// ---------------------------------------------------------------------------

/// AC22. An over-cap file is refused before a single frame exists. A caller cannot tell a truncated
/// file from a whole one once the frames have started, and a truncated `CLAUDE.md` is a wrong
/// `CLAUDE.md` — it would silently drop the project's last rule.
#[test]
fn a_file_over_the_cap_is_refused_rather_than_truncated() {
    // Given
    let worktree = a_worktree();
    worktree
        .with_file(".claude/huge.md", &vec![b'x'; 4096])
        .committed();

    // When
    let refusal = read_context_file_bytes(worktree.path(), ".claude/huge.md", CLAUDE_GLOBS, 1024);

    // Then
    let refusal = refusal.expect_err("an over-cap file must be refused");
    assert!(
        refusal.message().contains("4096") || refusal.message().contains("1024"),
        "the refusal must name the size that blew the cap: {}",
        refusal.message()
    );
}

/// AC22. And the cap is measured before the read, so asking for an enormous file costs nothing but
/// the `stat`.
#[test]
fn a_file_exactly_at_the_cap_is_served() {
    // Given
    let worktree = a_worktree();
    worktree
        .with_file(".claude/exact.md", &vec![b'x'; 1024])
        .committed();

    // When
    let bytes = read_context_file_bytes(worktree.path(), ".claude/exact.md", CLAUDE_GLOBS, 1024)
        .expect("a file exactly at the cap must be served");

    // Then
    assert_eq!(bytes.len(), 1024);
}

// ---------------------------------------------------------------------------
// The batched read — one call for the whole setup prefetch
// ---------------------------------------------------------------------------

/// Populating a split session's context directory reads every allow-listed path before the agent
/// process exists. One call per path made that 1 + N sequential peer round trips; this is the
/// reader behind the single call that replaces them, and its contract is the single-file reader's
/// applied to each path — byte-exact content, in the order asked for.
#[test]
fn a_batch_returns_every_requested_file_byte_for_byte() {
    // Given
    let worktree = a_worktree();
    worktree
        .with_file("CLAUDE.md", b"# rules\n")
        .with_file(".claude/settings.json", b"{\"a\":1}\n")
        .with_file(".claude/skills/tdd/SKILL.md", b"# tdd\n")
        .committed();

    // When
    let files = read_context_files_bytes(
        worktree.path(),
        &[
            "CLAUDE.md".to_string(),
            ".claude/settings.json".to_string(),
            ".claude/skills/tdd/SKILL.md".to_string(),
        ],
        CLAUDE_GLOBS,
        A_GENEROUS_CAP,
    )
    .expect("the batch must be served");

    // Then
    assert_eq!(
        files,
        vec![
            ("CLAUDE.md".to_string(), b"# rules\n".to_vec()),
            (".claude/settings.json".to_string(), b"{\"a\":1}\n".to_vec()),
            (
                ".claude/skills/tdd/SKILL.md".to_string(),
                b"# tdd\n".to_vec()
            ),
        ]
    );
}

/// A zero-byte file in the middle of a batch comes back as a zero-byte file, not as an absence. The
/// single-file path holds the same rule, and it is what keeps "the project ships an empty
/// `.mcp.json`" distinguishable from "that file never arrived" — a distinction the whole batch's
/// completeness check rests on.
#[test]
fn an_empty_file_in_a_batch_comes_back_empty_rather_than_missing() {
    // Given
    let worktree = a_worktree();
    worktree
        .with_file("CLAUDE.md", b"# rules\n")
        .with_file(".mcp.json", b"")
        .committed();

    // When
    let files = read_context_files_bytes(
        worktree.path(),
        &["CLAUDE.md".to_string(), ".mcp.json".to_string()],
        CLAUDE_GLOBS,
        A_GENEROUS_CAP,
    )
    .expect("the batch must be served");

    // Then
    assert_eq!(
        files[1],
        (".mcp.json".to_string(), Vec::<u8>::new()),
        "an empty allow-listed file must be carried as empty"
    );
}

/// One unlisted path fails the whole call. Serving the rest would leave the caller unable to tell
/// "the project does not ship that file" from "this host would not serve it", and the setup sync
/// this feeds must fail loudly rather than start an agent against guidance with a hole in it.
#[test]
fn a_batch_naming_one_unlisted_path_serves_none_of_it() {
    // Given
    let worktree = a_worktree();
    worktree
        .with_file("CLAUDE.md", b"# rules\n")
        .with_gitignored_file(".env", b"SECRET=hunter2\n")
        .committed();

    // When
    let refusal = read_context_files_bytes(
        worktree.path(),
        &["CLAUDE.md".to_string(), ".env".to_string()],
        CLAUDE_GLOBS,
        A_GENEROUS_CAP,
    );

    // Then
    assert_eq!(
        refusal
            .expect_err("a batch naming .env must be refused whole")
            .code(),
        tddy_rpc::Code::PermissionDenied
    );
}

/// The aggregate cap refuses before any bytes are read. Each of these files clears the per-file cap
/// comfortably; together they do not. Without the aggregate bound a caller could name a thousand
/// just-under-cap files and choose this host's allocation size.
#[test]
fn a_batch_over_the_aggregate_cap_is_refused_before_a_byte_is_read() {
    // Given
    let worktree = a_worktree();
    worktree
        .with_file("CLAUDE.md", &vec![b'x'; 600])
        .with_file("AGENTS.md", &vec![b'y'; 600])
        .committed();

    // When
    let refusal = read_context_files_bytes(
        worktree.path(),
        &["CLAUDE.md".to_string(), "AGENTS.md".to_string()],
        CLAUDE_GLOBS,
        1024,
    );

    // Then
    let refusal = refusal.expect_err("a batch over the aggregate cap must be refused");
    assert_eq!(refusal.code(), tddy_rpc::Code::InvalidArgument);
    assert!(
        refusal.message().contains("1024"),
        "the refusal must name the cap it blew: {}",
        refusal.message()
    );
}

/// A batch that names nothing is a round trip spent on nothing, and a caller that thinks it asked
/// for files would read the empty answer as "the project ships none".
#[test]
fn a_batch_naming_no_paths_is_refused() {
    // Given
    let worktree = a_worktree();
    worktree.with_file("CLAUDE.md", b"# rules\n").committed();

    // When
    let refusal = read_context_files_bytes(worktree.path(), &[], CLAUDE_GLOBS, A_GENEROUS_CAP);

    // Then
    assert_eq!(
        refusal.expect_err("an empty batch must be refused").code(),
        tddy_rpc::Code::InvalidArgument
    );
}

/// A repeated path would arrive as two runs of frames under one name — the one thing the stream's
/// reassembly contract says cannot happen — and would be charged twice against the aggregate cap.
#[test]
fn a_batch_naming_the_same_path_twice_is_refused() {
    // Given
    let worktree = a_worktree();
    worktree.with_file("CLAUDE.md", b"# rules\n").committed();

    // When
    let refusal = read_context_files_bytes(
        worktree.path(),
        &["CLAUDE.md".to_string(), "CLAUDE.md".to_string()],
        CLAUDE_GLOBS,
        A_GENEROUS_CAP,
    );

    // Then
    assert_eq!(
        refusal
            .expect_err("a batch repeating a path must be refused")
            .code(),
        tddy_rpc::Code::InvalidArgument
    );
}
