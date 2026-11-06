//! Unit: route bodies chosen by an agent's spec (`provider_route::by_spec`, `#agent-tools 13/19`).
//!
//! Feature doc: `docs/ft/daemon/provider-accounts.md`. Every agent here is a [`RoutedAgent`] built
//! in the test — no spec is loaded — so each case shows the (shape, providers) that picks the body.
//! The Anthropic-env and TOML-args bodies are pinned as literals — what the `Harness`-keyed routes
//! they replaced gave a routed Claude Code and Codex launch, byte for byte.

use pretty_assertions::assert_eq;
use rstest::rstest;
use tddy_agent_backend::provider_route::{
    agent_routable, bodies_accept, resolve_route, route_supported, HarnessRoute, ProviderKind,
    RouteEndpoints, RouteFile, RouteRefusal, RouteSecret, RouteShape, RouteSupport, RouteTarget,
    RoutedAgent,
};

const A_KEY: &str = "a-provider-key";
const A_MODEL: &str = "a-model";

/// An agent `id`, named `product`, routed by `shape` to `providers`, its key handed in `key_env`.
fn an_agent(
    id: &str,
    product: &str,
    shape: RouteShape,
    providers: &[ProviderKind],
    key_env: Option<&str>,
) -> RoutedAgent {
    RoutedAgent {
        id: id.to_string(),
        product: product.to_string(),
        support: RouteSupport {
            providers: providers.to_vec(),
            shape,
        },
        key_env: key_env.map(str::to_string),
    }
}

/// Claude Code's spec as a route sees it: Anthropic Messages, through the environment.
fn claude() -> RoutedAgent {
    an_agent(
        "claude",
        "Claude Code",
        RouteShape::Env,
        &[ProviderKind::Anthropic],
        None,
    )
}

/// Codex's spec: OpenAI Responses, through `-c` TOML overrides, its key in `CODEX_PROVIDER_KEY`.
fn codex() -> RoutedAgent {
    an_agent(
        "codex",
        "Codex",
        RouteShape::TomlArgs,
        &[ProviderKind::OpenaiResponses],
        Some("CODEX_PROVIDER_KEY"),
    )
}

/// OpenCode's spec: Responses or Chat Completions, through a JSON configuration file.
fn opencode() -> RoutedAgent {
    an_agent(
        "opencode",
        "OpenCode",
        RouteShape::JsonConfigFile,
        &[ProviderKind::OpenaiResponses, ProviderKind::OpenaiChat],
        Some("OPENCODE_PROVIDER_KEY"),
    )
}

/// Muse Code's spec: Meta only, its key alone in `META_API_KEY`.
fn muse() -> RoutedAgent {
    an_agent(
        "muse",
        "Muse Code",
        RouteShape::Env,
        &[ProviderKind::Meta],
        Some("META_API_KEY"),
    )
}

/// Cursor's spec: no route section, so no providers.
fn cursor() -> RoutedAgent {
    an_agent("cursor", "Cursor", RouteShape::Env, &[], None)
}

/// The Meta account: Anthropic Messages and OpenAI Responses at Meta's fixed roots.
fn a_meta_account() -> RouteTarget {
    RouteTarget {
        preset: "meta".to_string(),
        endpoints: RouteEndpoints {
            anthropic_messages: Some("https://api.meta.ai".to_string()),
            openai_responses: Some("https://api.meta.ai/v1".to_string()),
            openai_chat: None,
        },
        api_key: RouteSecret::new(A_KEY),
        model: A_MODEL.to_string(),
    }
}

/// An OpenAI-compatible account serving Chat Completions only.
fn a_chat_only_account() -> RouteTarget {
    RouteTarget {
        preset: "openai-compatible".to_string(),
        endpoints: RouteEndpoints {
            openai_chat: Some("https://chat.example/v1".to_string()),
            ..RouteEndpoints::default()
        },
        api_key: RouteSecret::new(A_KEY),
        model: A_MODEL.to_string(),
    }
}

/// `route`'s secret environment, exposed — what the agent child is handed.
fn secrets_of(route: &HarnessRoute) -> Vec<(String, String)> {
    route
        .secret_env
        .iter()
        .map(|(name, secret)| (name.clone(), secret.expose().to_string()))
        .collect()
}

#[rstest]
#[case::env(RouteShape::Env, &[ProviderKind::Anthropic, ProviderKind::Meta])]
#[case::toml_args(RouteShape::TomlArgs, &[ProviderKind::OpenaiResponses])]
#[case::json_config_file(
    RouteShape::JsonConfigFile,
    &[ProviderKind::OpenaiResponses, ProviderKind::OpenaiChat]
)]
fn a_shapes_bodies_accept_the_providers_a_body_exists_for(
    #[case] shape: RouteShape,
    #[case] expected: &[ProviderKind],
) {
    // When
    let accepted = bodies_accept(shape);

    // Then
    assert_eq!(accepted, expected);
}

