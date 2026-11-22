use std::path::Path;
use tddy_agent_launch::conversation_spawn::session_tool_role;
use tddy_core::session_lifecycle::{unified_session_dir_path, validate_session_id_segment};
use tddy_rpc::Status;
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};
use tddy_tool_engine::project_home_tools::{
    is_conversation_spawner_scoped_tool, is_conversation_spawner_tool, SessionToolRole,
};
use tddy_worktree_service::session_target::HomeCaller;

/// The calling session and its project, when its record names an orchestrator role; anything else
/// is refused.
pub(in crate::exec_tool) fn orchestrator_caller(
    sessions_base: &Path,
    session_id: &str,
    tool_name: &str,
) -> Result<HomeCaller, Status> {
    let dir = unified_session_dir_path(sessions_base, session_id);
    let metadata = tddy_core::read_session_metadata(&dir).map_err(|e| {
        Status::permission_denied(format!(
            "tool {tool_name} is refused: cannot read session {session_id}'s record ({e})"
        ))
    })?;
    if session_tool_role(&metadata) != SessionToolRole::ProjectHome {
        return Err(Status::permission_denied(format!(
            "tool {tool_name} is served only to an orchestrator session"
        )));
    }
    Ok(HomeCaller {
        session_id: session_id.to_string(),
        project_id: metadata.project_id,
    })
}

/// Refuse `tool_name` unless `session_id` is a project-home session — or a session that spawns
/// conversations calling one of its five tools ([`is_conversation_spawner_tool`]) — and name that
/// session's project, with the role that admitted it. The role is the session's own record, not
/// anything the request claims, and it is read once: the scope check that follows
/// ([`refuse_a_spawner_outside_its_own_sessions`]) is handed this role rather than re-reading it, so
/// a record that changes or becomes unreadable between the two cannot widen the call.
pub(super) fn project_home_caller(
    sessions_base: &Path,
    session_id: &str,
    tool_name: &str,
) -> Result<(HomeCaller, SessionToolRole), Status> {
    let session_dir = unified_session_dir_path(sessions_base, session_id);
    let metadata = tddy_core::read_session_metadata(&session_dir).map_err(|e| {
        Status::permission_denied(format!(
            "tool {tool_name} is refused: cannot read session {session_id}'s role ({e})"
        ))
    })?;
    let role = session_tool_role(&metadata);
    match role {
        SessionToolRole::ProjectHome => {}
        // A session that spawns conversations is served the start tools and the permission answer,
        // for its own project.
        SessionToolRole::ConversationSpawner if is_conversation_spawner_tool(tool_name) => {}
        // A read of a session it started: whose, is checked once the target is known
        // ([`refuse_a_spawner_outside_its_own_sessions`]).
        SessionToolRole::ConversationSpawner if is_conversation_spawner_scoped_tool(tool_name) => {}
        _ => {
            return Err(Status::permission_denied(format!(
                "tool {tool_name} is served only to a project-home session"
            )));
        }
    }
    Ok((
        HomeCaller {
            session_id: session_id.to_string(),
            project_id: metadata.project_id,
        },
        role,
    ))
}

/// Refuse a conversation spawner's [`is_conversation_spawner_scoped_tool`] call unless its target —
/// the `session_id` the call names — is a session the spawner started: one whose changeset names
/// `home` as its `orchestrator_session_id` (`#home-inbox` 11/11). Refused as permission denied,
/// naming the caller and the target; a target with no readable changeset is refused too. Any other
/// caller — `role` is the one [`project_home_caller`] admitted it with — or any other tool, passes:
/// their scope is the project's, checked by the target resolver.
pub(super) fn refuse_a_spawner_outside_its_own_sessions(
    sessions_base: &Path,
    home: &HomeCaller,
    role: SessionToolRole,
    req: &ExecuteToolRequest,
) -> Result<(), Status> {
    if role != SessionToolRole::ConversationSpawner
        || !is_conversation_spawner_scoped_tool(&req.tool_name)
    {
        return Ok(());
    }
    let refused = |why: &str| {
        Status::permission_denied(format!(
            "tool {} is refused: session {} may read only the sessions it started ({why})",
            req.tool_name, home.session_id
        ))
    };
    let target = serde_json::from_str::<serde_json::Value>(&req.args_json)
        .ok()
        .and_then(|args| args.get("session_id")?.as_str().map(str::to_string))
        .unwrap_or_default();
    if validate_session_id_segment(&target).is_err() {
        return Err(refused("the call names no valid session_id"));
    }
    // Same host only: a child recorded on another host has no changeset here (see
    // docs/dev/todo/2026-10-29-spawner-read-scope-is-same-host-only.md).
    let started_by =
        match tddy_core::read_changeset(&unified_session_dir_path(sessions_base, &target)) {
            Ok(changeset) => changeset.orchestrator_session_id,
            Err(tddy_core::WorkflowError::ChangesetMissing(_)) => None,
            Err(e) => {
                return Err(refused(&format!(
                    "the changeset of {target} could not be read ({e})"
                )))
            }
        };
    if started_by.as_deref() == Some(home.session_id.as_str()) {
        Ok(())
    } else {
        Err(refused(&format!("it did not start {target}")))
    }
}

/// Append the call to the session's tool-call log, as the local runner does for every other tool.
/// A failed write is logged and does not fail the call.
pub(in crate::exec_tool) fn record_call(
    sessions_base: &Path,
    req: &ExecuteToolRequest,
    response: &ExecuteToolResponse,
) {
    let session_dir = unified_session_dir_path(sessions_base, &req.session_id);
    let record = tddy_tool_engine::tool_call_log::ToolCallRecord {
        task_id: response.job_id.clone(),
        tool_name: req.tool_name.clone(),
        args_json: req.args_json.clone(),
        result_json: response.result_json.clone(),
        is_error: response.is_error,
        error_message: response.error_message.clone(),
        job_running: response.job_running,
        created_unix_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
    };
    if let Err(e) = tddy_tool_engine::tool_call_log::append_tool_call(&session_dir, &record) {
        log::warn!(
            "tool_call_log: failed to persist project-home tool call for session {}: {}",
            req.session_id,
            e
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_tool_engine::project_home_tools::SessionToolRole;

    #[test]
    fn an_unreadable_target_changeset_is_refused_as_unreadable_not_as_not_started() {
        let base = std::env::temp_dir().join(format!("tddy-caller-test-{}", std::process::id()));
        let target_dir = unified_session_dir_path(&base, "target-1");
        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::write(target_dir.join("changeset.yaml"), "::: not yaml: [\n").unwrap();
        let home = HomeCaller {
            session_id: "spawner-1".to_string(),
            project_id: "p1".to_string(),
        };
        let req = ExecuteToolRequest {
            session_id: "spawner-1".to_string(),
            tool_name: "ReadSessionActivity".to_string(),
            args_json: r#"{"session_id":"target-1"}"#.to_string(),
            ..Default::default()
        };
        let err = refuse_a_spawner_outside_its_own_sessions(
            &base,
            &home,
            SessionToolRole::ConversationSpawner,
            &req,
        )
        .unwrap_err();
        assert!(
            err.message().contains("could not be read"),
            "refusal must name the unreadable changeset, got: {}",
            err.message()
        );
        std::fs::remove_dir_all(&base).ok();
    }
}
