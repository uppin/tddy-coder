//! Refusing a served manifest that names anything this session may not hold.

use tddy_rpc::Status;
use tddy_sandbox::is_reached_through_link;
use tddy_sandbox::matches_context_globs;
use tddy_sandbox::{allow_listed_link, is_withheld_through_a_link, ContextLink, ContextManifest};
use tddy_worktree_service::worktree_files::validate_rel_path_shape;

/// Refuses a manifest naming anything the session's allow-list does not, and answers the links
/// whose targets it pulls in.
///
/// The split half's manifest arrives from *another host*, and every path in it is about to be
/// joined onto a directory here and written. Re-checking it against the compiled-in list is what
/// keeps that from being an instruction the peer gets to write: a `../` or an absolute path is
/// refused by [`matches_context_globs`] outright, and so is a path outside the agent's own table
/// row.
///
/// Links are re-checked by the same rule the codebase host's walk applied
/// ([`allow_listed_link`]), because a link target is a path *that host* chose: a link whose own
/// path is not allow-listed (a glob names it, or it roots a `dir/**` glob, as `.claude` does), one
/// that climbs out of the context dir (`../../.ssh`, which recreated would be the agent's window
/// onto this host's filesystem), or one pointing at something secret-shaped (`.claude/creds ->
/// .env`) is refused, whatever the peer decided. So is a link at a [`PREAMBLE_FILES`] path: the
/// codebase host follows those and lists them as files, and a `CLAUDE.md` recreated as a link would
/// carry the managed-codebase preamble somewhere the agent does not read it, or nowhere. A path no
/// glob names is then accepted only when it is the target of a link that passed, or under one —
/// and not withheld under the name the link exposes it by — the one thing a served link may pull
/// in.
///
/// Two refusals no single entry can be judged on, because an honest walk never produces either:
///
/// - **an entry beneath a served link.** The walk lists a link and never descends it, and a
///   canonical target never passes through one, so nothing it lists sits under a link's path. A
///   served `.claude/skills/planted.md` beside a served `.claude/skills` link would be written
///   wherever the recreated link points — and nested links (`.claude/a/b -> .claude`, then
///   `.claude/a/b/a/b/a/b/q -> t`) count their `..` steps from where they are *named* but resolve
///   from where they physically land, which is outside the context dir. The writers refuse a
///   symlinked parent as well ([`prepare_context_write`]); this refuses the manifest first;
/// - **anything readable under a withheld name through any served link**
///   ([`is_withheld_through_a_link`]) — a file, a link's target or a link itself. `.claude ->
///   .agents/claude` with `.agents/claude/settings.local.json` is the daemon-owned hooks file
///   reached by another name, whatever glob names its own path.
///
/// A refusal rather than a quiet skip. A peer serving something outside the list is a defect on one
/// side or the other, and dropping it silently would leave the two halves disagreeing about what
/// the agent is reading with nothing saying so.
pub(super) fn refuse_unlisted_paths(
    served: &ContextManifest,
    globs: &[&str],
) -> Result<Vec<ContextLink>, Status> {
    let mut links = Vec::new();
    for entry in served.entries() {
        let Some(target) = &entry.symlink_target else {
            continue;
        };
        match allow_listed_link(&entry.rel_path, target, globs) {
            Some(link) => links.push(link),
            None => {
                log::warn!(
                    "context_sync: refusing the served link {:?} -> {target:?}: its name is not \
                     allow-listed or is a preamble file, or its target leaves the context dir or \
                     is withheld",
                    entry.rel_path
                );
                return Err(Status::permission_denied(format!(
                    "the context source offered the link {:?} -> {target:?}, which this session \
                     may not recreate",
                    entry.rel_path
                )));
            }
        }
    }
    if let Some(entry) = served
        .entries()
        .iter()
        .find(|entry| lies_beneath_a_link(&entry.rel_path, &links))
    {
        log::warn!(
            "context_sync: refusing the manifest: {:?} lies beneath a served link",
            entry.rel_path
        );
        return Err(Status::permission_denied(format!(
            "the context source offered {:?} beneath one of its own links, which this session may \
             not write",
            entry.rel_path
        )));
    }
    if let Some(link) = links.iter().find(|link| {
        is_withheld_through_a_link(&link.link, &links)
            || is_withheld_through_a_link(&link.target, &links)
    }) {
        log::warn!(
            "context_sync: refusing the served link {:?} -> {:?}: it or its target is readable \
             under a withheld name through a served link",
            link.link,
            link.target
        );
        return Err(Status::permission_denied(format!(
            "the context source offered the link {:?} -> {:?}, which another of its links exposes \
             under a withheld name",
            link.link, link.target
        )));
    }
    match served.entries().iter().find(|entry| {
        entry.symlink_target.is_none()
            && (!is_syncable_rel_path(&entry.rel_path, globs, &links)
                || is_withheld_through_a_link(&entry.rel_path, &links))
    }) {
        Some(entry) => Err(Status::permission_denied(format!(
            "the context source offered {:?}, which this session's allow-list does not name",
            entry.rel_path
        ))),
        None => Ok(links),
    }
}

/// Whether `rel_path` has a **strict** ancestor that is one of `links`' own paths.
fn lies_beneath_a_link(rel_path: &str, links: &[ContextLink]) -> bool {
    let Some(path) = tddy_sandbox::normalized_context_path(rel_path) else {
        return false;
    };
    links.iter().any(|link| {
        path.strip_prefix(link.link.as_str())
            .is_some_and(|rest| rest.starts_with('/'))
    })
}

/// Whether a recorded or served path is one this syncer may act on at all.
///
/// The two gates are the same pair [`crate::context_files`] applies before it reads: the *shape*
/// (`..`, a leading separator, an absolute component) decided from the string alone, then the
/// compiled-in allow-list — or, for a path no glob names, whether one of `links` pulls it in
/// ([`is_reached_through_link`]). Neither consults the filesystem, so the answer is the same for a
/// path that exists and one that does not.
pub(super) fn is_syncable_rel_path(rel_path: &str, globs: &[&str], links: &[ContextLink]) -> bool {
    match validate_rel_path_shape(rel_path) {
        Ok(rel_slashed) => {
            matches_context_globs(&rel_slashed, globs)
                || is_reached_through_link(&rel_slashed, links)
        }
        Err(_) => false,
    }
}
