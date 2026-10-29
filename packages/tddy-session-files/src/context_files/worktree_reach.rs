//! What a context read may reach in a worktree: glob-named paths and the targets of allow-listed links.

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use tddy_rpc::Status;
use tddy_sandbox::{
    context_reach, is_withheld_through_a_link, matches_context_globs, root_relative, ContextReach,
};
use tddy_worktree_service::worktree_files::{canonicalize_root, validate_rel_path_shape};

/// The walk's answer about the worktree ([`tddy_sandbox::context_reach`]), taken at most once per
/// read and shared across a batch, because the batch is one read of one worktree — or handed in
/// already taken, by a source answering a whole tick's reads from the walk its manifest came from.
///
/// Every read needs it — a glob-named file is refused when a link exposes it under a withheld name
/// — but not before the requested path's shape has been checked, so a malformed request costs no
/// walk.
pub(super) struct WorktreeReach<'a> {
    worktree_root: &'a Path,
    globs: &'a [&'a str],
    walked: Option<Cow<'a, ContextReach>>,
}

impl<'a> WorktreeReach<'a> {
    pub(super) fn of(worktree_root: &'a Path, globs: &'a [&'a str]) -> Self {
        Self {
            worktree_root,
            globs,
            walked: None,
        }
    }

    pub(super) fn already_walked(
        worktree_root: &'a Path,
        globs: &'a [&'a str],
        reach: &'a ContextReach,
    ) -> Self {
        Self {
            worktree_root,
            globs,
            walked: Some(Cow::Borrowed(reach)),
        }
    }

    fn get(&mut self) -> Result<&ContextReach, Status> {
        let reach = match self.walked.take() {
            Some(reach) => reach,
            None => Cow::Owned(context_reach(self.worktree_root, self.globs).map_err(|e| {
                log::error!(
                    "resolve_allow_listed_context_file: walking {:?} failed: {e}",
                    self.worktree_root
                );
                Status::internal(format!("failed to walk the context tree: {e}"))
            })?),
        };
        Ok(self.walked.insert(reach))
    }
}

/// The absolute path an allow-listed context file resolves to, or why it may not be read.
///
/// The order of the four checks is a security property, not a preference:
///
/// 1. the path's **shape** — traversal or an absolute path is `INVALID_ARGUMENT`, decided from the
///    string alone, before anything touches the filesystem;
/// 2. the **allow-list, on the requested name** — a path no glob names and the walk does not list
///    is `PERMISSION_DENIED`, with the same message either way. Whether the walk lists it is
///    answered from the walk alone ([`WorktreeReach`]), never from a probe of the requested name, so the refusal is byte-identical whether or not a file is there. Asking the
///    filesystem about the name first would keep contents secret while handing out the existence
///    map;
/// 3. then **resolution** — absence is `NOT_FOUND`, and a symlink resolving outside the root is
///    `PERMISSION_DENIED`, because matching a glob is not the same as living in the repository;
/// 4. and finally the **allow-list again, on the resolved target's own root-relative path**: a
///    glob must name it, the walk must list it, or the walk lists the requested name as a preamble
///    file read from it (`CLAUDE.md -> docs/agent-rules.md` is listed as `CLAUDE.md` carrying that
///    file's bytes, so it is served them). Neither name may be readable under a withheld name
///    through any travelling link ([`tddy_sandbox::is_withheld_through_a_link`]). A path served
///    only because a link reaches it must resolve to itself — a second link inside a pulled-in
///    tree is a second hop, and the walk does not take it either.
///
/// The fourth check is the rule [`tddy_sandbox::ContextManifest::of_worktree`]'s walk applies to
/// the entries it lists. It has to be applied here as well as there, because the walk decides what
/// to *advertise* while this decides what to *serve*, and this one is handed a caller-supplied name.
///
/// Containment in the root is not a substitute for it. `.claude/creds -> ../.env` canonicalizes to
/// a path squarely inside the worktree, so every check above passes — and the file served under
/// that allow-listed name is the repository's `.env`. Nor is "some link reaches it" a blanket
/// pass: [`tddy_sandbox::allow_listed_link`] withholds secret-shaped targets and
/// [`tddy_sandbox::CONTEXT_EXCLUDE_GLOBS`] from every link, so `.claude/creds` and
/// `.claude/hooksalias.json -> settings.local.json` — the hooks file the daemon owns — stay
/// refused. On the split path those bytes would cross to another host and land in the agent's
/// readable working directory.
///
/// [`tddy_sandbox::root_relative`] is shared with the walk rather than re-derived here for the
/// blunt reason that a re-derivation is what went wrong: the rule was added to the walk alone, and
/// the reader kept serving what the manifest had already refused to name.
pub(super) fn resolve_allow_listed_context_file(
    worktree_root: &Path,
    rel_path: &str,
    globs: &[&str],
    reach: &mut WorktreeReach<'_>,
) -> Result<PathBuf, Status> {
    let rel_slashed = validate_rel_path_shape(rel_path)?;

    let named = matches_context_globs(&rel_slashed, globs);
    let reach = reach.get()?;
    if !named && !reach.lists_file(&rel_slashed, &rel_slashed) {
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
    let target_allowed = if named {
        (matches_context_globs(&resolved_rel, globs)
            || reach.lists_file(&rel_slashed, &resolved_rel)
            || reach.lists_file(&resolved_rel, &resolved_rel))
            && !is_withheld_through_a_link(&rel_slashed, reach.links())
            && !is_withheld_through_a_link(&resolved_rel, reach.links())
    } else {
        // Reached only through a link, so it must *be* the file the link reaches, not a further
        // link inside the pulled-in tree.
        tddy_sandbox::normalized_context_path(&rel_slashed).as_deref() == Some(&*resolved_rel)
    };
    if !target_allowed {
        log::warn!(
            "resolve_allow_listed_context_file: rejected {rel_path:?}: it resolves to \
             {resolved_rel:?}, which the agent's context allow-list does not name and no \
             allow-listed link reaches"
        );
        return Err(Status::permission_denied(
            "file is not named by the agent's context allow-list",
        ));
    }
    Ok(canonical_file)
}
