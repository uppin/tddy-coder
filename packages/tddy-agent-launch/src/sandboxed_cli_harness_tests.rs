//! Unit tests: a sandboxed CLI harness start hosts the harness under the runner's matching agent
//! kind, runs it on the model its jail can take, and refuses a first prompt the jail cannot deliver.
//!
//! Feature: docs/ft/daemon/opencode-cli-session.md
//! Feature: docs/ft/daemon/muse-cli-session.md

use std::path::Path;

use tddy_core::cli_harness::cli_harness;
use tddy_core::provider_route::{HarnessRoute, RouteFile, RouteSecret};
use tddy_rpc::Code;

use crate::svc_start_sandboxed_cli_harness_session::{
    cli_harness_runner_argv, refuse_first_prompt, require_provider_route,
    resolve_jail_agent_binary, sandboxed_agent_kind, sandboxed_runner_model, CliRunnerArgv,
};

/// An OpenCode route on `model`: a provider configuration file that names the model, and the
/// `--model` argument the unsandboxed launch appends.
fn an_opencode_route_on(model: &str) -> HarnessRoute {
    HarnessRoute {
        env: Vec::new(),
        secret_env: vec![(
            "TDDY_OPENCODE_PROVIDER_KEY".to_string(),
            RouteSecret::new("a-fireworks-key"),
        )],
        files: vec![RouteFile {
            name: "opencode.json".to_string(),
            contents: format!(r#"{{"model":"tddy-route/{model}"}}"#),
        }],
        args: vec!["--model".to_string(), format!("tddy-route/{model}")],
    }
}

/// A Muse Code route: a key in the environment and nothing else.
fn a_muse_route() -> HarnessRoute {
    HarnessRoute {
        secret_env: vec![(
            "TDDY_MUSE_PROVIDER_KEY".to_string(),
            RouteSecret::new("a-meta-key"),
        )],
        ..HarnessRoute::default()
    }
}

fn an_opencode_runner_argv(resume: bool) -> Vec<String> {
    cli_harness_runner_argv(&CliRunnerArgv {
        sandbox_runner_path: "/opt/tddy/tddy-sandbox-runner",
        session_id: "session-1",
        context_dir: Path::new("/sessions/session-1/sandbox/context"),
        tool_ipc_socket: Path::new("/tmp/tddy-session-1.sock"),
        tddy_tools_path: "/opt/tddy/tddy-tools",
        ready_marker: Path::new("/sessions/session-1/sandbox/sandbox.ready"),
        agent_kind: "opencode",
        agent_binary: "/usr/local/bin/opencode",
        model: "tddy-route/kimi-k2",
        egress_shim_port: 41000,
        resume,
    })
}

#[test]
fn an_opencode_session_runs_the_opencode_agent_kind() {
    // Given
    let session_type = "opencode-cli";

    // When
    let agent_kind = sandboxed_agent_kind(session_type);

    // Then
    assert_eq!(agent_kind, Some("opencode"));
}

#[test]
fn a_muse_session_runs_the_muse_agent_kind() {
    // Given
    let session_type = "muse-cli";

    // When
    let agent_kind = sandboxed_agent_kind(session_type);

    // Then
    assert_eq!(agent_kind, Some("muse"));
}

#[test]
fn a_session_type_the_runner_does_not_host_has_no_agent_kind() {
    // Given
    let session_type = "cursor-cli";

    // When
    let agent_kind = sandboxed_agent_kind(session_type);

    // Then
    assert_eq!(agent_kind, None);
}

#[test]
fn a_routed_opencode_session_runs_the_model_its_route_names() {
    // Given
    let opencode = cli_harness("opencode-cli").expect("registered");
    let route = an_opencode_route_on("kimi-k2");

    // When
    let model = sandboxed_runner_model(opencode, &route, "kimi-k2").expect("a model");

    // Then
    assert_eq!(model, "tddy-route/kimi-k2");
}

#[test]
fn a_routed_muse_session_keeps_its_own_model() {
    // Given
    let muse = cli_harness("muse-cli").expect("registered");
    let route = a_muse_route();

    // When
    let model = sandboxed_runner_model(muse, &route, "muse-wonder-1.0").expect("a model");

    // Then
    assert_eq!(model, "muse-wonder-1.0");
}

#[test]
fn a_routed_muse_session_without_a_model_is_refused() {
    // Given
    let muse = cli_harness("muse-cli").expect("registered");

    // When
    let refusal =
        sandboxed_runner_model(muse, &a_muse_route(), "").expect_err("a model is required");

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert!(refusal.message().contains("needs a model"));
}

#[test]
fn a_sandboxed_opencode_session_without_a_provider_account_is_refused() {
    // Given
    let opencode = cli_harness("opencode-cli").expect("registered");

    // When
    let refusal = require_provider_route(opencode, None)
        .expect_err("the jail holds no credentials of its own");

    // Then
    assert_eq!(refusal.code(), Code::FailedPrecondition);
    assert!(refusal
        .message()
        .contains("a sandboxed opencode-cli session needs a provider account"));
}

#[test]
fn a_sandboxed_muse_session_on_a_provider_account_runs_on_its_route() {
    // Given
    let muse = cli_harness("muse-cli").expect("registered");

    // When
    let route = require_provider_route(muse, Some(a_muse_route())).expect("an account was chosen");

    // Then
    assert_eq!(route, a_muse_route());
}

#[test]
fn a_routed_opencode_session_whose_route_names_no_model_is_refused() {
    // Given
    let opencode = cli_harness("opencode-cli").expect("registered");
    let route = HarnessRoute {
        args: Vec::new(),
        ..an_opencode_route_on("kimi-k2")
    };

    // When
    let refusal = sandboxed_runner_model(opencode, &route, "kimi-k2")
        .expect_err("the runner refuses a blank model, so the route must name one");

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
}

#[test]
fn a_fresh_opencode_start_runs_the_runner_in_opencode_mode_on_its_binary_and_model() {
    // Given / When
    let argv = an_opencode_runner_argv(false);

    // Then
    assert_eq!(
        argv,
        [
            "/opt/tddy/tddy-sandbox-runner",
            "--session-id",
            "session-1",
            "--context-dir",
            "/sessions/session-1/sandbox/context",
            "--tool-ipc-socket",
            "/tmp/tddy-session-1.sock",
            "--tddy-tools-path",
            "/opt/tddy/tddy-tools",
            "--ready-marker",
            "/sessions/session-1/sandbox/sandbox.ready",
            "--agent-kind",
            "opencode",
            "--agent-binary",
            "/usr/local/bin/opencode",
            "--model",
            "tddy-route/kimi-k2",
            "--egress-shim-port",
            "41000",
            "--stdio",
        ]
    );
}

#[test]
fn a_resume_is_a_fresh_start_plus_the_resume_flag() {
    // Given
    let mut expected = an_opencode_runner_argv(false);
    expected.push("--resume".to_string());

    // When
    let resumed = an_opencode_runner_argv(true);

    // Then
    assert_eq!(resumed, expected);
}

#[test]
fn a_first_prompt_is_refused_for_a_sandboxed_opencode_session() {
    // Given
    let opencode = cli_harness("opencode-cli").expect("registered");

    // When
    let refusal = refuse_first_prompt(opencode, "fix the failing test")
        .expect_err("the runner hosts no first prompt");

    // Then
    assert_eq!(refusal.code(), Code::FailedPrecondition);
    assert!(refusal.message().contains("opencode-cli"));
}

#[test]
fn a_blank_first_prompt_is_not_refused_for_a_sandboxed_opencode_session() {
    // Given
    let opencode = cli_harness("opencode-cli").expect("registered");

    // When
    let outcome = refuse_first_prompt(opencode, "   ");

    // Then
    assert!(outcome.is_ok());
}

/// The canonical absolute path of `file`, as the jail is given it.
fn canonical_text(file: &Path) -> String {
    std::fs::canonicalize(file)
        .expect("an existing file")
        .to_string_lossy()
        .into_owned()
}

#[test]
fn a_bare_harness_binary_is_run_at_its_absolute_path_on_the_path() {
    // Given
    let bin_dir = tempfile::tempdir().expect("a bin directory");
    let binary = bin_dir.path().join("opencode");
    std::fs::write(&binary, "").expect("a binary");
    let path_var = bin_dir.path().to_string_lossy().into_owned();

    // When
    let resolved = resolve_jail_agent_binary("opencode", &path_var).expect("found on the path");

    // Then
    assert_eq!(resolved, canonical_text(&binary));
}

#[test]
fn a_bare_harness_binary_missing_from_the_path_is_refused() {
    // Given
    let bin_dir = tempfile::tempdir().expect("an empty bin directory");
    let path_var = bin_dir.path().to_string_lossy().into_owned();

    // When
    let refusal = resolve_jail_agent_binary("opencode", &path_var)
        .expect_err("the jail cannot read a binary it cannot find");

    // Then
    assert_eq!(refusal.code(), Code::FailedPrecondition);
    assert!(refusal.message().contains("opencode"));
}

#[test]
fn a_harness_binary_named_by_a_path_is_not_searched_for_on_the_path() {
    // Given
    let bin_dir = tempfile::tempdir().expect("a bin directory");
    let binary = bin_dir.path().join("opencode-custom");
    std::fs::write(&binary, "").expect("a binary");
    let named = binary.to_string_lossy().into_owned();

    // When
    let resolved = resolve_jail_agent_binary(&named, "").expect("a path needs no search");

    // Then
    assert_eq!(resolved, canonical_text(&binary));
}
