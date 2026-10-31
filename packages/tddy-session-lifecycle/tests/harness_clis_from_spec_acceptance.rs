//! Acceptance: `opencode-cli`, `muse-cli` and `warp-cli` sessions launch from their tool
//! definitions (`tddy-coding-agent-specs/specs/{opencode,muse,warp}.yaml`) through the shared
//! launchers: a host start and resume renders the spec's argv, a jailed start lowers its route files
//! and config env from the spec's `route` section, Warp's resume token is read through the
//! `warp-resume-token` adapter, and Warp's jail refusal is its spec's `placements: [host]`.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-harness-clis-from-specs.md` (`#agent-tools 8/12`).
//!
//! The daemon is the real `DaemonSessionHost` given its coding agents as the daemon runtime gives
//! them (see `harness_clis_support`); the stub harness reports on its terminal what it was started
//! with. The jailed tests run a real Seatbelt jail with the real `tddy-sandbox-runner`, so they are
//! macOS-only: CI (ubuntu) never runs the `jailed` module.

mod harness_clis_support;

use harness_clis_support::{
    a_harness_daemon, ADaemonSpec, AHarness, META_ACCOUNT, META_KEY, RESUMED_SESSION_ID,
    STUB_RESUME_TOKEN, TEST_MODEL,
};
use pretty_assertions::assert_eq;
use tddy_rpc::Code;

