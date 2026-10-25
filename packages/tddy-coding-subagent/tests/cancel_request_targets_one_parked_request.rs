//! Unit tests: cancelling one parked permission request answers that request alone.
//!
//! A conversation's requests park in one queue, and a caller whose approval is dropped releases
//! only its own request. `cancel_pending` answers every request, so it cannot serve that caller.

use std::sync::Arc;
use std::time::Duration;

use tddy_agent_backend::{
    PermissionDecision, PermissionHandler, PermissionOptionView, PermissionRequest,
};
use tddy_coding_subagent::ParkedPermissions;
use tddy_discovery::subagent::PermissionAnswer;

const WAIT: Duration = Duration::from_secs(5);

fn a_request(title: &str) -> PermissionRequest {
    PermissionRequest {
        session_id: "sess-1".to_string(),
        tool_call_id: title.to_string(),
        title: Some(title.to_string()),
        raw_input: None,
        options: vec![PermissionOptionView {
            option_id: "allow".to_string(),
            name: "Allow".to_string(),
            kind: "allow_once".to_string(),
        }],
    }
}

/// Wait until a request parks at the front of `permissions`, and name it.
async fn the_front_request(permissions: &ParkedPermissions) -> String {
    tokio::time::timeout(WAIT, async {
        loop {
            if let Some(view) = permissions.pending() {
                return view.request_id;
            }
            permissions.parked().await;
        }
    })
    .await
    .expect("a request parks")
}

#[tokio::test]
async fn cancelling_one_parked_request_leaves_the_others_parked() {
    // Given: two requests parked on one conversation, the first at the front.
    let permissions = Arc::new(ParkedPermissions::default());
    let first = tokio::spawn({
        let permissions = Arc::clone(&permissions);
        async move { permissions.decide(a_request("Bash")).await }
    });
    let front = the_front_request(&permissions).await;
    let second = tokio::spawn({
        let permissions = Arc::clone(&permissions);
        async move { permissions.decide(a_request("Edit")).await }
    });

    // When: only the second request is cancelled, by the id it is named by.
    let cancelled = tokio::time::timeout(WAIT, async {
        loop {
            if permissions.cancel_request("perm-2") {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;

    // Then: the second answers cancelled, and the first is still the request pending.
    cancelled.expect("the second request parks, and is cancelled by its id");
    assert_eq!(
        second.await.expect("the second request finishes"),
        PermissionDecision::Cancelled
    );
    assert_eq!(
        permissions.pending().map(|view| view.request_id),
        Some(front.clone()),
        "cancelling the second request left the first pending"
    );
    permissions
        .answer(&PermissionAnswer::Selected {
            request_id: front,
            option_id: "allow".to_string(),
        })
        .expect("the first request still takes an answer");
    assert_eq!(
        first.await.expect("the first request finishes"),
        PermissionDecision::Selected {
            option_id: "allow".to_string()
        }
    );
}

#[tokio::test]
async fn cancelling_an_answered_request_does_nothing() {
    // Given: a request that has been answered.
    let permissions = Arc::new(ParkedPermissions::default());
    let asking = tokio::spawn({
        let permissions = Arc::clone(&permissions);
        async move { permissions.decide(a_request("Bash")).await }
    });
    let front = the_front_request(&permissions).await;
    permissions
        .answer(&PermissionAnswer::Selected {
            request_id: front.clone(),
            option_id: "allow".to_string(),
        })
        .expect("the request takes an answer");

    // When: it is cancelled after the fact, by the id it had.
    let was_parked = permissions.cancel_request(&front);

    // Then: nothing was parked to cancel, and the answer stands.
    assert!(!was_parked, "an answered request is no longer parked");
    assert_eq!(
        asking.await.expect("the request finishes"),
        PermissionDecision::Selected {
            option_id: "allow".to_string()
        }
    );
}
