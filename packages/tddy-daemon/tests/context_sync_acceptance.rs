//! Acceptance tests: populating and re-syncing a split session's context directory — AC23-AC25,
//! AC28-AC30.
//!
//! A split session's agent has no repository on its host: its working directory is a context
//! directory, and until now that directory held only the managed-codebase notice
//! (`TODO(remote-managed-worktree)`, `split_session.rs:136`). It is now populated from the codebase
//! daemon before the agent spawns, and kept current for the life of the session.
//!
//! The fetch is expressed as a [`ContextSource`] so both halves share one decision procedure: the
//! split path's source talks to the codebase daemon over LiveKit, the co-located path's reads the
//! worktree sitting beside it. These tests drive the procedure through an in-memory source — the
//! RPC carriage itself is `remote_managed_worktree_cross_host_acceptance.rs`'s job.
//!
//! PRD: docs/ft/daemon/agent-context-sync.md § Acceptance Criteria.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use pretty_assertions::assert_eq;
use tddy_daemon::context_sync::{ContextSource, ContextSyncer};
use tddy_daemon::split_session::build_split_context_dir;
use tddy_rpc::Status;
use tddy_sandbox::{ContextEntry, ContextManifest};

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

/// The host's `max_attachment_bytes` as these tests configure it — the co-located reader's cap is
/// the operator's setting, not a constant, so a fixture has to name one.
const A_GENEROUS_CAP: u64 = 64 * 1024 * 1024;

/// The record a context directory keeps of what it holds. Named here because two tests write one by
/// hand: the syncer's deletions are driven by it, and it lives in a directory the split path leaves
/// deliberately writable.
const SYNC_STATE_BASENAME: &str = ".tddy-context-sync.json";

/// A codebase host that answers manifests and reads out of memory, counting what was asked of it so
/// a test can assert that nothing was transferred.
struct ACodebaseHost {
    files: Mutex<BTreeMap<String, Vec<u8>>>,
    manifest_calls: AtomicUsize,
    read_calls: AtomicUsize,
    failing: Mutex<Option<String>>,
}

fn a_codebase_host() -> ACodebaseHost {
    ACodebaseHost {
        files: Mutex::new(BTreeMap::new()),
        manifest_calls: AtomicUsize::new(0),
        read_calls: AtomicUsize::new(0),
        failing: Mutex::new(None),
    }
}

impl ACodebaseHost {
    fn holding(self, rel_path: &str, contents: &str) -> Self {
        self.files
            .lock()
            .expect("lock")
            .insert(rel_path.to_string(), contents.as_bytes().to_vec());
        self
    }

    /// Bytes rather than text, for the guidance file that is not markdown at all.
    fn holding_bytes(self, rel_path: &str, contents: &[u8]) -> Self {
        self.files
            .lock()
            .expect("lock")
            .insert(rel_path.to_string(), contents.to_vec());
        self
    }

    fn edit(&self, rel_path: &str, contents: &str) {
        self.files
            .lock()
            .expect("lock")
            .insert(rel_path.to_string(), contents.as_bytes().to_vec());
    }

    fn remove(&self, rel_path: &str) {
        self.files.lock().expect("lock").remove(rel_path);
    }

    /// Make every call fail, as a dropped peer link does.
    fn unreachable(&self, why: &str) {
        *self.failing.lock().expect("lock") = Some(why.to_string());
    }

    fn reachable_again(&self) {
        *self.failing.lock().expect("lock") = None;
    }

    fn reads(&self) -> usize {
        self.read_calls.load(Ordering::SeqCst)
    }

    fn manifests(&self) -> usize {
        self.manifest_calls.load(Ordering::SeqCst)
    }

    fn refuse_if_unreachable(&self) -> Result<(), Status> {
        match self.failing.lock().expect("lock").as_deref() {
            Some(why) => Err(Status::unavailable(why.to_string())),
            None => Ok(()),
        }
    }
}

