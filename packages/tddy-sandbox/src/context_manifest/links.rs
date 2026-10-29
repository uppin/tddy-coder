//! Which symlinks an agent's context may carry, what each may pull in, and how one is written.

use crate::context_dir::PREAMBLE_FILES;
use crate::context_manifest::is_excluded;
use crate::context_manifest::matches_any_exclusion;
use crate::context_manifest::matches_context_globs;
use crate::context_manifest::normalize_context_path;
use crate::context_manifest::with_compiled_pattern;
use crate::context_manifest::ALLOW_MATCH_OPTIONS;
use std::path::{Path, PathBuf};

/// Paths a symlink may never carry into a context sync, at any depth, however its own name is
/// spelled.
///
/// An allow-listed link pulls its target in even when no glob names the target (see
/// [`walk_context_files`](super::walk::walk_context_files)), which turns every committed link under `.claude/` into a statement
/// about what else may travel. That widening is what the agent's skills need, and it is also a path
/// to `.claude/creds -> ../.env`, so the shapes a secret takes in a checkout are refused by name
/// rather than left to whether some glob happens to miss them: dotenv files, private keys, and the
/// `.git` directory — whose `config` carries remotes and credential helpers and whose objects are
/// the whole history.
///
/// Matched like [`CONTEXT_EXCLUDE_GLOBS`](super::CONTEXT_EXCLUDE_GLOBS), case-insensitively, for the same reason: an exclusion
/// may not under-match, and on macOS `.ENV` is the same file as `.env`.
///
/// This gates only what a **link** reaches. A file the allow-list names outright is the project's
/// own agent configuration and is listed as before.
pub const CONTEXT_SECRET_GLOBS: &[&str] = &[
    "**/.env",
    "**/.env.*",
    "**/*.pem",
    "**/*.key",
    "**/.git",
    "**/.git/**",
];

/// Whether a link may never reach `path` (already normalized): the daemon owns it
/// ([`CONTEXT_EXCLUDE_GLOBS`](super::CONTEXT_EXCLUDE_GLOBS)) or it is shaped like a secret ([`CONTEXT_SECRET_GLOBS`]).
pub(super) fn is_withheld_from_links(path: &str) -> bool {
    is_excluded(path) || matches_any_exclusion(path, CONTEXT_SECRET_GLOBS)
}

/// Whether `path` (already normalized) **or any directory above it** is withheld from links.
///
/// Asked of a link's target, not just of its full path: `**/.env` does not match
/// `config/.env/prod`, yet that file sits inside a secret as surely as `config/.env` is one. The
/// walk already asks segment by segment as it descends a pulled-in tree; this is the same question
/// for the segments *above* the place the link lands.
fn is_withheld_at_any_depth(path: &str) -> bool {
    let mut prefix = String::with_capacity(path.len());
    path.split('/').any(|segment| {
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(segment);
        is_withheld_from_links(&prefix)
    })
}

/// `path`'s remainder below `ancestor` (`""` for `ancestor` itself), or `None` when `path` is not
/// `ancestor` or beneath it — `tools/agent-skills-old` is not under `tools/agent-skills`.
fn below<'a>(path: &'a str, ancestor: &str) -> Option<&'a str> {
    let rest = path.strip_prefix(ancestor)?;
    if rest.is_empty() {
        Some(rest)
    } else {
        rest.strip_prefix('/')
    }
}

/// Whether `rel_path` can be opened, through **any** of `links`, under a name a link may never
/// reach — excluded ([`CONTEXT_EXCLUDE_GLOBS`](super::CONTEXT_EXCLUDE_GLOBS)) or secret-shaped ([`CONTEXT_SECRET_GLOBS`]).
///
/// `.claude -> .agents/claude` makes `.agents/claude/settings.local.json` the agent's
/// `.claude/settings.local.json`, the file the daemon's hooks writer owns, even though `.agents/**`
/// names it in its own right and its own path is excluded by nothing. Asked per link and answered
/// with `any`, so one link's innocent spelling (`.agents/hooks.json ->
/// shared/claude/settings.local.json`) cannot reopen a file another link exposes under the
/// excluded one.
///
/// `links` must be the travelling links ([`allow_listed_link`]'s answers that survived the walk).
/// One predicate for every half: the walk withholds such a file — and a link whose target or own
/// path is one — after it has listed everything, the reader refuses it, and the receiving syncer
/// refuses a manifest that offers it.
pub fn is_withheld_through_a_link(rel_path: &str, links: &[ContextLink]) -> bool {
    let Some(path) = normalize_context_path(rel_path) else {
        return false;
    };
    links.iter().any(|link| {
        let Some(rest) = below(&path, &link.target) else {
            return false;
        };
        let mut read_as = link.link.clone();
        is_withheld_from_links(&read_as)
            || rest
                .split('/')
                .filter(|segment| !segment.is_empty())
                .any(|segment| {
                    read_as.push('/');
                    read_as.push_str(segment);
                    is_withheld_from_links(&read_as)
                })
    })
}

