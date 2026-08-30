//! Keeping an agent's context directory in line with the repository it is a copy of.
//!
//! A managed-codebase agent reads its working directory for guidance — `CLAUDE.md`, `.claude/`,
//! `.cursor/`, `.agents/` — and that directory is not the repository. It is built at spawn from the
//! target repo under the session backend's glob allow-list, and it has to keep matching while the
//! session runs: a `CLAUDE.md` the developer edits an hour in is a rule the agent must start
//! obeying, and one the developer deletes is a rule it must stop obeying.
//!
//! One decision procedure, two sources. The split path's [`ContextSource`] talks to the daemon
//! holding the codebase over LiveKit; the co-located path's ([`LocalWorktreeSource`]) reads the
//! worktree sitting beside it. Both hand [`ContextSyncer::tick`] a
//! [`ContextManifest`](tddy_sandbox::ContextManifest), and the diff decides the rest — so a session
//! whose worktree is local cannot get a different sync than one whose worktree is a hop away.
//!
//! PRD: `docs/ft/daemon/agent-context-sync.md` § Design, § Continuous re-sync.

use std::path::{Path, PathBuf};

use tddy_rpc::Status;
use tddy_sandbox::{
    clear_context_stale, diff_manifests, mark_context_stale, matches_context_globs,
    prepend_preamble, ContextEntry, ContextManifest, PREAMBLE_FILES,
};

use crate::context_files::read_context_file_bytes;
use crate::worktree_files::validate_rel_path_shape;

/// Where the agent's guidance is read from, for one session.
///
/// A trait rather than two syncers because the *decision* — what to fetch, what to delete, what to
/// leave alone — must be identical on both halves, and the only thing that differs is where the
/// bytes come from.
pub trait ContextSource: Send + Sync {
    /// Every allow-listed path the repository currently serves, with its hash.
    fn manifest(&self) -> Result<ContextManifest, Status>;

    /// The raw bytes of one allow-listed path.
    fn read(&self, rel_path: &str) -> Result<Vec<u8>, Status>;
}

/// The co-located half's source: the worktree beside the agent, read directly.
///
/// No RPC is issued at all — the manifest and the bytes come off the same filesystem the agent's
/// daemon already owns. It goes through the same [`crate::context_files`] reader the remote half is
/// served by, so the allow-list is enforced once, in one place, for both.
///
/// `max_bytes` is the host's configured `max_attachment_bytes`, passed in rather than compiled in.
/// It is what the *serving* daemon applies before the first frame of a remote read leaves it, so a
/// constant here that merely happened to equal the shipped default would give the two halves
/// different caps the moment an operator tuned theirs — and this module's whole claim is that a
/// session whose worktree is local cannot get a different sync than one whose worktree is a hop
/// away.
pub struct LocalWorktreeSource {
    worktree_root: PathBuf,
    globs: &'static [&'static str],
    max_bytes: u64,
}

impl LocalWorktreeSource {
    pub fn new(worktree_root: PathBuf, globs: &'static [&'static str], max_bytes: u64) -> Self {
        Self {
            worktree_root,
            globs,
            max_bytes,
        }
    }
}

impl ContextSource for LocalWorktreeSource {
    fn manifest(&self) -> Result<ContextManifest, Status> {
        ContextManifest::of_worktree(&self.worktree_root, self.globs, self.max_bytes).map_err(|e| {
            log::error!(
                "context_sync: manifest of {:?} failed: {e}",
                self.worktree_root
            );
            Status::internal(format!("failed to read local context manifest: {e}"))
        })
    }

    fn read(&self, rel_path: &str) -> Result<Vec<u8>, Status> {
        read_context_file_bytes(&self.worktree_root, rel_path, self.globs, self.max_bytes)
    }
}

