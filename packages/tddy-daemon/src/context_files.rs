//! The allow-list-gated reader an agent's context directory is synced from.
//!
//! Deliberately **not** [`crate::worktree_files`]. That reader gates on git's listing, and the gate
//! is load-bearing: it is what keeps a `.gitignore`d `.env`, a credential a build wrote or a private
//! key unreadable. But agent configuration is routinely gitignored — Claude Code writes
//! `.claude/settings.local.json`, and this repo's own `.gitignore` hides `**/.cursor/mcp.json` and
//! `**/.cursor/hooks.json` — so a context sync built on that reader would omit precisely the files
//! the agent reads.
//!
//! So this reader keeps every traversal, containment and symlink guard its sibling applies
//! ([`crate::worktree_files::validate_rel_path_shape`], the canonicalize-and-contain check) and
//! replaces only the git-listing gate with [`tddy_sandbox::matches_context_globs`].
//!
//! It also applies the allow-list **at both ends of a symlink** — the name a file is asked for by
//! and the place its target sits in the tree — which is the rule
//! [`tddy_sandbox::ContextManifest::of_worktree`]'s walk applies to the entries it advertises. The
//! two must agree: a reader that asks only about the requested name serves `.claude/creds -> ../.env`
//! under a spelling every glob matches, out of a manifest that correctly never listed it.
//!
//! **Why swapping that gate is safe: no caller supplies the globs.** They are compiled into
//! `tddy_core::backend::context_globs_for_agent` and selected by the session's agent, so a request
//! can pick a table row but cannot name a path set. There is no spelling of a request that widens
//! the readable set to reach `.env`, which is exactly what the git gate was protecting against.
//!
//! One property of the sibling is preserved verbatim and is worth naming, because it constrains the
//! order of the checks below: a path the allow-list does not name is refused with the same code
//! *and the same message* whether or not a file sits there
//! (`worktree_files::resolve_listed_worktree_file`, its comment on the existence map). The gate is
//! therefore asked before anything touches the filesystem.
//!
//! PRD: `docs/ft/daemon/agent-context-sync.md`.

use std::path::{Path, PathBuf};

use tddy_rpc::Status;
use tddy_sandbox::{matches_context_globs, root_relative, ContextManifest};
use tddy_service::proto::connection::{
    ContextFileBatchChunk, ContextFileChunk, ContextManifestEntry,
};

use crate::connection_service::HOST_DOCUMENT_FRAME_BYTES;
use crate::worktree_files::{canonicalize_root, validate_rel_path_shape};

/// Bytes of file content carried per `StreamReadContextFile` frame.
///
/// Defined *as* [`HOST_DOCUMENT_FRAME_BYTES`] rather than as the same number, for the reason
/// `EXEC_TOOL_FRAME_BYTES` gives: the budget is a property of the transport, not of what rides on
/// it, and two constants free to drift would be two answers to one question. Staying under
/// `tddy_livekit::chunking::MAX_CHUNK_FRAME_BYTES` is what keeps a context read off the chunking
/// codec entirely — one fewer layer between an agent and its `CLAUDE.md`.
pub const CONTEXT_FILE_FRAME_BYTES: usize = HOST_DOCUMENT_FRAME_BYTES;

/// The globs a session of `session_type` syncs.
///
/// Two vocabularies meet here and neither is wrong. `tddy-core`'s table is keyed on **agent** names
/// (`claude`, `cursor`, `codex`) because that is what a `CodingBackend` calls itself and what the
/// sandbox app already passes as `--agent-kind`. The daemon dispatches on **session type** strings
/// (`claude-cli`, `cursor-cli`, `workspace`) because a session type names a spawn path, of which an
/// agent is only one property — `workspace` runs no agent at all.
///
/// Mapping rather than renaming either side: the session type is persisted in `.session.yaml` and
/// spoken by the web, and the agent name is spoken by the backends and by the sandbox CLI. A
/// session type this does not know falls to `context_globs_for_agent`'s shared base, which can only
/// ever sync less than a known backend.
pub fn context_globs_for_session_type(session_type: &str) -> &'static [&'static str] {
    tddy_core::backend::context_globs_for_agent(context_agent_for_session_type(session_type))
}

