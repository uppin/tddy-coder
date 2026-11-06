//! Unit: an operator start reaches the generic start as the `StartSessionRequest` it stands for,
//! and the generic start's answer comes back as what the operator is told (`#agent-tools 18/19`).

use pretty_assertions::assert_eq;
use tddy_daemon_extensions::{
    OperatorBranchChoice, OperatorBranchConflict, OperatorSessionStart, OperatorStartOutcome,
};
use tddy_service::proto::session::{BranchConflict, StartSessionRequest, StartSessionResponse};
use tddy_service::proto::types::BranchSession;
use tddy_session_lifecycle::connection_service::operator_session_starter::{
    operator_start_outcome, operator_start_request,
};

fn an_operator_start(branch: OperatorBranchChoice) -> OperatorSessionStart {
    OperatorSessionStart {
        os_user: "dev".to_string(),
        project_id: "proj".to_string(),
        session_type: "claude-cli".to_string(),
        model: "opus".to_string(),
        initial_prompt: "fix the bug".to_string(),
        branch,
    }
}

#[test]
fn a_new_branch_operator_start_is_a_tokenless_request_rejected_on_a_branch_conflict() {
    // Given
    let start = an_operator_start(OperatorBranchChoice::NewBranchFromBase {
        new_branch_name: "feature/fix-the-bug".to_string(),
        integration_base_ref: "origin/main".to_string(),
    });

    // When
    let request = operator_start_request(&start);

    // Then
    assert_eq!(
        request,
        StartSessionRequest {
            project_id: "proj".to_string(),
            session_type: "claude-cli".to_string(),
            model: "opus".to_string(),
            initial_prompt: "fix the bug".to_string(),
            branch_worktree_intent: "new_branch_from_base".to_string(),
            new_branch_name: "feature/fix-the-bug".to_string(),
            selected_integration_base_ref: "origin/main".to_string(),
            on_branch_conflict: "reject".to_string(),
            ..Default::default()
        }
    );
}

#[test]
fn a_work_on_branch_operator_start_selects_the_branch_to_work_on() {
    // Given
    let start = an_operator_start(OperatorBranchChoice::WorkOnBranch {
        branch: "feature/auth".to_string(),
    });

    // When
    let request = operator_start_request(&start);

    // Then
    assert_eq!(
        (
            request.branch_worktree_intent,
            request.selected_branch_to_work_on,
            request.new_branch_name,
        ),
        (
            "work_on_selected_branch".to_string(),
            "feature/auth".to_string(),
            String::new(),
        )
    );
}

#[test]
fn a_started_session_is_reported_by_its_id() {
    // Given
    let response = StartSessionResponse {
        session_id: "started-1".to_string(),
        ..Default::default()
    };

    // When
    let outcome = operator_start_outcome(response);

    // Then
    assert_eq!(
        outcome,
        OperatorStartOutcome::Started {
            session_id: "started-1".to_string()
        }
    );
}

#[test]
fn a_rejected_branch_conflict_is_reported_with_its_owner_and_suggested_name() {
    // Given
    let response = StartSessionResponse {
        branch_conflict: Some(BranchConflict {
            branch: "feature/auth".to_string(),
            owner: Some(BranchSession {
                exists: true,
                session_id: "owner-1".to_string(),
                is_active: true,
                ..Default::default()
            }),
            suggested_branch_name: "feature/auth-1".to_string(),
        }),
        ..Default::default()
    };

    // When
    let outcome = operator_start_outcome(response);

    // Then
    assert_eq!(
        outcome,
        OperatorStartOutcome::BranchConflict(OperatorBranchConflict {
            branch: "feature/auth".to_string(),
            owner_session_id: "owner-1".to_string(),
            suggested_branch_name: "feature/auth-1".to_string(),
        })
    );
}