/// A source whose bytes are already in hand.
///
/// The setup fetch reads *every* allow-listed path — that is what populating an empty directory
/// means — so materializing the whole set first and handing it to the synchronous builder costs
/// nothing extra and keeps the transport out of [`ContextSource`]. That matters because the split
/// half's transport is async and this trait is not: bridging the two inside a `manifest()` call
/// would mean blocking a runtime worker on a peer round trip.
pub struct PrefetchedContext {
    entries: Vec<ContextEntry>,
    files: std::collections::BTreeMap<String, Vec<u8>>,
}

impl PrefetchedContext {
    /// Fails rather than serving a partial set: an entry the manifest advertised but whose bytes
    /// never arrived is a context dir missing a rule, and nothing downstream could tell.
    pub fn new(
        entries: Vec<ContextEntry>,
        files: std::collections::BTreeMap<String, Vec<u8>>,
    ) -> Result<Self, Status> {
        if let Some(missing) = entries
            .iter()
            .find(|entry| !files.contains_key(&entry.rel_path))
        {
            return Err(Status::internal(format!(
                "the context manifest advertised {:?} but its bytes were never fetched",
                missing.rel_path
            )));
        }
        Ok(Self { entries, files })
    }
}

impl ContextSource for PrefetchedContext {
    fn manifest(&self) -> Result<ContextManifest, Status> {
        Ok(ContextManifest::from_entries(self.entries.clone()))
    }

    fn read(&self, rel_path: &str) -> Result<Vec<u8>, Status> {
        self.files
            .get(rel_path)
            .cloned()
            .ok_or_else(|| Status::not_found(format!("no fetched context file at {rel_path}")))
    }
}

/// Brings a context directory back in line with its source, and no further.
///
/// Meant to be held per session for as long as it runs and ticked on the `worktree.activity`
/// broadcast the session room's poll loop already publishes — no new timer and no filesystem
/// watcher, of which the repo has none anywhere.
///
/// TODO(agent-context-sync): nothing calls [`ContextSyncer::tick`] in production yet. The setup
/// sync (`build_split_context_dir`) is wired on both the split start and the split resume, so a
/// session's guidance is current when its agent starts; keeping it current *while* the session runs
/// still needs the trigger. Two things are missing and neither is mechanical:
///
/// - the split half's transport is async and [`ContextSource`] is synchronous, so a remote tick
///   needs either an async source or a blocking bridge — a decision, not a wiring;
/// - the co-located half writes into the jail's own `<sandbox_root>/context`, which
///   `tddy-sandbox-app` populates by copying a staged tree in, so who owns that directory once the
///   jail is up has to be settled before anything writes into it mid-session.
///
/// PRD § Continuous re-sync, AC25-AC30.
pub struct ContextSyncer {
    context_dir: PathBuf,
    globs: &'static [&'static str],
}