/// The agent name a session of `session_type` runs, in the vocabulary `tddy-core`'s table and the
/// two context RPCs are keyed on.
///
/// Named separately from [`context_globs_for_session_type`] because the split half sends the *name*
/// across the wire and lets the serving daemon look the row up: a request that carried the globs
/// themselves would let a caller name a path set, which is exactly what the compiled-in table exists
/// to prevent.
pub fn context_agent_for_session_type(session_type: &str) -> &'static str {
    match session_type.trim() {
        "claude-cli" => "claude",
        "cursor-cli" => "cursor",
        // Every other session type either runs no agent at all (`workspace`) or is not one this
        // mapping knows, and both answer with the empty name so `context_globs_for_agent` yields
        // its shared base — narrower than any backend's row, never the union of them.
        _ => "",
    }
}

/// The agent whose allow-list a **session** is served, read from what the daemon persisted about it
/// rather than from what the caller asked for.
///
/// This is the authoritative answer, and the reason it exists is a widening the request field would
/// otherwise permit. `ReadContextFileRequest.agent` and `ContextManifestRequest.agent` are
/// caller-chosen and authorization is per OS user, not per session
/// (`ConnectionServiceImpl::authorize_exec_tool_caller`), so a caller holding a valid token for a
/// `codex` session could name `cursor` and be served that row: `.claude/**`, `.cursor/**` and
/// `.mcp.json` out of a checkout it was never granted them on — the very files that carry API
/// tokens in MCP `env` blocks, and precisely the gitignored ones the git-listing gate used to
/// refuse. Trusting the field makes the enforced bound the *union* of every table row instead of
/// the session's own.
///
/// Two persisted facts decide it, in this order:
///
/// 1. **A paired agent means the split path.** The codebase half of a split placement is persisted
///    as a `workspace` session — it runs no agent of its own — while the agent it stands in for
///    lives on another daemon, whose `.session.yaml` this host cannot read. What *is* recorded here
///    is the back-pointer to that agent ([`crate::split_session::paired_agent`], written with the
///    session precisely so it cannot be stamped on later), and split placement is `claude-cli` only
///    (PRD `remote-managed-worktree.md` § Why claude-cli only). So a paired workspace session
///    serves Claude's row. Deriving it from the pairing rather than from `req.agent` keeps the
///    answer a property of what this daemon wrote down.
/// 2. **Otherwise the session's own type**, through [`context_agent_for_session_type`] — a
///    `workspace` session nobody is paired with runs no agent and gets the shared base.
pub fn context_agent_for_session(meta: &tddy_core::SessionMetadata) -> &'static str {
    if crate::split_session::paired_agent(meta).is_some() {
        return context_agent_for_session_type("claude-cli");
    }
    context_agent_for_session_type(meta.session_type.as_deref().unwrap_or_default())
}

/// Every allow-listed path under `worktree_root`, with the hash that says whether it moved.
///
/// The walk is [`ContextManifest::of_worktree`]'s, shared with the copier and the syncer on purpose:
/// a second traversal with its own idea of containment is how a manifest ends up advertising a path
/// the reader then refuses. A symlink whose target escapes the root, or lands somewhere the
/// allow-list does not name, is skipped there, so it is absent here too.
///
/// `max_bytes` is the same `max_attachment_bytes` [`read_context_file_bytes`] refuses over, and is
/// passed for exactly that reason: a manifest advertising a path this host's own reader would then
/// refuse is a setup fetch that cannot complete, so an over-cap file is left out of the manifest
/// rather than promised and denied.
pub fn context_manifest(
    worktree_root: &Path,
    globs: &[&str],
    max_bytes: u64,
) -> Result<Vec<ContextManifestEntry>, Status> {
    log::debug!(
        "context_manifest: worktree_root={worktree_root:?} globs={globs:?} max_bytes={max_bytes}"
    );
    let manifest = ContextManifest::of_worktree(worktree_root, globs, max_bytes).map_err(|e| {
        log::error!("context_manifest: walking {worktree_root:?} failed: {e}");
        Status::internal(format!("failed to build context manifest: {e}"))
    })?;
    let entries: Vec<ContextManifestEntry> = manifest
        .entries()
        .iter()
        .map(|entry| ContextManifestEntry {
            rel_path: entry.rel_path.clone(),
            sha256: entry.sha256.clone(),
            size_bytes: entry.size_bytes,
        })
        .collect();
    log::info!(
        "context_manifest: {} allow-listed path(s) under {worktree_root:?}",
        entries.len()
    );
    Ok(entries)
}

