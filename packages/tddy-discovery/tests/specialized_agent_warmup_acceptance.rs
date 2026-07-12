//! Acceptance tests for the specialized-agent warm-up gate.
//!
//! Feature: docs/ft/coder/1-WIP/PRD-2026-07-12-specialized-agent-warmup-gate.md
//! Changeset: docs/dev/1-WIP/2026-07-12-specialized-agent-warmup-gate.md
//!
//! The behavior-defining seam is `warm_up_agents` talking to an OpenAI-compatible endpoint, so these
//! drive it against a real `wiremock` server (deterministic, millisecond-fast via injected
//! `WarmupOptions`). One behavior per test, exact assertions, no branching.

use std::time::{Duration, Instant};

use tddy_discovery::agent_def::{SpecializedAgentDef, SubagentTool};
use tddy_discovery::warmup::{warm_up_agents, AgentWarmupError, WarmupOptions};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ─── builders / helpers ──────────────────────────────────────────────────────

/// A specialized agent pointed at `base_url`. Only `name`/`base_url` matter to warm-up; the rest are
/// valid defaults so a bare `a_warmup_agent(..)` is usable.
fn a_warmup_agent(name: &str, base_url: &str) -> SpecializedAgentDef {
    SpecializedAgentDef {
        name: name.to_string(),
        label: None,
        model: format!("{name}-model"),
        base_url: base_url.to_string(),
        system_prompt: None,
        system_prompt_path: None,
        tools: vec![SubagentTool::Read],
        max_turns: 1,
        replaces: Vec::new(),
    }
}

/// Sub-second budget so a "never ready" case fails within the integration-test timeout instead of
/// the 120s production default; the 20ms retry interval keeps transient-retry tests to a few
/// iterations, and the 200ms request timeout bounds a single probe.
fn fast_warmup_options() -> WarmupOptions {
    WarmupOptions {
        timeout: Duration::from_millis(500),
        retry_interval: Duration::from_millis(20),
        request_timeout: Duration::from_millis(200),
    }
}

/// Assert warm-up failed and return the error for further domain assertions.
fn assert_warmup_failed(result: Result<(), AgentWarmupError>) -> AgentWarmupError {
    match result {
        Err(e) => e,
        Ok(()) => panic!("expected warm-up to fail, but it succeeded"),
    }
}

trait WarmupErrorAssertions {
    fn assert_for_agent(&self, name: &str) -> &Self;
    fn assert_mentions(&self, fragment: &str) -> &Self;
}

impl WarmupErrorAssertions for AgentWarmupError {
    fn assert_for_agent(&self, name: &str) -> &Self {
        assert_eq!(self.agent, name, "warm-up error names the wrong agent");
        self
    }

    fn assert_mentions(&self, fragment: &str) -> &Self {
        let message = self.to_string();
        assert!(
            message.contains(fragment),
            "expected error message to contain '{fragment}', was '{message}'"
        );
        self
    }
}

async fn mount_chat_completions(server: &MockServer, template: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(template)
        .mount(server)
        .await;
}

// ─── AC1 ─────────────────────────────────────────────────────────────────────

/// A single agent whose endpoint answers a chat-completion is reported ready, and the probe carries
/// that agent's own model.
#[tokio::test]
async fn reports_an_agent_ready_once_its_endpoint_answers_a_chat_completion() {
    // Given
    let server = MockServer::start().await;
    mount_chat_completions(&server, ResponseTemplate::new(200)).await;
    let agent = a_warmup_agent("fastcontext", &server.uri());

    // When
    let result = warm_up_agents(std::slice::from_ref(&agent), &fast_warmup_options()).await;

    // Then
    assert_eq!(
        result,
        Ok(()),
        "a responsive endpoint must warm up successfully"
    );
    let requests = server
        .received_requests()
        .await
        .expect("wiremock must record requests");
    assert_eq!(requests.len(), 1, "exactly one probe must be issued");
    let body: serde_json::Value = requests[0].body_json().expect("probe body must be JSON");
    assert_eq!(
        body["model"],
        serde_json::json!("fastcontext-model"),
        "the probe must target the agent's own model"
    );
}

// ─── AC2 ─────────────────────────────────────────────────────────────────────

/// A `502` (Ollama's upstream-reachability status) is transient: the probe retries and the agent is
/// ready once the endpoint answers `200`.
#[tokio::test]
async fn retries_a_502_until_the_endpoint_becomes_ready() {
    // Given — first probe 502, every later probe 200
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(502))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    mount_chat_completions(&server, ResponseTemplate::new(200)).await;
    let agent = a_warmup_agent("fastcontext", &server.uri());

    // When
    let result = warm_up_agents(std::slice::from_ref(&agent), &fast_warmup_options()).await;

    // Then
    assert_eq!(
        result,
        Ok(()),
        "a 502-then-200 endpoint must warm up after a retry"
    );
}