impl ContextSyncer {
    pub fn new(context_dir: PathBuf, globs: &'static [&'static str]) -> Self {
        Self { context_dir, globs }
    }

    /// One re-sync: fetch the source manifest, apply the difference, clear the staleness warning.
    ///
    /// Steady state costs one manifest and no file content — an unchanged hash is not re-read.
    ///
    /// **Any** failure marks the context stale and returns the error. The session keeps running,
    /// because dropping a working session over a transient link failure would be worse than the
    /// staleness, but the agent is told at the place it reads that its guidance may have drifted.
    /// The next successful tick withdraws that line.
    pub fn tick(&self, source: &dyn ContextSource) -> Result<(), Status> {
        match self.synchronize(source) {
            Ok(()) => {
                if let Err(e) = clear_context_stale(&self.context_dir) {
                    log::error!(
                        "context_sync: clearing the staleness marker in {:?} failed: {e}",
                        self.context_dir
                    );
                    return Err(Status::internal(format!(
                        "failed to clear the context staleness marker: {e}"
                    )));
                }
                Ok(())
            }
            Err(failure) => {
                // Best-effort on purpose: the tick already failed, and a directory that cannot be
                // written is reported through `failure` rather than replaced by a second error
                // about the warning nobody could write.
                if let Err(e) = mark_context_stale(&self.context_dir) {
                    log::error!(
                        "context_sync: marking {:?} stale after {failure} failed too: {e}",
                        self.context_dir
                    );
                }
                Err(failure)
            }
        }
    }

    fn synchronize(&self, source: &dyn ContextSource) -> Result<(), Status> {
        synchronize_directory(&self.context_dir, self.globs, None, source)
    }

    /// Populates a context directory and records what it now holds.
    ///
    /// Shared with [`crate::split_session::build_split_context_dir`], which owns the directory's
    /// creation and the preamble's rendering; this owns what goes *into* it, so setup and every
    /// later tick agree on the answer by construction.
    ///
    /// **Not only a "fresh" directory.** The split *resume* path reaches this against a directory
    /// the previous run left behind, populated with a manifest the repository has since moved on
    /// from — which is the whole reason a resume re-fetches at all — so it goes through the same
    /// [`synchronize_directory`] a tick does, delete pass included.
    pub(crate) fn populate(
        context_dir: &Path,
        preamble: &str,
        globs: &[&str],
        source: &dyn ContextSource,
    ) -> Result<(), Status> {
        synchronize_directory(context_dir, globs, Some(preamble), source)
    }
}

/// **The one place that decides what a context directory should contain**, for the setup build and
/// for every later tick alike.
///
/// Setup used to have its own answer — write every served entry, then overwrite the record — and
/// the difference was not academic. The split resume reaches setup against a directory that already
/// exists and is not cleared, so a path the repository *retracted* while the session was stopped
/// survived the resume: the agent went on obeying a rule the project had withdrawn, which is
/// exactly the failure continuous re-sync exists to prevent. Worse, rewriting the record from the
/// served set alone dropped the stale file from `held`, so no later tick would ever see it as
/// something to delete either — it was orphaned for the life of the session. Two code paths each
/// deciding what the directory should hold is how that happened, so there is now one.
///
/// `preamble_override` is what setup contributes and a tick cannot: the preamble is rendered from
/// the session's *spawn* (its subagent roster, its withdrawals), so the build knows it and the
/// syncer only ever knows the one the last write recorded. `None` means "keep using the recorded
/// one", which is a tick.
fn synchronize_directory(
    context_dir: &Path,
    globs: &[&str],
    preamble_override: Option<&str>,
    source: &dyn ContextSource,
) -> Result<(), Status> {
    let state = SyncState::read(context_dir)?;
    let preamble = preamble_override.unwrap_or(&state.preamble).to_string();
    let served = source.manifest()?;
    refuse_unlisted_paths(&served, globs)?;
    let held = state.held_manifest(context_dir, globs);
    let diff = diff_manifests(&held, &served);

    // The recorded preamble is as much a part of what the directory holds as the hashes are: the
    // guidance files on disk are the repository's bytes *behind that text*, so a build whose roster
    // changed since the last one has to re-write them even where the repository has not moved. A
    // hash-only diff would leave the agent reading last run's withdrawal notice.
    let reworded = preamble != state.preamble;
    if !reworded && diff.fetch.is_empty() && diff.delete.is_empty() {
        log::debug!(
            "context_sync: {context_dir:?} already matches the repository; nothing transferred"
        );
        return Ok(());
    }
    // A rewording falls through even with nothing to transfer, because the record itself is then
    // out of date — and it is the record that later ticks read the preamble back out of.
    let fetch: Vec<&String> = if reworded {
        served
            .entries()
            .iter()
            .map(|entry| &entry.rel_path)
            .collect()
    } else {
        diff.fetch.iter().collect()
    };

    for rel_path in &diff.delete {
        remove_context_file(context_dir, rel_path, globs, &preamble)?;
    }
    for rel_path in &fetch {
        let bytes = source.read(rel_path.as_str())?;
        write_context_file(context_dir, rel_path.as_str(), &bytes, &preamble)?;
    }
    log::info!(
        "context_sync: {context_dir:?} fetched {} path(s), deleted {} path(s)",
        fetch.len(),
        diff.delete.len()
    );

    SyncState::from_entries(preamble, served.entries()).write(context_dir)
}