impl ContextSource for ACodebaseHost {
    fn manifest(&self) -> Result<ContextManifest, Status> {
        self.manifest_calls.fetch_add(1, Ordering::SeqCst);
        self.refuse_if_unreachable()?;
        let files = self.files.lock().expect("lock");
        Ok(ContextManifest::from_entries(
            files
                .iter()
                .map(|(rel_path, bytes)| ContextEntry {
                    rel_path: rel_path.clone(),
                    sha256: tddy_sandbox::sha256_hex(bytes),
                    size_bytes: bytes.len() as u64,
                })
                .collect(),
        ))
    }

    fn read(&self, rel_path: &str) -> Result<Vec<u8>, Status> {
        self.read_calls.fetch_add(1, Ordering::SeqCst);
        self.refuse_if_unreachable()?;
        self.files
            .lock()
            .expect("lock")
            .get(rel_path)
            .cloned()
            .ok_or_else(|| Status::not_found(format!("no such context file: {rel_path}")))
    }
}

fn a_session_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

fn read(context_dir: &Path, rel_path: &str) -> String {
    std::fs::read_to_string(context_dir.join(rel_path))
        .unwrap_or_else(|e| panic!("{rel_path} must be readable: {e}"))
}

// ---------------------------------------------------------------------------
// AC23-AC24 — the setup sync
// ---------------------------------------------------------------------------

/// AC23. The agent's working directory carries the target repo's guidance before the agent process
/// exists — a split agent that starts, reads its cwd and finds only the notice has already missed
/// the project's rules for its first turn.
#[test]
fn a_split_context_dir_is_populated_from_the_codebase_host_at_setup() {
    // Given
    let session_dir = a_session_dir();
    let host = a_codebase_host()
        .holding("CLAUDE.md", "# Project rules\n\nAlways run ./test.\n")
        .holding(".claude/skills/tdd/SKILL.md", "# TDD skill\n")
        .holding(".mcp.json", "{\"mcpServers\":{}}\n");

    // When
    let context_dir = build_split_context_dir(session_dir.path(), &[], CLAUDE_GLOBS, &host)
        .expect("building the context dir must succeed");

    // Then
    assert!(
        read(&context_dir, "CLAUDE.md").contains("Always run ./test."),
        "the target repo's own CLAUDE.md must reach the agent"
    );
    assert_eq!(
        read(&context_dir, ".claude/skills/tdd/SKILL.md"),
        "# TDD skill\n"
    );
    assert_eq!(read(&context_dir, ".mcp.json"), "{\"mcpServers\":{}}\n");
}

/// AC23. The managed-codebase preamble still leads the file — populating it from the repo must not
/// cost the rule that the codebase is elsewhere.
#[test]
fn the_populated_claude_md_still_opens_with_the_managed_codebase_preamble() {
    // Given
    let session_dir = a_session_dir();
    let host = a_codebase_host().holding("CLAUDE.md", "# Project rules\n");

    // When
    let context_dir = build_split_context_dir(session_dir.path(), &[], CLAUDE_GLOBS, &host)
        .expect("build must succeed");

    // Then
    let claude_md = read(&context_dir, "CLAUDE.md");
    assert!(
        claude_md.starts_with(tddy_sandbox::managed_codebase_preamble(&[]).trim_start()),
        "the preamble must lead: {}",
        &claude_md[..claude_md.len().min(200)]
    );
    assert!(claude_md.contains("# Project rules"));
}

/// AC24. A setup fetch that fails fails the whole build, so `StartSession` fails and the caller
/// tears down the worktree it created on the codebase daemon. A split session that cannot read its
/// project's guidance does not start — there is no partial context dir and no silent degradation.
#[test]
fn a_setup_fetch_that_fails_fails_the_build_rather_than_yielding_a_partial_dir() {
    // Given
    let session_dir = a_session_dir();
    let host = a_codebase_host().holding("CLAUDE.md", "# Project rules\n");
    host.unreachable("peer link dropped");

    // When
    let refusal = build_split_context_dir(session_dir.path(), &[], CLAUDE_GLOBS, &host);

    // Then
    let refusal = refusal.expect_err("an unreachable codebase host must fail the build");
    assert!(
        refusal.message().contains("peer link dropped"),
        "the refusal must carry the cause: {}",
        refusal.message()
    );
}

