//! Host-side session-action handlers for **no-bash mode** (docs/ft/coder/no-bash-mode.md).
//!
//! The in-jail `tddy-tools --mcp` relays `EstablishAction`/`ListActions`/`InvokeAction`
//! dispatches to the host (the session dir only exists here); `bridge::AppToolHandler`
//! intercepts them ahead of the generic exec-tool engine and calls into this module. The jail
//! is untrusted: `establish_action` re-validates the authored YAML authoritatively — the
//! in-jail pre-validation is only a cheap retry loop for the author model.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use tddy_core::session_actions::{
    derive_repo_key, ensure_action_architecture, invoke_action_core, list_action_summaries,
    parse_action_manifest_yaml, repo_actions_root, validate_authored_manifest, DiscoveryQuery,
};

/// Upper bound on an authored manifest — mirrors the in-jail `request_action` cap; anything
/// larger is a runaway generation, not an action.
const MAX_MANIFEST_BYTES: usize = 64 * 1024;

/// Authoritatively validate an authored manifest and write it under
/// `<session_dir>/actions/<id>.yaml`. Auto-establish: once this returns Ok, the action is
/// invocable. Idempotent for byte-identical re-submissions; a same-id manifest with different
/// content is a collision error (the author must pick a new id — established actions are never
/// silently redefined).
pub fn establish_action(session_dir: &Path, args: &Value) -> Result<Value, String> {
    let yaml = args
        .get("yaml")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "EstablishAction: missing required field `yaml`".to_string())?;
    if yaml.len() > MAX_MANIFEST_BYTES {
        return Err(format!(
            "EstablishAction: manifest is {} bytes; the limit is {MAX_MANIFEST_BYTES}",
            yaml.len()
        ));
    }
    let manifest = parse_action_manifest_yaml(yaml).map_err(|e| e.to_string())?;
    validate_authored_manifest(&manifest).map_err(|e| e.to_string())?;
    ensure_action_architecture(&manifest.architecture).map_err(|e| e.to_string())?;

    let actions_dir = session_dir.join("actions");
    std::fs::create_dir_all(&actions_dir)
        .map_err(|e| format!("create {}: {e}", actions_dir.display()))?;
    let path = actions_dir.join(format!("{}.yaml", manifest.id));
    match std::fs::read_to_string(&path) {
        Ok(existing) if existing == yaml => {} // byte-identical re-establish: idempotent ok
        Ok(_) => {
            return Err(format!(
                "EstablishAction: action id '{}' already exists with different content; \
                 choose a new id",
                manifest.id
            ));
        }
        Err(_) => {
            std::fs::write(&path, yaml).map_err(|e| format!("write {}: {e}", path.display()))?;
        }
    }

    Ok(json!({
        "id": manifest.id,
        "summary": manifest.summary,
        "path": path.to_string_lossy(),
        "has_input_schema": manifest.input_schema.is_some(),
    }))
}

/// List established actions: the session dir overlay plus the per-repo store, same merge as
/// `tddy-tools list-actions`.
pub fn list_actions(session_dir: &Path, worktree: &Path, args: &Value) -> Result<Value, String> {
    let query = DiscoveryQuery {
        path_prefix: args
            .get("path_prefix")
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        query: args.get("query").and_then(|v| v.as_str()).map(str::to_owned),
        limit: args.get("limit").and_then(|v| v.as_u64()).map(|v| v as usize),
        offset: args
            .get("offset")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(0),
    };
    let result = list_action_summaries(
        Some(session_dir),
        Some(worktree),
        &resolve_tddy_data_dir(),
        &query,
    )
    .map_err(|e| e.to_string())?;
    Ok(json!({
        "actions": result.actions,
        "total": result.total,
        "offset": query.offset,
        "limit": query.limit,
    }))
}

/// Invoke an established action, blocking until the subprocess exits (run under
/// `spawn_blocking` by the caller). The command runs with the worktree as working directory and
/// the same host privileges the `Shell` tool has — no-bash narrows *what* can run (established,
/// fixed-argv manifests), not *where* it runs.
pub fn invoke_action(session_dir: &Path, worktree: &Path, args: &Value) -> Result<Value, String> {
    let action = args
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "InvokeAction: missing required field `action`".to_string())?;
    let data_json = match args.get("data") {
        None | Some(Value::Null) => "{}".to_string(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    };
    let canon_worktree = std::fs::canonicalize(worktree).unwrap_or_else(|_| worktree.to_path_buf());
    let store_root = repo_actions_root(&resolve_tddy_data_dir(), &derive_repo_key(&canon_worktree));
    invoke_action_core(
        Some(session_dir),
        Some(store_root.as_path()),
        Some(&canon_worktree),
        action,
        &data_json,
    )
    .map_err(|e| e.to_string())
}