/// The raw bytes of one allow-listed path, or why it may not be read.
///
/// Mirrors [`crate::worktree_files::read_worktree_file_bytes`] in everything but the gate, including
/// the size refusal: `max_bytes` is measured with a `stat` and refused **before** the read, because
/// a caller cannot tell a truncated file from a whole one once the frames have started, and a
/// truncated `CLAUDE.md` is a wrong `CLAUDE.md` — it silently drops the project's last rule.
pub fn read_context_file_bytes(
    worktree_root: &Path,
    rel_path: &str,
    globs: &[&str],
    max_bytes: u64,
) -> Result<Vec<u8>, Status> {
    log::debug!(
        "read_context_file_bytes: worktree_root={worktree_root:?} rel_path={rel_path:?} \
         max_bytes={max_bytes}"
    );
    let canonical_file = resolve_allow_listed_context_file(worktree_root, rel_path, globs)?;
    sized_context_file(&canonical_file, rel_path, max_bytes)?;

    let bytes = std::fs::read(&canonical_file).map_err(|e| {
        log::error!("read_context_file_bytes: read {canonical_file:?} failed: {e}");
        Status::internal(format!("failed to read context file: {e}"))
    })?;
    log::info!(
        "read_context_file_bytes: read {} byte(s) from {rel_path:?}",
        bytes.len()
    );
    Ok(bytes)
}