/// Refuses a manifest naming anything the session's allow-list does not.
///
/// The split half's manifest arrives from *another host*, and every path in it is about to be
/// joined onto a directory here and written. Re-checking it against the compiled-in list is what
/// keeps that from being an instruction the peer gets to write: a `../` or an absolute path is
/// refused by [`matches_context_globs`] outright, and so is a path outside the agent's own table
/// row.
///
/// A refusal rather than a quiet skip. A peer serving something outside the list is a defect on one
/// side or the other, and dropping it silently would leave the two halves disagreeing about what
/// the agent is reading with nothing saying so.
fn refuse_unlisted_paths(served: &ContextManifest, globs: &[&str]) -> Result<(), Status> {
    match served
        .entries()
        .iter()
        .find(|entry| !matches_context_globs(&entry.rel_path, globs))
    {
        Some(entry) => Err(Status::permission_denied(format!(
            "the context source offered {:?}, which this session's allow-list does not name",
            entry.rel_path
        ))),
        None => Ok(()),
    }
}

/// Basename of the per-directory record of what a context directory holds.
///
/// Dot-prefixed and matched by no glob in any backend's table, so a tick never treats its own
/// bookkeeping as guidance to sync or to delete.
const SYNC_STATE_BASENAME: &str = ".tddy-context-sync.json";

/// What the last sync wrote into a context directory.
///
/// Kept beside the files rather than in memory for two reasons, and the second is the load-bearing
/// one:
///
/// - a syncer is constructed after the directory is built, and a syncer that started from "I hold
///   nothing" would re-transfer the whole configuration tree on its first tick;
/// - `CLAUDE.md` and `AGENTS.md` on disk are **not** what the repository serves — they carry the
///   managed-codebase preamble in front — so hashing them would report a difference on every tick
///   and re-fetch them forever. The hash recorded here is of the repository's bytes, which is what
///   the manifest is a manifest of.
///
/// The preamble travels with it because a tick has to reproduce the one the *build* chose, subagent
/// paragraph and all, and the replacements that produced it are a property of the session's spawn
/// rather than of the directory.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct SyncState {
    preamble: String,
    entries: Vec<SyncStateEntry>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SyncStateEntry {
    rel_path: String,
    sha256: String,
    size_bytes: u64,
}

