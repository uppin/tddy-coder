#![cfg(test)]
//! `ListDerivedHosts` addressing — the RPC, not the sources. What it lists is in
//! `tests/derived_hosts_acceptance.rs`.

use tddy_rpc::{Code, Request};
use tddy_service::proto::host::{HostService, ListDerivedHostsRequest};

use crate::test_util::{test_service, TEST_TOKEN};

const AN_UNKNOWN_HOST: &str = "daemon-on-some-other-machine";
const LISTING_WINDOW: std::time::Duration = std::time::Duration::from_secs(2);

/// The code a listing was refused with; `None` for one that was answered.
async fn refusal_of(
    service: &crate::service::HostServiceImpl,
    request: ListDerivedHostsRequest,
) -> Option<Code> {
    tokio::time::timeout(
        LISTING_WINDOW,
        service.list_derived_hosts(Request::direct(request)),
    )
    .await
    .expect("a listing reads one file and must not hang")
    .err()
    .map(|status| status.code)
}

#[tokio::test]
async fn refuses_a_listing_without_a_session() {
    // Given
    let temp = tempfile::tempdir().expect("tddy data dir");
    let service = test_service(temp.path());

    // When
    let refused = refusal_of(
        &service,
        ListDerivedHostsRequest {
            session_token: "not-a-session".to_string(),
            daemon_instance_id: String::new(),
        },
    )
    .await;

    // Then
    assert_eq!(refused, Some(Code::Unauthenticated));
}

#[tokio::test]
async fn refuses_a_listing_addressed_to_a_daemon_this_one_does_not_know() {
    // Given
    let temp = tempfile::tempdir().expect("tddy data dir");
    let service = test_service(temp.path());

    // When
    let refused = refusal_of(
        &service,
        ListDerivedHostsRequest {
            session_token: TEST_TOKEN.to_string(),
            daemon_instance_id: AN_UNKNOWN_HOST.to_string(),
        },
    )
    .await;

    // Then — a bad address, not this daemon's hosts under another daemon's name
    assert_eq!(refused, Some(Code::InvalidArgument));
}