/// Whether a link at `link_rel` names an allow-listed place: a glob names the path itself, or the
/// path is the **root of a `dir/**` glob** (`.claude` for `.claude/**`).
///
/// The second half is what lets a repository that keeps its whole Claude configuration elsewhere
/// (`.claude -> shared/claude`, or `.cursor -> .claude`) still ship it. `.claude/**` names every
/// path *inside* `.claude` and, to the `glob` crate, not `.claude` itself — so a link standing in
/// for the whole directory was asked about a name no row spells and dropped, along with everything
/// the agent reads through it. The directory is as much the agent's configuration as its contents
/// are; what it exposes is gated per file by [`is_reached_through_link`], under the name it is read
/// by through the link as well as its own.
///
/// Asked of links only. An ordinary directory at that path is walked, and its files are asked
/// about one by one, exactly as before.
pub(super) fn names_a_link(link_rel: &str, globs: &[&str]) -> bool {
    if matches_context_globs(link_rel, globs) {
        return true;
    }
    let Some(path) = normalize_context_path(link_rel) else {
        return false;
    };
    let roots_a_directory_glob = globs.iter().any(|glob| {
        glob.strip_suffix("/**").is_some_and(|dir| {
            with_compiled_pattern(dir, |pattern| {
                pattern.matches_with(&path, ALLOW_MATCH_OPTIONS)
            })
            .unwrap_or(false)
        })
    });
    roots_a_directory_glob && !is_excluded(&path)
}

/// Whether `rel_path` (already normalized) is one of [`PREAMBLE_FILES`] — a path a link may never
/// stand at, because the consumer writes the managed-codebase preamble in front of it.
pub(super) fn is_preamble_path(rel_path: &str) -> bool {
    PREAMBLE_FILES.contains(&rel_path)
}

/// One link that may travel: where it stands and where it points, both worktree-root-relative and
/// normalized — the answer of [`allow_listed_link`].
///
/// Both ends travel together because both are names a file behind the link is read by. A file
/// pulled in at `shared/claude/settings.local.json` through `.claude -> shared/claude` is opened by
/// the agent as `.claude/settings.local.json`, and only the pair says so.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextLink {
    /// The link's own path.
    pub link: String,
    /// The path it resolves to.
    pub target: String,
}

/// The link at `link_rel` pointing at `target_rel` (both worktree-root-relative), normalized, or
/// `None` when that link must not travel.
///
/// The one rule every half applies to a link — the walk that lists it, the reader that serves what
/// it reaches, and the syncer that recreates it from a manifest another host sent:
///
/// - the link's **own** path must be allow-listed — a glob names it, or it is the root of a
///   `dir/**` glob ([`names_a_link`]). Only that makes it part of the agent's configuration; a link
///   found anywhere else, including inside a tree another link pulled in, is not, which is what
///   keeps pulling a target in to one hop rather than a crawl;
/// - it must not stand at a **preamble path** ([`PREAMBLE_FILES`]). The consumer writes the
///   managed-codebase preamble in front of `CLAUDE.md`, and a recreated `CLAUDE.md -> AGENTS.md`
///   would put it wherever the link points — or nowhere, for a target that is not itself a
///   preamble file. The walk follows such a link instead ([`followed_preamble_target`]), so on the
///   receiving side one is a peer defect and refused;
/// - the **target** must be a relative path that stays inside the root — on the receiving side it
///   is a path the peer chose, so `../../.ssh` is refused here rather than recreated;
/// - and neither the target nor any directory above it may be withheld
///   ([`is_withheld_at_any_depth`]) — `config/.env/prod` is inside a secret.
///
/// What it cannot answer from strings alone is left to the walk: whether git ignores the target,
/// and whether the target is readable under a withheld name through another link
/// ([`is_withheld_through_a_link`]).
pub fn allow_listed_link(link_rel: &str, target_rel: &str, globs: &[&str]) -> Option<ContextLink> {
    let link = normalize_context_path(link_rel)?;
    if is_preamble_path(&link) || !names_a_link(&link, globs) {
        return None;
    }
    let target = normalize_context_path(target_rel)?;
    (!is_withheld_at_any_depth(&target)).then_some(ContextLink { link, target })
}

