//! Start-time readiness gate for specialized subagents (see
//! docs/ft/coder/1-WIP/PRD-2026-07-12-specialized-agent-warmup-gate.md).
//!
//! Before a sandbox session launches its in-jail agent CLI, [`warm_up_agents`] proactively "wakes"
//! every resolved [`SpecializedAgentDef`] by issuing a minimal chat-completion probe against its
//! endpoint and waits until each answers `200`. This pays an Ollama cold-start cost up front and
//! confirms the model actually responds, instead of letting the main agent's first `subagent_prompt`
//! stall mid-session. Session creation/resume fails hard (a returned [`AgentWarmupError`]) if any
//! agent never becomes ready within the budget — there is no fallback to starting anyway.
//!
//! Backend-agnostic: the `/v1/chat/completions` probe works identically for a local Ollama server
//! and the default SGLang `:30000` endpoint. Ollama's `502` (a *cloud/upstream* reachability failure,
//! not "model unloaded" — a local cold-load is a blocking `200`) is treated as a retryable transient,
//! while a definitive `404` (model not found) fails fast.

use std::time::{Duration, Instant};

use crate::agent_def::SpecializedAgentDef;

/// Log target for the warm-up gate's step output.
const LOG_TARGET: &str = "tddy_discovery::warmup";

/// Cap the transient error body captured for `last_error` so an oversized endpoint response does
/// not bloat the eventual error message.
const MAX_BODY_SNIPPET: usize = 200;

/// Tunable timing for the warm-up gate. Production uses [`WarmupOptions::default`] (120s budget, to
/// match the sandbox-ready timeout); tests inject sub-second values so the suite stays fast.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WarmupOptions {
    /// Total time budget to get a single agent ready before giving up.
    pub timeout: Duration,
    /// Wait between transient-failure retries.
    pub retry_interval: Duration,
    /// Per-probe HTTP timeout (a cold model load must fit within this).
    pub request_timeout: Duration,
}

impl Default for WarmupOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(120),
            retry_interval: Duration::from_secs(1),
            request_timeout: Duration::from_secs(120),
        }
    }
}

/// A single agent's warm-up failure — carries everything needed for an actionable message: which
/// agent, its endpoint, its model, and the last error observed before the budget elapsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentWarmupError {
    pub agent: String,
    pub base_url: String,
    pub model: String,
    pub last_error: String,
}

impl std::fmt::Display for AgentWarmupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "specialized agent '{}' at {} (model {}) did not become ready: {}",
            self.agent, self.base_url, self.model, self.last_error
        )
    }
}

impl std::error::Error for AgentWarmupError {}

/// How the probe's result maps onto the retry policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProbeOutcome {
    /// `2xx` — the endpoint answered; the agent is ready.
    Ready,
    /// Retryable: connection errors, timeouts, `408`/`429`, and `5xx` (incl. `502`/`503`/`504`).
    Transient,
    /// Non-retryable definitive failure (e.g. `400`/`401`/`403`/`404`).
    Fatal,
}

/// Classify an HTTP status code returned by the probe into a [`ProbeOutcome`].
pub(crate) fn classify_probe_status(status: u16) -> ProbeOutcome {
    match status {
        200..=299 => ProbeOutcome::Ready,
        408 | 429 => ProbeOutcome::Transient,
        500..=599 => ProbeOutcome::Transient,
        _ => ProbeOutcome::Fatal,
    }
}

/// Build the minimal one-token chat-completion request body used to wake `model`.
pub(crate) fn build_probe_body(model: &str) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "messages": [{ "role": "user", "content": "ping" }],
        "max_tokens": 1,
        "temperature": 0,
        "stream": false,
    })
}

/// Warm up every def and return `Ok(())` only once **all** are ready. Returns an [`AgentWarmupError`]
/// for the first agent that never becomes ready within `opts.timeout`. An empty `defs` is an
/// immediate `Ok(())` with no HTTP issued.
pub async fn warm_up_agents(
    defs: &[SpecializedAgentDef],
    opts: &WarmupOptions,
) -> Result<(), AgentWarmupError> {
    if defs.is_empty() {
        return Ok(());
    }

    let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
    log::info!(
        target: LOG_TARGET,
        "warming up {} specialized agent(s): {:?}",
        defs.len(),
        names
    );

    for def in defs {
        warm_up_one(def, opts).await?;
    }

    Ok(())
}

