//! `ListSshConfigHosts` addressing and honesty — the RPC, not the parser.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_rpc::{Request, Status};
use tddy_service::proto::host::{
    HostService, ListSshConfigHostsRequest, ListSshConfigHostsResponse, ProbeOutcome,
};

use crate::host_private_key::HostUserFiles;
use crate::test_util::{test_service, TEST_TOKEN};

const AN_UNKNOWN_HOST: &str = "daemon-on-some-other-machine";
const LISTING_WINDOW: std::time::Duration = std::time::Duration::from_secs(2);

async fn listed(
    service: &crate::service::HostServiceImpl,
    request: ListSshConfigHostsRequest,
) -> Result<ListSshConfigHostsResponse, Status> {
    tokio::time::timeout(
        LISTING_WINDOW,
        service.list_ssh_config_hosts(Request::direct(request)),
    )
    .await
    .expect("a listing reads one file and must not hang")
    .map(|response| response.into_inner())
}

fn operator_home_with_config(config: &str) -> (tempfile::TempDir, PathBuf) {
    let storage = tempfile::tempdir().expect("operator home storage");
    let home = storage.path().join("home").join("testdev");
    std::fs::create_dir_all(home.join(".ssh")).expect("operator has ~/.ssh");
    std::fs::write(home.join(".ssh").join("config"), config).expect("ssh config");
    (storage, home)
}

fn service_for_operator_home(home: &Path) -> (tempfile::TempDir, crate::service::HostServiceImpl) {
    let tddy_data = tempfile::tempdir().expect("tddy data dir");
    let service = test_service(tddy_data.path()).with_host_user_files(Arc::new(
        crate::host_private_key::UserFilesUnder::home(home).and_the_home_of("testdev", home),
    ));
    (tddy_data, service)
}

/// The only seam that can make `~/.ssh/config` unreadable without depending on who runs the test.
struct SshConfigUnreadable {
    files: crate::host_private_key::UserFilesUnder,
}

impl HostUserFiles for SshConfigUnreadable {
    fn home_dir(&self, os_user: &str) -> Result<PathBuf, String> {
        self.files.home_dir(os_user)
    }

    fn read_as_user(&self, os_user: &str, path: &Path) -> Result<Vec<u8>, String> {
        if path.file_name().is_some_and(|name| name == "config")
            && path.parent().is_some_and(|parent| parent.ends_with(".ssh"))
        {
            return Err("permission denied".to_string());
        }
        self.files.read_as_user(os_user, path)
    }

    fn list_files_as_user(&self, os_user: &str, dir: &Path) -> Result<Vec<PathBuf>, String> {
        self.files.list_files_as_user(os_user, dir)
    }
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
    let (_home_storage, home) =
        operator_home_with_config("Host buildbox\nHost *\n    StrictHostKeyChecking accept-new\n");
    let (_tddy_storage, service) = service_for_operator_home(&home);

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
    // Given — the parser/handler distinguish unread from empty
    let (_home_storage, home) = operator_home_with_config("Host buildbox\n");
    let tddy_data = tempfile::tempdir().expect("tddy data dir");
    let service =
        test_service(tddy_data.path()).with_host_user_files(Arc::new(SshConfigUnreadable {
            files: crate::host_private_key::UserFilesUnder::home(&home)
                .and_the_home_of("testdev", &home),
        }));

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