/// The normalized target a symlink at the preamble path `link_rel` is **followed** to, or `None`
/// when `link_rel` is not a preamble file the globs name, or the target is withheld.
///
/// A preamble file is never a link ([`allow_listed_link`]), so `CLAUDE.md -> AGENTS.md` — this
/// repository's own layout — is listed as an ordinary `CLAUDE.md` carrying its target's bytes, and
/// the reader serves those bytes under that name. The target need not be glob-named: it is not
/// pulled in as an entry of its own, only read *as* the preamble file. It is still held to every
/// guard a link target is — inside the root, never excluded, never secret-shaped at any depth —
/// because `CLAUDE.md -> .env` would otherwise carry the secret under the one name every row
/// allow-lists. The walk also holds it to the git gate when no glob names it ([`ContextReach`]).
///
/// The reader does not ask this directly: it asks the walk's own answer ([`ContextReach`]), so the
/// file one lists is the file the other serves.
pub(crate) fn followed_preamble_target(
    link_rel: &str,
    target_rel: &str,
    globs: &[&str],
) -> Option<String> {
    let link = normalize_context_path(link_rel)?;
    if !is_preamble_path(&link) || !matches_context_globs(&link, globs) {
        return None;
    }
    let target = normalize_context_path(target_rel)?;
    (!is_withheld_at_any_depth(&target)).then_some(target)
}

/// Whether `rel_path` is one of `links`' targets or lies beneath one, so an allow-listed link pulls
/// it in although no glob names it.
///
/// `links` must be the answers of [`allow_listed_link`]. Below a target, every path segment is
/// asked the withheld question again — a pulled-in tree is somebody else's directory, and its
/// `.git/` or `.env` is as secret there as anywhere — and asked of **both spellings**: the path
/// under the target, and the path under the link, which is the name the agent opens it by.
/// `.claude -> shared/claude` makes `shared/claude/settings.local.json` the agent's
/// `.claude/settings.local.json`, the file the daemon owns, so it stays behind although its own path
/// is excluded by nothing. This is the same pruning the walk applies as it descends.
///
/// Strings only, so it is the **receiving syncer's** question about a manifest another host sent;
/// it cannot see git, and it answers for one link at a time — the read-as exclusion across every
/// link is [`is_withheld_through_a_link`]. The reader asks the walk itself ([`context_reach`](super::walk::context_reach)).
pub fn is_reached_through_link(rel_path: &str, links: &[ContextLink]) -> bool {
    let Some(path) = normalize_context_path(rel_path) else {
        return false;
    };
    links.iter().any(|link| {
        let Some(rest) = below(&path, &link.target) else {
            return false;
        };
        let mut under_target = link.target.clone();
        let mut under_link = link.link.clone();
        rest.split('/')
            .filter(|segment| !segment.is_empty())
            .all(|segment| {
                under_target.push('/');
                under_target.push_str(segment);
                under_link.push('/');
                under_link.push_str(segment);
                !is_withheld_from_links(&under_target) && !is_withheld_from_links(&under_link)
            })
    })
}