impl SyncState {
    fn read(context_dir: &Path) -> Result<Self, Status> {
        let path = context_dir.join(SYNC_STATE_BASENAME);
        let raw = match std::fs::read_to_string(&path) {
            Ok(raw) => raw,
            // A directory with no record holds nothing this syncer put there, so the first tick
            // transfers the whole allow-list. Correct, merely not cheap — and it is the state a
            // context dir built by an older daemon is in.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => {
                return Err(Status::internal(format!(
                    "failed to read the context sync record: {e}"
                )))
            }
        };
        // A record that will not parse is read as "I hold nothing", the same value a missing one
        // yields, and the same *narrowing* argument the allow-list's unknown-agent default rests on:
        // the worst it can cost is re-transferring the whole allow-list once, and it can never
        // produce content the allow-list does not name, because every path is still gated by
        // `refuse_unlisted_paths` and by the reader. Refusing instead would be the wrong shape of
        // strict — this file lives in the split path's deliberately writable context dir, so a
        // single bad byte in it (an agent's stray write, a truncated write across a crash) would
        // fail every subsequent tick, mark the session permanently stale, and never rewrite the
        // record that caused it. The next successful tick overwrites it.
        match serde_json::from_str(&raw) {
            Ok(state) => Ok(state),
            Err(e) => {
                log::error!(
                    "context_sync: the context sync record at {path:?} is unreadable ({e}); \
                     treating the directory as holding nothing and re-syncing it in full"
                );
                Ok(Self::default())
            }
        }
    }

    fn write(&self, context_dir: &Path) -> Result<(), Status> {
        let raw = serde_json::to_string_pretty(self).map_err(|e| {
            Status::internal(format!("failed to render the context sync record: {e}"))
        })?;
        std::fs::write(context_dir.join(SYNC_STATE_BASENAME), raw)
            .map_err(|e| Status::internal(format!("failed to write the context sync record: {e}")))
    }

    /// What the directory holds, as a manifest comparable with the one the repository serves.
    ///
    /// A recorded path whose file is no longer on disk is dropped, so a file removed from under the
    /// syncer is fetched again rather than assumed present. The reverse — a file on disk with no
    /// record — is deliberately *not* inferred: its bytes are unknown, and guessing they match
    /// would leave the agent reading whatever happened to be there.
    ///
    /// Every recorded path is put through the same two gates a *served* manifest is
    /// ([`refuse_unlisted_paths`]), and for a sharper reason. This record lives inside the split
    /// path's context dir, which is deliberately writable because it is the agent's only scratch
    /// space on its host — so its contents are as much peer-supplied as the manifest is, and a
    /// recorded `../../.ssh/id_ed25519` reaches [`ContextSyncer::synchronize`] as a *deletion*
    /// (it is in `held` and absent from `served`) with nothing else on that path to stop it.
    ///
    /// Dropped rather than refused, because unlike a served manifest this is not a disagreement
    /// between two honest halves: it is a record that has been tampered with or corrupted, and
    /// refusing would wedge the session exactly as a malformed record used to. Dropping narrows —
    /// the entry is simply not something this syncer believes it holds.
    fn held_manifest(&self, context_dir: &Path, globs: &[&str]) -> ContextManifest {
        ContextManifest::from_entries(
            self.entries
                .iter()
                .filter(|entry| is_syncable_rel_path(&entry.rel_path, globs))
                .filter(|entry| context_dir.join(&entry.rel_path).exists())
                .map(|entry| ContextEntry {
                    rel_path: entry.rel_path.clone(),
                    sha256: entry.sha256.clone(),
                    size_bytes: entry.size_bytes,
                })
                .collect(),
        )
    }
}

impl From<&ContextEntry> for SyncStateEntry {
    fn from(entry: &ContextEntry) -> Self {
        Self {
            rel_path: entry.rel_path.clone(),
            sha256: entry.sha256.clone(),
            size_bytes: entry.size_bytes,
        }
    }
}

impl SyncState {
    fn from_entries(preamble: String, entries: &[ContextEntry]) -> Self {
        Self {
            preamble,
            entries: entries.iter().map(SyncStateEntry::from).collect(),
        }
    }
}

