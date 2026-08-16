//! Worktree file browsing for the Code pane (docs/ft/web/session-code-pane.md).
//!
//! Lists a single directory level and reads UTF-8 file contents under a session's git worktree,
//! respecting `.gitignore` and excluding `.git`. Mirrors the canonicalize/contain and traversal
//! guards in [`crate::session_workflow_files`], but rooted at the worktree (the git checkout at
//! `SessionEntry.repo_path`) instead of the session metadata directory, and generalized beyond a
//! fixed allowlist.

use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use tddy_rpc::Status;

/// Maximum bytes returned by a single [`read_worktree_file_utf8`] before the content is truncated.
pub const MAX_WORKTREE_FILE_BYTES: usize = 1024 * 1024; // 1 MiB

/// One entry in a single directory level of a worktree listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeEntry {
    /// Basename of the entry relative to its parent directory.
    pub name: String,
    pub is_dir: bool,
    /// On-disk size of a file entry, so a picker can refuse an oversized document before requesting
    /// it. A directory reports 0 — it has no attachable size and is never offered as a document. A
    /// path git still lists but the filesystem no longer holds (a tracked file deleted from the
    /// worktree) also reports 0: it has no bytes, and reading it fails for the same reason.
    pub size_bytes: u64,
}

/// Result of a size-capped UTF-8 file read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeFileContent {
    pub content_utf8: String,
    /// True when the file exceeded [`MAX_WORKTREE_FILE_BYTES`] and `content_utf8` was truncated.
    pub truncated: bool,
    /// Full on-disk size of the file in bytes, before any truncation.
    pub byte_size: u64,
}

/// Read a listed worktree file as **raw bytes**, applying exactly the guards
/// [`read_worktree_file_utf8`] applies and none of its encoding.
///
/// The two share [`validate_rel_path`] and the git-listing gate deliberately: they may differ in
/// what they *return*, never in what they *allow*. What they do not share is the 1 MiB truncation —
/// a caller reconstructing a file needs all of it or a refusal, and a silently shortened file is
/// indistinguishable from a correctly synced one.
///
/// Product contract: `docs/ft/daemon/session-worktree-sync.md` AC15-AC20.
pub fn read_worktree_file_bytes(
    worktree_root: &Path,
    rel_path: &str,
    max_bytes: u64,
) -> Result<Vec<u8>, Status> {
    log::debug!(
        "read_worktree_file_bytes: worktree_root={:?} rel_path={:?} max_bytes={}",
        worktree_root,
        rel_path,
        max_bytes
    );
    let canonical_file = resolve_listed_worktree_file(worktree_root, rel_path)?;

    // Measured and refused *before* the read, not after: the point of this reader is to hand back a
    // file a caller can reconstruct from, and a caller that asked for more than it can hold would
    // otherwise pay for the whole read to be told so.
    let byte_size = std::fs::metadata(&canonical_file)
        .map_err(|e| {
            log::error!(
                "read_worktree_file_bytes: metadata {:?} failed: {}",
                canonical_file,
                e
            );
            Status::internal(format!("failed to size worktree file: {}", e))
        })?
        .len();
    if byte_size > max_bytes {
        log::warn!(
            "read_worktree_file_bytes: refused {:?} at {} byte(s), over the {} byte cap",
            rel_path,
            byte_size,
            max_bytes
        );
        return Err(Status::invalid_argument(format!(
            "worktree file is {byte_size} bytes, over the {max_bytes} byte limit"
        )));
    }

    // No truncation and no decoding: a shortened or re-encoded file is indistinguishable from a
    // correct one once it has been written, and this reader exists for callers reconstructing a
    // PNG, a UTF-16 document or a file holding one stray byte.
    let bytes = std::fs::read(&canonical_file).map_err(|e| {
        log::error!(
            "read_worktree_file_bytes: read {:?} failed: {}",
            canonical_file,
            e
        );
        Status::internal(format!("failed to read worktree file: {}", e))
    })?;
    log::info!(
        "read_worktree_file_bytes: read {} byte(s) from {:?}",
        bytes.len(),
        rel_path
    );
    Ok(bytes)
}