// ---------------------------------------------------------------------------
// AC25-AC27 — the re-sync tick
// ---------------------------------------------------------------------------

/// AC25 + AC26. A tick with nothing changed asks for the manifest and transfers no file content.
/// Steady state on a 2 s broadcast must not re-read the whole config tree.
#[test]
fn a_tick_with_nothing_changed_fetches_the_manifest_and_transfers_no_content() {
    // Given
    let session_dir = a_session_dir();
    let host = a_codebase_host()
        .holding("CLAUDE.md", "# rules\n")
        .holding(".claude/settings.json", "{}\n");
    let context_dir = build_split_context_dir(session_dir.path(), &[], CLAUDE_GLOBS, &host)
        .expect("build must succeed");
    let syncer = ContextSyncer::new(context_dir, CLAUDE_GLOBS);
    let reads_after_setup = host.reads();
    let manifests_after_setup = host.manifests();

    // When
    syncer.tick(&host).expect("tick must succeed");

    // Then
    assert_eq!(
        host.manifests(),
        manifests_after_setup + 1,
        "a tick must ask for the manifest exactly once"
    );
    assert_eq!(
        host.reads(),
        reads_after_setup,
        "nothing moved, so nothing may be transferred"
    );
}

/// AC26. An edit on the codebase host reaches the agent's directory on the next tick, and only that
/// file is transferred.
#[test]
fn a_tick_after_an_edit_transfers_only_the_file_that_changed() {
    // Given
    let session_dir = a_session_dir();
    let host = a_codebase_host()
        .holding("CLAUDE.md", "# rules\n")
        .holding(".claude/settings.json", "{}\n");
    let context_dir = build_split_context_dir(session_dir.path(), &[], CLAUDE_GLOBS, &host)
        .expect("build must succeed");
    let syncer = ContextSyncer::new(context_dir.clone(), CLAUDE_GLOBS);
    let reads_after_setup = host.reads();

    // When
    host.edit("CLAUDE.md", "# rules\n\nAnd never use --no-verify.\n");
    syncer.tick(&host).expect("tick must succeed");

    // Then
    assert!(
        read(&context_dir, "CLAUDE.md").contains("never use --no-verify"),
        "the edited guidance must reach the agent"
    );
    assert_eq!(
        host.reads(),
        reads_after_setup + 1,
        "only the changed file may be transferred"
    );
}

/// AC27. A file the project deletes is deleted from the agent's directory. Leaving it behind has
/// the agent obeying a rule the project has retracted.
#[test]
fn a_tick_deletes_a_file_the_project_removed() {
    // Given
    let session_dir = a_session_dir();
    let host = a_codebase_host()
        .holding("CLAUDE.md", "# rules\n")
        .holding(".claude/skills/old/SKILL.md", "# retracted\n");
    let context_dir = build_split_context_dir(session_dir.path(), &[], CLAUDE_GLOBS, &host)
        .expect("build must succeed");
    let syncer = ContextSyncer::new(context_dir.clone(), CLAUDE_GLOBS);

    // When
    host.remove(".claude/skills/old/SKILL.md");
    syncer.tick(&host).expect("tick must succeed");

    // Then
    assert!(
        !context_dir.join(".claude/skills/old/SKILL.md").exists(),
        "a retracted skill must not linger in the agent's directory"
    );
}

