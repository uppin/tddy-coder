//! `ListSshConfigHosts` addressing and honesty — the RPC, not the parser.

use tddy_rpc::{Request, Status};
use tddy_service::proto::host::{
    HostService, ListSshConfigHostsRequest, ListSshConfigHostsResponse, ProbeOutcome,
};

use crate::test_util::{test_service, TEST_TOKEN};

const AN_UNKNOWN_HOST: &str = "daemon-on-some-other-machine";
const LISTING_WINDOW: std::time::Duration = std::time::Duration::from_secs(2);

async fn listed(
    service: &crate::service::HostServiceImpl,
    request: ListSshConfigHostsRequest,
) -> Result<ListSshConfigHostsResponse, Status> {
    tokio::time::timeout(
        LISTING_WINDOW,
        service.list_ssh_config_hosts(Request::new(request)),
    )
    .await
    .expect("a listing reads one file and must not hang")
    .map(|response| response.into_inner())
}

#[tokio::test]
async fn refuses_a_listing_without_a_session() {
    // Given
    let temp = tempfile::tempdir().expect("tddy data dir");
    let service = test_service(temp.path());

    // When
    let refused = listed(
        &service,
        ListSshConfigHostsRequest {
            session_token: "not-a-session".to_string(),
            daemon_instance_id: String::new(),
        },
    )
    .await;

    // Then
    assert_eq!(
        refused.err().map(|status| status.code),
        Some(tddy_rpc::Code::Unauthenticated)
    );
}

#[tokio::test]
async fn refuses_a_listing_addressed_to_a_host_this_daemon_does_not_know() {
    // Given
    let temp = tempfile::tempdir().expect("tddy data dir");
    let service = test_service(temp.path());

    // When
    let refused = listed(
        &service,
        ListSshConfigHostsRequest {
            session_token: TEST_TOKEN.to_string(),
            daemon_instance_id: AN_UNKNOWN_HOST.to_string(),
        },
    )
    .await;

    // Then — a bad address, not this host's config wearing another name
    assert_eq!(
        refused.err().map(|status| status.code),
        Some(tddy_rpc::Code::InvalidArgument)
    );
}

#[tokio::test]
async fn lists_explicit_aliases_from_the_operator_config() {
    // Given a host whose operator has Host buildbox and Host *
    let temp = tempfile::tempdir().expect("tddy data dir");
    let service = test_service(temp.path());

    // When they ask what destinations this host can ssh to
    let listed = listed(
        &service,
        ListSshConfigHostsRequest {
            session_token: TEST_TOKEN.to_string(),
            daemon_instance_id: String::new(),
        },
    )
    .await
    .expect("a readable config is a listing");

    // Then only the explicit alias is offered
    assert_eq!(listed.outcome, ProbeOutcome::Ok as i32);
    assert_eq!(
        listed
            .hosts
            .iter()
            .map(|host| host.alias.as_str())
            .collect::<Vec<_>>(),
        vec!["buildbox"]
    );
}

#[tokio::test]
async fn reports_an_unreadable_config_as_a_failure_not_as_zero_aliases() {
    // Given — the parser/handler distinguish unread from empty; unimplemented must not
    // collapse them.
    let temp = tempfile::tempdir().expect("tddy data dir");
    let service = test_service(temp.path());

    // When
    let listed = listed(
        &service,
        ListSshConfigHostsRequest {
            session_token: TEST_TOKEN.to_string(),
            daemon_instance_id: String::new(),
        },
    )
    .await
    .expect("unread is a response, not a transport failure");

    // Then
    assert_eq!(listed.outcome, ProbeOutcome::Failed as i32);
    assert!(
        listed.hosts.is_empty(),
        "a failed read must not invent aliases"
    );
    assert!(
        !listed.failure_reason.is_empty(),
        "FAILED without a reason looks like a missing outcome"
    );
}