/// The raw bytes of several allow-listed paths, or why none of them may be read.
///
/// This exists because of what setup sync costs without it. Populating a split session's context
/// directory reads *every* allow-listed path before the agent process exists, and one round trip
/// per file turns a 120-file `.claude/skills/` tree into 121 sequential peer calls — around eighteen
/// seconds of dead time on a 150 ms link, and 121 separate chances to trip `PEER_FORWARD_TIMEOUT`,
/// where an ordinary split start used to make no extra peer calls at all.
///
/// Every gate [`read_context_file_bytes`] applies is applied here per path, in the same order and
/// with the same statuses — this is that function's loop body, not a second reader with its own
/// idea of what may be read.
///
/// Two properties are the batch's own, and both are refusals:
///
/// - **Nothing is served unless everything can be.** One unlisted, missing or over-cap path fails
///   the whole call before a byte is read. A partially served batch would leave the caller unable
///   to tell "the project does not ship that file" from "this host would not serve it", and the
///   setup sync this feeds must fail loudly rather than start an agent against guidance with a hole
///   in it (PRD § Failure is loud at setup).
/// - **The aggregate is capped, and measured before the read.** The single-file path bounds one
///   file by `max_bytes`; a batch could otherwise ask for a thousand files just under it and name
///   this host's allocation size. The sum is checked against the same cap the split client applies
///   to the manifest it derived these paths from, so both ends refuse the same set.
pub fn read_context_files_bytes(
    worktree_root: &Path,
    rel_paths: &[String],
    globs: &[&str],
    max_bytes: u64,
) -> Result<Vec<(String, Vec<u8>)>, Status> {
    log::debug!(
        "read_context_files_bytes: worktree_root={worktree_root:?} paths={} max_bytes={max_bytes}",
        rel_paths.len()
    );
    if rel_paths.is_empty() {
        return Err(Status::invalid_argument(
            "a context file batch must name at least one path",
        ));
    }
    // A repeated path would be served as two runs of frames under one `rel_path`, which is the one
    // thing the stream's reassembly contract says cannot happen — and it would be charged twice
    // against the aggregate cap. Both are caller defects, so this is a refusal rather than a
    // silent de-duplication.
    let mut seen = std::collections::HashSet::with_capacity(rel_paths.len());
    if let Some(repeated) = rel_paths.iter().find(|path| !seen.insert(*path)) {
        return Err(Status::invalid_argument(format!(
            "the context file batch names {repeated:?} more than once"
        )));
    }

    // Resolved and sized first, all of them, before a single `read`: the aggregate refusal has to
    // happen before any bytes are held, and so does the per-path gate — a caller must not learn
    // that the first ten paths were readable from how far the call got.
    let mut resolved: Vec<(&String, PathBuf)> = Vec::with_capacity(rel_paths.len());
    let mut total_bytes: u64 = 0;
    for rel_path in rel_paths {
        let canonical_file = resolve_allow_listed_context_file(worktree_root, rel_path, globs)?;
        let byte_size = sized_context_file(&canonical_file, rel_path, max_bytes)?;
        total_bytes = total_bytes.saturating_add(byte_size);
        if total_bytes > max_bytes {
            log::warn!(
                "read_context_files_bytes: refused a batch of {} path(s) totalling more than the \
                 {max_bytes} byte cap at {rel_path:?}",
                rel_paths.len()
            );
            return Err(Status::invalid_argument(format!(
                "the context file batch is over {total_bytes} bytes, over the {max_bytes} byte \
                 limit"
            )));
        }
        resolved.push((rel_path, canonical_file));
    }

    let mut files: Vec<(String, Vec<u8>)> = Vec::with_capacity(resolved.len());
    let mut read_bytes: u64 = 0;
    for (rel_path, canonical_file) in resolved {
        let bytes = std::fs::read(&canonical_file).map_err(|e| {
            log::error!("read_context_files_bytes: read {canonical_file:?} failed: {e}");
            Status::internal(format!("failed to read context file {rel_path}: {e}"))
        })?;
        // Re-measured against what was actually read. The sizes above came from a `stat`, and a
        // file that grew between the two would otherwise carry the batch past the cap the caller
        // was promised it stayed under.
        read_bytes = read_bytes.saturating_add(bytes.len() as u64);
        if bytes.len() as u64 > max_bytes || read_bytes > max_bytes {
            log::warn!(
                "read_context_files_bytes: refused the batch: {rel_path:?} grew while it was being \
                 read, past the {max_bytes} byte cap"
            );
            return Err(Status::invalid_argument(format!(
                "context file {rel_path} grew past the {max_bytes} byte limit while being read"
            )));
        }
        files.push((rel_path.clone(), bytes));
    }
    log::info!(
        "read_context_files_bytes: read {} path(s), {read_bytes} byte(s) from {worktree_root:?}",
        files.len()
    );
    Ok(files)
}

/// The size of one already-resolved context file, refusing anything that is not a servable file.
///
/// Shared by the single read and the batch so the two cannot drift: same "not a regular file"
/// refusal, same cap, same statuses, and the cap measured with a `stat` before any bytes are held.
fn sized_context_file(
    canonical_file: &Path,
    rel_path: &str,
    max_bytes: u64,
) -> Result<u64, Status> {
    let metadata = std::fs::metadata(canonical_file).map_err(|e| {
        log::error!("sized_context_file: metadata {canonical_file:?} failed: {e}");
        Status::internal(format!("failed to size context file: {e}"))
    })?;
    // `.claude` is itself named by `.claude/**`-adjacent spellings and a directory can be reached
    // by a glob like `.agents/*`, so "allow-listed and inside the root" does not imply "a file".
    // Asked for one, `std::fs::read` fails with `Is a directory`, which would surface as an
    // INTERNAL — a request the caller got wrong reported as a fault on this host.
    if !metadata.is_file() {
        log::warn!("sized_context_file: refused {rel_path:?}: not a regular file");
        return Err(Status::invalid_argument(format!(
            "{rel_path} is not a regular file"
        )));
    }
    let byte_size = metadata.len();
    if byte_size > max_bytes {
        log::warn!(
            "sized_context_file: refused {rel_path:?} at {byte_size} byte(s), over the {max_bytes} \
             byte cap"
        );
        return Err(Status::invalid_argument(format!(
            "context file is {byte_size} bytes, over the {max_bytes} byte limit"
        )));
    }
    Ok(byte_size)
}

