//! Enumerate manifests under the per-repo store and optional session overlay.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use log::{debug, info, warn};
use serde::Serialize;

use super::error::SessionActionsError;
use super::manifest::parse_action_manifest_file;
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
        let repo_canon = std::fs::canonicalize(repo).unwrap_or_else(|_| repo.to_path_buf());
        let key = derive_repo_key(&repo_canon);
        let store = repo_actions_root(tddy_data_dir, &key);
        if store.is_dir() {
            collect_from_root(&store, &store, query, &mut by_path);
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
        if actions_dir.is_dir() {
            collect_from_root(&actions_dir, &actions_dir, query, &mut by_path);
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

/// Recursively collect manifests from `search_root` using glob, updating `by_path`.
/// `rel_base` is the root used to compute relative paths (equals `search_root` except when
/// narrowing by prefix for efficiency — currently always equal).
fn collect_from_root(
    search_root: &Path,
    rel_base: &Path,
    query: &DiscoveryQuery,
    by_path: &mut HashMap<String, ActionSummary>,
) {
    for ext in ["yaml", "yml"] {
        let pattern = format!("{root}/**/*.{ext}", root = search_root.display());
        let entries = match glob::glob(&pattern) {
            Ok(e) => e,
            Err(e) => {
                warn!(
                    target: "tddy_core::session_actions::list",
                    "invalid glob pattern '{}': {}", pattern, e
                );
                continue;
            }
        };
        for entry in entries {
            let abs_path = match entry {
                Ok(p) => p,
                Err(e) => {
                    warn!(
                        target: "tddy_core::session_actions::list",
                        "glob entry error: {}", e
                    );
                    continue;
                }
            };
            if !abs_path.is_file() {
                continue;
            }

            // Compute relative path (without extension) for this manifest.
            let rel = match abs_path.strip_prefix(rel_base) {
                Ok(r) => r.with_extension(""),
                Err(_) => continue,
            };
            let rel_str = rel.to_string_lossy().replace('\\', "/");

            // Apply path_prefix filter BEFORE parsing — cheap, scales to 10k manifests.
            if let Some(ref prefix) = query.path_prefix {
                if !rel_str.starts_with(prefix.as_str()) {
                    continue;
                }
            }

            // Parse the manifest (error-tolerant: skip malformed files with a warning).
            let manifest = match parse_action_manifest_file(&abs_path) {
                Ok(m) => m,
                Err(e) => {
                    warn!(
                        target: "tddy_core::session_actions::list",
                        "skipping malformed manifest {}: {}", abs_path.display(), e
                    );
                    continue;
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
    }
}
