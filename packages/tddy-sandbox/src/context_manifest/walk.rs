//! The one traversal of a worktree that decides what its context holds — shared by the manifest and the reader.

use crate::context_manifest::could_contain_matches;
use crate::context_manifest::git_ignore::git_ignored;
use crate::context_manifest::links::followed_preamble_target;
use crate::context_manifest::links::is_preamble_path;
use crate::context_manifest::links::is_withheld_from_links;
use crate::context_manifest::links::names_a_link;
use crate::context_manifest::links::{allow_listed_link, is_withheld_through_a_link, ContextLink};
use crate::context_manifest::matches_context_globs;
use crate::context_manifest::normalize_context_path;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One allow-listed path in a worktree: an ordinary file, or a symlink that travels as a link.
pub(crate) struct ContextFile {
    /// Worktree-root-relative and `/`-separated, the spelling both the manifest and the copier use.
    pub rel_path: String,
    /// Where the bytes live. For a link this is its canonical target, which is never read through
    /// the link — the target is listed as entries of its own. For a followed preamble link it is
    /// also the canonical target, and *is* read: those are the bytes the preamble file carries.
    pub source: PathBuf,
    /// `Some` when `rel_path` travels as a symlink: its target's root-relative path, canonical and
    /// `/`-separated. `None` for an ordinary file and for a followed preamble link.
    pub symlink_target: Option<String>,
}

/// Every allow-listed path under `root`, sorted by path. **Symlinks travel as links.**
///
/// A link is not descended into or read through; it is listed as itself, with its target, and the
/// consumer recreates it. The files behind it are listed once, under the target's own path. That is
/// what an agent's configuration actually needs — Claude Code reads skills only under `.claude/`,
/// and a repository that keeps them once under `.agents/` reaches them through
/// `.claude/skills -> ../.agents/skills`. Following the link instead listed the tree under
/// whichever of the two spellings `read_dir` reached first, which is the filesystem's order: one
/// host synced `.claude/**`, another `.agents/**`, and an agent on the second had no skills at all.
///
/// Which links travel, and what they bring ([`allow_listed_link`] is the rule):
///
/// - a link whose **own path** is allow-listed — a glob names it, or it is the root of a `dir/**`
///   glob, as `.claude -> shared/claude` is for `.claude/**` — is listed, and its target is
///   **pulled in** even when no glob names it — a file target as an entry, a directory target as
///   the files under it. Recreating a link to a path the context dir does not hold would hand the
///   agent a dangling one;
/// - **one hop only.** A link found inside a pulled-in tree names no allow-listed place of its own
///   and is dropped, so one committed link is not a path to any file in the checkout;
/// - a target that resolves **outside** the root, or that is **withheld** — the daemon's own file
///   ([`CONTEXT_EXCLUDE_GLOBS`]) or anything secret-shaped ([`CONTEXT_SECRET_GLOBS`]), at any depth
///   of a pulled-in tree — never travels, and neither does the link to it. Containment alone is
///   not the guard: `.claude/creds -> ../.env` resolves inside the root, and on the split path its
///   bytes would cross to another host into the agent's readable working directory;
/// - a pulled-in file is withheld under **either name** it has: its own path, and the path the agent
///   reads it by through the link (`shared/claude/settings.local.json` is `.claude/
///   settings.local.json` through `.claude -> shared/claude`);
/// - a link to its own ancestor (`.claude/self -> .claude`) is listed and, like every link, not
///   descended, so the walk terminates and the files it loops over are listed once;
/// - a path that travels **only because a link reaches it** — no glob names it — travels only if
///   git does not ignore it ([`git_ignored`]): the link's target, every file pulled in under it,
///   and a followed preamble link's target. Ignored files are where a checkout keeps what it must
///   not publish — local credentials, build output, caches — and a committed link into one is not a
///   reason to send it to another host. Untracked but not ignored still travels. A glob-named path
///   is not asked: gitignored agent configuration named by a glob is served on purpose (AC16);
/// - finally, a file readable under a withheld name through **any** travelling link is withheld,
///   whatever else names it, and so is a link whose target or own path is one
///   ([`is_withheld_through_a_link`]) — `.claude -> .agents/claude` would otherwise expose
///   `.agents/claude/settings.local.json` as the daemon-owned `.claude/settings.local.json`.
///
/// **A preamble file is never a link.** A symlink at a [`PREAMBLE_FILES`] path is *followed*
/// ([`followed_preamble_target`]): listed as an ordinary file whose bytes are its target's, so the
/// managed-codebase preamble always leads the file the agent reads. Its target is not pulled in by
/// that — it is listed only if a glob names it on its own.
///
/// A dangling link is skipped: it has no bytes to sync and never had.
pub(crate) fn walk_context_files(root: &Path, globs: &[&str]) -> anyhow::Result<Vec<ContextFile>> {
    let root = std::fs::canonicalize(root)?;
    Ok(context_tree(&root, globs)?.files.into_values().collect())
}

