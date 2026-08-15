//! Acceptance: `remote_git.RemoteGitService` admission — who may run which git verb against which
//! repository.
//!
//! PRD: docs/ft/daemon/remote-git-repo.md § Server — authorization and resolution (AC7–AC14).
//!
//! Every rejection here happens **before a process is spawned**. Two properties are what keep this
//! service from being a remote shell, and both are asserted rather than assumed: the verb
//! whitelist is closed and enforced server-side, and the repository path is read from the daemon's
//! own project registry, never from the request.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use pretty_assertions::assert_eq;
use rstest::rstest;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::project_storage::{write_projects, ProjectData};
use tddy_daemon::remote_git_service::{
    open_from_first_frame, resolve_git_verb, resolve_project_repo, AuthorizedGitRequest, GitVerb,
    ProjectsDirResolver, RemoteGitServiceImpl, UserResolver,
};
use tddy_rpc::Code;
use tddy_service::proto::remote_git::{GitClientFrame, GitOpen};

const VALID_TOKEN: &str = "valid-token";
const GITHUB_USER: &str = "testuser";
const OS_USER: &str = "testuser";
const PROJECT_ID: &str = "0198f1b0-0000-7000-8000-000000000001";
const PROJECT_NAME: &str = "my-app";

/// A daemon that maps `testuser` (GitHub) to `testuser` (OS).
fn a_daemon_config() -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("daemon.yaml");
    std::fs::write(
        &path,
        format!("users:\n  - github_user: \"{GITHUB_USER}\"\n    os_user: \"{OS_USER}\"\n"),
    )
    .expect("write daemon.yaml");
    let config = DaemonConfig::load(&path).expect("config must parse");
    (dir, config)
}

fn a_project(name: &str, project_id: &str, main_repo_path: &std::path::Path) -> ProjectData {
    ProjectData {
        project_id: project_id.to_string(),
        name: name.to_string(),
        git_url: format!("https://github.com/example/{name}.git"),
        main_repo_path: main_repo_path.to_string_lossy().to_string(),
        main_branch_ref: None,
        remote_name: None,
        host_repo_paths: HashMap::new(),
    }
}

/// A daemon serving one project, whose checkout exists on disk.
struct AServingDaemon {
    service: RemoteGitServiceImpl,
    repo_path: PathBuf,
    _config_dir: tempfile::TempDir,
    _home: tempfile::TempDir,
}

fn a_serving_daemon() -> AServingDaemon {
    let home = tempfile::tempdir().expect("tempdir");
    let projects_dir = home.path().join(".tddy").join("projects");
    let repo_path = home.path().join("repos").join(PROJECT_NAME);
    std::fs::create_dir_all(&repo_path).expect("create repo dir");
    write_projects(
        &projects_dir,
        &[a_project(PROJECT_NAME, PROJECT_ID, &repo_path)],
    )
    .expect("write projects.yaml");

    let (config_dir, config) = a_daemon_config();
    let user_resolver: UserResolver =
        Arc::new(|token| (token == VALID_TOKEN).then(|| GITHUB_USER.to_string()));
    let resolver_dir = projects_dir.clone();
    let projects_dir_resolver: ProjectsDirResolver =
        Arc::new(move |os_user| (os_user == OS_USER).then(|| resolver_dir.clone()));

    AServingDaemon {
        service: RemoteGitServiceImpl::new(user_resolver, projects_dir_resolver, Arc::new(config)),
        repo_path,
        _config_dir: config_dir,
        _home: home,
    }
}

fn an_open_frame(project_ref: &str) -> GitOpen {
    GitOpen {
        session_token: VALID_TOKEN.to_string(),
        project_ref: project_ref.to_string(),
        verb: "git-upload-pack".to_string(),
    }
}

fn authorized(daemon: &AServingDaemon, open: GitOpen) -> AuthorizedGitRequest {
    daemon
        .service
        .authorize_open(&open)
        .expect("open must be authorized")
}

fn refused(daemon: &AServingDaemon, open: GitOpen) -> tddy_rpc::Status {
    daemon
        .service
        .authorize_open(&open)
        .expect_err("open must be refused")
}

// --- AC7: the open frame ---------------------------------------------------------------------