/// Split a context file into ordered [`CONTEXT_FILE_FRAME_BYTES`] frames.
///
/// Every frame repeats the file's full size, so a reader knows the total from the first one and can
/// tell a completed stream from a truncated one without counting frames. A zero-byte file still
/// yields exactly one (empty) frame — the discipline `worktree_file_frames` already follows, so
/// "the file is empty" never has to be told apart from "the stream produced nothing".
pub fn context_file_frames(bytes: &[u8]) -> Vec<ContextFileChunk> {
    let total_byte_size = bytes.len() as u64;
    let mut frames: Vec<ContextFileChunk> = bytes
        .chunks(CONTEXT_FILE_FRAME_BYTES)
        .map(|chunk| ContextFileChunk {
            data: chunk.to_vec(),
            total_byte_size,
        })
        .collect();
    if frames.is_empty() {
        frames.push(ContextFileChunk {
            data: Vec::new(),
            total_byte_size,
        });
    }
    frames
}

/// Frame a whole batch: every file's frames, in the order the files were read, each tagged with the
/// path it belongs to.
///
/// Two properties a client depends on, both produced here rather than assumed:
///
/// - **at least one frame per file**, so an empty file is one empty frame rather than nothing at
///   all — the single-file reader's rule, and the reason "the file is empty" never has to be told
///   apart from "the bytes never came";
/// - **`end_of_file` on the last frame of each file**, so the boundary is stated rather than
///   inferred from a byte count that a truncated stream would also satisfy on its way to a total it
///   never reaches.
///
/// Frames of one file stay consecutive, which is what lets a client append into one buffer per path
/// without holding every partial file at once.
pub fn context_file_batch_frames(files: &[(String, Vec<u8>)]) -> Vec<ContextFileBatchChunk> {
    let mut frames = Vec::new();
    for (rel_path, bytes) in files {
        let total_byte_size = bytes.len() as u64;
        let mut chunks: Vec<&[u8]> = bytes.chunks(CONTEXT_FILE_FRAME_BYTES).collect();
        if chunks.is_empty() {
            chunks.push(&[]);
        }
        let last_index = chunks.len() - 1;
        for (index, chunk) in chunks.into_iter().enumerate() {
            frames.push(ContextFileBatchChunk {
                rel_path: rel_path.clone(),
                data: chunk.to_vec(),
                total_byte_size,
                end_of_file: index == last_index,
            });
        }
    }
    frames
}