/// What the walk lists, keyed by path, and the links that travel.
struct ContextTree {
    files: BTreeMap<String, ContextFile>,
    links: Vec<ContextLink>,
}

/// The walk behind [`walk_context_files`] and [`context_reach`], under the canonical `root`.
///
/// In this order, so nothing is crawled on behalf of a link that is about to be dropped: list the
/// allow-listed tree; drop the links (and followed preamble links) whose unnamed target git
/// ignores; drop the links [`is_withheld_through_a_link`] withholds; withhold the listed files it
/// withholds; pull in what the surviving links reach; drop the pulled-in files git ignores or the
/// predicate withholds.
fn context_tree(root: &Path, globs: &[&str]) -> anyhow::Result<ContextTree> {
    let mut listed = BTreeMap::new();
    let mut found_links = Vec::new();
    collect_allow_listed(root, "", root, globs, &mut listed, &mut found_links)?;

    // A followed preamble link reads its bytes from somewhere other than its own path.
    let followed_targets: BTreeMap<String, String> = listed
        .values()
        .filter(|file| file.symlink_target.is_none())
        .filter_map(|file| {
            let source_rel = root_relative(&file.source, root)?;
            (source_rel != file.rel_path).then(|| (file.rel_path.clone(), source_rel))
        })
        .collect();
    let ignored = git_ignored(
        root,
        found_links
            .iter()
            .map(|(_, link)| link.target.as_str())
            .chain(followed_targets.values().map(String::as_str))
            .filter(|path| !matches_context_globs(path, globs)),
    );
    found_links.retain(|(_, link)| !ignored.contains(&link.target));
    let ungated: Vec<ContextLink> = found_links.iter().map(|(_, link)| link.clone()).collect();
    found_links.retain(|(_, link)| {
        !is_withheld_through_a_link(&link.target, &ungated)
            && !is_withheld_through_a_link(&link.link, &ungated)
    });
    let links: Vec<ContextLink> = found_links.iter().map(|(_, link)| link.clone()).collect();

    listed.retain(|rel_path, file| match &file.symlink_target {
        Some(_) => links.iter().any(|link| &link.link == rel_path),
        None => {
            followed_targets
                .get(rel_path)
                .is_none_or(|target| !ignored.contains(target))
                && !is_withheld_through_a_link(rel_path, &links)
        }
    });

    let mut pulled = BTreeMap::new();
    for (target, link) in &found_links {
        pull_in_link_target(target, &link.target, &link.link, &mut pulled)?;
    }
    // A pulled-in file the globs also name is listed on the globs' authority, as it already is.
    pulled.retain(|rel_path, _| !listed.contains_key(rel_path));
    let ignored = git_ignored(
        root,
        pulled
            .keys()
            .map(String::as_str)
            .filter(|path| !matches_context_globs(path, globs)),
    );
    pulled.retain(|rel_path, _| {
        !ignored.contains(rel_path) && !is_withheld_through_a_link(rel_path, &links)
    });
    // A `BTreeMap` both sorts and de-duplicates: a file two links both reach is listed once.
    listed.extend(pulled);
    Ok(ContextTree {
        files: listed,
        links,
    })
}

/// The walk's answer about one worktree, for the **reader**: the links that travel, and every
/// ordinary file the manifest lists with the root-relative path its bytes are read from.
///
/// The reader is handed a caller-supplied path and has to answer "may this be served?" exactly as
/// the walk answered "may this be listed?". Asking the walk itself — rather than re-deriving the
/// link rule, the git gate and the read-as exclusion beside it — is what keeps the two agreeing.
#[derive(Debug, Clone, Default)]
pub struct ContextReach {
    links: Vec<ContextLink>,
    files: BTreeMap<String, String>,
}

impl ContextReach {
    /// The links that travel, as [`is_withheld_through_a_link`] expects them.
    pub fn links(&self) -> &[ContextLink] {
        &self.links
    }

    /// Whether the walk lists `rel_path` as an ordinary file whose bytes are read from `read_from`
    /// — itself for a plain or pulled-in file, the target for a followed preamble link.
    pub fn lists_file(&self, rel_path: &str, read_from: &str) -> bool {
        let (Some(path), Some(read_from)) = (
            normalize_context_path(rel_path),
            normalize_context_path(read_from),
        ) else {
            return false;
        };
        self.files.get(&path) == Some(&read_from)
    }
}

