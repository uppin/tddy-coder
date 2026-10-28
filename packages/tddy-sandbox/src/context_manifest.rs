//! What counts as an agent's *context*, and the smallest set of changes that brings a context
//! directory back in line with the target repo.
//!
//! Three things have to agree on the answer: the copier that builds the directory at spawn, the
//! manifest the syncer diffs on every `worktree.activity` tick, and the reader that serves the
//! bytes. They agree because they all go through [`matches_context_globs`] and
//! [`walk_context_files`] here — a second traversal with its own idea of containment is how a
//! manifest ends up advertising a path the reader then refuses.
//!
//! PRD: docs/ft/daemon/agent-context-sync.md § Design.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, PoisonError, RwLock};

use glob::{MatchOptions, Pattern};
use sha2::{Digest, Sha256};

/// How the **allow-list** is matched.
///
/// `require_literal_separator` is the load-bearing one: without it a root-level `CLAUDE.md` would
/// also claim `vendor/some-crate/CLAUDE.md`, because `*` and `?` would happily cross a `/`.
/// Case sensitivity holds even on macOS, where the filesystem does not care but the repository
/// records the case it committed — `claude.md` is not the file Claude Code reads. Leading dots are
/// deliberately *not* literal: nearly every pattern here names a dotted directory.
const ALLOW_MATCH_OPTIONS: MatchOptions = MatchOptions {
    case_sensitive: true,
    require_literal_separator: true,
    require_literal_leading_dot: false,
};

/// How [`CONTEXT_EXCLUDE_GLOBS`] is matched — the same options, **case-insensitively**.
///
/// The asymmetry with [`ALLOW_MATCH_OPTIONS`] is deliberate, and it is the difference between what
/// the two tables cost when they are wrong. An allow-list may under-match safely: the worst a
/// case that does not line up can do is leave a file out of the sync, and the agent reads less
/// guidance than the project ships. An exclusion may **not** under-match: the worst *it* can do is
/// serve the file the daemon owns. On macOS and Windows the filesystem is case-insensitive, so
/// `.claude/Settings.local.json` and `.claude/settings.local.json` are one file — a case-sensitive
/// exclusion is then a one-keystroke bypass of a rule that exists to stop the sync racing the
/// daemon's own hooks writer.
///
/// So the exclusion is asked about the file the operating system would actually open, while the
/// allow-list stays asked about the spelling the repository committed.
const EXCLUDE_MATCH_OPTIONS: MatchOptions = MatchOptions {
    case_sensitive: false,
    require_literal_separator: true,
    require_literal_leading_dot: false,
};

/// Paths no agent's context sync may carry, however generously the positive table names them.
///
/// One entry, and it earns its exclusion list. `.claude/settings.local.json` is the file the daemon
/// **writes** into a managed session's working directory: `write_claude_hooks_settings`
/// (`tddy_daemon::connection_service`) renders the six Claude Code hooks that report a session's
/// status and stores them there, atomically and as a whole-file replace. Syncing the repository's
/// own copy into the same directory is therefore not "extra guidance" but a race with an obvious
/// loser — at setup the sync writes first and the hooks overwrite it, and once the re-sync trigger
/// lands, the first edit to the repo's copy would land *after* the hooks and silently disable
/// status reporting for the rest of the session.
///
/// So the daemon owns that path on a managed session, and the sync does not touch it. This is a
/// *narrowing* of the compiled-in table rather than a fallback: it can only ever sync less.
///
/// It lives here, beside [`matches_context_globs`], rather than in `tddy_core`'s per-agent table,
/// so that **both halves are excluded by one check**. Every consumer — the copier that builds a
/// context directory, [`ContextManifest::of_worktree`], the daemon's allow-list reader and the
/// syncer's delete path — asks this one predicate, and a second list consulted by only one of them
/// is precisely how a manifest comes to advertise a path the reader then refuses. `tddy_core`'s
/// table documents the omission; this is where it is enforced.
pub const CONTEXT_EXCLUDE_GLOBS: &[&str] = &[".claude/settings.local.json"];

