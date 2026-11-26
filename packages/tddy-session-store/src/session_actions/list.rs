//! Enumerate manifests under the per-repo store and optional session overlay.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use log::{debug, info, warn};
use serde::Serialize;

use super::error::SessionActionsError;
use super::manifest::parse_action_manifest_file_on;
use super::paths::{derive_repo_key, repo_actions_root};

/// Flat filter and pagination parameters for action discovery.
#[derive(Debug, Clone, Default)]
pub struct DiscoveryQuery {
    /// Only return actions whose relative path starts with this prefix
    /// (e.g. `"packages/foo"` to scope to one package).
    pub path_prefix: Option<String>,
    /// Case-insensitive substring filter applied to the action `id` or `summary`.
    pub query: Option<String>,
    /// Maximum number of actions to return (pagination window). `None` returns all matches.
    pub limit: Option<usize>,
    /// Zero-based offset into the sorted, filtered result set.
    pub offset: usize,
}

/// Paginated result of an action discovery call.
#[derive(Debug, Clone)]
pub struct ActionListResult {
    /// Actions in the current page (sorted ascending by `path`).
    pub actions: Vec<ActionSummary>,
    /// Total number of matching actions before pagination.
    pub total: usize,
}

/// Summary of one discovered action manifest.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ActionSummary {
    /// Manifest-declared identifier (display / legacy invocation).
    pub id: String,
    /// One-line human description from the manifest `summary` field.
    pub summary: String,
    pub has_input_schema: bool,
    pub has_output_schema: bool,
    /// Relative path under the discovery root, without extension.
    /// Use this as `--action <path>` for invocation.
    /// Example: `packages/foo/build` (store root) or `run-tests` (session overlay).
    pub path: String,
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/// Discover, filter, and paginate action manifests across all roots:
///  1. Per-repo store: `<tddy_data_dir>/actions/<repo_key>/` (derived from `repo_root`).
///  2. Session overlay: `<session_dir>/actions/` (flat, backward-compatible).
///
/// Manifests that fail to parse are skipped with a `warn!` log (error-tolerant).
/// Session-overlay entries win over store entries when both share the same relative `path`.
///
/// Returns `SessionActionsError::MissingActionsDir` only when **no** root is available at all.
pub fn list_action_summaries(
    session_dir: Option<&Path>,
    repo_root: Option<&Path>,
    tddy_data_dir: &Path,
    query: &DiscoveryQuery,
) -> Result<ActionListResult, SessionActionsError> {
    list_action_summaries_on(
        &tddy_fs::DiskFs,
        session_dir,
        repo_root,
        tddy_data_dir,
        query,
    )
}

/// [`list_action_summaries`] through the filesystem port.
pub fn list_action_summaries_on(
    fs: &dyn tddy_fs::Fs,
    session_dir: Option<&Path>,
    repo_root: Option<&Path>,
    tddy_data_dir: &Path,
    query: &DiscoveryQuery,
) -> Result<ActionListResult, SessionActionsError> {
    debug!(
        target: "tddy_core::session_actions::list",
        "list_action_summaries: session_dir={:?} repo_root={:?} query={:?}",
        session_dir.map(|p| p.display().to_string()),
        repo_root.map(|p| p.display().to_string()),
        query,
    );

    // path → ActionSummary; session overlay wins on collision (inserted last).
    let mut by_path: HashMap<String, ActionSummary> = HashMap::new();

    // 1. Per-repo store (if we have a repo root to derive the key from).
    if let Some(repo) = repo_root {
        let repo_canon = fs.canonicalize(repo).unwrap_or_else(|_| repo.to_path_buf());
        let key = derive_repo_key(&repo_canon);
        let store = repo_actions_root(tddy_data_dir, &key);
        if is_dir_on(fs, &store) {
            collect_from_root_on(fs, &store, &store, query, &mut by_path);
        } else {
            debug!(
                target: "tddy_core::session_actions::list",
                "per-repo store does not exist yet: {}",
                store.display()
            );
        }
    }

    // 2. Session overlay (legacy flat `<session_dir>/actions/`).
    if let Some(session) = session_dir {
        let actions_dir = session.join("actions");
        if is_dir_on(fs, &actions_dir) {
            collect_from_root_on(fs, &actions_dir, &actions_dir, query, &mut by_path);
        }
    }

    if by_path.is_empty() && repo_root.is_none() && session_dir.is_none() {
        // No roots at all — propagate the "missing" error like the original single-root API.
        let fallback = session_dir
            .map(|s| s.join("actions"))
            .unwrap_or_else(|| PathBuf::from("actions"));
        return Err(SessionActionsError::MissingActionsDir(fallback));
    }

    // Sort by path, apply query filter, then paginate.
    let mut all: Vec<ActionSummary> = by_path.into_values().collect();
    all.sort_by(|a, b| a.path.cmp(&b.path));

    if let Some(q) = query.query.as_deref() {
        let q_lower = q.to_lowercase();
        all.retain(|s| {
            s.id.to_lowercase().contains(&q_lower)
                || s.summary.to_lowercase().contains(&q_lower)
                || s.path.to_lowercase().contains(&q_lower)
        });
    }

    let total = all.len();
    let page: Vec<ActionSummary> = all
        .into_iter()
        .skip(query.offset)
        .take(query.limit.unwrap_or(usize::MAX))
        .collect();

    info!(
        target: "tddy_core::session_actions::list",
        "list_action_summaries: total={} returned={}",
        total,
        page.len()
    );

    Ok(ActionListResult {
        actions: page,
        total,
    })
}

