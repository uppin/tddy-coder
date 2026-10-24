//! Warp as a CLI harness: registered, started bare, resumed into the conversation whose token it
//! printed on exit, never routed onto a provider account, and offering no models.
//!
//! Feature: docs/ft/daemon/warp-cli-session.md

use tddy_agent_backend::cli_harness::{cli_harness, CliHarness, CliLaunch};
use tddy_agent_backend::cli_harness_warp::{resume_token_from_output, warp_cli_models};
use tddy_agent_backend::provider_route::{
    resolve_route, Harness, RouteEndpoints, RouteRefusal, RouteSecret, RouteTarget,
};

fn warp() -> &'static CliHarness {
    cli_harness("warp-cli").expect("warp-cli is registered")
}

/// An account of `preset` serving every wire protocol, so only the harness can refuse it.
fn an_account(preset: &str) -> RouteTarget {
    RouteTarget {
        preset: preset.to_string(),
        endpoints: RouteEndpoints {
            anthropic_messages: Some("https://example".to_string()),
            openai_responses: Some("https://example/v1".to_string()),
            openai_chat: Some("https://example/v1".to_string()),
        },
        api_key: RouteSecret::new("a-key"),
        model: "some-model".to_string(),
    }
}

/// What Warp prints as it exits: dimmed prose, then the command in bold, with CRLF line ends.
fn warps_exit_banner(token: &str) -> Vec<u8> {
    format!(
        "\x1b[2mResume this conversation with:\x1b[0m\r\n  \x1b[1mwarp --resume {token}\x1b[0m\r\n"
    )
    .into_bytes()
}

#[test]
fn warp_is_registered_as_a_cli_harness() {
    // Then
    assert_eq!(warp().label, "Warp");
    assert_eq!(warp().route_harness, Harness::WarpCli);
    assert_eq!(warp().context_agent, "warp");
    assert_eq!(warp().default_binary, "warp");
    assert_eq!(warp().binary_env, "TDDY_WARP_CLI");
    assert_eq!(warp().route_config_env, None);
}

#[test]
fn a_warp_session_starts_with_no_arguments() {
    // When: whatever else the form carried, Warp takes neither a model nor a first prompt
    let argv = (warp().argv)(&CliLaunch {
        binary: "/opt/warp/bin/warp",
        model: "some-model",
        session_id: "sess-1",
        initial_prompt: Some("plan the refactor"),
        ..CliLaunch::default()
    });

    // Then
    assert_eq!(argv, vec!["/opt/warp/bin/warp"]);
}

#[test]
fn a_resumed_warp_session_continues_its_recorded_conversation() {
    // When
    let argv = (warp().argv)(&CliLaunch {
        binary: "warp",
        conversation_id: Some("tok-123"),
        resume: true,
        ..CliLaunch::default()
    });

    // Then
    assert_eq!(argv, vec!["warp", "--resume", "tok-123"]);
}

#[test]
fn a_resumed_warp_session_without_a_token_starts_fresh() {
    // When
    let without_one = (warp().argv)(&CliLaunch {
        binary: "warp",
        resume: true,
        ..CliLaunch::default()
    });
    let with_a_blank_one = (warp().argv)(&CliLaunch {
        binary: "warp",
        conversation_id: Some("   "),
        resume: true,
        ..CliLaunch::default()
    });

    // Then: never a bare `--resume`
    assert_eq!(without_one, vec!["warp"]);
    assert_eq!(with_a_blank_one, vec!["warp"]);
}

#[test]
fn the_last_resume_line_on_exit_names_the_token() {
    // Given: a resumed run's terminal shows the earlier banner above the new one
    let output = [warps_exit_banner("tok-old"), warps_exit_banner("tok-new")].concat();

    // When
    let token = resume_token_from_output(&output);

    // Then
    assert_eq!(token.as_deref(), Some("tok-new"));
}

#[test]
fn escape_codes_around_the_resume_line_are_ignored() {
    // When
    let token = resume_token_from_output(&warps_exit_banner("01JC9Z8K2Q"));

    // Then
    assert_eq!(token.as_deref(), Some("01JC9Z8K2Q"));
}

#[test]
fn a_window_title_set_before_the_resume_line_is_ignored() {
    // Given: OSC sequences closed by BEL and by ST, the second one naming a different token
    let output =
        b"\x1b]0;warp --resume tok-in-title\x07\x1b]2;tab\x1b\\  warp --resume tok-shown\r\n";

    // When
    let token = resume_token_from_output(output);

    // Then: only the printed line counts, and nothing of either title leaks into it
    assert_eq!(token.as_deref(), Some("tok-shown"));
}

#[test]
fn a_device_control_string_after_the_resume_line_is_ignored() {
    // Given: a DCS query, holding a resume line of its own, after the printed one
    let output = b"warp --resume tok-shown\r\n\x1bP+q544e\x1b\\\x1bPwarp --resume tok-in-dcs\x1b\\";

    // When
    let token = resume_token_from_output(output);

    // Then
    assert_eq!(token.as_deref(), Some("tok-shown"));
}

#[test]
fn single_character_escapes_inside_the_resume_line_are_ignored() {
    // Given: a charset designation (an intermediate byte, then a final one) and a keypad-mode
    // escape between the words of the line
    let output = b"\x1b(Bwarp \x1b=--resume\x1b> tok-123\r\n";

    // When
    let token = resume_token_from_output(output);

    // Then
    assert_eq!(token.as_deref(), Some("tok-123"));
}

#[test]
fn output_without_a_resume_line_names_no_token() {
    // Given: a run killed before Warp could print its banner
    let output = b"\x1b[1mWarp\x1b[0m agent ready\r\n> write the tests\r\n";

    // When
    let token = resume_token_from_output(output);

    // Then
    assert_eq!(token, None);
}

#[test]
fn warp_accepts_no_provider_account() {
    // When
    let refusals: Vec<RouteRefusal> = ["anthropic", "fireworks", "meta", "openai"]
        .into_iter()
        .map(|preset| resolve_route(Harness::WarpCli, &an_account(preset)).expect_err("refused"))
        .collect();

    // Then
    assert_eq!(
        refusals,
        vec![
            RouteRefusal::HarnessCannotBeRouted {
                harness: Harness::WarpCli
            };
            4
        ]
    );
    assert_eq!(
        refusals[0].to_string(),
        "Warp cannot be pointed at another provider; it always runs on its own vendor"
    );
}

#[test]
fn warp_offers_no_models() {
    // When
    let catalog = warp_cli_models();

    // Then
    assert!(catalog.models.is_empty());
    assert_eq!(catalog.default_model, "");
}
