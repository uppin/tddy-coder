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

use crate::context_dir::PREAMBLE_FILES;

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
/// reach — excluded ([`CONTEXT_EXCLUDE_GLOBS`]) or secret-shaped ([`CONTEXT_SECRET_GLOBS`]).
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
fn names_a_link(link_rel: &str, globs: &[&str]) -> bool {
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
fn is_preamble_path(rel_path: &str) -> bool {
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
/// link is [`is_withheld_through_a_link`]. The reader asks the walk itself ([`context_reach`]).
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
/// Growth is bounded by construction: the keys are the compiled-in tables' rows, the individual
/// path segments of those rows and the directory a `dir/**` row roots ([`names_a_link`]), never
/// anything a caller or a peer names.
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

/// Which of `rel_paths` (relative to `root`, canonical) git ignores, asked in **one**
/// `git check-ignore --stdin -z` per call rather than one process per file.
///
/// `check-ignore` without `--no-index` is the question wanted: it applies the ignore rules
/// (`.gitignore` at every level, `.git/info/exclude`, `core.excludesFile`) to untracked paths, and
/// answers *not ignored* for a tracked file even where a pattern matches it — a committed file is
/// published already. A path inside an ignored directory is reported ignored.
///
/// **Fails closed.** When `root` is not in a git repository, or git cannot be run or answers with
/// anything but its two "answered" exit codes (`0` some ignored, `1` none), every path asked about
/// is treated as ignored and the reason logged at `warn`. That is a narrowing, not a fallback: a
/// link then brings nothing a glob does not also name, and the agent reads less of what the project
/// ships rather than something it should not have.
fn git_ignored<'a>(root: &Path, rel_paths: impl Iterator<Item = &'a str>) -> BTreeSet<String> {
    let asked: BTreeSet<String> = rel_paths.map(str::to_string).collect();
    if asked.is_empty() {
        return asked;
    }
    match ask_git_check_ignore(root, &asked) {
        Ok(ignored) => ignored,
        Err(e) => {
            log::warn!(
                "context_manifest: cannot tell which of {} link-reached path(s) under {root:?} git \
                 ignores ({e:#}); pulling none of them in",
                asked.len()
            );
            asked
        }
    }
}

fn ask_git_check_ignore(root: &Path, asked: &BTreeSet<String>) -> anyhow::Result<BTreeSet<String>> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["check-ignore", "--stdin", "-z"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("running git check-ignore failed: {e}"))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("git check-ignore has no stdin"))?;
    let input: Vec<u8> = asked
        .iter()
        .flat_map(|path| path.bytes().chain(std::iter::once(0)))
        .collect();
    // Fed from a thread: git answers as it reads, and a list long enough to fill both pipes would
    // otherwise deadlock the two processes on each other.
    let feeder = std::thread::spawn(move || stdin.write_all(&input));
    let output = child.wait_with_output()?;
    let fed = feeder
        .join()
        .map_err(|_| anyhow::anyhow!("the thread feeding git check-ignore panicked"))?;
    // The exit status first: a git that refused (`not a git repository`) may have stopped reading
    // its stdin, and the broken pipe that follows is not the reason worth reporting.
    if !matches!(output.status.code(), Some(0) | Some(1)) {
        anyhow::bail!(
            "git check-ignore exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    fed.map_err(|e| anyhow::anyhow!("feeding paths to git check-ignore failed: {e}"))?;
    Ok(output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| String::from_utf8_lossy(path).into_owned())
        .collect())
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
    /// `Some` when `rel_path` travels as a symlink: the worktree-root-relative path it resolves
    /// to, `/`-separated. A link carries no bytes of its own — the consumer recreates it, and its
    /// target is listed as entries of its own. Its `size_bytes` is `0` and its `sha256` is
    /// [`sha256_hex`] of the target's spelling, so a retargeted link is a changed entry.
    ///
    /// Never `Some` at a [`PREAMBLE_FILES`] path: a symlink there is followed, and listed as an
    /// ordinary file whose hash and size are its target's ([`followed_preamble_target`]).
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