/// What the walk of `root` lists ([`ContextReach`]).
pub fn context_reach(root: &Path, globs: &[&str]) -> anyhow::Result<ContextReach> {
    let root = std::fs::canonicalize(root)?;
    let tree = context_tree(&root, globs)?;
    let files = tree
        .files
        .into_values()
        .filter(|file| file.symlink_target.is_none())
        .filter_map(|file| {
            let source_rel = root_relative(&file.source, &root)?;
            Some((file.rel_path, source_rel))
        })
        .collect();
    Ok(ContextReach {
        links: tree.links,
        files,
    })
}

/// The allow-listed tree: ordinary files the globs name, followed preamble links, and the links
/// whose own path is allow-listed. `links` collects each travelling link's canonical target beside
/// the link itself.
fn collect_allow_listed(
    dir: &Path,
    dir_rel: &str,
    root: &Path,
    globs: &[&str],
    found: &mut BTreeMap<String, ContextFile>,
    links: &mut Vec<(PathBuf, ContextLink)>,
) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        // A name no glob could ever spell cannot be allow-listed.
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let rel_path = if dir_rel.is_empty() {
            name
        } else {
            format!("{dir_rel}/{name}")
        };
        // `symlink_metadata`, so a link is seen as a link rather than as whatever it points at.
        let file_type = std::fs::symlink_metadata(entry.path())?.file_type();
        if file_type.is_symlink() {
            if is_preamble_path(&rel_path) {
                if let Some(target) = resolve_preamble_link(&entry.path(), &rel_path, root, globs)?
                {
                    found.insert(
                        rel_path.clone(),
                        ContextFile {
                            rel_path,
                            source: target,
                            symlink_target: None,
                        },
                    );
                }
                continue;
            }
            // Asked before resolving, so a link no row could allow-list costs no `canonicalize` —
            // and cannot fail the walk with an error about a path that was never going to travel.
            if !names_a_link(&rel_path, globs) {
                continue;
            }
            let Some((target, link)) = resolve_link(&entry.path(), &rel_path, root, globs)? else {
                continue;
            };
            found.insert(
                rel_path.clone(),
                ContextFile {
                    rel_path,
                    source: target.clone(),
                    symlink_target: Some(link.target.clone()),
                },
            );
            links.push((target, link));
        } else if file_type.is_dir() {
            if could_contain_matches(&rel_path, globs) {
                collect_allow_listed(&entry.path(), &rel_path, root, globs, found, links)?;
            }
        } else if file_type.is_file() && matches_context_globs(&rel_path, globs) {
            found.insert(
                rel_path.clone(),
                ContextFile {
                    rel_path,
                    source: entry.path(),
                    symlink_target: None,
                },
            );
        }
    }
    Ok(())
}

/// Lists what one travelling link reaches: the file itself, or every file under the directory.
///
/// Nothing here consults the globs or git — reaching the target is the link's doing, and
/// [`context_tree`] gates what comes back — but every path is asked [`is_withheld_from_links`] as
/// it is reached, under its own path and under `read_as`, the name the agent opens it by through
/// the link, so a pulled-in tree's `.git/` or `.env` stays behind, `.claude -> shared/claude` does
/// not expose `.claude/settings.local.json`, and no symlink inside it is followed or listed (one
/// hop). [`is_reached_through_link`] asks the receiving syncer's half of the same question.
fn pull_in_link_target(
    target: &Path,
    target_rel: &str,
    read_as: &str,
    found: &mut BTreeMap<String, ContextFile>,
) -> anyhow::Result<()> {
    // `target` is canonical, so this is the target itself and never another link.
    let file_type = std::fs::symlink_metadata(target)?.file_type();
    if file_type.is_file() {
        found
            .entry(target_rel.to_string())
            .or_insert_with(|| ContextFile {
                rel_path: target_rel.to_string(),
                source: target.to_path_buf(),
                symlink_target: None,
            });
    } else if file_type.is_dir() {
        for entry in std::fs::read_dir(target)? {
            let entry = entry?;
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            let rel_path = format!("{target_rel}/{name}");
            let read_as = format!("{read_as}/{name}");
            if is_withheld_from_links(&rel_path) || is_withheld_from_links(&read_as) {
                log::debug!(
                    "context_manifest: {rel_path} (read as {read_as}) is withheld; not pulling it in"
                );
                continue;
            }
            if std::fs::symlink_metadata(entry.path())?
                .file_type()
                .is_symlink()
            {
                log::debug!(
                    "context_manifest: {rel_path} is a link inside a pulled-in tree; links pull \
                     in one hop only, so it is not listed"
                );
                continue;
            }
            pull_in_link_target(&entry.path(), &rel_path, &read_as, found)?;
        }
    }
    Ok(())
}