#[rstest]
#[case::anthropic_on_anthropic_messages(claude(), "anthropic-compatible", &["anthropic-messages"], Ok(()))]
#[case::anthropic_on_chat_only(
    claude(),
    "openai-compatible",
    &["openai-chat"],
    Err("Claude Code needs an account whose endpoint serves Anthropic Messages, and this one does not")
)]
#[case::responses_on_responses(codex(), "openai-compatible", &["openai-responses"], Ok(()))]
#[case::responses_on_chat_only(
    codex(),
    "openai-compatible",
    &["openai-chat"],
    Err("Codex needs an account whose endpoint serves OpenAI Responses, and this one does not")
)]
#[case::responses_or_chat_on_chat_only(opencode(), "openai-compatible", &["openai-chat"], Ok(()))]
#[case::responses_or_chat_on_anthropic_only(
    opencode(),
    "anthropic-compatible",
    &["anthropic-messages"],
    Err("OpenCode needs an account whose endpoint serves OpenAI Responses or OpenAI Chat Completions, and this one does not")
)]
#[case::meta_on_meta(muse(), "meta", &["anthropic-messages", "openai-responses"], Ok(()))]
#[case::meta_on_fireworks(
    muse(),
    "fireworks",
    &["anthropic-messages"],
    Err("Muse Code does not accept a fireworks account; it runs on Meta only")
)]
#[case::no_providers(
    cursor(),
    "meta",
    &["anthropic-messages", "openai-responses"],
    Err("Cursor cannot be pointed at another provider; it always runs on its own vendor")
)]
fn route_supported_answers_the_agents_declared_providers_against_the_account(
    #[case] agent: RoutedAgent,
    #[case] preset: &str,
    #[case] protocols: &[&str],
    #[case] expected: Result<(), &str>,
) {
    // When
    let answer = route_supported(&agent, preset, protocols).map_err(|refusal| refusal.to_string());

    // Then — refusals name the product an operator knows the agent by
    assert_eq!(answer, expected.map_err(str::to_string));
}

#[test]
fn an_agent_that_declares_no_provider_is_refused_by_both_entry_points_carrying_its_id() {
    // Given
    let agent = cursor();
    let refused = RouteRefusal::AgentCannotBeRouted {
        agent: "cursor".to_string(),
        product: "Cursor".to_string(),
    };

    // When
    let answers = (
        agent_routable(&agent),
        resolve_route(&agent, &a_meta_account()).map(|_| ()),
    );

    // Then — never defaulted to an Anthropic environment route
    assert_eq!(answers, (Err(refused.clone()), Err(refused)));
}

#[test]
fn a_declared_pair_no_body_serves_is_not_yet_routable_and_names_the_product_and_id() {
    // Given — TOML arguments to an Anthropic endpoint: no body is that
    let agent = an_agent(
        "lark",
        "Lark",
        RouteShape::TomlArgs,
        &[ProviderKind::Anthropic],
        None,
    );

    // When
    let answers = (
        agent_routable(&agent).map_err(|refusal| refusal.to_string()),
        resolve_route(&agent, &a_meta_account())
            .map(|_| ())
            .map_err(|refusal| refusal.to_string()),
    );

    // Then
    let not_yet = "Lark (lark) cannot run on a provider account in this build yet".to_string();
    assert_eq!(answers, (Err(not_yet.clone()), Err(not_yet)));
}

#[test]
fn every_agent_a_body_serves_is_routable() {
    // When
    let answers: Vec<Result<(), RouteRefusal>> = [claude(), codex(), opencode(), muse()]
        .iter()
        .map(agent_routable)
        .collect();

    // Then
    assert_eq!(answers, vec![Ok(()), Ok(()), Ok(()), Ok(())]);
}

#[test]
fn an_env_route_to_meta_hands_the_key_in_the_specs_key_env_and_nothing_else() {
    // When
    let route = resolve_route(&muse(), &a_meta_account()).expect("Muse Code routes on Meta");

    // Then — no endpoint, file or argument: the session keeps its own model
    assert_eq!(
        (
            route.env.clone(),
            secrets_of(&route),
            route.files.clone(),
            route.args.clone()
        ),
        (
            Vec::new(),
            vec![("META_API_KEY".to_string(), A_KEY.to_string())],
            Vec::new(),
            Vec::new(),
        )
    );
}