#[test]
fn reads_the_open_payload_from_the_streams_first_frame() {
    // Given a first frame carrying an open
    let frame = GitClientFrame {
        open: Some(an_open_frame(PROJECT_NAME)),
        stdin: Vec::new(),
        stdin_eof: false,
    };

    // When
    let open = open_from_first_frame(frame).expect("a frame with an open must yield it");

    // Then
    assert_eq!(open.project_ref, PROJECT_NAME);
    assert_eq!(open.verb, "git-upload-pack");
}

#[test]
fn rejects_a_first_frame_that_carries_only_stdin_bytes() {
    // Given a client that started sending data without opening
    let frame = GitClientFrame {
        open: None,
        stdin: b"0000".to_vec(),
        stdin_eof: false,
    };

    // When
    let status = open_from_first_frame(frame).expect_err("an unopened stream must be refused");

    // Then
    assert_eq!(status.code(), Code::InvalidArgument);
}

// --- AC8 / AC9: authentication and OS-user mapping -------------------------------------------

#[rstest]
#[case::absent("")]
#[case::malformed("not-a-token")]
#[case::forged("v1.eyJsb2dpbiI6ImF0dGFja2VyIn0.deadbeef")]
fn refuses_an_unusable_session_token_as_unauthenticated(#[case] token: &str) {
    // Given a token the daemon's resolver does not accept
    let daemon = a_serving_daemon();
    let open = GitOpen {
        session_token: token.to_string(),
        ..an_open_frame(PROJECT_NAME)
    };

    // When
    let status = refused(&daemon, open);

    // Then
    assert_eq!(status.code(), Code::Unauthenticated);
}

#[test]
fn refuses_a_valid_login_that_maps_to_no_os_user() {
    // Given a token that resolves to a GitHub login absent from the daemon's `users:` map
    let home = tempfile::tempdir().expect("tempdir");
    let (config_dir, config) = a_daemon_config();
    let user_resolver: UserResolver = Arc::new(|_| Some("unmapped-user".to_string()));
    let projects_dir_resolver: ProjectsDirResolver = Arc::new(|_| None);
    let daemon = AServingDaemon {
        service: RemoteGitServiceImpl::new(user_resolver, projects_dir_resolver, Arc::new(config)),
        repo_path: home.path().to_path_buf(),
        _config_dir: config_dir,
        _home: home,
    };

    // When
    let status = refused(&daemon, an_open_frame(PROJECT_NAME));

    // Then
    assert_eq!(status.code(), Code::PermissionDenied);
}

// --- AC10 / AC11: project resolution, and the absence of a path surface ------------------------

#[test]
fn resolves_a_project_by_its_name_to_the_registrys_main_repo_path() {
    // Given a daemon serving "my-app"
    let daemon = a_serving_daemon();

    // When
    let request = authorized(&daemon, an_open_frame(PROJECT_NAME));

    // Then
    assert_eq!(request.repo_path, daemon.repo_path);
    assert_eq!(request.os_user, OS_USER);
}

#[test]
fn resolves_a_project_by_its_id_as_well_as_its_name() {
    // Given the same project addressed by its uuid
    let daemon = a_serving_daemon();

    // When
    let request = authorized(&daemon, an_open_frame(PROJECT_ID));

    // Then
    assert_eq!(request.repo_path, daemon.repo_path);
}

#[test]
fn refuses_a_project_reference_the_registry_does_not_name() {
    // Given a reference to a project this user does not have
    let daemon = a_serving_daemon();

    // When
    let status = refused(&daemon, an_open_frame("someone-elses-project"));

    // Then
    assert_eq!(status.code(), Code::NotFound);
}

#[rstest]
#[case::absolute_root("/etc")]
#[case::absolute_repo("/var/lib/other-user/repos/secret")]
#[case::traversal("../../../etc")]
#[case::dot("..")]
fn treats_a_filesystem_path_as_an_unknown_project_name_rather_than_a_location(
    #[case] project_ref: &str,
) {
    // Given a client that sends a path where a project reference belongs
    let daemon = a_serving_daemon();

    // When
    let status = refused(&daemon, an_open_frame(project_ref));

    // Then it is a failed name lookup — the request never selects a directory
    assert_eq!(status.code(), Code::NotFound);
}