/// AC27, bounded by AC13. The project deleting its own `CLAUDE.md` puts that path in the tick's
/// `delete` list, and unlinking it would leave the agent's working directory with no file carrying
/// the one rule that matters most — that the codebase is elsewhere and reachable only through
/// `mcp__tddy-tools__*`. Both builders guarantee the file exists even for a repo that has none, so
/// a tick must not be able to undo that: the project's content goes, the preamble stays.
#[test]
fn a_tick_never_deletes_the_preamble_file_the_agent_reads_its_orientation_from() {
    // Given
    let session_dir = a_session_dir();
    let host = a_codebase_host().holding("CLAUDE.md", "# Project rules\n");
    let context_dir = build_split_context_dir(session_dir.path(), &[], CLAUDE_GLOBS, &host)
        .expect("build must succeed");
    let syncer = ContextSyncer::new(context_dir.clone(), CLAUDE_GLOBS);

    // When
    host.remove("CLAUDE.md");
    syncer.tick(&host).expect("tick must succeed");

    // Then
    let claude_md = read(&context_dir, "CLAUDE.md");
    assert!(
        claude_md.contains("mcp__tddy-tools__"),
        "the managed-codebase rule must survive the repo dropping its own CLAUDE.md: {claude_md}"
    );
    assert!(
        !claude_md.contains("# Project rules"),
        "but the retracted content itself must go: {claude_md}"
    );
}

/// The sync record lives in the split path's context directory, which is deliberately writable —
/// it is the agent's only scratch space on its host. So its contents are as much peer-supplied as a
/// manifest is, and a recorded `../escape.md` reaches the tick as a *deletion*: it is in `held` and
/// absent from `served`. Nothing else on the delete path stops it, because `Path::join` normalizes
/// nothing and `Path::starts_with` compares components, so `ctx/../escape.md` "starts with" `ctx`.
#[test]
fn a_deletion_recorded_for_a_path_outside_the_context_dir_never_reaches_the_filesystem() {
    // Given
    let session_dir = a_session_dir();
    let host = a_codebase_host().holding("CLAUDE.md", "# Project rules\n");
    let context_dir = build_split_context_dir(session_dir.path(), &[], CLAUDE_GLOBS, &host)
        .expect("build must succeed");
    let outside = session_dir.path().join("escape.md");
    std::fs::write(&outside, "not the sync's to delete\n").expect("write");
    let tampered_record = concat!(
        r#"{"preamble":"Managed Codebase","entries":"#,
        r#"[{"rel_path":"../escape.md","sha256":"deadbeef","size_bytes":1}]}"#
    );
    std::fs::write(context_dir.join(SYNC_STATE_BASENAME), tampered_record)
        .expect("write the tampered record");
    let syncer = ContextSyncer::new(context_dir, CLAUDE_GLOBS);

    // When
    let outcome = syncer.tick(&host);

    // Then
    assert!(
        outside.exists(),
        "a traversal spelled into the sync record must not unlink a file outside the context dir"
    );
    assert!(
        outcome.is_ok(),
        "and the tampered entry must be dropped rather than wedging the session: {outcome:?}"
    );
}

/// A record that will not parse is read as "I hold nothing" — the same value a missing one yields.
/// Refusing instead wedges the session for good: every tick fails, every tick marks the guidance
/// stale, and none of them ever rewrites the byte that caused it. The file sits in a directory the
/// agent can write to, so that is not a hypothetical.
#[test]
fn a_corrupt_sync_record_re_syncs_in_full_instead_of_wedging_the_session() {
    // Given
    let session_dir = a_session_dir();
    let host = a_codebase_host().holding("CLAUDE.md", "# Project rules\n");
    let context_dir = build_split_context_dir(session_dir.path(), &[], CLAUDE_GLOBS, &host)
        .expect("build must succeed");
    std::fs::write(context_dir.join(SYNC_STATE_BASENAME), "{ not json at all").expect("write");
    let syncer = ContextSyncer::new(context_dir.clone(), CLAUDE_GLOBS);

    // When
    host.edit(
        "CLAUDE.md",
        "# Project rules\n\nAnd challenge the developer.\n",
    );
    let outcome = syncer.tick(&host);

    // Then
    assert!(
        outcome.is_ok(),
        "an unreadable record must cost a re-transfer, not the session: {outcome:?}"
    );
    assert!(
        read(&context_dir, "CLAUDE.md").contains("challenge the developer"),
        "and the tick must have re-synced the whole allow-list"
    );
}

