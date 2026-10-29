//! Writing, linking and removing paths in a context directory without ever leaving it.

use std::path::Path;
use tddy_rpc::Status;
use tddy_sandbox::{
    is_reached_through_link, matches_context_globs, prepare_context_write, prepend_preamble,
    ContextLink, PREAMBLE_FILES,
};
use tddy_worktree_service::worktree_files::validate_rel_path_shape;

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
///
/// **Never through a link above it.** Every directory between the context dir and the file must be
/// a real directory ([`prepare_context_write`]); a symlinked one is `PERMISSION_DENIED`, so a
/// recreated link cannot carry a write outside the context dir.
pub(super) fn write_context_file(
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
    let target = prepare_context_write(context_dir, rel_path).map_err(write_refusal)?;
    // A path that held a link and now holds a file: the write must replace the link, not land in
    // whatever it pointed at — for a preamble file that would be another guidance file overwritten.
    unlink_if_symlink(&target)?;

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

/// Recreates a served link at `rel_path`, pointing at `target` (root-relative, vetted by
/// [`refuse_unlisted_paths`]), written relative to the link's own directory so it resolves inside
/// the context dir (`.claude/skills` -> `../.agents/skills`).
///
/// Whatever already sits at the path — the link before it was retargeted, or a file the link
/// replaced — is removed first. A directory is not: a link only lands where a directory was once
/// the delete pass has emptied it, and one still holding files is a context dir holding something
/// no record knows about.
///
/// The link's parent is held to the same rule as a file's ([`prepare_context_write`]): its `..`
/// steps are counted from the parent it is named under, so that parent must be the directory it
/// physically lands in.
pub(super) fn write_context_link(
    context_dir: &Path,
    rel_path: &str,
    target: &str,
) -> Result<(), Status> {
    let Some(rel_path) = tddy_sandbox::normalized_context_path(rel_path) else {
        log::warn!(
            "context_sync: refusing to write the link {rel_path:?}: it names no path under the \
             context dir"
        );
        return Err(Status::invalid_argument(format!(
            "cannot write the link {rel_path:?} into the context dir: it names no relative path"
        )));
    };
    let link = prepare_context_write(context_dir, &rel_path).map_err(write_refusal)?;
    match std::fs::symlink_metadata(&link) {
        Ok(metadata) if metadata.is_dir() => {
            return Err(Status::internal(format!(
                "cannot recreate the link {rel_path:?}: a directory is in its way"
            )))
        }
        Ok(_) => std::fs::remove_file(&link).map_err(|e| {
            Status::internal(format!(
                "failed to replace {link:?} in the context dir: {e}"
            ))
        })?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(Status::internal(format!(
                "failed to inspect {link:?} in the context dir: {e}"
            )))
        }
    }
    tddy_sandbox::create_context_link(&link, &rel_path, target).map_err(|e| {
        log::error!("context_sync: linking {rel_path} -> {target} failed: {e}");
        Status::internal(format!("failed to create the link {link:?}: {e}"))
    })
}

/// A failure to prepare a write's parent ([`prepare_context_write`]): a symlinked parent is a
/// refusal, anything else a fault on this host.
fn write_refusal(e: std::io::Error) -> Status {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        Status::permission_denied(e.to_string())
    } else {
        Status::internal(format!("failed to prepare a write in the context dir: {e}"))
    }
}

/// Removes `path` when it is a symlink, so the write that follows replaces the link instead of
/// going through it.
fn unlink_if_symlink(path: &Path) -> Result<(), Status> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => std::fs::remove_file(path)
            .map_err(|e| Status::internal(format!("failed to replace the link {path:?}: {e}"))),
        _ => Ok(()),
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
///
/// **A link is removed as a link.** `remove_file` on a symlink unlinks the link itself and never
/// what it points at, so dropping `.claude/skills -> ../.agents/skills` leaves the `.agents/skills`
/// tree the repository still ships. The containment assertion canonicalizes the link's *parent*,
/// not the link, so a link whose target is already gone is still removable.
pub(super) fn remove_context_file(
    context_dir: &Path,
    rel_path: &str,
    globs: &[&str],
    links: &[ContextLink],
    preamble: &str,
) -> Result<(), Status> {
    let rel_slashed = validate_rel_path_shape(rel_path)?;
    if !matches_context_globs(&rel_slashed, globs) && !is_reached_through_link(&rel_slashed, links)
    {
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
