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

mod context_writes;
mod manifest_gate;
pub(crate) mod sources;

use std::path::{Path, PathBuf};

use tddy_rpc::Status;
use tddy_sandbox::{
    allow_listed_link, clear_context_stale, diff_manifests, mark_context_stale, ContextEntry,
    ContextLink, ContextManifest,
};

use crate::context_sync::context_writes::remove_context_file;
use crate::context_sync::context_writes::write_context_file;
use crate::context_sync::context_writes::write_context_link;
use crate::context_sync::manifest_gate::is_syncable_rel_path;
use crate::context_sync::manifest_gate::refuse_unlisted_paths;
use tddy_worktree_service::worktree_files::validate_rel_path_shape;

/// Where the agent's guidance is read from, for one session.
///
/// A trait rather than two syncers because the *decision* — what to fetch, what to delete, what to
/// leave alone — must be identical on both halves, and the only thing that differs is where the
/// bytes come from.
pub trait ContextSource: Send + Sync {
    /// Every allow-listed path the repository currently serves, with its hash.
    fn manifest(&self) -> Result<ContextManifest, Status>;

    /// The raw bytes of one allow-listed path. Never asked of a link entry: a link carries no
    /// bytes, and is recreated from the target its manifest entry names.
    fn read(&self, rel_path: &str) -> Result<Vec<u8>, Status>;
}

pub use crate::context_sync::sources::{LocalWorktreeSource, PrefetchedContext};

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
    /// Shared with `tddy_daemon::split_session::build_split_context_dir`, which owns the directory's
    /// creation and the preamble's rendering; this owns what goes *into* it, so setup and every
    /// later tick agree on the answer by construction.
    ///
    /// **Not only a "fresh" directory.** The split *resume* path reaches this against a directory
    /// the previous run left behind, populated with a manifest the repository has since moved on
    /// from — which is the whole reason a resume re-fetches at all — so it goes through the same
    /// [`synchronize_directory`] a tick does, delete pass included.
    pub fn populate(
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
    let served_links = refuse_unlisted_paths(&served, globs)?;
    let held_links = state.links(globs);
    let held = state.held_manifest(context_dir, globs, &held_links);
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

    // A path a link pulled in is deleted on the authority of the link that pulled it in — usually
    // one the record holds and the repository has just dropped, so the served links alone would
    // refuse to clean up after it.
    let deletable_links: Vec<ContextLink> = held_links
        .into_iter()
        .chain(served_links.iter().cloned())
        .collect();
    for rel_path in &diff.delete {
        remove_context_file(context_dir, rel_path, globs, &deletable_links, &preamble)?;
    }
    let served_entries: std::collections::BTreeMap<&str, &ContextEntry> = served
        .entries()
        .iter()
        .map(|entry| (entry.rel_path.as_str(), entry))
        .collect();
    for rel_path in &fetch {
        let link_target = served_entries
            .get(rel_path.as_str())
            .and_then(|entry| entry.symlink_target.as_deref());
        match link_target {
            // `refuse_unlisted_paths` has vetted it; a link is recreated, never read.
            Some(target) => write_context_link(context_dir, rel_path.as_str(), target)?,
            None => {
                let bytes = source.read(rel_path.as_str())?;
                write_context_file(context_dir, rel_path.as_str(), &bytes, &preamble)?;
            }
        }
    }
    log::info!(
        "context_sync: {context_dir:?} fetched {} path(s), deleted {} path(s)",
        fetch.len(),
        diff.delete.len()
    );

    SyncState::from_entries(preamble, served.entries()).write(context_dir)
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
    /// Where a recorded link points. Defaulted so a record written before links travelled still
    /// reads — as holding ordinary files only, which is what it held.
    #[serde(default)]
    symlink_target: Option<String>,
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
    ///
    /// A recorded link is held when a symlink sits at its path — asked with `symlink_metadata`,
    /// because `exists()` follows the link and would call a link to a target not yet written
    /// absent. A recorded file under a pulled-in target passes the allow-list on the authority of
    /// `links`, the recorded links that pulled it in ([`Self::links`]).
    fn held_manifest(
        &self,
        context_dir: &Path,
        globs: &[&str],
        links: &[ContextLink],
    ) -> ContextManifest {
        ContextManifest::from_entries(
            self.entries
                .iter()
                .filter(|entry| match &entry.symlink_target {
                    Some(target) => {
                        validate_rel_path_shape(&entry.rel_path).is_ok()
                            && allow_listed_link(&entry.rel_path, target, globs).is_some()
                            && std::fs::symlink_metadata(context_dir.join(&entry.rel_path))
                                .is_ok_and(|metadata| metadata.file_type().is_symlink())
                    }
                    None => {
                        is_syncable_rel_path(&entry.rel_path, globs, links)
                            && context_dir.join(&entry.rel_path).exists()
                    }
                })
                .map(|entry| ContextEntry {
                    rel_path: entry.rel_path.clone(),
                    sha256: entry.sha256.clone(),
                    size_bytes: entry.size_bytes,
                    symlink_target: entry.symlink_target.clone(),
                })
                .collect(),
        )
    }

    /// The links this record holds that pass [`allow_listed_link`] — the gate a served link
    /// passes, applied to a record that is as peer-writable as a manifest.
    fn links(&self, globs: &[&str]) -> Vec<ContextLink> {
        self.entries
            .iter()
            .filter_map(|entry| {
                let target = entry.symlink_target.as_deref()?;
                allow_listed_link(&entry.rel_path, target, globs)
            })
            .collect()
    }
}

impl From<&ContextEntry> for SyncStateEntry {
    fn from(entry: &ContextEntry) -> Self {
        Self {
            rel_path: entry.rel_path.clone(),
            sha256: entry.sha256.clone(),
            size_bytes: entry.size_bytes,
            symlink_target: entry.symlink_target.clone(),
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