/// Writes one synced path, keeping the managed-codebase preamble in front of the guidance files.
///
/// Without the prepend a tick would silently strip the rule that the codebase is elsewhere the first
/// time the project edited its own `CLAUDE.md` — the agent would keep working, and start reaching
/// for native file tools against a directory that is not its repository.
///
/// **The path is normalized before anything is keyed off it**, and both things keyed off it are the
/// reason. `rel_path` arrives from a manifest, and on the split half that manifest comes from
/// another host, so its spelling is the peer's choice: `matches_context_globs` normalizes
/// internally and answers a bare `bool`, so `./CLAUDE.md` and `.claude\settings.json` both pass the
/// gate and then arrive here spelled the way the peer wrote them. Keyed off the raw string,
/// `./CLAUDE.md` fails the [`PREAMBLE_FILES`] lookup and is written **without the
/// managed-codebase preamble** — the one invariant this feature exists to hold, lost with the file
/// still landing in the right place — and `.claude\settings.json` joins to a single file with a
/// literal backslash in its name on Linux, next to the `.claude/` directory the agent actually
/// reads. Normalizing once here means the file the gate approved and the file on disk are the same
/// file.
fn write_context_file(
    context_dir: &Path,
    rel_path: &str,
    bytes: &[u8],
    preamble: &str,
) -> Result<(), Status> {
    let Some(rel_path) = tddy_sandbox::normalized_context_path(rel_path) else {
        log::warn!(
            "context_sync: refusing to write {rel_path:?}: it names no path under the context dir"
        );
        return Err(Status::invalid_argument(format!(
            "cannot write {rel_path:?} into the context dir: it names no relative path"
        )));
    };
    let rel_path = rel_path.as_str();
    let target = context_dir.join(rel_path);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            Status::internal(format!(
                "failed to create {parent:?} in the context dir: {e}"
            ))
        })?;
    }

    // Guidance is markdown by definition, so a `CLAUDE.md` whose bytes are not UTF-8 is not
    // guidance — and the preamble cannot be put in front of it. Writing it through anyway is the
    // one outcome that must not happen: it produces a context dir whose `CLAUDE.md` carries no
    // managed-codebase rule at all, which is the exact invariant this feature exists to hold, lost
    // silently behind a log line nobody reads. Both setup paths already refuse the same input
    // (`context_dir.rs`'s and `split_session.rs`'s `read_to_string`), so refusing here is what makes
    // a tick and a build agree rather than a fallback the CLAUDE.md forbids.
    let payload = if PREAMBLE_FILES.contains(&rel_path) {
        let body = std::str::from_utf8(bytes).map_err(|e| {
            log::error!(
                "context_sync: {rel_path} is not UTF-8 ({e}), so the managed-codebase preamble \
                 cannot lead it"
            );
            Status::invalid_argument(format!(
                "{rel_path} must be UTF-8 to carry the managed-codebase preamble: {e}"
            ))
        })?;
        prepend_preamble(preamble, body).into_bytes()
    } else {
        bytes.to_vec()
    };

    std::fs::write(&target, payload)
        .map_err(|e| Status::internal(format!("failed to write {target:?}: {e}")))
}

/// Whether a recorded or served path is one this syncer may act on at all.
///
/// The two gates are the same pair [`crate::context_files`] applies before it reads: the *shape*
/// (`..`, a leading separator, an absolute component) decided from the string alone, then the
/// compiled-in allow-list. Neither consults the filesystem, so the answer is the same for a path
/// that exists and one that does not.
fn is_syncable_rel_path(rel_path: &str, globs: &[&str]) -> bool {
    match validate_rel_path_shape(rel_path) {
        Ok(rel_slashed) => matches_context_globs(&rel_slashed, globs),
        Err(_) => false,
    }
}