/// `path`'s place under `root`, `/`-separated, or `None` when it is not under `root` at all.
///
/// Both must be canonical by the time this is asked — here `root` comes from
/// [`walk_context_files`] and `path` from [`resolve_link`] — so a plain prefix strip is the whole
/// of it.
///
/// Exported because the **reader** needs the identical answer. The walk asks where a link's target
/// sits in the tree before letting it travel ([`allow_listed_link`]), and that rule is only
/// half enforced if the reader does not ask the same: it is handed a caller-supplied name, and
/// asking the allow-list about that name alone serves `.claude/creds -> ../.env` under a spelling
/// every glob matches. It has to ask about the resolved target's own root-relative path too, which
/// is this — and it has to be *this* function rather than a second prefix strip beside it, because
/// the drift between the two halves of that rule is what made the reader serve the file the walk
/// had already refused.
pub fn root_relative(path: &Path, root: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let joined = rel
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    (!joined.is_empty()).then_some(joined)
}

/// Where the link at `entry` points, as `(canonical target, root-relative target)`, or `None` when
/// it dangles or resolves outside `root`.
///
/// A missing target is the one error swallowed here: a link pointing at a file the project deleted
/// has no bytes to sync and never had. Every other error — `EACCES`, `ELOOP`, `EIO` — is
/// propagated, because treating it as "skip" would quietly drop a link the project still ships, and
/// a dropped manifest entry is a *delete* in the agent's context directory: a transient error would
/// withdraw guidance with nothing anywhere saying so.
fn resolve_within_root(entry: &Path, root: &Path) -> anyhow::Result<Option<(PathBuf, String)>> {
    let target = match std::fs::canonicalize(entry) {
        Ok(target) => target,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            log::debug!("context_manifest: {entry:?} is a dangling symlink; skipping it");
            return Ok(None);
        }
        Err(e) => {
            log::error!("context_manifest: resolving the symlink {entry:?} failed: {e}");
            return Err(
                anyhow::Error::new(e).context(format!("failed to resolve the symlink {entry:?}"))
            );
        }
    };
    // `None` for a target outside the root, and for the root itself — a link to the whole checkout
    // is not agent configuration.
    let Some(target_rel) = root_relative(&target, root) else {
        log::debug!("context_manifest: {entry:?} resolves outside {root:?}; skipping it");
        return Ok(None);
    };
    Ok(Some((target, target_rel)))
}

/// The link at `entry` (spelled `link_rel`), as `(canonical target, the link)`, or `None` when it
/// must not travel ([`resolve_within_root`], then [`allow_listed_link`]).
fn resolve_link(
    entry: &Path,
    link_rel: &str,
    root: &Path,
    globs: &[&str],
) -> anyhow::Result<Option<(PathBuf, ContextLink)>> {
    let Some((target, target_rel)) = resolve_within_root(entry, root)? else {
        return Ok(None);
    };
    match allow_listed_link(link_rel, &target_rel, globs) {
        Some(link) => Ok(Some((target, link))),
        None => {
            log::debug!(
                "context_manifest: {link_rel} -> {target_rel} reaches a withheld path; skipping it"
            );
            Ok(None)
        }
    }
}

/// The canonical file a symlink at the preamble path `link_rel` is followed to, or `None` when it
/// may not be ([`resolve_within_root`], then [`followed_preamble_target`]) or does not lead to a
/// file — a `CLAUDE.md` that is a directory has no bytes to put the preamble in front of.
fn resolve_preamble_link(
    entry: &Path,
    link_rel: &str,
    root: &Path,
    globs: &[&str],
) -> anyhow::Result<Option<PathBuf>> {
    let Some((target, target_rel)) = resolve_within_root(entry, root)? else {
        return Ok(None);
    };
    if followed_preamble_target(link_rel, &target_rel, globs).is_none() {
        log::debug!(
            "context_manifest: {link_rel} -> {target_rel} is not allow-listed or reaches a \
             withheld path; skipping it"
        );
        return Ok(None);
    }
    if !std::fs::metadata(&target)?.is_file() {
        log::debug!("context_manifest: {link_rel} -> {target_rel} is not a file; skipping it");
        return Ok(None);
    }
    Ok(Some(target))
}
