//! Unit tests: a parked permission request reports the ACP tool kind of the call that asked, so the
//! main agent (and the user it hands the request to) reads what kind of call is waiting.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md § permission requests (`toolCall.kind`)

use std::sync::Arc;
use std::time::Duration;

use tddy_agent_backend::{PermissionHandler, PermissionOptionView, PermissionRequest};
use tddy_coding_subagent::ParkedPermissions;
use tddy_discovery::subagent::PermissionRequestView;

const WAIT: Duration = Duration::from_secs(5);

/// A request from the agent for a call of `kind`, offering one option.
fn a_request_of_kind(kind: Option<&str>) -> PermissionRequest {
    PermissionRequest {
        session_id: "sess-1".to_string(),
        tool_call_id: "call-1".to_string(),
        title: Some("cargo test".to_string()),
        raw_input: None,
        kind: kind.map(str::to_string),
        options: vec![PermissionOptionView {
            option_id: "allow".to_string(),
            name: "Allow".to_string(),
            kind: "allow_once".to_string(),
        }],
    }
}

/// Park `request` on a conversation's permissions and read the view the caller is shown of it.
async fn the_view_of(request: PermissionRequest) -> PermissionRequestView {
    let permissions = Arc::new(ParkedPermissions::default());
    let _asking = tokio::spawn({
        let permissions = Arc::clone(&permissions);
        async move { permissions.decide(request).await }
    });
    tokio::time::timeout(WAIT, async {
        loop {
            if let Some(view) = permissions.pending() {
                return view;
            }
            permissions.parked().await;
        }
    })
    .await
    .expect("the request parks")
}

#[tokio::test]
async fn a_parked_request_reports_the_tool_kind_the_agent_named() {
    // When
    let view = the_view_of(a_request_of_kind(Some("execute"))).await;

    // Then
    assert_eq!(view.tool_call.kind, "execute");
}

#[tokio::test]
async fn a_parked_request_with_no_tool_kind_reports_other() {
    // When
    let view = the_view_of(a_request_of_kind(None)).await;

    // Then
    assert_eq!(view.tool_call.kind, "other");
}