/// Lists the immediate children of `rel_path` under `worktree_root`, directories first then files
/// (each group alphabetical). Excludes `.git` and any `.gitignore`'d path (via git's index +
/// untracked-but-not-ignored view). `rel_path` is relative to the worktree root (empty = root);
/// traversal (`..`), absolute paths, and any resolution outside the worktree root are rejected.
pub fn list_worktree_directory_entries(
    worktree_root: &Path,
    rel_path: &str,
) -> Result<Vec<WorktreeEntry>, Status> {
    log::debug!(
        "list_worktree_directory_entries: worktree_root={:?} rel_path={:?}",
        worktree_root,
        rel_path
    );
    let dir_prefix = validate_rel_path(worktree_root, rel_path)?;

    let files = git_listed_files(worktree_root)?;

    // Filter git's flat file list to the immediate children of `dir_prefix`: a path with exactly
    // one remaining segment is a file; deeper paths contribute the directory named by their first
    // remaining segment (deduplicated).
    let mut dirs: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut file_names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for path in &files {
        let remaining = match child_remainder(path, &dir_prefix) {
            Some(r) => r,
            None => continue,
        };
        match remaining.split_once('/') {
            Some((first, _rest)) => {
                dirs.insert(first.to_string());
            }
            None => {
                file_names.insert(remaining.to_string());
            }
        }
    }

    let mut out: Vec<WorktreeEntry> = Vec::with_capacity(dirs.len() + file_names.len());
    out.extend(dirs.into_iter().map(|name| WorktreeEntry {
        name,
        is_dir: true,
        size_bytes: 0,
    }));
    let dir_path = worktree_root.join(&dir_prefix);
    out.extend(file_names.into_iter().map(|name| {
        let size_bytes = on_disk_size_bytes(&dir_path.join(&name));
        WorktreeEntry {
            name,
            is_dir: false,
            size_bytes,
        }
    }));
    log::info!(
        "list_worktree_directory_entries: {} entrie(s) under {:?}",
        out.len(),
        rel_path
    );
    Ok(out)
}

/// Reads a worktree file as UTF-8, capped at [`MAX_WORKTREE_FILE_BYTES`]. Applies the same unsafe
/// path rejection as [`list_worktree_directory_entries`], and refuses any path not surfaced by the
/// listing (so `.git` and `.gitignore`'d files cannot be read).
pub fn read_worktree_file_utf8(
    worktree_root: &Path,
    rel_path: &str,
) -> Result<WorktreeFileContent, Status> {
    log::debug!(
        "read_worktree_file_utf8: worktree_root={:?} rel_path={:?}",
        worktree_root,
        rel_path
    );
    let canonical_file = resolve_listed_worktree_file(worktree_root, rel_path)?;

    let bytes = std::fs::read(&canonical_file).map_err(|e| {
        log::error!(
            "read_worktree_file_utf8: read {:?} failed: {}",
            canonical_file,
            e
        );
        Status::internal(format!("failed to read worktree file: {}", e))
    })?;
    let byte_size = bytes.len() as u64;

    let (payload, truncated) = if bytes.len() > MAX_WORKTREE_FILE_BYTES {
        (&bytes[..MAX_WORKTREE_FILE_BYTES], true)
    } else {
        (&bytes[..], false)
    };
    let content_utf8 = String::from_utf8(payload.to_vec())
        .map_err(|_| Status::failed_precondition("file is not valid UTF-8"))?;

    log::info!(
        "read_worktree_file_utf8: read {} byte(s) (truncated={}) from {:?}",
        byte_size,
        truncated,
        rel_path
    );
    Ok(WorktreeFileContent {
        content_utf8,
        truncated,
        byte_size,
    })
}

