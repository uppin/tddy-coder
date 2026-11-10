//! Acceptance: what each built-in spec declares in `coding_agent.route` is what a route body serves,
//! the shape words a spec writes name the body format they are, and a route refusal names the
//! product a spec declares (`#agent-tools 13/19`).
//!
//! Feature doc: `docs/ft/daemon/provider-accounts.md`. A route body is chosen by the spec's
//! (shape, provider served), never by the agent's name, so a spec that declares a provider no body
//! serves would offer an account no start could use. These tests read the built-in registry, loaded
//! over a `<tddyhome>/tools` of the test's own, and drive the route bodies of
//! `tddy_agent_backend::provider_route` with one account per provider.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_agent_backend::provider_route::{agent_routable, resolve_route};
use tddy_agent_backend::provider_route::{
    bodies_accept, HarnessRoute, RouteEndpoints, RouteSecret, RouteTarget,
};
use tddy_coding_agent_specs::{
    builtin_specs, routed_agent, CodingAgentRegistry, CodingAgentSpec, ProviderKind,
};

/// A `<tddyhome>/tools` holding `files` (`(name, YAML)`).
struct ATddyHomeTools {
    _home: tempfile::TempDir,
    tools: PathBuf,
}

fn a_tddyhome_tools_holding(files: &[(&str, &str)]) -> ATddyHomeTools {
    let home = tempfile::tempdir().unwrap();
    let tools = home.path().join("tools");
    std::fs::create_dir_all(&tools).unwrap();
    for (name, yaml) in files {
        std::fs::write(tools.join(name), yaml).unwrap();
    }
    ATddyHomeTools { _home: home, tools }
}

/// The registry a host with nothing in its `<tddyhome>/tools` loads: the built-ins.
fn the_builtin_registry() -> (ATddyHomeTools, CodingAgentRegistry) {
    let home = a_tddyhome_tools_holding(&[]);
    let registry = CodingAgentRegistry::load(&home.tools).expect("the built-ins load");
    (home, registry)
}

/// The built-in `id`'s embedded YAML.
fn the_builtin_yaml(id: &str) -> &'static str {
    let file_name = format!("/{id}.yaml");
    builtin_specs()
        .into_iter()
        .find(|(path, _)| path.ends_with(&file_name))
        .map(|(_, yaml)| yaml)
        .unwrap_or_else(|| panic!("{id}.yaml is built in"))
}

/// An account whose endpoint serves exactly `provider`, running `model`.
fn an_account_serving(provider: ProviderKind) -> RouteTarget {
    let root = |url: &str| Some(url.to_string());
    let (preset, endpoints) = match provider {
        ProviderKind::Anthropic => (
            "anthropic-compatible",
            RouteEndpoints {
                anthropic_messages: root("https://anthropic.example"),
                ..RouteEndpoints::default()
            },
        ),
        ProviderKind::OpenaiResponses => (
            "openai-compatible",
            RouteEndpoints {
                openai_responses: root("https://responses.example/v1"),
                ..RouteEndpoints::default()
            },
        ),
        ProviderKind::OpenaiChat => (
            "openai-compatible",
            RouteEndpoints {
                openai_chat: root("https://chat.example/v1"),
                ..RouteEndpoints::default()
            },
        ),
        ProviderKind::Meta => (
            "meta",
            RouteEndpoints {
                anthropic_messages: root("https://api.meta.ai"),
                openai_responses: root("https://api.meta.ai/v1"),
                openai_chat: None,
            },
        ),
    };
    RouteTarget {
        preset: preset.to_string(),
        endpoints,
        api_key: RouteSecret::new("a-provider-key"),
        model: "a-model".to_string(),
    }
}

/// The providers of `spec`'s `route.providers` that its shape's body accepts and resolves a route
/// on, in the order the spec declares them.
fn the_providers_its_body_serves(spec: &CodingAgentSpec) -> Vec<ProviderKind> {
    let agent = routed_agent(spec);
    agent
        .support
        .providers
        .iter()
        .copied()
        .filter(|provider| bodies_accept(agent.support.shape).contains(provider))
        .filter(|provider| resolve_route(&agent, &an_account_serving(*provider)).is_ok())
        .collect()
}