/// A `CLAUDE.md` whose bytes are not UTF-8 cannot carry the preamble in front of them, and writing
/// it through anyway produces the one file this feature exists to prevent: an agent's working
/// directory whose `CLAUDE.md` says nothing about the codebase being elsewhere. Both setup paths
/// already refuse the same input, so the sync refuses it too rather than quietly disagreeing.
#[test]
fn a_preamble_file_that_is_not_utf8_fails_the_sync_rather_than_being_written_bare() {
    // Given
    let session_dir = a_session_dir();
    let host = a_codebase_host().holding_bytes("CLAUDE.md", &[0xff, 0xfe, 0x00, 0x01]);

    // When
    let refusal = build_split_context_dir(session_dir.path(), &[], CLAUDE_GLOBS, &host);

    // Then
    let refusal = refusal.expect_err("a non-UTF-8 CLAUDE.md must fail the sync");
    assert!(
        refusal.message().contains("CLAUDE.md"),
        "the refusal must name the file that cannot carry the preamble: {}",
        refusal.message()
    );
}

// ---------------------------------------------------------------------------
// AC28-AC29 — a failing tick is survivable and visible
// ---------------------------------------------------------------------------

/// AC28. A tick that fails does not kill the session — dropping a working session over a transient
/// link failure is worse than the staleness — but the agent is told its guidance may have drifted.
#[test]
fn a_failing_tick_keeps_the_session_and_marks_the_guidance_stale() {
    // Given
    let session_dir = a_session_dir();
    let host = a_codebase_host().holding("CLAUDE.md", "# Project rules\n");
    let context_dir = build_split_context_dir(session_dir.path(), &[], CLAUDE_GLOBS, &host)
        .expect("build must succeed");
    let syncer = ContextSyncer::new(context_dir.clone(), CLAUDE_GLOBS);

    // When
    host.unreachable("peer link dropped");
    let outcome = syncer.tick(&host);

    // Then
    assert!(
        outcome.is_err(),
        "the tick must report the failure to its caller"
    );
    let claude_md = read(&context_dir, "CLAUDE.md");
    assert!(
        claude_md.contains("STALE"),
        "the agent must be told its guidance may have drifted: {claude_md}"
    );
    assert!(
        claude_md.contains("# Project rules"),
        "and must keep the guidance it already had: {claude_md}"
    );
}

/// AC29. Recovery clears the warning, so a session that reconnects stops flagging guidance that is
/// now current.
#[test]
fn a_tick_that_recovers_clears_the_staleness_warning() {
    // Given
    let session_dir = a_session_dir();
    let host = a_codebase_host().holding("CLAUDE.md", "# Project rules\n");
    let context_dir = build_split_context_dir(session_dir.path(), &[], CLAUDE_GLOBS, &host)
        .expect("build must succeed");
    let syncer = ContextSyncer::new(context_dir.clone(), CLAUDE_GLOBS);
    host.unreachable("peer link dropped");
    let _ = syncer.tick(&host);

    // When
    host.reachable_again();
    syncer.tick(&host).expect("the recovered tick must succeed");

    // Then
    let claude_md = read(&context_dir, "CLAUDE.md");
    assert!(
        !claude_md.contains("STALE"),
        "a recovered sync must stop warning: {claude_md}"
    );
    assert!(claude_md.contains("# Project rules"));
}

// ---------------------------------------------------------------------------
// AC30 — the co-located half runs the same procedure
// ---------------------------------------------------------------------------

