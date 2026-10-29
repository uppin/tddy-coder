//! What a conversation spawner (a grill-me session) is offered for the sessions it started alone
//! (`#home-inbox` 11/11): `ReadSessionActivity`, so it can read the `request_id` its answer tools
//! need. The daemon's scope check — the target's changeset must name the spawner — is
//! `tddy-daemon-rpc`'s `spawner_read_scope_acceptance.rs`.

use pretty_assertions::assert_eq;
use tddy_tool_engine::project_home_tools::{
    is_conversation_spawner_scoped_tool, is_conversation_spawner_tool, session_offered_catalog_for,
    SessionToolRole,
};
use tddy_tool_engine::start_session_args::StartChoices;

#[test]
fn read_session_activity_is_the_spawners_one_scoped_tool() {
    // Given the project-home tools a spawner could be offered
    // Then ReadSessionActivity is scoped to its own sessions
    assert!(is_conversation_spawner_scoped_tool("ReadSessionActivity"));
}

#[test]
fn no_other_project_home_tool_is_scoped_to_a_spawners_sessions() {
    // Given the tools a spawner must not read or act on other sessions with
    for tool in [
        "ReadSessionTranscript",
        "ReadSessionTerminal",
        "SendSessionKeys",
        "ListProjectSessions",
        "ReadSessionWorktree",
        "DiffSessionWorktree",
        "WaitProjectNotifications",
        "TerminateSession",
        "NotifyOperator",
    ] {
        // Then none is a scoped spawner tool
        assert!(
            !is_conversation_spawner_scoped_tool(tool),
            "{tool} is not offered to a spawner"
        );
    }
}

#[test]
fn a_scoped_tool_is_one_the_spawner_is_offered_beside_its_six() {
    // Given a spawner's offered catalogue
    let offered: Vec<String> = session_offered_catalog_for(
        SessionToolRole::ConversationSpawner,
        &StartChoices {
            hosts: Vec::new(),
            recipes: Vec::new(),
        },
    )
    .into_iter()
    .map(|tool| tool.name)
    .collect();

    // Then the scoped read is offered, as are the six it already had
    assert!(
        offered.iter().any(|tool| tool == "ReadSessionActivity"),
        "the spawner's tools: {offered:?}"
    );
    let six_and_the_read: Vec<&str> = [
        "StartSession",
        "GetSessionStart",
        "CancelSessionStart",
        "DescribeSessionStartOptions",
        "AnswerSessionPermission",
        "AnswerSessionQuestion",
        "ReadSessionActivity",
    ]
    .into_iter()
    .filter(|tool| is_conversation_spawner_tool(tool) || is_conversation_spawner_scoped_tool(tool))
    .collect();
    assert_eq!(six_and_the_read.len(), 7, "{six_and_the_read:?}");
}
