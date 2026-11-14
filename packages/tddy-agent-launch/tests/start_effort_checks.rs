//! A start that names an effort its agent cannot take is refused before anything is created
//! (`#orchestrators` 3/11): a level the agent's spec does not declare, or any level for an agent
//! whose spec declares none. Both name the agent, and the first names its levels.
//!
//! The agents are this host's built-in specs, as the daemon installs them.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-11-10-start-effort.md`.

use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_agent_launch::spec_launch::HostCodingAgents;
use tddy_agent_launch::start_request_checks::refuse_an_effort_the_agent_cannot_take;
use tddy_rpc::Code;
use tddy_service::proto::session::StartSessionRequest;

/// The coding agents of a host with no `<tddyhome>/tools` overrides: the built-in specs, on a host
/// whose binaries are never resolved.
fn the_builtin_agents() -> HostCodingAgents {
    let no_overrides = tempfile::tempdir().expect("an empty tools dir");
    HostCodingAgents::new(
        tddy_coding_agent_specs::load_agent_registry(no_overrides.path())
            .expect("the built-in coding agents load"),
        Arc::new(tddy_actions::tool::ProcessToolHost),
    )
}

fn a_start_of(session_type: &str, effort: &str) -> StartSessionRequest {
    StartSessionRequest {
        session_type: session_type.to_string(),
        model: "a-model".to_string(),
        effort: effort.to_string(),
        ..Default::default()
    }
}

/// What the check answered, as the caller is told it.
fn the_answer_to(start: &StartSessionRequest) -> Result<(), (Code, String)> {
    refuse_an_effort_the_agent_cannot_take(start, Some(&the_builtin_agents()))
        .map_err(|status| (status.code, status.message))
}

#[test]
fn an_unknown_level_is_refused_naming_the_agent_and_its_levels() {
    // When
    let answer = the_answer_to(&a_start_of("claude-cli", "turbo"));

    // Then
    assert_eq!(
        answer,
        Err((
            Code::InvalidArgument,
            "effort \"turbo\" is not one of claude's effort levels (low, medium, high, xhigh, max)"
                .to_string()
        ))
    );
}

#[test]
fn any_effort_for_an_agent_without_levels_is_refused() {
    // When
    let answer = the_answer_to(&a_start_of("opencode-cli", "high"));

    // Then
    assert_eq!(
        answer,
        Err((
            Code::InvalidArgument,
            "effort is not supported for opencode-cli sessions: opencode declares no effort levels"
                .to_string()
        ))
    );
}