/// Whether a worktree-root-relative path is named by any of `globs` and by no exclusion.
///
/// This single predicate gates everything downstream, so it refuses rather than resolves anything
/// that could reach outside the worktree: an absolute path or one containing `..` is a bug in the
/// caller, not a path to normalize. Redundant `./` prefixes and `\` separators *are* normalized,
/// because both spell the same file and a disagreement here surfaces as a file that silently stops
/// syncing.
///
/// [`CONTEXT_EXCLUDE_GLOBS`] is checked **after** the allow-list and wins over it, because the
/// `glob` crate has no negation and narrowing the positive patterns instead would replace one
/// readable `.claude/**` with a thicket of patterns nobody could review.
///
/// A pattern that does not parse matches nothing. The glob tables are compiled in, so an
/// unparsable one is a programming error — logged at `error` rather than swallowed, because
/// treating it as matching nothing disables a whole tree (`.claude/**` mistyped is every Claude
/// setting gone) on both halves at once, and the symptom is an agent that quietly stops reading its
/// project's rules.
pub fn matches_context_globs(rel_path: &str, globs: &[&str]) -> bool {
    let Some(path) = normalize_context_path(rel_path) else {
        return false;
    };
    let allowed = globs.iter().any(|glob| {
        with_compiled_pattern(glob, |pattern| {
            pattern.matches_with(&path, ALLOW_MATCH_OPTIONS)
        })
        .unwrap_or(false)
    });
    allowed && !is_excluded(&path)
}

/// Whether an already-normalized path is one [`CONTEXT_EXCLUDE_GLOBS`] withholds.
fn is_excluded(path: &str) -> bool {
    matches_any_exclusion(path, CONTEXT_EXCLUDE_GLOBS)
}

fn matches_any_exclusion(path: &str, exclusions: &[&str]) -> bool {
    exclusions.iter().any(|glob| {
        with_compiled_pattern(glob, |pattern| {
            pattern.matches_with(path, EXCLUDE_MATCH_OPTIONS)
        })
        .unwrap_or(false)
    })
}

/// Paths a symlink may never carry into a context sync, at any depth, however its own name is
/// spelled.
///
/// An allow-listed link pulls its target in even when no glob names the target (see
/// [`walk_context_files`]), which turns every committed link under `.claude/` into a statement
/// about what else may travel. That widening is what the agent's skills need, and it is also a path
/// to `.claude/creds -> ../.env`, so the shapes a secret takes in a checkout are refused by name
/// rather than left to whether some glob happens to miss them: dotenv files, private keys, and the
/// `.git` directory — whose `config` carries remotes and credential helpers and whose objects are
/// the whole history.
///
/// Matched like [`CONTEXT_EXCLUDE_GLOBS`], case-insensitively, for the same reason: an exclusion
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
/// ([`CONTEXT_EXCLUDE_GLOBS`]) or it is shaped like a secret ([`CONTEXT_SECRET_GLOBS`]).
fn is_withheld_from_links(path: &str) -> bool {
    is_excluded(path) || matches_any_exclusion(path, CONTEXT_SECRET_GLOBS)
}

/// The normalized target of a link at `link_rel` pointing at `target_rel` (both
/// worktree-root-relative), or `None` when that link must not travel.
///
/// The one rule every half applies to a link — the walk that lists it, the reader that serves what
/// it reaches, and the syncer that recreates it from a manifest another host sent:
///
/// - the link's **own** path must be allow-listed. Only that makes it part of the agent's
///   configuration; a link found anywhere else, including inside a tree another link pulled in, is
///   not, which is what keeps pulling a target in to one hop rather than a crawl;
/// - the **target** must be a relative path that stays inside the root — on the receiving side it
///   is a path the peer chose, so `../../.ssh` is refused here rather than recreated;
/// - and the target must not be withheld ([`is_withheld_from_links`]).
pub fn allow_listed_link_target(
    link_rel: &str,
    target_rel: &str,
    globs: &[&str],
) -> Option<String> {
    if !matches_context_globs(link_rel, globs) {
        return None;
    }
    let target = normalize_context_path(target_rel)?;
    (!is_withheld_from_links(&target)).then_some(target)
}

