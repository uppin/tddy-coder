//! The two places a context directory's bytes come from: the worktree beside it, or bytes already
//! fetched from the host holding it.

use crate::context_files::{read_context_file_bytes, read_context_file_bytes_within};
use crate::context_sync::ContextSource;
use std::path::PathBuf;
use std::sync::PoisonError;
use tddy_rpc::Status;
use tddy_sandbox::{context_reach, ContextEntry, ContextManifest, ContextReach};

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
    /// The walk the last manifest was taken with, which that tick's reads are answered from.
    ///
    /// A tick is one manifest and then a read per path it fetches. Answering each read with a
    /// fresh walk — and, where links travel, a `git check-ignore` — made a tick cost one walk per
    /// file; and a read is only correct if it reaches what the manifest it serves reached, which a
    /// later walk of a worktree still being edited need not. `None` until the first manifest, when a
    /// read walks for itself.
    reach: std::sync::Mutex<Option<ContextReach>>,
}

impl LocalWorktreeSource {
    pub fn new(worktree_root: PathBuf, globs: &'static [&'static str], max_bytes: u64) -> Self {
        Self {
            worktree_root,
            globs,
            max_bytes,
            reach: std::sync::Mutex::new(None),
        }
    }

    fn held_reach(&self) -> std::sync::MutexGuard<'_, Option<ContextReach>> {
        // A poisoned lock held a walk taken before some other thread panicked, which is as good an
        // answer as it was then; it is replaced by the next manifest either way.
        self.reach.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl ContextSource for LocalWorktreeSource {
    fn manifest(&self) -> Result<ContextManifest, Status> {
        let failed = |e: String| {
            log::error!(
                "context_sync: manifest of {:?} failed: {e}",
                self.worktree_root
            );
            Status::internal(format!("failed to read local context manifest: {e}"))
        };
        let manifest =
            ContextManifest::of_worktree(&self.worktree_root, self.globs, self.max_bytes)
                .map_err(|e| failed(format!("{e:#}")))?;
        let reach =
            context_reach(&self.worktree_root, self.globs).map_err(|e| failed(format!("{e:#}")))?;
        *self.held_reach() = Some(reach);
        Ok(manifest)
    }

    fn read(&self, rel_path: &str) -> Result<Vec<u8>, Status> {
        match self.held_reach().as_ref() {
            Some(reach) => read_context_file_bytes_within(
                &self.worktree_root,
                rel_path,
                self.globs,
                self.max_bytes,
                reach,
            ),
            None => {
                read_context_file_bytes(&self.worktree_root, rel_path, self.globs, self.max_bytes)
            }
        }
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
    /// never arrived is a context dir missing a rule, and nothing downstream could tell. A link
    /// entry has no bytes to arrive, so it is not asked for any.
    pub fn new(
        entries: Vec<ContextEntry>,
        files: std::collections::BTreeMap<String, Vec<u8>>,
    ) -> Result<Self, Status> {
        if let Some(missing) = entries
            .iter()
            .find(|entry| entry.symlink_target.is_none() && !files.contains_key(&entry.rel_path))
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