/// AC30. The co-located path reads the worktree beside it rather than issuing RPCs, and reaches the
/// same answer — one decision procedure, two sources. A session whose worktree is local must not
/// get a different sync than one whose worktree is a hop away.
#[test]
fn the_co_located_source_produces_the_same_manifest_as_a_remote_one() {
    // Given
    let worktree = tempfile::tempdir().expect("tempdir");
    for (rel_path, contents) in [
        ("CLAUDE.md", "# rules\n"),
        (".claude/settings.json", "{}\n"),
        ("README.md", "# not synced\n"),
    ] {
        let path = worktree.path().join(rel_path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, contents).expect("write");
    }
    let remote = a_codebase_host()
        .holding("CLAUDE.md", "# rules\n")
        .holding(".claude/settings.json", "{}\n");

    // When
    let local = tddy_daemon::context_sync::LocalWorktreeSource::new(
        worktree.path().to_path_buf(),
        CLAUDE_GLOBS,
        A_GENEROUS_CAP,
    );
    let from_local = ContextSource::manifest(&local).expect("local manifest");
    let from_remote = remote.manifest().expect("remote manifest");

    // Then
    assert_eq!(from_local.entries(), from_remote.entries());
}

/// AC30. And a co-located tick applies the same diff — an edit to the worktree beside the agent
/// reaches its context directory without any RPC at all.
#[test]
fn a_co_located_tick_picks_up_an_edit_to_the_worktree_beside_it() {
    // Given
    let worktree = tempfile::tempdir().expect("tempdir");
    std::fs::write(worktree.path().join("CLAUDE.md"), "# rules\n").expect("write");
    let source = tddy_daemon::context_sync::LocalWorktreeSource::new(
        worktree.path().to_path_buf(),
        CLAUDE_GLOBS,
        A_GENEROUS_CAP,
    );
    let session_dir = a_session_dir();
    let context_dir = build_split_context_dir(session_dir.path(), &[], CLAUDE_GLOBS, &source)
        .expect("build must succeed");
    let syncer = ContextSyncer::new(context_dir.clone(), CLAUDE_GLOBS);

    // When
    std::fs::write(
        worktree.path().join("CLAUDE.md"),
        "# rules\n\nAnd challenge the developer.\n",
    )
    .expect("write");
    syncer.tick(&source).expect("tick must succeed");

    // Then
    assert!(
        read(&context_dir, "CLAUDE.md").contains("challenge the developer"),
        "a co-located edit must reach the context dir"
    );
}

// ---------------------------------------------------------------------------
// AC27 across a resume — the setup build is not only a *fresh* build
// ---------------------------------------------------------------------------

/// AC27, on the path a **resume** takes. `build_split_context_dir` runs on the split resume as well
/// as the split start, and there the directory already exists with the previous run's files in it:
/// nothing clears it, unlike the co-located resume, which does. So the setup build has to be able
/// to *retract* as well as write.
///
/// Leaving the file behind is worse than it first looks. Setup used to rewrite the sync record from
/// the served set alone, so the stale file stopped being recorded as held — and a later tick, which
/// deletes exactly what it holds and the repository no longer serves, would never see it either. It
/// was orphaned for the life of the session, with the agent obeying a rule the project withdrew
/// while it was stopped.
#[test]
fn a_rebuild_drops_a_path_the_project_retracted_while_the_session_was_stopped() {
    // Given
    let session_dir = a_session_dir();
    let host = a_codebase_host()
        .holding("CLAUDE.md", "# Project rules\n")
        .holding(
            ".claude/skills/retracted/SKILL.md",
            "# withdrawn guidance\n",
        );
    let context_dir = build_split_context_dir(session_dir.path(), &[], CLAUDE_GLOBS, &host)
        .expect("the first build must succeed");
    host.remove(".claude/skills/retracted/SKILL.md");

    // When
    build_split_context_dir(session_dir.path(), &[], CLAUDE_GLOBS, &host)
        .expect("the rebuild must succeed");

    // Then
    assert!(
        !context_dir
            .join(".claude/skills/retracted/SKILL.md")
            .exists(),
        "a rebuild must retract guidance the repository no longer serves, not leave it behind"
    );
    assert!(
        read(&context_dir, "CLAUDE.md").contains("# Project rules"),
        "and must leave the guidance the repository still serves in place"
    );
}