#[test]
fn resolves_a_project_only_against_the_calling_users_own_registry() {
    // Given a registry that exists for `testuser` and a caller mapped to nobody
    let home = tempfile::tempdir().expect("tempdir");
    let projects_dir = home.path().join(".tddy").join("projects");
    let repo_path = home.path().join("repos").join(PROJECT_NAME);
    std::fs::create_dir_all(&repo_path).expect("create repo dir");
    write_projects(
        &projects_dir,
        &[a_project(PROJECT_NAME, PROJECT_ID, &repo_path)],
    )
    .expect("write projects.yaml");

    // When another OS user's registry is asked for the same name
    let other_projects_dir = home.path().join("other").join("projects");
    std::fs::create_dir_all(&other_projects_dir).expect("create other projects dir");
    let status = resolve_project_repo(&other_projects_dir, PROJECT_NAME)
        .expect_err("another user's registry must not resolve it");

    // Then
    assert_eq!(status.code(), Code::NotFound);
}

// --- AC14: the checkout must exist -------------------------------------------------------------

#[test]
fn refuses_a_registered_project_whose_checkout_is_missing_from_disk() {
    // Given a registry row pointing at a directory that was moved or deleted
    let home = tempfile::tempdir().expect("tempdir");
    let projects_dir = home.path().join(".tddy").join("projects");
    let vanished = home.path().join("repos").join("vanished");
    write_projects(
        &projects_dir,
        &[a_project("vanished", PROJECT_ID, &vanished)],
    )
    .expect("write projects.yaml");

    // When
    let status = resolve_project_repo(&projects_dir, "vanished")
        .expect_err("a missing checkout must be refused");

    // Then it is a precondition failure, not a lookup failure — the project does exist
    assert_eq!(status.code(), Code::FailedPrecondition);
}

// --- AC12: the closed verb whitelist, enforced here rather than on the client ------------------

#[rstest]
#[case::hyphenated_upload("git-upload-pack", GitVerb::UploadPack)]
#[case::spaced_upload("git upload-pack", GitVerb::UploadPack)]
#[case::hyphenated_receive("git-receive-pack", GitVerb::ReceivePack)]
#[case::spaced_receive("git receive-pack", GitVerb::ReceivePack)]
fn admits_each_git_pack_verb(#[case] verb: &str, #[case] expected: GitVerb) {
    // Given a whitelisted verb
    // When
    let resolved = resolve_git_verb(verb).expect("a pack verb must be admitted");

    // Then
    assert_eq!(resolved, expected);
}

#[rstest]
#[case::shell("sh")]
#[case::bash_with_command("bash -c id")]
#[case::empty("")]
#[case::archive("git-upload-archive")]
#[case::lookalike_prefix("git-upload-packet")]
#[case::arbitrary_git("git log")]
#[case::injection("git-upload-pack; id")]
fn refuses_any_verb_outside_the_pack_whitelist(#[case] verb: &str) {
    // Given a verb the client should never have sent
    // When
    let status = resolve_git_verb(verb).expect_err("a non-pack verb must be refused");

    // Then
    assert_eq!(status.code(), Code::PermissionDenied);
}

#[test]
fn enforces_the_verb_whitelist_on_the_server_even_though_the_client_also_checks_it() {
    // Given a client that skipped its own check — the only threat model that matters here
    let daemon = a_serving_daemon();
    let open = GitOpen {
        verb: "sh -c 'cat ~/.ssh/id_rsa'".to_string(),
        ..an_open_frame(PROJECT_NAME)
    };

    // When
    let status = refused(&daemon, open);

    // Then
    assert_eq!(status.code(), Code::PermissionDenied);
}

#[test]
fn carries_the_resolved_verb_through_to_the_authorized_request() {
    // Given a push
    let daemon = a_serving_daemon();
    let open = GitOpen {
        verb: "git-receive-pack".to_string(),
        ..an_open_frame(PROJECT_NAME)
    };

    // When
    let request = authorized(&daemon, open);

    // Then
    assert_eq!(request.verb, GitVerb::ReceivePack);
}