/// The absolute path a readable worktree file resolves to, or why it may not be read.
///
/// Every reader goes through here, so the guards are stated once: the two readers may differ in
/// what they *return*, never in what they *allow*.
///
/// - a traversing or absolute `rel_path` is `INVALID_ARGUMENT` ([`validate_rel_path`]);
/// - a path git does not list is `PERMISSION_DENIED`, which is what keeps `.git` and every
///   `.gitignore`'d file — a local `.env`, a credential a build wrote — unreadable;
/// - a path that resolves outside the root is `PERMISSION_DENIED` even when git lists it, because
///   a tracked symlink is a listed path whose *target* is somebody else's file;
/// - a path the filesystem does not hold is `NOT_FOUND`.
fn resolve_listed_worktree_file(worktree_root: &Path, rel_path: &str) -> Result<PathBuf, Status> {
    validate_rel_path(worktree_root, rel_path)?;
    let rel_slashed = rel_path.replace('\\', "/");

    // The listing is asked FIRST, and the order is a security property rather than a preference.
    //
    // This gate exists to keep `.gitignore`'d paths — a local `.env`, a credential a build wrote,
    // a private key — unreadable. Answering "does it exist?" before "may you see it?" would keep
    // the *contents* secret while handing out the existence map: probe `.env` and read
    // PERMISSION_DENIED when it is there and NOT_FOUND when it is not. So an unlisted path is
    // refused identically whether or not anything is on disk at that name.
    let files = git_listed_files(worktree_root)?;
    if !files.iter().any(|f| f == &rel_slashed) {
        log::warn!(
            "resolve_listed_worktree_file: rejected path not surfaced by listing: {:?}",
            rel_path
        );
        return Err(Status::permission_denied(
            "file is not a listed worktree file",
        ));
    }

    // Only now is absence worth reporting, and only for a path git *does* list — a tracked file
    // deleted from the worktree but not yet staged. Saying NOT_FOUND here leaks nothing, because
    // the caller has already been told the path is listed.
    //
    // `symlink_metadata`, so the question is only ever about a name inside the worktree — following
    // a link here would let a tracked symlink turn this answer into a report on somebody else's
    // file. A dangling link exists as a link and is refused further down, on resolution.
    let joined = worktree_root.join(&rel_slashed);
    if let Err(e) = std::fs::symlink_metadata(&joined) {
        if e.kind() == std::io::ErrorKind::NotFound {
            log::debug!(
                "resolve_listed_worktree_file: listed path {:?} is not on disk: {}",
                rel_path,
                e
            );
            return Err(Status::not_found("worktree file not found"));
        }
    }

    let canonical_root = canonicalize_root(worktree_root)?;
    let canonical_file = joined.canonicalize().map_err(|e| {
        log::debug!(
            "resolve_listed_worktree_file: canonicalize {:?} failed: {}",
            joined,
            e
        );
        Status::not_found("worktree file not found")
    })?;
    if !canonical_file.starts_with(&canonical_root) {
        log::warn!(
            "resolve_listed_worktree_file: rejected path outside worktree: {:?} (root {:?})",
            canonical_file,
            canonical_root
        );
        return Err(Status::permission_denied(
            "resolved path escapes worktree root",
        ));
    }
    Ok(canonical_file)
}

/// Size of a listed file as the filesystem reports it, following symlinks — a link's target is what
/// a reader would actually pull, so that is the size a cap must be applied to. A path git lists but
/// the filesystem no longer holds has no size to report and yields 0.
fn on_disk_size_bytes(path: &Path) -> u64 {
    match std::fs::metadata(path) {
        Ok(metadata) => metadata.len(),
        Err(e) => {
            log::warn!("on_disk_size_bytes: metadata {:?} unavailable: {}", path, e);
            0
        }
    }
}

/// Canonicalizes the worktree root, rejecting a missing/inaccessible directory.
fn canonicalize_root(worktree_root: &Path) -> Result<PathBuf, Status> {
    worktree_root.canonicalize().map_err(|e| {
        log::debug!(
            "canonicalize_root: canonicalize {:?} failed: {}",
            worktree_root,
            e
        );
        Status::failed_precondition("worktree root is not accessible")
    })
}

/// Validates `rel_path` (rejecting absolute paths, leading separators, and `..` traversal) and
/// returns the directory prefix (empty for the root) with forward-slash separators. Also confirms
/// the resolved path stays under the canonicalized worktree root.
fn validate_rel_path(worktree_root: &Path, rel_path: &str) -> Result<String, Status> {
    if rel_path.starts_with('/') || rel_path.starts_with('\\') {
        log::debug!(
            "validate_rel_path: rejected leading separator: {:?}",
            rel_path
        );
        return Err(Status::invalid_argument("rel_path must be relative"));
    }
    // The traversal check reads the path with backslashes already taken as separators, the way
    // every use of it below does. On Unix `..\secret.txt` is one legal filename rather than a
    // traversal, so checking the raw form here and the slashed form afterwards is how a request
    // gets refused for one reason while being looked up as something else entirely.
    let rel_slashed = rel_path.replace('\\', "/");
    let rel = Path::new(&rel_slashed);
    for comp in rel.components() {
        match comp {
            Component::ParentDir => {
                log::debug!(
                    "validate_rel_path: rejected traversal segment: {:?}",
                    rel_path
                );
                return Err(Status::invalid_argument("rel_path must not contain '..'"));
            }
            Component::Prefix(_) | Component::RootDir => {
                log::debug!(
                    "validate_rel_path: rejected absolute component: {:?}",
                    rel_path
                );
                return Err(Status::invalid_argument("rel_path must be relative"));
            }
            _ => {}
        }
    }

    let canonical_root = canonicalize_root(worktree_root)?;
    if !rel_slashed.is_empty() {
        let joined = worktree_root.join(&rel_slashed);
        if let Ok(canonical) = joined.canonicalize() {
            if !canonical.starts_with(&canonical_root) {
                log::warn!(
                    "validate_rel_path: rejected path outside worktree: {:?} (root {:?})",
                    canonical,
                    canonical_root
                );
                return Err(Status::permission_denied(
                    "resolved path escapes worktree root",
                ));
            }
        }
    }

    Ok(rel_slashed.trim_matches('/').to_string())
}