/// And the record agrees, so the directory is not merely correct once but correct from then on: a
/// tick straight after the rebuild finds nothing left to do.
#[test]
fn a_tick_after_a_rebuild_that_retracted_a_path_has_nothing_left_to_repair() {
    // Given
    let session_dir = a_session_dir();
    let host = a_codebase_host()
        .holding("CLAUDE.md", "# Project rules\n")
        .holding(
            ".claude/skills/retracted/SKILL.md",
            "# withdrawn guidance\n",
        );
    let context_dir = build_split_context_dir(session_dir.path(), &[], CLAUDE_GLOBS, &host)
        .expect("the first build must succeed");
    host.remove(".claude/skills/retracted/SKILL.md");
    build_split_context_dir(session_dir.path(), &[], CLAUDE_GLOBS, &host)
        .expect("the rebuild must succeed");
    let reads_before = host.reads();

    // When
    ContextSyncer::new(context_dir.clone(), CLAUDE_GLOBS)
        .tick(&host)
        .expect("the tick must succeed");

    // Then
    assert_eq!(
        host.reads(),
        reads_before,
        "the rebuild already left the directory matching the repository, so the tick must transfer \
         nothing"
    );
    assert!(
        !context_dir
            .join(".claude/skills/retracted/SKILL.md")
            .exists(),
        "and the retracted path must still be gone"
    );
}

// ---------------------------------------------------------------------------
// The spelling a manifest arrives in is not the spelling the directory is keyed on
// ---------------------------------------------------------------------------

/// On the split half the manifest comes from another host, so the *spelling* of every path in it is
/// the peer's choice. `matches_context_globs` normalizes internally and answers a bare `bool`, so
/// `./CLAUDE.md` passes the allow-list and then arrives at the writer spelled with its `./` intact.
///
/// Keyed off that raw string, it lands in the right place and misses the `PREAMBLE_FILES` lookup —
/// the file is written **without the managed-codebase preamble**, so the agent's working directory
/// no longer tells it that its codebase is elsewhere and reachable only through
/// `mcp__tddy-tools__*`. Nothing downstream can tell: the file exists and holds the project's rules.
#[test]
fn a_guidance_file_served_under_a_redundant_dot_slash_still_leads_with_the_preamble() {
    // Given
    let session_dir = a_session_dir();
    let host = a_codebase_host().holding("./CLAUDE.md", "# Project rules\n");

    // When
    let context_dir = build_split_context_dir(session_dir.path(), &[], CLAUDE_GLOBS, &host)
        .expect("build must succeed");

    // Then
    let claude_md = read(&context_dir, "CLAUDE.md");
    assert!(
        claude_md.starts_with(tddy_sandbox::managed_codebase_preamble(&[]).trim_start()),
        "the preamble must lead however the path was spelled: {}",
        &claude_md[..claude_md.len().min(200)]
    );
    assert!(claude_md.contains("# Project rules"));
}

/// The same rule, on the separator. A served `.claude\settings.json` passes the allow-list — which
/// treats `\` as a separator, as `worktree_files` does — and, joined raw, creates one file whose
/// name contains a literal backslash on Linux, sitting beside the `.claude/` directory the agent
/// actually reads.
#[test]
fn a_path_served_with_backslash_separators_lands_in_the_directory_the_agent_reads() {
    // Given
    let session_dir = a_session_dir();
    let host = a_codebase_host()
        .holding("CLAUDE.md", "# Project rules\n")
        .holding(".claude\\settings.json", "{}\n");

    // When
    let context_dir = build_split_context_dir(session_dir.path(), &[], CLAUDE_GLOBS, &host)
        .expect("build must succeed");

    // Then
    assert_eq!(read(&context_dir, ".claude/settings.json"), "{}\n");
}
