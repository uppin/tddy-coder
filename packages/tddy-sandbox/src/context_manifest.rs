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

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
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
    CONTEXT_EXCLUDE_GLOBS.iter().any(|glob| {
        with_compiled_pattern(glob, |pattern| {
            pattern.matches_with(path, EXCLUDE_MATCH_OPTIONS)
        })
        .unwrap_or(false)
    })
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

/// One allow-listed file in a worktree, paired with the path its bytes actually live at — which is
/// the link target when the entry was a symlink, and the allow-list has been asked about *both*
/// spellings (see [`walk_context_files`]).
pub(crate) struct ContextFile {
    /// Worktree-root-relative and `/`-separated, the spelling both the manifest and the copier use.
    pub rel_path: String,
    pub source: PathBuf,
}

/// Every allow-listed file under `root`, sorted by path.
///
/// A symlink is followed only when **both ends** are allow-listed: the name it is reached by *and*
/// the place its target sits in the tree. Checking only the name is not enough and checking only
/// containment in the root is actively wrong — `.claude/creds -> ../.env` resolves inside the root
/// and would otherwise be published under a name every glob happily matches, handing the whole
/// point of the allow-list away. On the split path those bytes then cross to another host and land
/// in the agent's readable working directory.
///
/// So a link out of the root (`.claude/x -> /etc/passwd`) is skipped because its target is not
/// under `root`, and a link to a sibling inside it (`.claude/creds -> ../.env`,
/// `.claude/x -> node_modules/…`) is skipped because its target's own root-relative path is named
/// by no glob. Only a link between two allow-listed places — `.claude/tdd.md -> .agents/skills/tdd/SKILL.md`
/// — survives, which is the one case an agent-config tree actually has a use for. (A link to a
/// *directory* already inside the walk is additionally deduplicated by `visited`, so an aliased
/// tree is listed once, under whichever spelling the walk reached first.)
pub(crate) fn walk_context_files(root: &Path, globs: &[&str]) -> anyhow::Result<Vec<ContextFile>> {
    let root = std::fs::canonicalize(root)?;
    let mut found = Vec::new();
    let mut visited = HashSet::new();
    collect_context_files(&root, "", &root, globs, &mut visited, &mut found)?;
    found.sort_by(|left, right| left.rel_path.cmp(&right.rel_path));
    Ok(found)
}

fn collect_context_files(
    dir: &Path,
    dir_rel: &str,
    root: &Path,
    globs: &[&str],
    visited: &mut HashSet<PathBuf>,
    found: &mut Vec<ContextFile>,
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
        let Some(source) = resolve_within_root(&entry.path(), root)? else {
            continue;
        };
        // Where the bytes actually sit, spelled the way the allow-list is written. For a plain
        // entry this is `rel_path` itself; for a symlink it is the *target*'s place in the tree,
        // and that is the half a containment-only guard never asked about.
        let Some(source_rel) = root_relative(&source, root) else {
            log::warn!(
                "context_manifest: {source:?} resolved outside {root:?} after the containment \
                 check; skipping it"
            );
            continue;
        };
        let metadata = std::fs::symlink_metadata(&source)?;
        if metadata.is_dir() {
            // Both ends again: the name must be able to lead somewhere allow-listed, and so must
            // the directory the link lands in — otherwise `.claude/x -> node_modules` would be
            // descended into under a name that matches `.claude/**`.
            if !could_contain_matches(&rel_path, globs)
                || !could_contain_matches(&source_rel, globs)
                || !visited.insert(source.clone())
            {
                continue;
            }
            collect_context_files(&source, &rel_path, root, globs, visited, found)?;
        } else if metadata.is_file()
            && matches_context_globs(&rel_path, globs)
            && matches_context_globs(&source_rel, globs)
        {
            found.push(ContextFile { rel_path, source });
        }
    }
    Ok(())
}

/// `path`'s place under `root`, `/`-separated, or `None` when it is not under `root` at all.
///
/// Both must be canonical by the time this is asked — here `root` comes from
/// [`walk_context_files`] and `path` from [`resolve_within_root`] — so a plain prefix strip is the
/// whole of it.
///
/// Exported because the **reader** needs the identical answer. The both-ends symlink rule
/// ([`walk_context_files`]) is only half enforced if the walk applies it and the reader does not:
/// the reader is handed a caller-supplied name, and asking the allow-list about that name alone
/// serves `.claude/creds -> ../.env` under a spelling every glob matches. It has to ask about the
/// resolved target's own root-relative path too, which is this — and it has to be *this* function
/// rather than a second prefix strip beside it, because the drift between the two halves of that
/// rule is what made the reader serve the file the walk had already refused.
pub fn root_relative(path: &Path, root: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let joined = rel
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    (!joined.is_empty()).then_some(joined)
}

/// The path to read `entry` through, or `None` when it must not be read at all.
///
/// Containment in `root` is only half the guard; the caller asks the allow-list about the resolved
/// path as well (see [`walk_context_files`]). A missing target is the one error swallowed here: a
/// link pointing at a file the project deleted has no bytes to sync and never had. Every other
/// error — `EACCES`, `ELOOP`, `EIO` — is propagated, because treating it as "skip" would quietly
/// drop a file the project still ships, and a dropped manifest entry is a *delete* in the agent's
/// context directory: a transient error would withdraw guidance with nothing anywhere saying so.
fn resolve_within_root(entry: &Path, root: &Path) -> anyhow::Result<Option<PathBuf>> {
    if !std::fs::symlink_metadata(entry)?.file_type().is_symlink() {
        return Ok(Some(entry.to_path_buf()));
    }
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
    if !target.starts_with(root) {
        log::debug!("context_manifest: {entry:?} resolves outside {root:?}; skipping it");
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
