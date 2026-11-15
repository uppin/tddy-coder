//! `ReadSessionWorktree` and `DiffSessionWorktree`: a target session's checkout, read and
//! diffed for an orchestrator agent. A target without a checkout of its own — a split
//! session's agent half — is refused here; the daemon redirects such a call to its codebase
//! half before it reaches this code (see `is_worktree_target_tool`).
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use crate::project_home_diff::worktree_diff;
use crate::project_home_tools::resolve_target;
use crate::project_home_tools::ProjectHomePort;
use crate::project_home_tools::DIFF_SESSION_WORKTREE;
use crate::project_home_tools::READ_SESSION_WORKTREE;
use crate::ToolOutcome;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Component, Path};
use tddy_worktree_service::session_target::HomeCaller;
use tddy_worktree_service::worktree_files::{
    list_worktree_directory_entries, read_worktree_file_utf8,
};

/// The arguments of the two tools that name a target session.
#[derive(Deserialize)]
struct TargetArgs {
    session_id: String,
    #[serde(default)]
    path: String,
}

fn parse_target_args(tool_name: &str, args: Value) -> Result<TargetArgs, ToolOutcome> {
    serde_json::from_value(args)
        .map_err(|e| ToolOutcome::err(format!("{tool_name}: invalid arguments: {e}")))
}

pub(crate) async fn read_session_worktree(
    port: &dyn ProjectHomePort,
    home: &HomeCaller,
    args: Value,
) -> ToolOutcome {
    let args = match parse_target_args(READ_SESSION_WORKTREE, args) {
        Ok(a) => a,
        Err(outcome) => return outcome,
    };
    let target = match resolve_target(port, home, &args.session_id).await {
        Ok(t) => t,
        Err(outcome) => return outcome,
    };
    let root = match target.worktree() {
        Ok(root) => root.to_path_buf(),
        Err(refused) => return ToolOutcome::err(refused.to_string()),
    };
    let rel = match contained_rel_path(&root, &args.path) {
        Ok(rel) => rel,
        Err(message) => return ToolOutcome::err(message),
    };
    let read = tokio::task::spawn_blocking(move || read_or_list(&root, &rel)).await;
    match read {
        Ok(Ok(value)) => ToolOutcome::ok(value.to_string()),
        Ok(Err(message)) => ToolOutcome::err(format!("{READ_SESSION_WORKTREE}: {message}")),
        Err(e) => ToolOutcome::err(format!("{READ_SESSION_WORKTREE}: read task failed: {e}")),
    }
}

/// A directory listing when `rel` is a directory, otherwise the file's text. Both go through the
/// worktree reader that the Code pane uses, so a git-ignored or `.git` path is not readable here.
fn read_or_list(root: &Path, rel: &str) -> Result<Value, String> {
    if rel.is_empty() || root.join(rel).is_dir() {
        let entries =
            list_worktree_directory_entries(root, rel).map_err(|s| s.message().to_string())?;
        return Ok(
            json!({ "path": rel, "entries": entries.iter().map(|e| json!({
            "name": e.name, "is_dir": e.is_dir, "size_bytes": e.size_bytes
        })).collect::<Vec<_>>() }),
        );
    }
    let file = read_worktree_file_utf8(root, rel).map_err(|s| s.message().to_string())?;
    Ok(json!({
        "path": rel,
        "content": file.content_utf8,
        "truncated": file.truncated,
        "byte_size": file.byte_size,
    }))
}

/// `path` as a worktree-relative path with no `.` segments, or the refusal when it leaves the
/// worktree: absolute, climbing above the root, or resolving through a symlink to outside it.
fn contained_rel_path(worktree: &Path, path: &str) -> Result<String, String> {
    let outside = || format!("path {path:?} is outside the worktree");
    let as_path = Path::new(path);
    if as_path.is_absolute() {
        return Err(outside());
    }
    let mut segments: Vec<String> = Vec::new();
    for component in as_path.components() {
        match component {
            Component::Normal(segment) => segments.push(segment.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir => {
                if segments.pop().is_none() {
                    return Err(outside());
                }
            }
            Component::RootDir | Component::Prefix(_) => return Err(outside()),
        }
    }
    let rel = segments.join("/");
    if let (Ok(root), Ok(full)) = (worktree.canonicalize(), worktree.join(&rel).canonicalize()) {
        if !full.starts_with(&root) {
            return Err(outside());
        }
    }
    Ok(rel)
}

pub(crate) async fn diff_session_worktree(
    port: &dyn ProjectHomePort,
    home: &HomeCaller,
    args: Value,
) -> ToolOutcome {
    let args = match parse_target_args(DIFF_SESSION_WORKTREE, args) {
        Ok(a) => a,
        Err(outcome) => return outcome,
    };
    let target = match resolve_target(port, home, &args.session_id).await {
        Ok(t) => t,
        Err(outcome) => return outcome,
    };
    let base_ref = if target.base_ref.is_empty() {
        "HEAD".to_string()
    } else {
        target.base_ref.clone()
    };
    let worktree = match target.worktree() {
        Ok(worktree) => worktree,
        Err(refused) => return ToolOutcome::err(refused.to_string()),
    };
    match worktree_diff(worktree, &base_ref).await {
        Ok(diff) => ToolOutcome::ok(
            json!({ "session_id": target.session_id, "base_ref": base_ref, "diff": diff })
                .to_string(),
        ),
        Err(e) => ToolOutcome::err(format!("{DIFF_SESSION_WORKTREE}: {e}")),
    }
}
