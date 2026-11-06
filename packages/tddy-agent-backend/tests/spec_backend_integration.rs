//! `SpecBackend` — the workflow backend built from a coding agent's spec — over its ACP transport:
//! an agent that never answers is refused at its session-open deadline instead of being waited on
//! forever, and an agent whose spec has no `route` section refuses a provider route
//! (`#agent-tools 16/19`).
//!
//! Changeset: `docs/dev/1-WIP/2026-11-04-agent-tools-spec-backend.md`.
//!
//! The ACP agent is "dropped in": nothing in Rust names it. It is a shell script that reads its
//! stdin and never writes a byte back — the silent peer a mis-declared `coding_agent.acp.command`
//! starts.

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_agent_backend::provider_route::by_spec::RouteRefusal;
use tddy_agent_backend::provider_route::{HarnessRoute, RouteSupport, RoutedAgent};
use tddy_agent_backend::{
    AcpAgentCommand, CodingBackend, InvokeRequest, SpecAcpBackend, SpecBackend,
};

/// The deadline the silent agent is given — short, so the refusal is observed in a second.
const SESSION_OPEN_DEADLINE: Duration = Duration::from_secs(1);

/// How long the test waits for the refusal before it calls the backend hung. Well past the
/// deadline, so a correct refusal is never mistaken for a hang.
const HANG_GUARD: Duration = Duration::from_secs(15);

/// A dropped-in ACP agent: its id, its product, and the script it runs as.
struct ADroppedInAcpAgent {
    dir: tempfile::TempDir,
    agent: RoutedAgent,
}

/// `acme-acp`, "Acme Code", whose spec declares no `coding_agent.route` section.
fn a_dropped_in_acp_agent() -> ADroppedInAcpAgent {
    ADroppedInAcpAgent {
        dir: tempfile::tempdir().expect("a directory for the agent"),
        agent: RoutedAgent {
            id: "acme-acp".to_string(),
            product: "Acme Code".to_string(),
            support: RouteSupport::default(),
            key_env: None,
        },
    }
}

impl ADroppedInAcpAgent {
    /// The agent reads every request and answers none — not even `initialize`.
    fn that_never_answers(&self) -> AcpAgentCommand {
        let script = self.dir.path().join("acme-acp");
        std::fs::write(&script, "#!/bin/sh\ncat > /dev/null\n").expect("write the agent");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
            .expect("make the agent executable");
        AcpAgentCommand {
            program: script,
            args: Vec::new(),
            env: Vec::new(),
            cwd: None,
        }
    }

    fn working_dir(&self) -> PathBuf {
        self.dir.path().to_path_buf()
    }

    fn backend(&self, command: AcpAgentCommand) -> SpecBackend {
        SpecBackend::Acp(SpecAcpBackend::new(self.agent.clone(), command))
    }

    fn backend_with_deadline(&self, command: AcpAgentCommand, deadline: Duration) -> SpecBackend {
        SpecBackend::Acp(
            SpecAcpBackend::new(self.agent.clone(), command).with_session_open_deadline(deadline),
        )
    }
}

/// A route onto an Anthropic-compatible account — any route will do: the agent takes none.
fn an_anthropic_account_route() -> HarnessRoute {
    HarnessRoute {
        env: vec![(
            "ANTHROPIC_BASE_URL".to_string(),
            "https://llm.example.test".to_string(),
        )],
        ..HarnessRoute::default()
    }
}

#[tokio::test]
async fn a_dropped_in_acp_agent_that_never_answers_session_new_is_refused_at_its_deadline_naming_the_agent(
) {
    // Given
    let agent = a_dropped_in_acp_agent();
    let backend = agent.backend_with_deadline(agent.that_never_answers(), SESSION_OPEN_DEADLINE);

    // When
    let outcome = tokio::time::timeout(
        HANG_GUARD,
        backend.invoke(InvokeRequest {
            prompt: "Plan the feature".to_string(),
            working_dir: Some(agent.working_dir()),
            ..InvokeRequest::default()
        }),
    )
    .await
    .expect("the turn must be refused at its session-open deadline, not hang");

    // Then
    assert_eq!(
        outcome
            .map(|response| response.output)
            .map_err(|e| e.to_string()),
        Err("the ACP agent `acme-acp` did not open a session within \
             1s; it is refused rather than waited on"
            .to_string())
    );
}

#[test]
fn a_spec_backend_without_a_route_section_refuses_a_provider_route() {
    // Given
    let agent = a_dropped_in_acp_agent();
    let backend = agent.backend(agent.that_never_answers());

    // When
    let refusal = backend
        .with_provider_route(an_anthropic_account_route())
        .expect_err("an agent with no route section cannot run on a provider account");

    // Then
    assert_eq!(
        (refusal.clone(), refusal.to_string()),
        (
            RouteRefusal::AgentCannotBeRouted {
                agent: "acme-acp".to_string(),
                product: "Acme Code".to_string(),
            },
            "Acme Code cannot be pointed at another provider; it always runs on its own vendor"
                .to_string()
        )
    );
}