/// Resolve the tddy data directory (profile default or `$HOME/.tddy`) — same rule as
/// `tddy-tools`' `session_actions_cli`.
fn resolve_tddy_data_dir() -> PathBuf {
    tddy_core::output::default_tddy_data_dir().unwrap_or_else(|| {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home).join(".tddy")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_MANIFEST: &str = "\
version: 1
id: echo-hi
summary: Echo a greeting
architecture: native
command: [echo, hi]
";

    fn establish(session_dir: &Path, yaml: &str) -> Result<Value, String> {
        establish_action(session_dir, &json!({ "yaml": yaml }))
    }

    /// The happy path: establish writes `<session>/actions/<id>.yaml` and returns the summary.
    #[test]
    fn establish_writes_the_manifest_under_the_session_actions_dir() {
        // Given
        let session = tempfile::tempdir().unwrap();

        // When
        let summary = establish(session.path(), VALID_MANIFEST).expect("establish must succeed");

        // Then
        assert_eq!(summary["id"].as_str(), Some("echo-hi"));
        assert_eq!(summary["has_input_schema"].as_bool(), Some(false));
        let written = session.path().join("actions").join("echo-hi.yaml");
        assert_eq!(std::fs::read_to_string(written).unwrap(), VALID_MANIFEST);
    }

    /// Invalid YAML is rejected without writing anything — the host is the authority even when
    /// the jail's pre-validation was bypassed.
    #[test]
    fn establish_rejects_invalid_yaml_without_writing() {
        // Given
        let session = tempfile::tempdir().unwrap();

        // When
        let err = establish(session.path(), "not: [valid").expect_err("must reject");

        // Then
        assert!(err.contains("YAML"), "got: {err}");
        assert!(!session.path().join("actions").exists());
    }

    /// Unknown manifest keys are rejected (deny_unknown_fields) — same rule as `invoke-action`.
    #[test]
    fn establish_rejects_unknown_manifest_keys() {
        let session = tempfile::tempdir().unwrap();
        let yaml = format!("{VALID_MANIFEST}bogus: 1\n");
        establish(session.path(), &yaml).expect_err("unknown keys must be rejected");
    }

    /// A traversal id never becomes a path: rejected before any filesystem access.
    #[test]
    fn establish_rejects_a_path_traversal_id() {
        let session = tempfile::tempdir().unwrap();
        let yaml = "\
version: 1
id: ../../escape
summary: s
architecture: native
command: [echo]
";
        let err = establish(session.path(), yaml).expect_err("traversal id must be rejected");
        assert!(err.contains("id"), "got: {err}");
        assert!(!session.path().join("actions").exists());
    }

    /// Re-establishing byte-identical content is idempotent; different content under the same id
    /// is a collision error — an established action is never silently redefined.
    #[test]
    fn establish_is_idempotent_for_identical_content_but_rejects_redefinition() {
        // Given
        let session = tempfile::tempdir().unwrap();
        establish(session.path(), VALID_MANIFEST).expect("first establish");

        // When / Then — identical bytes: ok
        establish(session.path(), VALID_MANIFEST).expect("identical re-establish must be ok");

        // And — same id, different command: rejected, file unchanged
        let redefined = VALID_MANIFEST.replace("[echo, hi]", "[rm, -rf, /]");
        let err = establish(session.path(), &redefined).expect_err("redefinition must be rejected");
        assert!(err.contains("already exists"), "got: {err}");
        let written = session.path().join("actions").join("echo-hi.yaml");
        assert_eq!(std::fs::read_to_string(written).unwrap(), VALID_MANIFEST);
    }

    /// The full loop a no-bash session runs: establish → list shows it → invoke executes the
    /// manifest argv and reports the child's output.
    #[test]
    fn establish_list_invoke_round_trip() {
        // Given
        let session = tempfile::tempdir().unwrap();
        let worktree = tempfile::tempdir().unwrap();
        establish(session.path(), VALID_MANIFEST).expect("establish");

        // When — list
        let listed =
            list_actions(session.path(), worktree.path(), &json!({})).expect("list must succeed");

        // Then
        let ids: Vec<&str> = listed["actions"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|a| a["id"].as_str())
            .collect();
        assert!(ids.contains(&"echo-hi"), "got: {listed}");

        // When — invoke
        let result = invoke_action(
            session.path(),
            worktree.path(),
            &json!({"action": "echo-hi"}),
        )
        .expect("invoke must succeed");

        // Then — the child ran and its output is reported
        assert_eq!(result["exit_code"].as_i64(), Some(0), "got: {result}");
        assert_eq!(result["stdout"].as_str(), Some("hi\n"), "got: {result}");
    }

    /// Invoking an unknown action is a typed error, not a panic.
    #[test]
    fn invoke_reports_an_unknown_action_id() {
        let session = tempfile::tempdir().unwrap();
        let worktree = tempfile::tempdir().unwrap();
        let err = invoke_action(
            session.path(),
            worktree.path(),
            &json!({"action": "ghost"}),
        )
        .expect_err("unknown action must be an error");
        assert!(err.contains("ghost"), "got: {err}");
    }
}