/// Warm up a single agent, retrying transient failures until `opts.timeout` elapses. Returns
/// [`AgentWarmupError`] if the endpoint never answers `200` in time, or immediately on a fatal
/// status.
async fn warm_up_one(
    def: &SpecializedAgentDef,
    opts: &WarmupOptions,
) -> Result<(), AgentWarmupError> {
    let base_url = def.base_url.trim_end_matches('/').to_string();
    let url = format!("{base_url}/v1/chat/completions");
    let body = build_probe_body(&def.model);

    log::info!(
        target: LOG_TARGET,
        "waking specialized agent '{}' (model {}) at {} …",
        def.name,
        def.model,
        base_url
    );

    let client = match reqwest::Client::builder()
        .timeout(opts.request_timeout)
        .build()
    {
        Ok(client) => client,
        Err(e) => {
            return Err(AgentWarmupError {
                agent: def.name.clone(),
                base_url: base_url.clone(),
                model: def.model.clone(),
                last_error: format!("failed to build HTTP client: {e}"),
            });
        }
    };

    let started = Instant::now();
    let deadline = started + opts.timeout;
    // Assigned by every non-returning branch of the probe below before it is read.
    let mut last_error: String;

    loop {
        match client.post(&url).json(&body).send().await {
            Ok(response) => {
                let status = response.status();
                match classify_probe_status(status.as_u16()) {
                    ProbeOutcome::Ready => {
                        log::info!(
                            target: LOG_TARGET,
                            "specialized agent '{}' is ready ({:?})",
                            def.name,
                            started.elapsed()
                        );
                        return Ok(());
                    }
                    ProbeOutcome::Fatal => {
                        let snippet = short_body(response.text().await.unwrap_or_default());
                        return Err(AgentWarmupError {
                            agent: def.name.clone(),
                            base_url,
                            model: def.model.clone(),
                            last_error: format!("HTTP {status}: {snippet}"),
                        });
                    }
                    ProbeOutcome::Transient => {
                        let snippet = short_body(response.text().await.unwrap_or_default());
                        last_error = format!("HTTP {status}: {snippet}");
                    }
                }
            }
            Err(e) => {
                last_error = e.to_string();
            }
        }

        let elapsed = started.elapsed();
        if elapsed >= opts.timeout {
            break;
        }
        log::warn!(
            target: LOG_TARGET,
            "specialized agent '{}' not ready yet ({}); retrying in {:?} (elapsed {:?} of {:?})",
            def.name,
            last_error,
            opts.retry_interval,
            elapsed,
            opts.timeout
        );
        tokio::time::sleep(opts.retry_interval).await;
        if Instant::now() >= deadline {
            break;
        }
    }

    Err(AgentWarmupError {
        agent: def.name.clone(),
        base_url,
        model: def.model.clone(),
        last_error,
    })
}

/// Truncate a probe response body to [`MAX_BODY_SNIPPET`] chars so it can be embedded in an error
/// message without dumping a large payload.
fn short_body(body: String) -> String {
    let trimmed = body.trim();
    if trimmed.len() <= MAX_BODY_SNIPPET {
        return trimmed.to_string();
    }
    let cut = trimmed
        .char_indices()
        .take_while(|(i, _)| *i < MAX_BODY_SNIPPET)
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    format!("{}…", &trimmed[..cut])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Connection-level failures, rate limits, and server-side `5xx` (including Ollama's `502`,
    /// which is an upstream/proxy reachability failure, not "model unloaded") are all retryable.
    #[test]
    fn classifies_upstream_and_rate_limit_statuses_as_transient() {
        // Given / When / Then
        assert_eq!(classify_probe_status(502), ProbeOutcome::Transient, "502");
        assert_eq!(classify_probe_status(503), ProbeOutcome::Transient, "503");
        assert_eq!(classify_probe_status(504), ProbeOutcome::Transient, "504");
        assert_eq!(classify_probe_status(500), ProbeOutcome::Transient, "500");
        assert_eq!(classify_probe_status(429), ProbeOutcome::Transient, "429");
    }

    /// Definitive client errors — including `404` (model not found) — are non-retryable: waiting will
    /// not fix them, so warm-up must fail fast rather than burn the whole budget.
    #[test]
    fn classifies_client_error_statuses_as_fatal() {
        // Given / When / Then
        assert_eq!(classify_probe_status(400), ProbeOutcome::Fatal, "400");
        assert_eq!(classify_probe_status(401), ProbeOutcome::Fatal, "401");
        assert_eq!(classify_probe_status(403), ProbeOutcome::Fatal, "403");
        assert_eq!(classify_probe_status(404), ProbeOutcome::Fatal, "404");
    }

    /// A `200` means the endpoint answered — the agent is ready.
    #[test]
    fn classifies_success_as_ready() {
        // Given / When / Then
        assert_eq!(classify_probe_status(200), ProbeOutcome::Ready);
    }

    /// The probe body wakes the def's own model with a single throwaway token and no sampling — the
    /// cheapest request that still forces Ollama to load the model.
    #[test]
    fn builds_a_one_token_probe_body_for_the_given_model() {
        // Given / When
        let body = build_probe_body("hf.co/some/Model-GGUF:Q4_K_M");

        // Then
        assert_eq!(
            body["model"],
            serde_json::json!("hf.co/some/Model-GGUF:Q4_K_M")
        );
        assert_eq!(body["max_tokens"], serde_json::json!(1));
        assert_eq!(body["temperature"], serde_json::json!(0));
        assert_eq!(body["stream"], serde_json::json!(false));
        assert_eq!(body["messages"][0]["role"], serde_json::json!("user"));
    }

    /// The error message names the agent, its endpoint, and its model, so a user can see exactly what
    /// to fix without reading logs.
    #[test]
    fn error_display_names_agent_base_url_and_model() {
        // Given
        let err = AgentWarmupError {
            agent: "fastcontext".to_string(),
            base_url: "http://localhost:11434".to_string(),
            model: "qwen2.5-coder:7b".to_string(),
            last_error: "connection refused".to_string(),
        };

        // When
        let message = err.to_string();

        // Then
        assert!(
            message.contains("fastcontext"),
            "message must name the agent: {message}"
        );
        assert!(
            message.contains("http://localhost:11434"),
            "message must name the base_url: {message}"
        );
        assert!(
            message.contains("qwen2.5-coder:7b"),
            "message must name the model: {message}"
        );
    }
}
