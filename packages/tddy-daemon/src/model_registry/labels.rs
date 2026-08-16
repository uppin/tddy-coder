//! Capability labels — the vocabulary the Models screen filters and badges on.
//!
//! Providers describe a model with their own capability tokens (Ollama's `/api/show` answers
//! `["completion","tools"]`). This maps those onto the fixed label set `models.proto` documents:
//! `"llm" | "embedding" | "vision" | "tools" | "reranker" | "unknown"`.

/// The label for a model whose capabilities could not be determined. `models.proto` gives this its
/// own value precisely so "we could not tell" is never expressed as an empty label list, which
/// would read as "this model has no capabilities".
pub const UNDETERMINABLE_LABEL: &str = "unknown";

/// Derive a model's labels from the capability tokens its provider reported.
///
/// Labels come back in the order the provider listed the capabilities, deduplicated. A model whose
/// capabilities are empty — or contain nothing this vocabulary recognises — is labelled
/// `"unknown"`. That is deliberately *not* a guessed `"llm"`: "we could not tell" and "it is a
/// chat model" are different answers, and only one of them is true.
pub fn capabilities_to_labels(capabilities: &[String]) -> Vec<String> {
    let mut labels: Vec<String> = Vec::new();
    for capability in capabilities {
        let Some(label) = label_for(capability) else {
            continue;
        };
        if !labels.iter().any(|existing| existing == label) {
            labels.push(label.to_string());
        }
    }
    if labels.is_empty() {
        return vec![UNDETERMINABLE_LABEL.to_string()];
    }
    labels
}

/// Derive a model's labels from the per-model capability flags an OpenAI-compatible `/v1/models`
/// listing reported for it.
///
/// Each argument is `None` when the provider said nothing about that capability — which is the
/// usual case: only some OpenAI-compatible providers (Fireworks) report per-model flags, while
/// OpenAI's own listing carries no capability information at all. A model no flag was reported
/// *true* for is `"unknown"`, exactly as [`capabilities_to_labels`] treats an unrecognised token:
/// "we could not tell" is not "it is a chat model".
///
/// `supports_chat: Some(false)` is an answer, not silence — the model is still `"unknown"` rather
/// than labelled, because nothing here says what it *is*, only what it is not.
pub fn reported_capabilities_to_labels(
    supports_chat: Option<bool>,
    supports_tools: Option<bool>,
    supports_image_input: Option<bool>,
) -> Vec<String> {
    let reported = [
        (supports_chat, "completion"),
        (supports_tools, "tools"),
        (supports_image_input, "vision"),
    ]
    .into_iter()
    .filter(|(supported, _)| *supported == Some(true))
    .map(|(_, capability)| capability.to_string())
    .collect::<Vec<_>>();
    capabilities_to_labels(&reported)
}

/// The label one provider capability token maps to, or `None` when the token is outside the
/// vocabulary (a newer provider capability we have no badge for).
fn label_for(capability: &str) -> Option<&'static str> {
    match capability {
        "completion" | "chat" | "llm" => Some("llm"),
        "embedding" | "embed" => Some("embedding"),
        "vision" => Some("vision"),
        "tools" => Some("tools"),
        "rerank" | "reranker" => Some("reranker"),
        _ => None,
    }
}
