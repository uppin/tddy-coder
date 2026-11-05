//! `SpecBackend`'s own rules (`#agent-tools 16/19`): which transport a spec runs over, how an agent
//! that cannot take a workflow turn is refused, what the backend is named, and the shared stub
//! test double that replaces `AnyBackend::Stub`.
//!
//! Changeset: `docs/dev/1-WIP/2026-11-04-agent-tools-spec-backend.md`. The decided rule: a spec
//! declaring both `headless` and `acp` runs **headless** unless the session or request asks for ACP.

use std::path::PathBuf;

use pretty_assertions::assert_eq;
use tddy_agent_backend::provider_route::{RouteSupport, RoutedAgent};
use tddy_agent_backend::{
    AcpAgentCommand, CodingBackend, SharedBackend, SpecAcpBackend, SpecBackend, SpecBackendRefusal,
    SpecTransport,
};

/// Which `coding_agent` turn sections a spec declares.
#[derive(Clone, Copy)]
struct Declares {
    headless: bool,
    acp: bool,
}

const BOTH_SECTIONS: Declares = Declares {
    headless: true,
    acp: true,
};
const ONLY_HEADLESS: Declares = Declares {
    headless: true,
    acp: false,
};
const ONLY_ACP: Declares = Declares {
    headless: false,
    acp: true,
};
const NO_TURN_SECTION: Declares = Declares {
    headless: false,
    acp: false,
};

fn the_transport_of(
    agent: &str,
    declares: Declares,
    asked: Option<SpecTransport>,
) -> Result<SpecTransport, SpecBackendRefusal> {
    SpecTransport::chosen(agent, declares.headless, declares.acp, asked)
}

fn an_acp_backend_for(agent: &str) -> SpecBackend {
    SpecBackend::Acp(SpecAcpBackend::new(
        RoutedAgent {
            id: agent.to_string(),
            product: "Acme Code".to_string(),
            support: RouteSupport::default(),
            key_env: None,
        },
        AcpAgentCommand {
            program: PathBuf::from("/nonexistent/acme-acp"),
            ..AcpAgentCommand::default()
        },
    ))
}

#[test]
fn a_spec_declaring_both_sections_runs_headless_when_nothing_asks_for_acp() {
    // When
    let transport = the_transport_of("acme", BOTH_SECTIONS, None);

    // Then
    assert_eq!(transport, Ok(SpecTransport::Headless));
}

#[test]
fn a_spec_declaring_both_sections_runs_acp_when_the_session_asks_for_it() {
    // When
    let transport = the_transport_of("acme", BOTH_SECTIONS, Some(SpecTransport::Acp));

    // Then
    assert_eq!(transport, Ok(SpecTransport::Acp));
}

#[test]
fn a_spec_declaring_only_acp_runs_acp_when_nothing_is_asked() {
    // When
    let transport = the_transport_of("claude-acp", ONLY_ACP, None);

    // Then
    assert_eq!(transport, Ok(SpecTransport::Acp));
}

#[test]
fn a_spec_declaring_only_headless_runs_headless_when_nothing_is_asked() {
    // When
    let transport = the_transport_of("oz", ONLY_HEADLESS, None);

    // Then
    assert_eq!(transport, Ok(SpecTransport::Headless));
}

#[test]
fn a_spec_declaring_only_headless_refuses_an_acp_request_naming_the_agent() {
    // When
    let refusal = the_transport_of("oz", ONLY_HEADLESS, Some(SpecTransport::Acp))
        .expect_err("oz declares no acp section");

    // Then
    assert_eq!(
        (refusal.clone(), refusal.to_string()),
        (
            SpecBackendRefusal::TransportNotDeclared {
                agent: "oz".to_string(),
                asked: SpecTransport::Acp,
            },
            "agent `oz` was asked to run over acp, which its spec does not declare".to_string()
        )
    );
}

#[test]
fn a_spec_declaring_neither_section_is_refused_naming_the_agent() {
    // When
    let refusal = the_transport_of("muse", NO_TURN_SECTION, None).expect_err("muse is PTY-only");

    // Then
    assert_eq!(
        (refusal.clone(), refusal.to_string()),
        (
            SpecBackendRefusal::NoTurnTransport {
                agent: "muse".to_string()
            },
            "agent `muse` declares no `coding_agent.headless` or `coding_agent.acp` section, so \
             it cannot run a workflow turn"
                .to_string()
        )
    );
}

#[test]
fn an_acp_spec_backend_is_named_by_its_spec_id() {
    // Given
    let backend = an_acp_backend_for("acme-acp");

    // When
    let name = backend.name().to_string();

    // Then
    assert_eq!(name, "acme-acp");
}

#[test]
fn an_acp_spec_backend_reports_its_transport() {
    // Given
    let backend = an_acp_backend_for("acme-acp");

    // When
    let transport = backend.transport();

    // Then
    assert_eq!(transport, SpecTransport::Acp);
}

#[test]
fn the_shared_stub_backend_is_named_stub() {
    // When
    let backend = SharedBackend::stub();

    // Then
    assert_eq!(backend.name(), "stub");
}