/// Whether `rel_path` is one of `link_targets` or lies beneath one, so an allow-listed link pulls
/// it in although no glob names it.
///
/// `link_targets` must be the answers of [`allow_listed_link_target`]. Below a target, every path
/// segment is asked the withheld question again — a pulled-in tree is somebody else's directory,
/// and its `.git/` or `.env` is as secret there as anywhere — which is the same pruning the walk
/// applies as it descends, so the reader and the manifest refuse the same files.
pub fn is_reached_through_link(rel_path: &str, link_targets: &[String]) -> bool {
    let Some(path) = normalize_context_path(rel_path) else {
        return false;
    };
    link_targets.iter().any(|target| {
        let Some(below) = path.strip_prefix(target.as_str()) else {
            return false;
        };
        if !below.is_empty() && !below.starts_with('/') {
            // `tools/agent-skills-old` is not under `tools/agent-skills`.
            return false;
        }
        let mut prefix = target.clone();
        below
            .split('/')
            .filter(|segment| !segment.is_empty())
            .all(|segment| {
                prefix.push('/');
                prefix.push_str(segment);
                !is_withheld_from_links(&prefix)
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
pub fn relative_link_target(link_rel: &str, target_rel: &str) -> PathBuf {
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

/// Every glob this process has been asked about, compiled once.
///
/// Memoized rather than re-parsed, and both halves of that matter:
///
/// - **cost.** [`matches_context_globs`] is asked twice per file and once per directory segment of
///   a walk that runs on every `worktree.activity` tick, so a 120-file tree used to spend roughly
///   1200 `Pattern::new` parses every couple of seconds per session, for a table of five patterns.
/// - **noise.** A parse failure is logged at `error`, and it has to be: a mistyped `.claude/**`
///   silently disables every Claude setting on both halves at once. But logging it *per match*
///   turned one bad row into hundreds of identical lines per manifest build. Compiling once means
///   the failure is reported once, when the pattern is first constructed, which is where it
///   belongs.
///
/// Growth is bounded by construction: the keys are the compiled-in tables' rows and the individual
/// path segments of those rows, never anything a caller or a peer names.
static COMPILED_PATTERNS: LazyLock<RwLock<HashMap<String, Option<Pattern>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Runs `f` against `glob` compiled, or answers `None` when it does not parse.
///
/// Every caller treats `None` as "matches nothing", which is the safe reading — but a silent one
/// would let a typo in a compiled-in table turn off an entire configuration tree with nothing
/// anywhere saying so, hence the `error` at construction.
///
/// A closure rather than a returned `Pattern` so a cache hit costs a lock and a lookup instead of
/// cloning the pattern's token vector on every one of the thousand-odd matches a walk performs.
fn with_compiled_pattern<R>(glob: &str, f: impl FnOnce(&Pattern) -> R) -> Option<R> {
    {
        let cache = COMPILED_PATTERNS
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(compiled) = cache.get(glob) {
            return compiled.as_ref().map(f);
        }
    }
    let compiled = match Pattern::new(glob) {
        Ok(pattern) => Some(pattern),
        Err(e) => {
            log::error!(
                "context_manifest: the compiled-in context glob {glob:?} does not parse ({e}); it \
                 matches nothing, so everything it was meant to name is out of sync"
            );
            None
        }
    };
    let mut cache = COMPILED_PATTERNS
        .write()
        .unwrap_or_else(PoisonError::into_inner);
    // Another thread may have compiled the same glob between the two locks; either answer is the
    // same pattern, so whichever landed first stays.
    cache
        .entry(glob.to_string())
        .or_insert(compiled)
        .as_ref()
        .map(f)
}

/// The one spelling of a context path every consumer must key off, or `None` when the string is not
/// a worktree-root-relative path to a file at all.
///
/// Exported because [`matches_context_globs`] normalizes *internally* and answers only a `bool`, so
/// a caller that gates on it and then joins the raw string is working with two different paths: a
/// served `./CLAUDE.md` passes the gate and then fails a `PREAMBLE_FILES` lookup, and a served
/// `.claude\settings.json` passes the gate and then creates a file with a backslash in its name on
/// Linux. Both are silent — the file is written, just not the file the gate approved. Normalizing
/// once at the boundary and keying everything downstream off *that* string is what keeps the path
/// the allow-list answered about and the path on disk the same path.
pub fn normalized_context_path(rel_path: &str) -> Option<String> {
    normalize_context_path(rel_path)
}

/// `None` for anything that is not a worktree-root-relative path to a file.
fn normalize_context_path(rel_path: &str) -> Option<String> {
    let slashed = rel_path.replace('\\', "/");
    if slashed.starts_with('/') {
        return None;
    }
    let mut segments = Vec::new();
    for segment in slashed.split('/') {
        match segment {
            ".." => return None,
            "" | "." => continue,
            other => segments.push(other),
        }
    }
    if segments.is_empty() {
        return None;
    }
    Some(segments.join("/"))
}

/// Whether descending into `dir_rel` could still turn up a match, so a manifest of a working
/// repository does not walk `target/`, `node_modules/` and `.git/` once every couple of seconds.
///
/// Conservative by construction: a `**` in the pattern absorbs any remaining depth and answers yes
/// immediately, so pruning can cost an unnecessary `read_dir` but can never hide a file.
fn could_contain_matches(dir_rel: &str, globs: &[&str]) -> bool {
    globs
        .iter()
        .any(|glob| pattern_reaches_below(glob, dir_rel))
}

fn pattern_reaches_below(glob: &str, dir_rel: &str) -> bool {
    let mut pattern_segments = glob.split('/');
    for dir_segment in dir_rel.split('/') {
        match pattern_segments.next() {
            Some("**") => return true,
            Some(segment) => {
                // An unparsable segment reaches nothing, so the walk does not descend. Not logged
                // here: `with_compiled_pattern` already named it once, at construction, and the
                // whole glob it came out of is unparsable too (a bad segment such as `a**b` makes
                // `Pattern::new` fail on the full pattern), so `matches_context_globs` reports that
                // half. Repeating it here fired once per directory of every walk.
                let reaches = with_compiled_pattern(segment, |pattern| {
                    pattern.matches_with(dir_segment, ALLOW_MATCH_OPTIONS)
                });
                if reaches != Some(true) {
                    return false;
                }
            }
            // The pattern ran out of segments at this depth: it names a file here, not below.
            None => return false,
        }
    }
    pattern_segments.next().is_some()
}

/// One allow-listed path in a worktree: an ordinary file, or a symlink that travels as a link.
pub(crate) struct ContextFile {
    /// Worktree-root-relative and `/`-separated, the spelling both the manifest and the copier use.
    pub rel_path: String,
    /// Where the bytes live. For a link this is its canonical target, which is never read through
    /// the link — the target is listed as entries of its own.
    pub source: PathBuf,
    /// `Some` when `rel_path` is a symlink: its target's root-relative path, canonical and
    /// `/`-separated.
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
/// Which links travel, and what they bring ([`allow_listed_link_target`] is the rule):
///
/// - a link whose **own path** is allow-listed is listed, and its target is **pulled in** even when
///   no glob names it — a file target as an entry, a directory target as the files under it.
///   Recreating a link to a path the context dir does not hold would hand the agent a dangling one;
/// - **one hop only.** A link found inside a pulled-in tree names no allow-listed place of its own
///   and is dropped, so one committed link is not a path to any file in the checkout;
/// - a target that resolves **outside** the root, or that is **withheld** — the daemon's own file
///   ([`CONTEXT_EXCLUDE_GLOBS`]) or anything secret-shaped ([`CONTEXT_SECRET_GLOBS`]), at any depth
///   of a pulled-in tree — never travels, and neither does the link to it. Containment alone is
///   not the guard: `.claude/creds -> ../.env` resolves inside the root, and on the split path its
///   bytes would cross to another host into the agent's readable working directory;
/// - a link to its own ancestor (`.claude/self -> .claude`) is listed and, like every link, not
///   descended, so the walk terminates and the files it loops over are listed once.
///
/// A dangling link is skipped: it has no bytes to sync and never had.
pub(crate) fn walk_context_files(root: &Path, globs: &[&str]) -> anyhow::Result<Vec<ContextFile>> {
    let root = std::fs::canonicalize(root)?;
    let mut found = BTreeMap::new();
    let mut links = Vec::new();
    collect_allow_listed(&root, "", &root, globs, &mut found, &mut links)?;
    for (target, target_rel) in links {
        pull_in_link_target(&target, &target_rel, &mut found)?;
    }
    // A `BTreeMap` both sorts and de-duplicates: a pulled-in file the globs also name, or one two
    // links both reach, is listed once.
    Ok(found.into_values().collect())
}

/// The targets of every allow-listed link under `root` — the paths [`walk_context_files`] pulls in
/// although no glob names them, as [`is_reached_through_link`] expects them.
///
/// Exported for the **reader**, which is handed a caller-supplied path and has to answer "does an
/// allow-listed link reach this?" exactly as the walk did. Asking this rather than re-deriving the
/// rule is what keeps the manifest and the reader agreeing; it walks only the allow-listed tree, not
/// what the links pull in.
pub fn context_link_targets(root: &Path, globs: &[&str]) -> anyhow::Result<Vec<String>> {
    let root = std::fs::canonicalize(root)?;
    let mut found = BTreeMap::new();
    let mut links = Vec::new();
    collect_allow_listed(&root, "", &root, globs, &mut found, &mut links)?;
    Ok(links
        .into_iter()
        .map(|(_, target_rel)| target_rel)
        .collect())
}

/// The allow-listed tree: ordinary files the globs name, and the links whose own path they name.
/// `links` collects each travelling link's `(canonical target, root-relative target)`.
fn collect_allow_listed(
    dir: &Path,
    dir_rel: &str,
    root: &Path,
    globs: &[&str],
    found: &mut BTreeMap<String, ContextFile>,
    links: &mut Vec<(PathBuf, String)>,
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
            if !matches_context_globs(&rel_path, globs) {
                continue;
            }
            let Some((target, target_rel)) = resolve_link(&entry.path(), &rel_path, root, globs)?
            else {
                continue;
            };
            found.insert(
                rel_path.clone(),
                ContextFile {
                    rel_path,
                    source: target.clone(),
                    symlink_target: Some(target_rel.clone()),
                },
            );
            links.push((target, target_rel));
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
/// Nothing here consults the globs — reaching the target is the link's doing — but every path is
/// asked [`is_withheld_from_links`] as it is reached, so a pulled-in tree's `.git/` or `.env` stays
/// behind, and no symlink inside it is followed or listed (one hop).
fn pull_in_link_target(
    target: &Path,
    target_rel: &str,
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
            if is_withheld_from_links(&rel_path) {
                log::debug!("context_manifest: {rel_path} is withheld; not pulling it in");
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
            pull_in_link_target(&entry.path(), &rel_path, found)?;
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
/// sits in the tree before letting it travel ([`allow_listed_link_target`]), and that rule is only
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

/// Where the link at `entry` (spelled `link_rel`) points, as `(canonical target, root-relative
/// target)`, or `None` when it must not travel.
///
/// A missing target is the one error swallowed here: a link pointing at a file the project deleted
/// has no bytes to sync and never had. Every other error — `EACCES`, `ELOOP`, `EIO` — is
/// propagated, because treating it as "skip" would quietly drop a link the project still ships, and
/// a dropped manifest entry is a *delete* in the agent's context directory: a transient error would
/// withdraw guidance with nothing anywhere saying so.
fn resolve_link(
    entry: &Path,
    link_rel: &str,
    root: &Path,
    globs: &[&str],
) -> anyhow::Result<Option<(PathBuf, String)>> {
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
    match allow_listed_link_target(link_rel, &target_rel, globs) {
        Some(target_rel) => Ok(Some((target, target_rel))),
        None => {
            log::debug!(
                "context_manifest: {link_rel} -> {target_rel} reaches a withheld path; skipping it"
            );
            Ok(None)
        }
    }
}

/// Lowercase hex SHA-256, the identity a manifest entry is compared on.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex_of(&digest)
}

fn hex_of(digest: &[u8]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// `(hash, byte count)` of a file, read through a fixed-size buffer.
///
/// The count comes back rather than being taken from a second `stat` so the two can never disagree:
/// the size a manifest entry advertises is the size of the bytes its hash was taken over.
fn hash_file(path: &Path) -> anyhow::Result<(String, u64)> {
    use std::io::Read;

    let file = std::fs::File::open(path)?;
    let mut reader = std::io::BufReader::with_capacity(HASH_CHUNK_BYTES, file);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; HASH_CHUNK_BYTES];
    let mut size_bytes: u64 = 0;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        size_bytes += read as u64;
    }
    Ok((hex_of(&hasher.finalize()), size_bytes))
}

/// One allow-listed path as the manifest advertises it.
///
/// `size_bytes` rides along so a client can refuse an over-cap file before spending a read on it;
/// it is never what decides that a file moved — see [`diff_manifests`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextEntry {
    pub rel_path: String,
    pub sha256: String,
    pub size_bytes: u64,
    /// `Some` when `rel_path` is a symlink in the repository: the worktree-root-relative path it
    /// resolves to, `/`-separated. A link carries no bytes of its own — the consumer recreates it,
    /// and its target is listed as entries of its own. Its `size_bytes` is `0` and its `sha256` is
    /// [`sha256_hex`] of the target's spelling, so a retargeted link is a changed entry.
    pub symlink_target: Option<String>,
}

/// Everything the allow-list names, in a stable order.
///
/// Sorting is not cosmetic: two manifests of the same tree have to compare equal, and one is
/// compared against another on every `worktree.activity` tick.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContextManifest {
    entries: Vec<ContextEntry>,
}

/// Bytes held in memory at a time while hashing one file.
///
/// The hash is the only thing wanted from the file, so there is no reason for its length to decide
/// how much memory the walk costs — see [`ContextManifest::of_worktree`].
const HASH_CHUNK_BYTES: usize = 64 * 1024;

impl ContextManifest {
    /// Reads the manifest straight off a worktree — the co-located syncer's source, and the
    /// codebase daemon's answer to `StreamContextManifest`.
    ///
    /// Two bounds, and both matter on a daemon that runs this once per `worktree.activity` tick:
    ///
    /// - the hash is **streamed** in [`HASH_CHUNK_BYTES`] chunks rather than taken over a
    ///   `read`-the-whole-file buffer, so a multi-gigabyte artefact that happened to land under
    ///   `.claude/` costs constant memory instead of its own size;
    /// - a file over `max_bytes` is **left out of the manifest entirely**. It has to be: the reader
    ///   that serves the bytes (`tddy_daemon::context_files::read_context_file_bytes`) refuses over
    ///   that same cap before its first frame, so advertising the path would produce a manifest
    ///   whose every consumer then fails on it — and at setup that is a session which cannot start
    ///   at all. Omitting it is a *narrowing*: the agent sees less guidance, never guidance from
    ///   somewhere it should not have reached. A file that large is not guidance in any case.
    pub fn of_worktree(root: &Path, globs: &[&str], max_bytes: u64) -> anyhow::Result<Self> {
        let mut entries = Vec::new();
        for file in walk_context_files(root, globs)? {
            if let Some(target) = file.symlink_target {
                // A link carries no bytes; its identity is where it points, so retargeting it
                // changes its hash and the diff fetches it although no file's bytes moved.
                entries.push(ContextEntry {
                    rel_path: file.rel_path,
                    sha256: sha256_hex(target.as_bytes()),
                    size_bytes: 0,
                    symlink_target: Some(target),
                });
                continue;
            }
            let declared_size = std::fs::metadata(&file.source)?.len();
            if declared_size > max_bytes {
                log::warn!(
                    "context_manifest: {} is {declared_size} byte(s), over the {max_bytes} byte \
                     cap; leaving it out of the manifest",
                    file.rel_path
                );
                continue;
            }
            let (sha256, size_bytes) = hash_file(&file.source)?;
            // Re-checked against what was actually read: a file that grew past the cap between the
            // stat and the hash would otherwise be advertised at a size the reader refuses.
            if size_bytes > max_bytes {
                log::warn!(
                    "context_manifest: {} grew to {size_bytes} byte(s) while being hashed, over \
                     the {max_bytes} byte cap; leaving it out of the manifest",
                    file.rel_path
                );
                continue;
            }
            entries.push(ContextEntry {
                rel_path: file.rel_path,
                sha256,
                size_bytes,
                symlink_target: None,
            });
        }
        Ok(Self::from_entries(entries))
    }

    pub fn from_entries(mut entries: Vec<ContextEntry>) -> Self {
        entries.sort_by(|left, right| left.rel_path.cmp(&right.rel_path));
        Self { entries }
    }

    pub fn entries(&self) -> &[ContextEntry] {
        &self.entries
    }
}

/// The work one re-sync tick has to do, and no more.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ManifestDiff {
    /// Paths to read from the repo and write into the context dir, sorted.
    pub fetch: Vec<String>,
    /// Paths to remove from the context dir because the repo no longer serves them, sorted.
    pub delete: Vec<String>,
}

/// What the context dir must do to match the repo: `held` is what it has, `served` is what the
/// repo offers.
///
/// The hash is the sole authority on whether a file moved. A differing `size_bytes` never triggers
/// a fetch — two files of different lengths cannot share a hash, so a size that disagrees with a
/// matching hash is a bug upstream, and re-reading on it would churn the directory forever.
pub fn diff_manifests(held: &ContextManifest, served: &ContextManifest) -> ManifestDiff {
    let held_hashes: BTreeMap<&str, &str> = held
        .entries()
        .iter()
        .map(|entry| (entry.rel_path.as_str(), entry.sha256.as_str()))
        .collect();
    let served_paths: BTreeSet<&str> = served
        .entries()
        .iter()
        .map(|entry| entry.rel_path.as_str())
        .collect();

    ManifestDiff {
        fetch: served
            .entries()
            .iter()
            .filter(|entry| {
                held_hashes.get(entry.rel_path.as_str()) != Some(&entry.sha256.as_str())
            })
            .map(|entry| entry.rel_path.clone())
            .collect(),
        delete: held
            .entries()
            .iter()
            .filter(|entry| !served_paths.contains(entry.rel_path.as_str()))
            .map(|entry| entry.rel_path.clone())
            .collect(),
    }
}