/// Returns the immediate-child remainder of `path` relative to `dir_prefix` (with forward-slash
/// separators), or `None` if `path` is not under `dir_prefix`.
fn child_remainder<'a>(path: &'a str, dir_prefix: &str) -> Option<&'a str> {
    if dir_prefix.is_empty() {
        return Some(path);
    }
    let with_sep = format!("{dir_prefix}/");
    path.strip_prefix(&with_sep)
}

/// Runs git in the worktree to list tracked and untracked-but-not-ignored files (NUL-separated),
/// excluding `.git` and every `.gitignore`'d path. Paths are worktree-root-relative, forward-slash
/// separated.
pub(crate) fn git_listed_files(worktree_root: &Path) -> Result<Vec<String>, Status> {
    let mut args: Vec<OsString> = vec![
        "ls-files".into(),
        "--cached".into(),
        "--others".into(),
        "--exclude-standard".into(),
        "-z".into(),
    ];
    // git treats `info/` as a shared path, so a linked worktree's private `<gitdir>/info/exclude`
    // is never consulted by --exclude-standard (that only reads the common repo's info/exclude).
    // Feed the worktree's own exclude in explicitly so its private ignores hide files here too;
    // for a main repo this path is the common exclude that --exclude-standard already reads, so it
    // is a harmless no-op.
    if let Some(exclude) = worktree_private_exclude_file(worktree_root) {
        args.push("--exclude-from".into());
        args.push(exclude.into_os_string());
    }
    let out = Command::new("git")
        .arg("-C")
        .arg(worktree_root)
        .args(&args)
        .output()
        .map_err(|e| {
            log::error!("git_listed_files: spawn git ls-files failed: {}", e);
            Status::internal(format!("failed to run git ls-files: {}", e))
        })?;
    if !out.status.success() {
        log::warn!("git_listed_files: git ls-files failed: {:?}", out.status);
        return Err(Status::failed_precondition(
            "git ls-files failed for worktree",
        ));
    }
    let files = out
        .stdout
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect();
    Ok(files)
}

