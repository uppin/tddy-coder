//! Content-addressed action cache (SHA-256), atomic per-action JSON entries.
//!
//! Layout: `{repo_root}/.tddy-build/cache/{target_id}/{action_id}.json`.
//! Atomic write mirrors `tddy-core`'s `flush_action_cache_document_atomic`
//! (tmp + `sync_all` + `rename`).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::BuildError;
use crate::proto::{ActionCacheEntry, ActionType, BuildAction, FileFingerprint, OutputKind};

const CACHE_DIR: &str = ".tddy-build/cache";

/// Cache read/write policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CacheMode {
    /// Read and write the local cache.
    #[default]
    ReadWrite,
    /// Read the local cache, never write.
    ReadOnly,
    /// Same as [`CacheMode::ReadOnly`] (no remote in v1).
    Offline,
}

impl CacheMode {
    /// Whether this mode persists new entries.
    pub fn writes(self) -> bool {
        matches!(self, CacheMode::ReadWrite)
    }
}

/// Compute the content-addressed cache key for `action` given its resolved input
/// file fingerprints. Returns `"sha256:<hex64>"`. Deterministic and independent
/// of input/env iteration order.
pub fn compute_cache_key(action: &BuildAction, input_fingerprints: &[FileFingerprint]) -> String {
    let mut hasher = Sha256::new();

    hasher.update(action.id.as_bytes());
    hasher.update(b"\0type\0");
    hasher.update(action_type_str(action.r#type).as_bytes());

    // Command argv is ordered — do not sort.
    hasher.update(b"\0cmd\0");
    hasher.update(
        serde_json::to_string(&action.command)
            .unwrap_or_default()
            .as_bytes(),
    );

    hasher.update(b"\0wd\0");
    hasher.update(action.working_dir.as_bytes());

    // Env is order-independent — sort by key.
    hasher.update(b"\0env\0");
    let mut env: Vec<(&String, &String)> = action.env.iter().collect();
    env.sort_by(|a, b| a.0.cmp(b.0));
    for (k, v) in env {
        hasher.update(k.as_bytes());
        hasher.update(b"=");
        hasher.update(v.as_bytes());
        hasher.update(b";");
    }

    // Inputs, sorted by path.
    hasher.update(b"\0inputs\0");
    let mut fps: Vec<&FileFingerprint> = input_fingerprints.iter().collect();
    fps.sort_by(|a, b| a.path.cmp(&b.path));
    for fp in fps {
        hasher.update(format!("{}:{}:{};", fp.path, fp.size, fp.mtime_ms).as_bytes());
    }

    // Outputs, sorted by path.
    hasher.update(b"|outputs|");
    let mut outs: Vec<_> = action.outputs.iter().collect();
    outs.sort_by(|a, b| a.path.cmp(&b.path));
    for out in outs {
        hasher.update(format!("{}:{};", out.path, output_kind_str(out.kind)).as_bytes());
    }

    // Tool deps, sorted.
    hasher.update(b"|tool_deps|");
    let mut tool_deps: Vec<&String> = action.tool_dep_ids.iter().collect();
    tool_deps.sort();
    for td in tool_deps {
        hasher.update(td.as_bytes());
        hasher.update(b";");
    }

    format!("sha256:{}", hex::encode(hasher.finalize()))
}

/// Load the cache entry for `{target_id}/{action_id}`, returning it only when its
/// recorded key matches `expected_key` and every output path still exists.
pub fn lookup_cache(
    repo_root: &Path,
    target_id: &str,
    action_id: &str,
    expected_key: &str,
) -> Option<ActionCacheEntry> {
    let path = cache_entry_path(repo_root, target_id, action_id);
    let raw = fs::read_to_string(&path).ok()?;
    let entry: ActionCacheEntry = serde_json::from_str(&raw).ok()?;

    if entry.cache_key != expected_key {
        return None;
    }
    for output in &entry.output_paths {
        if !repo_root.join(output).exists() {
            return None;
        }
    }
    Some(entry)
}

/// Atomically persist `entry` for `target_id`.
pub fn persist_cache(
    repo_root: &Path,
    target_id: &str,
    entry: &ActionCacheEntry,
) -> Result<(), BuildError> {
    let path = cache_entry_path(repo_root, target_id, &entry.action_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| BuildError::Io(e.to_string()))?;
    }

    let tmp_path = path.with_file_name(format!(
        "{}.part{}.tmp",
        sanitize(&entry.action_id),
        uuid::Uuid::now_v7()
    ));
    let serialized =
        serde_json::to_vec_pretty(entry).map_err(|e| BuildError::Manifest(e.to_string()))?;

    {
        let mut file = fs::File::create(&tmp_path).map_err(|e| BuildError::Io(e.to_string()))?;
        file.write_all(&serialized)
            .map_err(|e| BuildError::Io(e.to_string()))?;
        file.sync_all().map_err(|e| BuildError::Io(e.to_string()))?;
    }
    fs::rename(&tmp_path, &path).map_err(|e| BuildError::Io(e.to_string()))?;
    Ok(())
}

fn cache_entry_path(repo_root: &Path, target_id: &str, action_id: &str) -> PathBuf {
    repo_root
        .join(CACHE_DIR)
        .join(sanitize(target_id))
        .join(format!("{}.json", sanitize(action_id)))
}

/// Make an id filesystem-safe (target ids contain `/` and `:`).
fn sanitize(id: &str) -> String {
    id.chars()
        .map(|c| match c {
            '/' | ':' | '\\' | ' ' => '_',
            other => other,
        })
        .collect()
}

fn action_type_str(value: i32) -> &'static str {
    match ActionType::try_from(value) {
        Ok(ActionType::Command) => "command",
        Ok(ActionType::Copy) => "copy",
        Ok(ActionType::Tool) => "tool",
        _ => "unspecified",
    }
}

fn output_kind_str(value: i32) -> &'static str {
    match OutputKind::try_from(value) {
        Ok(OutputKind::File) => "file",
        Ok(OutputKind::Directory) => "directory",
        _ => "unspecified",
    }
}
