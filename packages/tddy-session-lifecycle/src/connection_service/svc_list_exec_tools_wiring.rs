//! `ListExecTools` relayed out of a jail (`#project-home` 5/7).
//!
//! The agent's `tddy-tools` asks, per `tools/list`, which exec tools its session is offered. Inside a
//! jail the only way to the daemon is the runner's relay, so the question arrives here the way
//! `ConversationWorktree` does: with no token to check, because the jail is trusted for exactly the one
//! session it was built for.

use crate::connection_service::daemon_rpc_handler::BoundJailSession;
use prost::Message as _;
use tddy_host_service::peer_routing::PeerRouting;
use tddy_rpc::Status;
use tddy_service::proto::exec_tools::{ListExecToolsRequest, ListExecToolsResponse, ToolDef};
use tddy_tool_engine::project_home_tools::session_offered_catalog_for;
use tddy_tool_engine::start_session_args::{HostChoice, StartChoices};
use tddy_tool_engine::work_queue_tools::caller_records::{
    with_work_tools_offered_to, work_queue_caller_from_records,
};

/// What `StartSession`'s schema offers on this daemon: every eligible host on `routing`'s roster,
/// every workflow recipe this daemon supports, and every coding agent `agents` starts.
pub fn start_choices(
    routing: &PeerRouting,
    agents: &tddy_coding_agent_specs::CodingAgentRegistry,
) -> StartChoices {
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
        // Every PTY agent this daemon starts, from its specs: the session type it is started
        // as, the toggle's label (its `product`), whether a start must name a model, and the
        // permission modes its launch takes.
        agents: agents
            .all()
            .into_iter()
            .filter_map(|spec| {
                let session_type = agents.session_type_of(&spec.id)?.to_string();
                agents.for_session_type(&session_type).map(|_| {
                    tddy_tool_engine::start_session_args::AgentChoice {
                        session_type,
                        label: spec
                            .agent
                            .product
                            .clone()
                            .unwrap_or_else(|| spec.id.clone()),
                        requires_model: spec
                            .has(tddy_coding_agent_specs::Capability::RequiresModel),
                        permission_modes: spec.agent.permission_modes.clone(),
                        routable: !spec.route_support().providers.is_empty(),
                    }
                })
            })
            .collect(),
        // Every session type this daemon's tool files declare, with its label.
        session_types: agents
            .session_types()
            .into_iter()
            .map(
                |declared| tddy_tool_engine::start_session_args::SessionTypeChoice {
                    session_type: declared.session_type.clone(),
                    label: declared.label.clone(),
                },
            )
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
    let role = tddy_agent_launch::conversation_spawn::session_tool_role(&metadata);
    let access = match bound.sessions_base() {
        Some(sessions_base) => work_queue_caller_from_records(sessions_base, &bound.session_id)
            .map_err(|e| Status::permission_denied(format!("the stack tools are refused: {e}")))?
            .map(|(_, access)| access),
        None => None,
    };
    let tools: Vec<ToolDef> =
        with_work_tools_offered_to(session_offered_catalog_for(role, choices), access)
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
    use tddy_changeset::work_queue_fixtures::a_stack_host;
    use tddy_core::changeset::{Changeset, WorkItem, WorkQueue};
    use tddy_core::session_metadata::PROJECT_HOME_ROLE;

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
            agents: Vec::new(),
            session_types: Vec::new(),
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
    fn a_stack_node_jail_is_answered_status_and_read_for_its_stack() {
        // Given an orchestrator whose node n1 is bound to a session that runs in a jail
        let host = a_stack_host();
        let orch = host.orchestrator(
            "orch",
            WorkQueue {
                version: 1,
                nodes: vec![WorkItem {
                    node_id: "n1".to_string(),
                    title: "Store".to_string(),
                    session_id: Some("child".to_string()),
                    ..WorkItem::default()
                }],
            },
        );
        let mut meta = tddy_core::read_session_metadata(&orch).unwrap();
        meta.session_role = Some("orchestrator".to_string());
        tddy_core::session_metadata::write_session_metadata(&orch, &meta).unwrap();
        let child = host.session(
            "child",
            Changeset {
                orchestrator_session_id: Some("orch".to_string()),
                ..Changeset::default()
            },
        );
        let bound = BoundJailSession {
            session_id: "child".to_string(),
            session_dir: child,
        };

        // When the jail asks which tools its session is offered
        let names = names_answered(&bound, "child").expect("answered");

        // Then its stack tools are the two reads alone
        let work_queue_tools: Vec<&str> = names
            .iter()
            .map(String::as_str)
            .filter(|name| tddy_tool_engine::work_queue_tools::is_stack_tool(name))
            .collect();
        assert_eq!(work_queue_tools, vec!["work_queue_status", "work_pr_read"]);
    }

    #[test]
    fn a_jails_sessions_base_is_the_directory_holding_its_sessions_directory() {
        // Given a jail bound to `<base>/sessions/s-1`
        let base = std::path::Path::new("/data/alice");
        let bound = BoundJailSession {
            session_id: "s-1".to_string(),
            session_dir: base.join("sessions").join("s-1"),
        };

        // When its sessions base is asked
        let answered = bound.sessions_base();

        // Then it is `<base>`
        assert_eq!(answered, Some(base));
    }

    #[test]
    fn a_session_dir_outside_a_sessions_directory_has_no_sessions_base() {
        // Given a jail bound to a directory not under `sessions/`
        let bound = BoundJailSession {
            session_id: "s-1".to_string(),
            session_dir: std::path::PathBuf::from("/data/alice/elsewhere/s-1"),
        };

        // When its sessions base is asked
        let answered = bound.sessions_base();

        // Then there is none
        assert_eq!(answered, None);
    }

    #[test]
    fn a_request_naming_another_session_is_refused() {
        let (_dir, bound) = jail_of("s-1", None);

        let refused = names_answered(&bound, "someone-elses").expect_err("refused");

        assert_eq!(refused.code, tddy_rpc::Code::PermissionDenied);
    }
}