/// What to write into a symlink at `link_rel` so it resolves to `target_rel`, both
/// worktree-root-relative: the target spelled **relative to the link's own directory**
/// (`.claude/skills` -> `../.agents/skills`).
///
/// Relative rather than absolute because the link is recreated inside a context directory that is
/// not the repository; an absolute target would point back at a path on the host that built the
/// manifest, which on the split path is not even this machine.
pub(crate) fn relative_link_target(link_rel: &str, target_rel: &str) -> PathBuf {
    let link_parent: Vec<&str> = {
        let mut segments: Vec<&str> = link_rel.split('/').filter(|s| !s.is_empty()).collect();
        segments.pop();
        segments
    };
    let target: Vec<&str> = target_rel.split('/').filter(|s| !s.is_empty()).collect();
    let common = link_parent
        .iter()
        .zip(&target)
        .take_while(|(left, right)| left == right)
        .count();
    let mut relative = PathBuf::new();
    for _ in common..link_parent.len() {
        relative.push("..");
    }
    for segment in &target[common..] {
        relative.push(segment);
    }
    if relative.as_os_str().is_empty() {
        // A link to its own directory (`.claude/self -> .claude`).
        relative.push(".");
    }
    relative
}

/// Creates the symlink at `link_path` (the on-disk spelling of `link_rel`) pointing at
/// `target_rel`, written relative per [`relative_link_target`].
///
/// Shared by the copier and the syncer so the two write the same link for the same entry.
pub fn create_context_link(
    link_path: &Path,
    link_rel: &str,
    target_rel: &str,
) -> std::io::Result<()> {
    let relative = relative_link_target(link_rel, target_rel);
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(relative, link_path)
    }
    #[cfg(not(unix))]
    {
        // TODO(agent-context-sync): a context link on a platform without unix symlinks. Nothing
        // here runs off unix today; refused loudly rather than flattened into a copy.
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            format!(
                "cannot create the context link {link_path:?} -> {relative:?} on this platform"
            ),
        ))
    }
}

/// Where to write the entry `rel_path` inside `context_dir`, once every directory above it exists
/// as a **real directory** inside `context_dir` — or `PermissionDenied` when one is a symlink.
///
/// A recreated link sits on the path of a later entry only if the manifest put one beneath a
/// link, which an honest walk never does: it lists a link and never descends it. But a manifest
/// from another host is that host's choice, and string checks alone do not see what the
/// filesystem does. `.claude/a/b -> .claude` makes `.claude/a/b/a/b/a/b/q` physically live in
/// `.claude`, so a link there written as `../../../../../../../t` — counted from its *named*
/// parent — climbs six levels above the context dir, and a file beneath it lands wherever `t`
/// names. `create_dir_all` and `std::fs::write` follow every one of those links.
///
/// So the parent is built one component at a time with `symlink_metadata`, refusing a symlink
/// before anything is created through it, and then its canonical path is required to be
/// `canonical(context_dir)` joined with the logical parent — no component of it a link. Shared by
/// the copier and both of the syncer's writers so none of them writes through a link.
pub fn prepare_context_write(context_dir: &Path, rel_path: &str) -> std::io::Result<PathBuf> {
    use std::io::{Error, ErrorKind};

    let rel = normalize_context_path(rel_path).ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidInput,
            format!("{rel_path:?} names no path under the context dir"),
        )
    })?;
    std::fs::create_dir_all(context_dir)?;
    let mut segments: Vec<&str> = rel.split('/').collect();
    segments.pop();
    let mut dir = context_dir.to_path_buf();
    for segment in &segments {
        dir.push(segment);
        match std::fs::symlink_metadata(&dir) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                log::warn!(
                    "context_manifest: refusing to write {rel:?}: {dir:?} above it is a symlink"
                );
                return Err(Error::new(
                    ErrorKind::PermissionDenied,
                    format!("cannot write {rel:?} into the context dir: {dir:?} is a symlink"),
                ));
            }
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => {
                return Err(Error::new(
                    ErrorKind::AlreadyExists,
                    format!(
                        "cannot write {rel:?} into the context dir: {dir:?} is not a directory"
                    ),
                ))
            }
            Err(e) if e.kind() == ErrorKind::NotFound => std::fs::create_dir(&dir)?,
            Err(e) => return Err(e),
        }
    }
    let expected = segments
        .iter()
        .fold(std::fs::canonicalize(context_dir)?, |path, segment| {
            path.join(segment)
        });
    let actual = std::fs::canonicalize(&dir)?;
    if actual != expected {
        log::warn!(
            "context_manifest: refusing to write {rel:?}: its parent resolves to {actual:?}, not \
             {expected:?}"
        );
        return Err(Error::new(
            ErrorKind::PermissionDenied,
            format!("cannot write {rel:?}: its parent directory resolves outside the context dir"),
        ));
    }
    Ok(context_dir.join(&rel))
}