#[test]
fn every_built_in_specs_route_providers_are_ones_its_route_body_accepts_and_resolves() {
    // Given
    let (_home, registry) = the_builtin_registry();

    // When — for every built-in that declares a route, the declared providers its body serves
    let served: BTreeMap<String, Vec<ProviderKind>> = registry
        .all()
        .into_iter()
        .filter(|spec| spec.route_support().is_routable())
        .map(|spec| (spec.id.clone(), the_providers_its_body_serves(spec)))
        .collect();

    // Then — each is every provider its spec declares: no spec offers an account no body serves
    assert_eq!(
        served,
        BTreeMap::from([
            ("claude".to_string(), vec![ProviderKind::Anthropic]),
            ("claude-acp".to_string(), vec![ProviderKind::Anthropic]),
            ("codex".to_string(), vec![ProviderKind::OpenaiResponses]),
            ("codex-acp".to_string(), vec![ProviderKind::OpenaiResponses]),
            ("muse".to_string(), vec![ProviderKind::Meta]),
            (
                "opencode".to_string(),
                vec![ProviderKind::OpenaiResponses, ProviderKind::OpenaiChat]
            ),
        ])
    );
}

/// The load refusal of a `<tddyhome>/tools/<id>.yaml` that is the built-in with `from` replaced by
/// `to`.
fn the_load_refusal_of_an_override_of(id: &str, from: &str, to: &str) -> String {
    let yaml = the_builtin_yaml(id).replace(from, to);
    assert_ne!(yaml, the_builtin_yaml(id), "the override changes {id}.yaml");
    let home = a_tddyhome_tools_holding(&[(&format!("{id}.yaml"), &yaml)]);
    CodingAgentRegistry::load(Path::new(&home.tools))
        .map(|registry| registry.all().len())
        .expect_err("an override using a removed shape word must not load")
        .to_string()
}

#[test]
fn an_override_using_a_removed_route_shape_word_is_refused_at_load_naming_the_new_word() {
    // Given — overrides of codex.yaml and opencode.yaml that still write the shape words the route
    // bodies were renamed from
    // When
    let refusals = (
        the_load_refusal_of_an_override_of("codex", "shape: toml_args", "shape: args"),
        the_load_refusal_of_an_override_of(
            "opencode",
            "shape: inline_config",
            "shape: config_file",
        ),
    );

    // Then — each names the file and the word that replaced the one it wrote
    let (codex, opencode) = refusals;
    assert_eq!(
        (
            codex.contains("codex.yaml")
                && codex.contains("route.shape `args` was renamed to `toml_args`"),
            opencode.contains("opencode.yaml")
                && opencode.contains("route.shape `config_file` was renamed to `json_config_file`"),
        ),
        (true, true),
        "codex: {codex}\nopencode: {opencode}"
    );
}

#[test]
fn every_built_in_spec_declares_the_product_its_route_refusals_name() {
    // Given
    let (_home, registry) = the_builtin_registry();

    // When
    let products: BTreeMap<String, Option<String>> = registry
        .all()
        .into_iter()
        .map(|spec| (spec.id.clone(), spec.agent.product.clone()))
        .collect();

    // Then — `coding_agent.product`, the operator-facing name, is declared by every built-in
    let declared = |product: &str| Some(product.to_string());
    assert_eq!(
        products,
        BTreeMap::from([
            ("claude".to_string(), declared("Claude Code")),
            ("claude-acp".to_string(), declared("Claude Code")),
            ("codex".to_string(), declared("Codex")),
            ("codex-acp".to_string(), declared("Codex")),
            ("cursor".to_string(), declared("Cursor")),
            ("muse".to_string(), declared("Muse Code")),
            ("opencode".to_string(), declared("OpenCode")),
            ("oz".to_string(), declared("Oz")),
            ("warp".to_string(), declared("Warp")),
        ])
    );
}

#[test]
fn an_agent_without_a_route_is_refused_naming_the_product_its_spec_declares() {
    // Given — cursor.yaml declares `product: Cursor` and no route section
    let (_home, registry) = the_builtin_registry();
    let cursor = routed_agent(registry.get("cursor").expect("cursor is built in"));

    // When
    let refusal = agent_routable(&cursor).map_err(|refusal| refusal.to_string());

    // Then
    assert_eq!(
        refusal,
        Err(
            "Cursor cannot be pointed at another provider; it always runs on its own vendor"
                .to_string()
        )
    );
}