#[test]
fn an_env_route_to_meta_for_an_agent_without_a_key_env_is_refused_as_not_yet_routable() {
    // Given — a Meta route with nowhere to hand the key
    let agent = an_agent("lark", "Lark", RouteShape::Env, &[ProviderKind::Meta], None);

    // When
    let answer = resolve_route(&agent, &a_meta_account()).map(|_| ());

    // Then
    assert_eq!(
        answer,
        Err(RouteRefusal::NotYetRoutable {
            agent: "lark".to_string(),
            product: "Lark".to_string(),
        })
    );
}

#[test]
fn a_toml_args_route_for_an_agent_without_a_key_env_is_refused_as_not_yet_routable() {
    // Given — a Responses route with no variable to name as the provider's `env_key`
    let agent = an_agent(
        "lark",
        "Lark",
        RouteShape::TomlArgs,
        &[ProviderKind::OpenaiResponses],
        None,
    );

    // When
    let answer = resolve_route(&agent, &a_meta_account()).map(|_| ());

    // Then — no fallback variable: the key is never handed in a name the spec did not declare
    assert_eq!(
        answer,
        Err(RouteRefusal::NotYetRoutable {
            agent: "lark".to_string(),
            product: "Lark".to_string(),
        })
    );
}

#[test]
fn an_anthropic_env_route_pins_the_endpoint_and_every_model_alias_and_hands_the_key_as_a_secret() {
    // When
    let route = resolve_route(&claude(), &a_meta_account()).expect("Claude Code routes");

    // Then — what a routed Claude Code launch has always been given
    let pinned = |name: &str| (name.to_string(), A_MODEL.to_string());
    assert_eq!(
        route,
        HarnessRoute {
            env: vec![
                (
                    "ANTHROPIC_BASE_URL".to_string(),
                    "https://api.meta.ai".to_string()
                ),
                pinned("ANTHROPIC_MODEL"),
                pinned("ANTHROPIC_DEFAULT_OPUS_MODEL"),
                pinned("ANTHROPIC_DEFAULT_SONNET_MODEL"),
                pinned("ANTHROPIC_DEFAULT_HAIKU_MODEL"),
                pinned("CLAUDE_CODE_SUBAGENT_MODEL"),
            ],
            secret_env: vec![("ANTHROPIC_AUTH_TOKEN".to_string(), RouteSecret::new(A_KEY))],
            files: Vec::new(),
            args: vec!["--model".to_string(), A_MODEL.to_string()],
        }
    );
}

#[test]
fn a_toml_args_route_declares_a_responses_provider_through_config_overrides() {
    // When
    let route = resolve_route(&codex(), &a_meta_account()).expect("Codex routes on Meta");

    // Then — what a routed Codex launch has always been given
    let id = "tddy-route";
    let args: Vec<String> = [
        "-c".to_string(),
        format!("model_provider=\"{id}\""),
        "-c".to_string(),
        format!("model_providers.{id}.name=\"Meta\""),
        "-c".to_string(),
        format!("model_providers.{id}.base_url=\"https://api.meta.ai/v1\""),
        "-c".to_string(),
        format!("model_providers.{id}.env_key=\"CODEX_PROVIDER_KEY\""),
        "-c".to_string(),
        format!("model_providers.{id}.wire_api=\"responses\""),
        "-m".to_string(),
        A_MODEL.to_string(),
    ]
    .into();
    assert_eq!(
        (route.env.clone(), secrets_of(&route), route.args.clone()),
        (
            Vec::new(),
            vec![("CODEX_PROVIDER_KEY".to_string(), A_KEY.to_string())],
            args,
        )
    );
}

#[test]
fn a_json_config_file_route_reads_its_key_from_the_specs_key_env() {
    // When
    let route = resolve_route(&opencode(), &a_chat_only_account()).expect("OpenCode routes");

    // Then — Chat Completions on the account's root; the file names `{env:OPENCODE_PROVIDER_KEY}`
    // and holds no secret
    let file = serde_json::json!({
        "model": format!("tddy-route/{A_MODEL}"),
        "provider": {
            "tddy-route": {
                "npm": "@ai-sdk/openai-compatible",
                "options": {
                    "baseURL": "https://chat.example/v1",
                    "apiKey": "{env:OPENCODE_PROVIDER_KEY}",
                },
                "models": { (A_MODEL): {} },
            },
        },
    });
    assert_eq!(
        (secrets_of(&route), route.files.clone(), route.args.clone()),
        (
            vec![("OPENCODE_PROVIDER_KEY".to_string(), A_KEY.to_string())],
            vec![RouteFile {
                name: "opencode.json".to_string(),
                contents: serde_json::to_string_pretty(&file).unwrap(),
            }],
            vec!["--model".to_string(), format!("tddy-route/{A_MODEL}")],
        )
    );
}
