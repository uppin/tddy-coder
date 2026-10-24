//! A start of a registered CLI harness the daemon cannot serve is refused up front: over a managed
//! codebase for every harness, and sandboxed for one the sandbox runner does not host.

use tddy_core::cli_harness::cli_harness;
use tddy_rpc::Code;
use tddy_service::proto::session::StartSessionRequest;

use crate::start_request_checks::refuse_an_unsupported_cli_harness_start;

fn a_start(sandbox: bool, managed_codebase: bool) -> StartSessionRequest {
    StartSessionRequest {
        sandbox,
        managed_codebase,
        ..Default::default()
    }
}

#[test]
fn a_start_over_a_managed_codebase_is_refused_naming_the_harness() {
    // When
    let refusal = refuse_an_unsupported_cli_harness_start(
        cli_harness("warp-cli").unwrap(),
        &a_start(false, true),
    )
    .expect_err("refused");

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert_eq!(
        refusal.message(),
        "managed_codebase is not supported for warp-cli sessions"
    );
}

#[test]
fn a_sandboxed_start_of_a_harness_the_runner_does_not_host_is_refused_naming_the_runner() {
    // When
    let refusal = refuse_an_unsupported_cli_harness_start(
        cli_harness("warp-cli").unwrap(),
        &a_start(true, false),
    )
    .expect_err("refused");

    // Then
    assert_eq!(refusal.code(), Code::FailedPrecondition);
    assert_eq!(
        refusal.message(),
        "the sandbox runner does not host warp-cli"
    );
}

#[test]
fn a_sandboxed_start_of_a_harness_the_runner_hosts_goes_on() {
    // When / Then
    refuse_an_unsupported_cli_harness_start(
        cli_harness("opencode-cli").unwrap(),
        &a_start(true, false),
    )
    .expect("OpenCode runs in the sandbox");
}

#[test]
fn an_unsandboxed_start_goes_on() {
    // When / Then
    refuse_an_unsupported_cli_harness_start(
        cli_harness("warp-cli").unwrap(),
        &a_start(false, false),
    )
    .expect("a plain Warp start goes on");
}