/// Removes a path the repository no longer serves, and any directory it leaves empty.
///
/// Leaving a retracted skill behind has the agent obeying a rule the project has withdrawn, and
/// leaving the empty `.claude/skills/old/` behind has it reading a tree that no longer exists in
/// the repository it is a copy of.
///
/// **A preamble file is never unlinked.** Both builders guarantee `CLAUDE.md` and `AGENTS.md` exist
/// carrying the managed-codebase rule even when the target repo has neither (PRD AC13); a project
/// that deletes its own `CLAUDE.md` mid-session would otherwise put that file in `delete` and cost
/// the agent the only line telling it that its codebase is somewhere else and reachable only
/// through `mcp__tddy-tools__*`. So the repo's content goes and the preamble stays, which is
/// byte-for-byte the file a build against that same repo would have produced.
///
/// **Every other path is re-gated before it is touched**, even though `held_manifest` has already
/// filtered the record it came from. The gates are cheap and the failure is not: this runs
/// `remove_file` on a path joined onto a directory, and `Path::join` normalizes nothing while
/// `Path::starts_with` compares components, so the parent-cleanup guard below reads `ctx/../../x`
/// as living happily under `ctx`. The containment assertion is on the canonical parent for the same
/// reason — it is the only one of the three that a symlinked directory cannot talk its way past.
fn remove_context_file(
    context_dir: &Path,
    rel_path: &str,
    globs: &[&str],
    preamble: &str,
) -> Result<(), Status> {
    let rel_slashed = validate_rel_path_shape(rel_path)?;
    if !matches_context_globs(&rel_slashed, globs) {
        log::warn!(
            "context_sync: refusing to delete {rel_path:?} from {context_dir:?}: this session's \
             allow-list does not name it"
        );
        return Err(Status::permission_denied(format!(
            "cannot delete {rel_path:?}: this session's allow-list does not name it"
        )));
    }
    // Normalized for the same reason [`write_context_file`] normalizes: the `PREAMBLE_FILES`
    // lookup and the `join` below must both be about the path the gate just approved, not about
    // whatever spelling of it a record or a peer's manifest happened to carry.
    let Some(rel_slashed) = tddy_sandbox::normalized_context_path(&rel_slashed) else {
        log::warn!(
            "context_sync: refusing to delete {rel_path:?} from {context_dir:?}: it names no path \
             under the context dir"
        );
        return Err(Status::invalid_argument(format!(
            "cannot delete {rel_path:?}: it names no relative path"
        )));
    };
    if PREAMBLE_FILES.contains(&rel_slashed.as_str()) {
        return write_context_file(context_dir, &rel_slashed, b"", preamble);
    }

    let target = context_dir.join(&rel_slashed);
    assert_within_context_dir(context_dir, &target)?;
    match std::fs::remove_file(&target) {
        Ok(()) => {}
        // Already gone is the state this asked for.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(Status::internal(format!(
                "failed to remove {target:?} from the context dir: {e}"
            )))
        }
    }

    let mut parent = target.parent().map(Path::to_path_buf);
    while let Some(dir) = parent {
        if dir == context_dir || assert_within_context_dir(context_dir, &dir).is_err() {
            break;
        }
        // Only an *empty* directory goes; `remove_dir` failing on a non-empty one is how the walk
        // stops without having to list it first.
        if std::fs::remove_dir(&dir).is_err() {
            break;
        }
        parent = dir.parent().map(Path::to_path_buf);
    }
    Ok(())
}

/// Refuses a path that does not resolve inside `context_dir`.
///
/// The last of the three guards and the only one that asks the filesystem, so it catches what a
/// string check cannot: a `.claude/` inside the context dir that is itself a symlink somewhere
/// else. `path` is a file that may or may not exist by the time this is asked — an already-deleted
/// one is the common case — so it is its *parent* that is canonicalized, which is the directory the
/// unlink actually happens in.
fn assert_within_context_dir(context_dir: &Path, path: &Path) -> Result<(), Status> {
    let canonical_dir = context_dir.canonicalize().map_err(|e| {
        Status::internal(format!(
            "context dir {context_dir:?} is not accessible: {e}"
        ))
    })?;
    let parent = path
        .parent()
        .ok_or_else(|| Status::invalid_argument(format!("{path:?} names no directory")))?;
    let canonical_parent = parent.canonicalize().map_err(|e| {
        log::debug!("context_sync: canonicalizing {parent:?} failed: {e}");
        Status::invalid_argument(format!(
            "{parent:?} is not a directory in the context dir: {e}"
        ))
    })?;
    if !canonical_parent.starts_with(&canonical_dir) {
        log::warn!(
            "context_sync: refusing to touch {path:?}: it resolves to {canonical_parent:?}, \
             outside {canonical_dir:?}"
        );
        return Err(Status::permission_denied(format!(
            "{path:?} resolves outside the context dir"
        )));
    }
    Ok(())
}
