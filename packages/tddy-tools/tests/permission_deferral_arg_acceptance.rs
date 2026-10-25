//! `subagent_resume`'s `deferPermissionToUser` argument.
//!
//! Feature: docs/ft/web/agent-activity-pane.md § The user answers a deferred permission request

use tddy_tools::permission_deferral_arg::parse_permission_deferral;

#[test]
fn a_request_id_is_deferred_to_the_user() {
    assert_eq!(
        parse_permission_deferral(&serde_json::json!({ "deferPermissionToUser": "perm-1" })),
        Ok(Some("perm-1".to_string()))
    );
}

#[test]
fn no_deferral_is_none() {
    assert_eq!(
        parse_permission_deferral(&serde_json::json!({ "sessionId": "c-1" })),
        Ok(None)
    );
}

#[test]
fn deferring_and_answering_at_once_is_refused_naming_both() {
    let refusal = parse_permission_deferral(&serde_json::json!({
        "deferPermissionToUser": "perm-1",
        "permission": { "requestId": "perm-1", "optionId": "allow-once" },
    }))
    .expect_err("one or the other");
    assert!(
        refusal.contains("deferPermissionToUser") && refusal.contains("permission"),
        "{refusal}"
    );
}
