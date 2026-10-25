//! `ListExecTools` relayed out of a jail (`#project-home` 5/7).
//!
//! The agent's `tddy-tools` asks, per `tools/list`, which exec tools its session is offered. Inside a
//! jail the only way to the daemon is the runner's relay, so the question arrives here the way
//! `ConversationWorktree` does: with no token to check, because the jail is trusted for exactly the one
//! session it was built for.

use crate::connection_service::daemon_rpc_handler::BoundJailSession;
use prost::Message as _;
use tddy_core::session_metadata::PROJECT_HOME_ROLE;
use tddy_host_service::peer_routing::PeerRouting;
use tddy_rpc::Status;
use tddy_service::proto::exec_tools::{ListExecToolsRequest, ListExecToolsResponse, ToolDef};
use tddy_tool_engine::project_home_tools::session_offered_catalog;
use tddy_tool_engine::start_session_args::{HostChoice, StartChoices};

/// What `StartSession`'s schema offers on this daemon: every eligible host on `routing`'s roster,
/// and every workflow recipe this daemon supports.
pub fn start_choices(routing: &PeerRouting) -> StartChoices {
    StartChoices {
        hosts: routing
            .eligible_daemon_source()
            .list_eligible_daemons()
            .into_iter()
            .map(|daemon| HostChoice {
                daemon_instance_id: daemon.instance_id.0,
                label: daemon.label,
            })
            .collect(),
        recipes: tddy_workflow_recipes::approval_policy::supported_workflow_recipe_cli_names()
            .iter()
            .map(|recipe| recipe.to_string())
            .collect(),
    }
}

/// Serve a `ListExecTools` request relayed out of the jail built for `bound`.
///
/// A request naming any other session is refused (`PermissionDenied`): the runner rewrites every
/// request to its own session, so one that names another did not come through the runner. The role is
/// read from the session's own record under the session directory the daemon resolved for the jail.
/// A project-home session's `StartSession` offers `choices` ([`start_choices`]).
pub(crate) fn list_exec_tools_from_jail(
    bound: &BoundJailSession,
    payload: &[u8],
    choices: &StartChoices,
) -> Result<Vec<u8>, Status> {
    let req = ListExecToolsRequest::decode(payload)
        .map_err(|e| Status::invalid_argument(format!("decode ListExecToolsRequest: {e}")))?;
    if req.session_id != bound.session_id {
        log::warn!(
            "ListExecTools: the jail of session {} named session {:?}; refused",
            bound.session_id,
            req.session_id
        );
        return Err(Status::permission_denied(format!(
            "this jail serves session {}, not {:?}",
            bound.session_id, req.session_id
        )));
    }
    let metadata = tddy_core::read_session_metadata(&bound.session_dir).map_err(|e| {
        Status::not_found(format!(
            "session {}: cannot read its record to tell which tools it is offered ({e})",
            bound.session_id
        ))
    })?;
    let tools = session_offered_catalog(
        metadata.session_role.as_deref() == Some(PROJECT_HOME_ROLE),
        choices,
    )
    .into_iter()
    .map(|tool| ToolDef {
        name: tool.name,
        description: tool.description,
        input_schema_json: tool.input_schema_json,
    })
    .collect();
    Ok(ListExecToolsResponse { tools }.encode_to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jail_of(session_id: &str, role: Option<&str>) -> (tempfile::TempDir, BoundJailSession) {
        let dir = tempfile::tempdir().expect("a temporary session dir");
        let metadata = tddy_core::SessionMetadata {
            session_role: role.map(str::to_string),
            ..tddy_session_split::service_util::starting_session_metadata(
                session_id,
                "p-1",
                "workspace",
            )
        };
        tddy_core::write_session_metadata(dir.path(), &metadata).expect("the record is written");
        let bound = BoundJailSession {
            session_id: session_id.to_string(),
            session_dir: dir.path().to_path_buf(),
        };
        (dir, bound)
    }

    fn names_answered(bound: &BoundJailSession, asked_as: &str) -> Result<Vec<String>, Status> {
        let request = ListExecToolsRequest {
            session_id: asked_as.to_string(),
            ..Default::default()
        };
        let choices = StartChoices {
            hosts: Vec::new(),
            recipes: Vec::new(),
        };
        list_exec_tools_from_jail(bound, &request.encode_to_vec(), &choices).map(|bytes| {
            ListExecToolsResponse::decode(bytes.as_slice())
                .expect("a ListExecToolsResponse")
                .tools
                .into_iter()
                .map(|tool| tool.name)
                .collect()
        })
    }

    #[test]
    fn a_project_home_jail_is_answered_its_read_only_tools_and_the_project_home_tools() {
        let (_dir, bound) = jail_of("home-1", Some(PROJECT_HOME_ROLE));

        let names = names_answered(&bound, "home-1").expect("answered");

        assert!(
            names.contains(&"ListProjectSessions".to_string()),
            "{names:?}"
        );
        assert!(names.contains(&"Read".to_string()), "{names:?}");
        assert!(!names.contains(&"Write".to_string()), "{names:?}");
    }

    #[test]
    fn an_ordinary_jail_is_answered_the_catalog_without_the_project_home_tools() {
        let (_dir, bound) = jail_of("s-1", None);

        let names = names_answered(&bound, "s-1").expect("answered");

        assert!(names.contains(&"Write".to_string()), "{names:?}");
        assert!(
            !names.contains(&"ListProjectSessions".to_string()),
            "{names:?}"
        );
    }

    #[test]
    fn a_request_naming_another_session_is_refused() {
        let (_dir, bound) = jail_of("s-1", None);

        let refused = names_answered(&bound, "someone-elses").expect_err("refused");

        assert_eq!(refused.code, tddy_rpc::Code::PermissionDenied);
    }
}