/// Resolves the worktree's own git-dir `info/exclude` file when it exists. For a linked worktree
/// this is the per-worktree private exclude (`<repo>/.git/worktrees/<id>/info/exclude`); for a main
/// repo it is the common `.git/info/exclude`.
fn worktree_private_exclude_file(worktree_root: &Path) -> Option<PathBuf> {
    let out = Command::new("git")
        .arg("-C")
        .arg(worktree_root)
        .args(["rev-parse", "--absolute-git-dir"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let gitdir = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if gitdir.is_empty() {
        return None;
    }
    let exclude = Path::new(&gitdir).join("info").join("exclude");
    exclude.is_file().then_some(exclude)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::process::Command;

    const README: &str = "# Hello Worktree\n\n- alpha\n- beta\n";
    const MAIN_RS: &str = "fn main() { println!(\"worktree-code-pane\"); }\n";
    const LIB_RS: &str = "pub fn lib() {}\n";

    fn run_git(cwd: &Path, args: &[&str]) {
        let status = Command::new("git")
            .current_dir(cwd)
            .args(args)
            .status()
            .unwrap_or_else(|e| panic!("git {args:?} in {cwd:?}: {e}"));
        assert!(status.success(), "git {args:?} failed in {cwd:?}");
    }

    /// A git worktree fixture:
    /// ```text
    /// src/{lib.rs, main.rs}
    /// README.md
    /// .env, ignored.txt, node_modules/pkg/index.js   (all ignored)
    /// ```
    /// Ignore patterns live in `.git/info/exclude` (not a tracked `.gitignore`) so no extra file
    /// appears in the root listing.
    fn a_worktree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        run_git(root, &["init", "-q"]);
        std::fs::write(
            root.join(".git/info/exclude"),
            "ignored.txt\n.env\nnode_modules/\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
        std::fs::write(root.join("README.md"), README).unwrap();
        std::fs::write(root.join("src/main.rs"), MAIN_RS).unwrap();
        std::fs::write(root.join("src/lib.rs"), LIB_RS).unwrap();
        std::fs::write(root.join(".env"), "SECRET=must-not-appear\n").unwrap();
        std::fs::write(root.join("ignored.txt"), "junk\n").unwrap();
        std::fs::write(root.join("node_modules/pkg/index.js"), "// vendored\n").unwrap();
        dir
    }

    /// A directory row — no attachable size, so always 0 bytes.
    fn a_dir(name: &str) -> WorktreeEntry {
        WorktreeEntry {
            name: name.to_string(),
            is_dir: true,
            size_bytes: 0,
        }
    }

    /// A file row carrying the byte length of the fixture content stored under `name`.
    fn a_file(name: &str, contents: &str) -> WorktreeEntry {
        WorktreeEntry {
            name: name.to_string(),
            is_dir: false,
            size_bytes: contents.len() as u64,
        }
    }

    #[test]
    fn lists_root_directories_first_excluding_git_and_ignored_paths() {
        // Given
        let wt = a_worktree();

        // When
        let entries = list_worktree_directory_entries(wt.path(), "").unwrap();

        // Then — the source dir precedes the root file; .git, .env, ignored.txt and node_modules
        // are all absent.
        assert_eq!(entries, vec![a_dir("src"), a_file("README.md", README)]);
    }

    #[test]
    fn lists_a_subdirectorys_files_alphabetically_when_expanded() {
        // Given
        let wt = a_worktree();

        // When
        let entries = list_worktree_directory_entries(wt.path(), "src").unwrap();

        // Then
        assert_eq!(
            entries,
            vec![a_file("lib.rs", LIB_RS), a_file("main.rs", MAIN_RS)]
        );
    }

    #[test]
    fn rejects_a_parent_traversal_rel_path() {
        // Given
        let wt = a_worktree();

        // When / Then
        assert!(list_worktree_directory_entries(wt.path(), "../secrets").is_err());
    }

    #[test]
    fn rejects_an_absolute_rel_path() {
        // Given
        let wt = a_worktree();

        // When / Then
        assert!(list_worktree_directory_entries(wt.path(), "/etc").is_err());
    }

    #[test]
    fn reads_a_file_as_utf8_reporting_untruncated_size() {
        // Given
        let wt = a_worktree();

        // When
        let content = read_worktree_file_utf8(wt.path(), "README.md").unwrap();

        // Then
        assert_eq!(
            content,
            WorktreeFileContent {
                content_utf8: README.to_string(),
                truncated: false,
                byte_size: README.len() as u64,
            }
        );
    }

    #[test]
    fn refuses_to_read_a_gitignored_file() {
        // Given — `.env` is ignored, so it is never surfaced by the listing and cannot be read.
        let wt = a_worktree();

        // When / Then
        assert!(read_worktree_file_utf8(wt.path(), ".env").is_err());
    }

    #[test]
    fn rejects_reading_a_traversal_rel_path() {
        // Given
        let wt = a_worktree();

        // When / Then
        assert!(read_worktree_file_utf8(wt.path(), "../../etc/passwd").is_err());
    }

    #[test]
    fn truncates_a_file_larger_than_the_read_cap() {
        // Given — a tracked file one byte over the cap (single-byte chars keep the boundary valid).
        let wt = a_worktree();
        let big = "a".repeat(MAX_WORKTREE_FILE_BYTES + 1);
        std::fs::write(wt.path().join("big.txt"), &big).unwrap();

        // When
        let content = read_worktree_file_utf8(wt.path(), "big.txt").unwrap();

        // Then
        assert_eq!(
            content,
            WorktreeFileContent {
                content_utf8: "a".repeat(MAX_WORKTREE_FILE_BYTES),
                truncated: true,
                byte_size: (MAX_WORKTREE_FILE_BYTES + 1) as u64,
            }
        );
    }
}