#[tokio::test]
#[serial_test::serial]
async fn a_jailed_opencode_start_without_a_route_is_refused_as_today() {
    // Given — no provider account: a jail holds no OpenCode credentials of its own
    let daemon = a_harness_daemon(ADaemonSpec::of(AHarness::OpenCode));

    // When
    let refusal = daemon
        .start(daemon.a_start(true, None, None))
        .await
        .map(|(session_id, _jail)| session_id)
        .expect_err("a jailed OpenCode needs a route");

    // Then — refused up front, by the spec's `route.required_in_jail`, and nothing is spawned
    assert_eq!(
        (
            refusal.code,
            refusal
                .message
                .contains("a sandboxed opencode-cli session needs a provider account"),
            daemon.spawn_marker.exists(),
        ),
        (Code::FailedPrecondition, true, false),
        "{}",
        refusal.message
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_sandboxed_warp_cli_start_is_refused_failed_precondition_naming_the_placements() {
    // Given
    let daemon = a_harness_daemon(ADaemonSpec::of(AHarness::Warp));

    // When
    let refusal = daemon
        .start(daemon.a_start(true, None, None))
        .await
        .map(|(session_id, _jail)| session_id)
        .expect_err("warp.yaml's placements exclude the jail");

    // Then — the refusal names the session type and the spec's placements; nothing is spawned
    assert_eq!(
        (refusal.code, refusal.message, daemon.spawn_marker.exists()),
        (
            Code::FailedPrecondition,
            "the sandbox runner does not host warp-cli: warp cannot run in the jail: its spec's \
             placements are [host]"
                .to_string(),
            false,
        )
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_warp_cli_host_session_records_its_resume_token_through_the_adapter() {
    // Given — the stub `warp` prints Warp's exit banner naming its token, then exits
    let daemon = a_harness_daemon(ADaemonSpec::of(AHarness::Warp));

    // When
    let (session_id, _host) = daemon
        .start(daemon.a_start(false, None, None))
        .await
        .expect("a host warp-cli start succeeds");
    let report = daemon.stub_report(&session_id).await;

    // Then — warp.yaml's argv (no model, no prompt, no token yet), and the token recorded
    assert_eq!(
        (
            report.argv(),
            daemon
                .resume_token_reaching(&session_id, STUB_RESUME_TOKEN)
                .await,
        ),
        (
            vec![daemon.stub_path()],
            Some(STUB_RESUME_TOKEN.to_string())
        )
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_harness_profile_naming_opencode_starts_from_the_spec() {
    // Given — a profile running `opencode-cli` on the Meta account
    let mut spec = ADaemonSpec::of(AHarness::OpenCode);
    spec.profiles = vec![(
        "opencode-on-meta".to_string(),
        format!(
            "name: opencode-on-meta\nharness: opencode-cli\nprovider_account: {META_ACCOUNT}\nmodel: {TEST_MODEL}\n"
        ),
    )];
    let daemon = a_harness_daemon(spec);
    let mut start = daemon.a_start(false, None, None);
    start.session_type = String::new();
    start.model = String::new();
    start.agent = "opencode-on-meta".to_string();

    // When
    let (session_id, _host) = daemon
        .start(start)
        .await
        .expect("a start through the profile succeeds");
    let report = daemon.stub_report(&session_id).await;

    // Then — opencode.yaml's argv on the route's model, its config file named in OPENCODE_CONFIG
    // and its key in TDDY_OPENCODE_PROVIDER_KEY, as the spec's `route` section declares
    let config = report
        .route_config()
        .expect("the routed launch names its configuration file");
    assert_eq!(
        (
            daemon.metadata_of(&session_id).session_type,
            report.argv(),
            config["model"].clone(),
            report.env("TDDY_OPENCODE_PROVIDER_KEY"),
        ),
        (
            Some("opencode-cli".to_string()),
            vec![
                daemon.stub_path(),
                "--model".to_string(),
                format!("tddy-route/{TEST_MODEL}"),
            ],
            serde_json::json!(format!("tddy-route/{TEST_MODEL}")),
            META_KEY.to_string(),
        )
    );
}

/// The host halves of the start-and-resume tests: no jail, so they run everywhere.
mod host {
    use std::path::PathBuf;

    use pretty_assertions::assert_eq;

    use super::harness_clis_support::{a_harness_daemon, ADaemonSpec, AHarness, AHarnessDaemon};
    use super::{RESUMED_SESSION_ID, TEST_MODEL};

    /// What a host start and a host resume reported.
    struct AHostStartAndResume {
        started_argv: Vec<String>,
        started_cwd: PathBuf,
        /// The checkout the started session records.
        started_worktree: PathBuf,
        resumed_argv: Vec<String>,
        resumed_cwd: PathBuf,
        /// The checkout the exited session recorded.
        exited_worktree: PathBuf,
    }

    /// A host start with [`TEST_MODEL`] and a first prompt, then a host resume of an exited
    /// session.
    async fn a_host_start_and_resume(daemon: &AHarnessDaemon) -> AHostStartAndResume {
        let exited = tempfile::tempdir().unwrap();
        let exited_worktree = std::fs::canonicalize(exited.path()).unwrap();
        daemon.an_exited_host_session(&exited_worktree, None);
        let (session_id, _host) = daemon
            .start(daemon.a_start(false, None, Some("  fix the build  ")))
            .await
            .expect("a host start succeeds");
        let started = daemon.stub_report(&session_id).await;
        daemon.resume(RESUMED_SESSION_ID).await;
        let resumed = daemon.stub_report(RESUMED_SESSION_ID).await;
        AHostStartAndResume {
            started_argv: started.argv(),
            started_cwd: started.cwd(),
            started_worktree: daemon.worktree_of(&session_id),
            resumed_argv: resumed.argv(),
            resumed_cwd: resumed.cwd(),
            exited_worktree,
        }
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn a_host_opencode_start_and_resume_render_opencode_yamls_argv() {
        // Given
        let daemon = a_harness_daemon(ADaemonSpec::of(AHarness::OpenCode));

        // When
        let run = a_host_start_and_resume(&daemon).await;

        // Then — the spec's argv, each in its session's checkout
        let stub = daemon.stub_path();
        assert_eq!(
            (
                run.started_argv,
                run.started_cwd,
                run.resumed_argv,
                run.resumed_cwd,
            ),
            (
                vec![
                    stub.clone(),
                    "--model".to_string(),
                    TEST_MODEL.to_string(),
                    "--prompt".to_string(),
                    "fix the build".to_string(),
                ],
                run.started_worktree,
                vec![stub, "--continue".to_string()],
                run.exited_worktree,
            )
        );
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn a_host_muse_start_and_resume_render_muse_yamls_argv() {
        // Given
        let daemon = a_harness_daemon(ADaemonSpec::of(AHarness::Muse));

        // When
        let run = a_host_start_and_resume(&daemon).await;

        // Then — the spec's argv, each in its session's checkout
        let stub = daemon.stub_path();
        assert_eq!(
            (
                run.started_argv,
                run.started_cwd,
                run.resumed_argv,
                run.resumed_cwd,
            ),
            (
                vec![
                    stub.clone(),
                    "--model".to_string(),
                    TEST_MODEL.to_string(),
                    "fix the build".to_string(),
                ],
                run.started_worktree,
                vec![stub, "--continue".to_string()],
                run.exited_worktree,
            )
        );
    }
}

#[cfg(target_os = "macos")]
mod jailed {
    use pretty_assertions::assert_eq;
    use tddy_testing_commons::a_stub_http_endpoint_answering_ok;

    use super::harness_clis_support::{
        a_harness_daemon, ADaemonSpec, AHarness, COMPATIBLE_ACCOUNT, COMPATIBLE_KEY,
    };
    use super::{META_ACCOUNT, META_KEY, TEST_MODEL};

    #[tokio::test]
    async fn host_and_jailed_opencode_cli_starts_and_resumes_spawn_stubs_whose_argv_env_and_route_files_match_the_spec(
    ) {
        // Given
        let daemon = a_harness_daemon(ADaemonSpec::of(AHarness::OpenCode));

        // When — a jailed start on the Meta account, then its resume
        let (session_id, _jail) = daemon
            .start(daemon.a_start(true, Some(META_ACCOUNT), None))
            .await
            .expect("a jailed opencode-cli start on an account succeeds");
        let started = daemon.stub_report(&session_id).await;
        let _resumed_jail = daemon.resume(&session_id).await;
        let resumed = daemon.stub_report(&session_id).await;

        // Then — the route's model on the argv; OPENCODE_CONFIG names the route's file, written
        // owner-only under the spec's `route.files_dir`; the key in the spec's `route.key_env`
        let config_path = started.env("OPENCODE_CONFIG");
        assert_eq!(
            (
                started.argv()[1..].to_vec(),
                std::path::Path::new(&config_path)
                    .parent()
                    .map(|dir| dir.to_path_buf()),
                started.route_config_mode(),
                started.route_config().map(|config| config["model"].clone()),
                started.env("TDDY_OPENCODE_PROVIDER_KEY"),
                resumed.argv()[1..].to_vec(),
            ),
            (
                vec!["--model".to_string(), format!("tddy-route/{TEST_MODEL}")],
                Some(daemon.route_files_dir_of(&session_id)),
                "600".to_string(),
                Some(serde_json::json!(format!("tddy-route/{TEST_MODEL}"))),
                META_KEY.to_string(),
                vec!["--continue".to_string()],
            )
        );
    }

    #[tokio::test]
    async fn host_and_jailed_muse_cli_starts_and_resumes_do_the_same() {
        // Given
        let daemon = a_harness_daemon(ADaemonSpec::of(AHarness::Muse));

        // When — a jailed start on the Meta account, then its resume
        let (session_id, _jail) = daemon
            .start(daemon.a_start(true, Some(META_ACCOUNT), None))
            .await
            .expect("a jailed muse-cli start on an account succeeds");
        let started = daemon.stub_report(&session_id).await;
        let _resumed_jail = daemon.resume(&session_id).await;
        let resumed = daemon.stub_report(&session_id).await;

        // Then — the session's model (Meta's route names none), the key in META_API_KEY and no
        // configuration file: muse.yaml's route is environment only
        assert_eq!(
            (
                started.argv()[1..].to_vec(),
                started.env("META_API_KEY"),
                started.env("OPENCODE_CONFIG"),
                resumed.argv()[1..].to_vec(),
                resumed.env("META_API_KEY"),
            ),
            (
                vec!["--model".to_string(), TEST_MODEL.to_string()],
                META_KEY.to_string(),
                "UNSET".to_string(),
                vec!["--continue".to_string()],
                META_KEY.to_string(),
            )
        );
    }

    #[tokio::test]
    async fn each_jail_reaches_only_its_declared_hosts_plus_its_routes_base_url_host() {
        // Given — OpenCode on an OpenAI-compatible account whose base URL is a local stand-in, and
        // Muse on Meta; each probes the route's stand-in and an undeclared one
        let route_host = a_stub_http_endpoint_answering_ok().await;
        let undeclared = a_stub_http_endpoint_answering_ok().await;
        let route_target = format!("127.0.0.1:{}", route_host.port());
        let undeclared_target = format!("127.0.0.1:{}", undeclared.port());
        let probes = vec![route_target.clone(), undeclared_target.clone()];
        let opencode = a_harness_daemon(ADaemonSpec {
            probes: probes.clone(),
            compatible_base_url: Some(format!("http://{route_target}/v1")),
            ..ADaemonSpec::of(AHarness::OpenCode)
        });
        let muse = a_harness_daemon(ADaemonSpec {
            probes,
            ..ADaemonSpec::of(AHarness::Muse)
        });

        // When
        let (opencode_session, _opencode_jail) = opencode
            .start(opencode.a_start(true, Some(COMPATIBLE_ACCOUNT), None))
            .await
            .expect("a jailed opencode-cli start on the compatible account succeeds");
        let opencode_report = opencode.stub_report(&opencode_session).await;
        let (muse_session, _muse_jail) = muse
            .start(muse.a_start(true, Some(META_ACCOUNT), None))
            .await
            .expect("a jailed muse-cli start on Meta succeeds");
        let muse_report = muse.stub_report(&muse_session).await;

        // Then — OpenCode reaches its route's base-URL host and nothing else; Muse, whose route is
        // Meta's, reaches neither stand-in
        assert_eq!(
            (
                opencode_report.connect_status(&route_target),
                opencode_report.connect_status(&undeclared_target),
                opencode_report.env("TDDY_OPENCODE_PROVIDER_KEY"),
                muse_report.connect_status(&route_target),
                muse_report.connect_status(&undeclared_target),
            ),
            (
                "200".to_string(),
                "403".to_string(),
                COMPATIBLE_KEY.to_string(),
                "403".to_string(),
                "403".to_string(),
            )
        );
    }
}