// ─── AC3 ─────────────────────────────────────────────────────────────────────

/// A `503` (server up but not ready yet) is transient: the probe retries until the endpoint answers.
#[tokio::test]
async fn retries_a_503_until_the_endpoint_becomes_ready() {
    // Given
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(2)
        .mount(&server)
        .await;
    mount_chat_completions(&server, ResponseTemplate::new(200)).await;
    let agent = a_warmup_agent("fastcontext", &server.uri());

    // When
    let result = warm_up_agents(std::slice::from_ref(&agent), &fast_warmup_options()).await;

    // Then
    assert_eq!(
        result,
        Ok(()),
        "a 503-then-200 endpoint must warm up after retries"
    );
}

// ─── AC4 ─────────────────────────────────────────────────────────────────────

/// An endpoint that never answers `200` fails with an error naming the agent, its endpoint, and its
/// model.
#[tokio::test]
async fn fails_with_an_actionable_error_when_an_agent_never_becomes_ready() {
    // Given — the endpoint is 502 for the whole budget
    let server = MockServer::start().await;
    mount_chat_completions(&server, ResponseTemplate::new(502)).await;
    let base_url = server.uri();
    let agent = a_warmup_agent("fastcontext", &base_url);

    // When
    let result = warm_up_agents(std::slice::from_ref(&agent), &fast_warmup_options()).await;

    // Then
    assert_warmup_failed(result)
        .assert_for_agent("fastcontext")
        .assert_mentions(&base_url)
        .assert_mentions("fastcontext-model");
}

/// An unreachable endpoint (connection refused for the whole budget) fails the same way — the
/// connection-error path is transient-then-fatal-on-deadline, not an immediate hard error.
#[tokio::test]
async fn fails_when_the_endpoint_is_unreachable_for_the_whole_budget() {
    // Given — port 1 has nothing listening, so every probe is refused immediately
    let agent = a_warmup_agent("fastcontext", "http://127.0.0.1:1");

    // When
    let result = warm_up_agents(std::slice::from_ref(&agent), &fast_warmup_options()).await;

    // Then
    assert_warmup_failed(result).assert_for_agent("fastcontext");
}

// ─── AC5 ─────────────────────────────────────────────────────────────────────

/// With several agents, warm-up fails if any one never becomes ready, and the error names the
/// failing agent — not the healthy one.
#[tokio::test]
async fn fails_if_any_one_of_several_agents_never_becomes_ready() {
    // Given — one healthy endpoint, one that is always 502
    let healthy = MockServer::start().await;
    mount_chat_completions(&healthy, ResponseTemplate::new(200)).await;
    let broken = MockServer::start().await;
    mount_chat_completions(&broken, ResponseTemplate::new(502)).await;

    let agents = vec![
        a_warmup_agent("healthy", &healthy.uri()),
        a_warmup_agent("broken", &broken.uri()),
    ];

    // When
    let result = warm_up_agents(&agents, &fast_warmup_options()).await;

    // Then
    assert_warmup_failed(result).assert_for_agent("broken");
}

// ─── AC6 ─────────────────────────────────────────────────────────────────────

/// An empty agent set warms up nothing and succeeds immediately.
#[tokio::test]
async fn warms_up_nothing_for_an_empty_agent_set() {
    // Given
    let no_agents: Vec<SpecializedAgentDef> = Vec::new();

    // When
    let result = warm_up_agents(&no_agents, &fast_warmup_options()).await;

    // Then
    assert_eq!(result, Ok(()), "no agents means nothing to warm up");
}

// ─── AC7 ─────────────────────────────────────────────────────────────────────

/// A definitive `404` (model not found) fails fast — well within the budget — instead of retrying to
/// the deadline.
#[tokio::test]
async fn fails_fast_when_the_model_is_not_found() {
    // Given
    let server = MockServer::start().await;
    mount_chat_completions(&server, ResponseTemplate::new(404)).await;
    let agent = a_warmup_agent("fastcontext", &server.uri());
    let opts = fast_warmup_options();

    // When
    let started = Instant::now();
    let result = warm_up_agents(std::slice::from_ref(&agent), &opts).await;
    let elapsed = started.elapsed();

    // Then
    assert_warmup_failed(result).assert_for_agent("fastcontext");
    assert!(
        elapsed < opts.timeout,
        "a 404 must fail fast (elapsed {elapsed:?}) rather than exhaust the {:?} budget",
        opts.timeout
    );
}