/// `Path::is_dir` through the port: a symlink to a directory counts, as it does on disk.
fn is_dir_on(fs: &dyn tddy_fs::Fs, path: &Path) -> bool {
    is_kind_on(fs, path, tddy_fs::FsKind::Dir)
}

/// `Path::is_file` through the port: a symlink to a file counts, as it does on disk.
fn is_file_on(fs: &dyn tddy_fs::Fs, path: &Path) -> bool {
    is_kind_on(fs, path, tddy_fs::FsKind::File)
}

fn is_kind_on(fs: &dyn tddy_fs::Fs, path: &Path, kind: tddy_fs::FsKind) -> bool {
    match fs.metadata(path).map(|meta| meta.kind) {
        Ok(found) if found == kind => true,
        // The port reports a symlink as such; `Path::{is_dir, is_file}` follow it.
        Ok(tddy_fs::FsKind::Symlink) => fs
            .canonicalize(path)
            .and_then(|target| fs.metadata(&target))
            .map(|meta| meta.kind == kind)
            .unwrap_or(false),
        _ => false,
    }
}

/// Recursively collect manifests from `search_root`, updating `by_path`.
/// `rel_base` is the root used to compute relative paths (equals `search_root` except when
/// narrowing by prefix for efficiency — currently always equal).
///
/// The walk matches what `{root}/**/*.yaml` plus `{root}/**/*.yml` used to glob: every
/// `*.yaml`/`*.yml` file at any depth, symlinked directories not descended, symlinked files
/// followed. A root holding glob metacharacters (`[`, `*`) is walked literally now — glob read
/// them as a pattern.
fn collect_from_root_on(
    fs: &dyn tddy_fs::Fs,
    search_root: &Path,
    rel_base: &Path,
    query: &DiscoveryQuery,
    by_path: &mut HashMap<String, ActionSummary>,
) {
    let mut stack = vec![search_root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs.read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) => {
                warn!(
                    target: "tddy_core::session_actions::list",
                    "cannot list {}: {}", dir.display(), e
                );
                continue;
            }
        };
        for abs_path in entries {
            match fs.metadata(&abs_path).map(|meta| meta.kind) {
                Ok(tddy_fs::FsKind::Dir) => stack.push(abs_path),
                Ok(tddy_fs::FsKind::File) | Ok(tddy_fs::FsKind::Symlink) => {
                    collect_manifest_on(fs, rel_base, query, by_path, &abs_path);
                }
                // A socket is an endpoint, not a manifest; an unreadable entry is skipped.
                Ok(tddy_fs::FsKind::Socket) | Err(_) => continue,
            }
        }
    }
}

fn collect_manifest_on(
    fs: &dyn tddy_fs::Fs,
    rel_base: &Path,
    query: &DiscoveryQuery,
    by_path: &mut HashMap<String, ActionSummary>,
    abs_path: &Path,
) {
    // `*.yaml` in glob terms: any name ending in `.yaml`, case-sensitively — bytes, as
    // glob matched, so a non-UTF-8 name is judged like any other.
    let is_manifest = abs_path.file_name().is_some_and(|name| {
        let bytes = name.as_encoded_bytes();
        bytes.ends_with(b".yaml") || bytes.ends_with(b".yml")
    });
    if !is_manifest || !is_file_on(fs, abs_path) {
        return;
    }

    // Compute relative path (without extension) for this manifest.
    let rel = match abs_path.strip_prefix(rel_base) {
        Ok(r) => r.with_extension(""),
        Err(_) => return,
    };
    let rel_str = rel.to_string_lossy().replace('\\', "/");

    // Apply path_prefix filter BEFORE parsing — cheap, scales to 10k manifests.
    if let Some(ref prefix) = query.path_prefix {
        if !rel_str.starts_with(prefix.as_str()) {
            return;
        }
    }

    // Parse the manifest (error-tolerant: skip malformed files with a warning).
    let manifest = match parse_action_manifest_file_on(fs, abs_path) {
        Ok(m) => m,
        Err(e) => {
            warn!(
                target: "tddy_core::session_actions::list",
                "skipping malformed manifest {}: {}", abs_path.display(), e
            );
            return;
        }
    };

    let summary = ActionSummary {
        id: manifest.id,
        summary: manifest.summary,
        has_input_schema: manifest.input_schema.is_some(),
        has_output_schema: manifest.output_schema.is_some(),
        path: rel_str.clone(),
    };
    // Insert/overwrite — last writer wins (session overlay is inserted after store).
    by_path.insert(rel_str, summary);
}