#[test]
fn every_id_a_route_was_once_chosen_by_resolves_through_the_registry_to_its_spec() {
    // Given — the eleven ids the closed `Harness` table named: tool-session agents and CLI session
    // types
    let (_home, registry) = the_builtin_registry();

    // When — an agent id is `get`, a session type is `for_session_type`
    let resolved: Vec<(String, Option<String>)> = [
        "claude",
        "claude-acp",
        "claude-cli",
        "codex",
        "codex-acp",
        "opencode",
        "opencode-cli",
        "muse-cli",
        "warp-cli",
        "cursor",
        "cursor-cli",
    ]
    .into_iter()
    .map(|id| {
        let spec = registry.get(id).or_else(|| registry.for_session_type(id));
        (id.to_string(), spec.map(|spec| spec.id.clone()))
    })
    .collect();

    // Then — a session type names the agent it runs; an id no spec holds is `None`, never claude
    let expected = [
        ("claude", "claude"),
        ("claude-acp", "claude-acp"),
        ("claude-cli", "claude"),
        ("codex", "codex"),
        ("codex-acp", "codex-acp"),
        ("opencode", "opencode"),
        ("opencode-cli", "opencode"),
        ("muse-cli", "muse"),
        ("warp-cli", "warp"),
        ("cursor", "cursor"),
        ("cursor-cli", "cursor"),
    ]
    .map(|(id, spec)| (id.to_string(), Some(spec.to_string())));
    assert_eq!(resolved, expected.to_vec());
    assert_eq!(
        (registry.get("stub"), registry.for_session_type("tool")),
        (None, None)
    );
}

/// Every variable name `route` hands the agent process: its plain and secret environment.
fn the_variables_handed_by(route: &HarnessRoute) -> Vec<String> {
    route
        .env
        .iter()
        .map(|(name, _)| name.clone())
        .chain(route.secret_env.iter().map(|(name, _)| name.clone()))
        .collect()
}

#[test]
fn a_routed_codex_hands_its_key_in_codex_provider_key_and_never_in_a_tddy_variable() {
    // Given
    let (_home, registry) = the_builtin_registry();

    for id in ["codex", "codex-acp"] {
        let agent = routed_agent(
            registry
                .get(id)
                .unwrap_or_else(|| panic!("{id} is built in")),
        );

        // When
        let route = resolve_route(&agent, &an_account_serving(ProviderKind::OpenaiResponses))
            .unwrap_or_else(|refusal| panic!("{id}: {refusal}"));

        // Then — the key is in the spec's `route.key_env`, which the provider names as its
        // `env_key`; `TDDY_ROUTE_API_KEY` is not set
        let secrets: Vec<(&str, &str)> = route
            .secret_env
            .iter()
            .map(|(name, value)| (name.as_str(), value.expose()))
            .collect();
        assert_eq!(
            secrets,
            vec![("CODEX_PROVIDER_KEY", "a-provider-key")],
            "{id}"
        );
        assert!(
            route
                .args
                .contains(&"model_providers.tddy-route.env_key=\"CODEX_PROVIDER_KEY\"".to_string()),
            "{id}: {:?}",
            route.args
        );
        assert!(
            !the_variables_handed_by(&route).contains(&"TDDY_ROUTE_API_KEY".to_string()),
            "{id}"
        );
    }
}

#[test]
fn no_built_in_route_hands_the_agent_process_a_tddy_variable() {
    // Given
    let (_home, registry) = the_builtin_registry();

    // When — every route a built-in resolves, on an account for each provider it serves
    let tddy_variables: Vec<(String, String)> = registry
        .all()
        .into_iter()
        .filter(|spec| spec.route_support().is_routable())
        .flat_map(|spec| {
            let agent = routed_agent(spec);
            the_providers_its_body_serves(spec)
                .into_iter()
                .filter_map(move |provider| {
                    resolve_route(&agent, &an_account_serving(provider)).ok()
                })
                .flat_map(|route| {
                    let mut named = the_variables_handed_by(&route);
                    named.extend(route.args.clone());
                    named
                })
                .filter(|named| named.contains("TDDY_"))
                .map(move |named| (spec.id.clone(), named))
                .collect::<Vec<_>>()
        })
        .collect();

    // Then — the agent process sees no `TDDY_*` route variable, by name or in an argument
    assert_eq!(tddy_variables, Vec::<(String, String)>::new());
}
