//! The Codex arm of [`super::resolve_route`]: an account declared as a Responses provider through
//! `-c` config overrides.

use super::{route_supported, served_protocols, Harness, HarnessRoute, RouteRefusal, RouteTarget};

/// Codex reads its providers from `config.toml`, and each override it is given is a `-c` argument.
/// The account is declared as a Responses provider under [`CODEX_ROUTE_PROVIDER_ID`] and selected
/// with `model_provider`, so the person's own providers are untouched. The key is named by the
/// provider's `env_key` and delivered in that variable, never as an argument. `-m` pins the model.
pub(super) fn codex_route(
    harness: Harness,
    target: &RouteTarget,
) -> Result<HarnessRoute, RouteRefusal> {
    route_supported(
        harness,
        &target.preset,
        &served_protocols(&target.endpoints),
    )?;
    let Some(root) = target.endpoints.openai_responses.as_deref() else {
        return Err(RouteRefusal::ProtocolNotServed {
            harness,
            needs: "OpenAI Responses",
        });
    };
    let Some(name) = codex_provider_name(&target.preset) else {
        return Err(RouteRefusal::NotYetRoutable(harness));
    };
    let id = CODEX_ROUTE_PROVIDER_ID;
    Ok(HarnessRoute {
        env: Vec::new(),
        secret_env: vec![(CODEX_ROUTE_KEY_ENV.to_string(), target.api_key.clone())],
        files: Vec::new(),
        args: vec![
            "-c".to_string(),
            format!("model_provider={}", toml_string(CODEX_ROUTE_PROVIDER_ID)),
            "-c".to_string(),
            format!("model_providers.{id}.name={}", toml_string(name)),
            "-c".to_string(),
            format!("model_providers.{id}.base_url={}", toml_string(root)),
            "-c".to_string(),
            format!(
                "model_providers.{id}.env_key={}",
                toml_string(CODEX_ROUTE_KEY_ENV)
            ),
            "-c".to_string(),
            format!("model_providers.{id}.wire_api={}", toml_string("responses")),
            "-m".to_string(),
            target.model.clone(),
        ],
    })
}

/// `value` as a TOML basic string, for a `-c key=value` override. JSON string syntax is a valid
/// TOML basic string for every value a URL or a display name can hold, and it escapes what TOML
/// would otherwise end the string on.
fn toml_string(value: &str) -> String {
    serde_json::to_string(value).expect("a string always serializes as JSON")
}

/// The display name each preset is declared under as a Codex provider (`model_providers.<id>.name`).
/// Exactly the presets that can serve OpenAI Responses — `meta` and `openai-compatible` in
/// `tddy_accounts::presets()` — with those labels. A preset missing here is refused by
/// [`resolve_route`], never named by a guess.
const CODEX_ROUTE_PRESET_NAMES: &[(&str, &str)] =
    &[("meta", "Meta"), ("openai-compatible", "OpenAI-compatible")];

fn codex_provider_name(preset: &str) -> Option<&'static str> {
    CODEX_ROUTE_PRESET_NAMES
        .iter()
        .find(|(id, _)| *id == preset)
        .map(|(_, name)| *name)
}

/// The provider id a Codex route declares the account under (`-c model_providers.<id>.*`), so the
/// person's own `~/.codex/config.toml` providers are never shadowed.
pub const CODEX_ROUTE_PROVIDER_ID: &str = "tddy-route";

/// The variable a Codex route's key is delivered in; the route names it as the provider's `env_key`.
pub const CODEX_ROUTE_KEY_ENV: &str = "TDDY_ROUTE_API_KEY";
