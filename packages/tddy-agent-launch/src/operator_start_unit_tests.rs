//! An operator start may not ask for anything a peer holds (`#agent-tools 30/32`): its undo is keyed
//! by the operator's OS user, and a peer authenticates a delete by a session token the operator
//! start does not carry. So each of the four fields that would make one is refused, naming it.
//!
//! These are unit tests on the refusal because no public entry can carry the fields to
//! `LaunchSessions::start_session_for_os_user`: the operator port builds its request from
//! `OperatorSessionStart`, which has none of them.

use tddy_rpc::{Code, Status};
use tddy_service::proto::session::StartSessionRequest;

use crate::operator_start::refuse_peer_held_parts;

fn an_operator_start() -> StartSessionRequest {
    StartSessionRequest {
        project_id: "019d105b-ac0f-78d3-9a89-409731145a36".to_string(),
        session_type: "claude-cli".to_string(),
        on_branch_conflict: "reject".to_string(),
        ..Default::default()
    }
}

fn its_refusal(req: StartSessionRequest) -> Status {
    refuse_peer_held_parts(&req).expect_err("an operator start naming a peer-held part is refused")
}

fn assert_refused_naming(refusal: Status, field: &str) {
    assert_eq!(
        refusal.code(),
        Code::InvalidArgument,
        "refused as a bad request; got {:?}: {}",
        refusal.code(),
        refusal.message()
    );
    assert!(
        refusal.message().contains(field),
        "the refusal names {field}: {}",
        refusal.message()
    );
}

#[test]
fn an_operator_start_naming_a_codebase_host_is_refused_naming_the_field() {
    let refusal = its_refusal(StartSessionRequest {
        codebase_daemon_instance_id: "workstation-b".to_string(),
        managed_codebase: true,
        ..an_operator_start()
    });

    assert_refused_naming(refusal, "codebase_daemon_instance_id");
}

#[test]
fn an_operator_start_asking_for_a_sandboxed_codebase_is_refused_naming_the_field() {
    let refusal = its_refusal(StartSessionRequest {
        sandboxed_codebase: true,
        ..an_operator_start()
    });

    assert_refused_naming(refusal, "sandboxed_codebase");
}

#[test]
fn an_operator_start_seeding_specialized_agents_is_refused_naming_the_field() {
    let refusal = its_refusal(StartSessionRequest {
        specialized_agents: vec!["reviewer".to_string()],
        ..an_operator_start()
    });

    assert_refused_naming(refusal, "specialized_agents");
}

#[test]
fn an_operator_start_placing_agents_on_clones_is_refused_naming_the_field() {
    let refusal = its_refusal(StartSessionRequest {
        clone_placed_agents: vec!["reviewer".to_string()],
        ..an_operator_start()
    });

    assert_refused_naming(refusal, "clone_placed_agents");
}

#[test]
fn an_operator_start_naming_no_peer_held_part_is_admitted() {
    let admitted = refuse_peer_held_parts(&an_operator_start());

    assert!(
        admitted.is_ok(),
        "an operator start that names no peer-held part goes on: {admitted:?}"
    );
}
