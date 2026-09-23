//! The traversal check every path-bearing exec tool's arguments pass before any I/O.
//!
//! Moved from `tddy-session-lifecycle`'s `connection_service.rs` with the exec-tool handlers, its
//! only caller.

use std::path::Path;

use tddy_rpc::Status;

/// Reject an obvious path traversal in a path-bearing exec tool's arguments, before any I/O.
///
/// The worktree root is the boundary an exec tool call is confined to; a `..` component asks to
/// leave it, which is refused rather than normalized away.
pub(super) fn reject_exec_tool_path_traversal(
    tool_name: &str,
    args_json: &str,
) -> Result<(), Status> {
    if !matches!(tool_name, "Read" | "Write" | "StrReplace" | "Delete") {
        return Ok(());
    }
    let args: serde_json::Value =
        serde_json::from_str(args_json).unwrap_or(serde_json::Value::Null);
    let Some(path) = args.get("path").and_then(|v| v.as_str()) else {
        return Ok(());
    };
    if Path::new(path)
        .components()
        .any(|c| c == std::path::Component::ParentDir)
    {
        return Err(Status::permission_denied(
            "path contains '..' components (traversal rejected)",
        ));
    }
    Ok(())
}