/// The absolute path an allow-listed context file resolves to, or why it may not be read.
///
/// The order of the four checks is a security property, not a preference:
///
/// 1. the path's **shape** — traversal or an absolute path is `INVALID_ARGUMENT`, decided from the
///    string alone, before anything touches the filesystem;
/// 2. the **allow-list, on the requested name** — a path no glob names is `PERMISSION_DENIED`, also
///    decided from the string alone, so the refusal is byte-identical whether or not a file sits at
///    that name. Asking the filesystem first would keep contents secret while handing out the
///    existence map;
/// 3. then **resolution** — absence is `NOT_FOUND`, and a symlink resolving outside the root is
///    `PERMISSION_DENIED`, because matching a glob is not the same as living in the repository;
/// 4. and finally the **allow-list again, on the resolved target's own root-relative path**.
///
/// The fourth check is the both-ends symlink rule, and it is the same rule
/// [`tddy_sandbox::ContextManifest::of_worktree`]'s walk applies to the entries it lists. It has to
/// be applied here as well as there, because the walk decides what to *advertise* while this
/// decides what to *serve*, and this one is handed a caller-supplied name.
///
/// Containment in the root is not a substitute for it. `.claude/creds -> ../.env` canonicalizes to
/// a path squarely inside the worktree, so every check above passes — and the file served under
/// that allow-listed name is the repository's `.env`. The same trick reaches any file in the
/// checkout (`.claude/alias.json -> node_modules/…`), and it reaches
/// [`tddy_sandbox::CONTEXT_EXCLUDE_GLOBS`] too: `.claude/hooksalias.json -> settings.local.json`
/// would hand back the hooks file the daemon owns and this exclusion exists to withhold. On the
/// split path those bytes then cross to another host and land in the agent's readable working
/// directory.
///
/// [`tddy_sandbox::root_relative`] is shared with the walk rather than re-derived here for the
/// blunt reason that a re-derivation is what went wrong: the rule was added to the walk alone, and
/// the reader kept serving what the manifest had already refused to name.
fn resolve_allow_listed_context_file(
    worktree_root: &Path,
    rel_path: &str,
    globs: &[&str],
) -> Result<PathBuf, Status> {
    let rel_slashed = validate_rel_path_shape(rel_path)?;

    if !matches_context_globs(&rel_slashed, globs) {
        log::warn!(
            "resolve_allow_listed_context_file: rejected path outside the allow-list: {rel_path:?}"
        );
        return Err(Status::permission_denied(
            "file is not named by the agent's context allow-list",
        ));
    }

    // Now that the caller has been told the path is allow-listed, absence leaks nothing. As in the
    // sibling reader this is `symlink_metadata`, so the question stays about a name inside the
    // worktree; a dangling link exists as a link and is refused below, on resolution.
    let joined = worktree_root.join(&rel_slashed);
    if let Err(e) = std::fs::symlink_metadata(&joined) {
        if e.kind() == std::io::ErrorKind::NotFound {
            log::debug!(
                "resolve_allow_listed_context_file: allow-listed path {rel_path:?} is not on disk: {e}"
            );
            return Err(Status::not_found("context file not found"));
        }
    }

    let canonical_root = canonicalize_root(worktree_root)?;
    let canonical_file = joined.canonicalize().map_err(|e| {
        log::debug!("resolve_allow_listed_context_file: canonicalize {joined:?} failed: {e}");
        Status::not_found("context file not found")
    })?;
    if !canonical_file.starts_with(&canonical_root) {
        log::warn!(
            "resolve_allow_listed_context_file: rejected path outside worktree: \
             {canonical_file:?} (root {canonical_root:?})"
        );
        return Err(Status::permission_denied(
            "resolved path escapes worktree root",
        ));
    }

    // Where the bytes actually sit, spelled the way the allow-list is written — for a plain file
    // this is `rel_slashed` again, and for a symlink it is the target's own place in the tree.
    let Some(resolved_rel) = root_relative(&canonical_file, &canonical_root) else {
        log::warn!(
            "resolve_allow_listed_context_file: {canonical_file:?} has no path under \
             {canonical_root:?} after the containment check; refusing it"
        );
        return Err(Status::permission_denied(
            "resolved path escapes worktree root",
        ));
    };
    if !matches_context_globs(&resolved_rel, globs) {
        log::warn!(
            "resolve_allow_listed_context_file: rejected {rel_path:?}: it resolves to \
             {resolved_rel:?}, which the agent's context allow-list does not name"
        );
        return Err(Status::permission_denied(
            "file is not named by the agent's context allow-list",
        ));
    }
    Ok(canonical_file)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two vocabularies meet correctly: a `cursor-cli` session gets Cursor's list, which is the
    /// one that includes `.cursor/**`, rather than the shared base an unmapped name would yield.
    #[test]
    fn a_cursor_cli_session_syncs_the_cursor_tree() {
        // When
        let globs = context_globs_for_session_type("cursor-cli");

        // Then
        assert!(globs.contains(&".cursor/**"), "got {globs:?}");
    }

    /// A `workspace` session runs no agent, so it falls to the shared base — narrower than any
    /// backend's list, never the union of them.
    #[test]
    fn a_session_type_that_names_no_agent_falls_to_the_shared_base() {
        // When
        let globs = context_globs_for_session_type("workspace");

        // Then
        assert!(!globs.contains(&".cursor/**"), "got {globs:?}");
        assert!(!globs.contains(&".claude/**"), "got {globs:?}");
        assert!(globs.contains(&"AGENTS.md"), "got {globs:?}");
    }
}
