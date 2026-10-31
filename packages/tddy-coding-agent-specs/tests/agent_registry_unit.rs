//! The coding-agent registry (`#agent-tools 12/12`): one enumeration of the agents a host offers,
//! each with the capabilities and route support its spec declares.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-coding-agent-registry.md`.
//!
//! Each registry here is loaded from a `<tddyhome>/tools` of the test's own — empty, so the
//! built-ins are what loads, unless a test drops a file in.

use std::collections::BTreeSet;
use std::path::Path;

use pretty_assertions::assert_eq;
use tddy_actions::tool::RunSection;
use tddy_coding_agent_specs::{
    builtin_specs, Capability, CodingAgentRegistry, CodingAgentSection, CodingAgentSpec,
    ProviderKind, RouteShape, RouteSupport,
};

/// A `<tddyhome>` whose `tools/` holds `files` (`(name, YAML)`), and the registry loaded over it.
fn a_registry_with_tools(files: &[(&str, &str)]) -> (tempfile::TempDir, CodingAgentRegistry) {
    let home = tempfile::tempdir().unwrap();
    let tools = home.path().join("tools");
    std::fs::create_dir_all(&tools).unwrap();
    for (name, yaml) in files {
        std::fs::write(tools.join(name), yaml).unwrap();
    }
    let registry = CodingAgentRegistry::load(&tools).expect("the registry loads");
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

/// The coding-agent view of `yaml`, read straight from its `run` and `coding_agent` sections.
fn the_spec_in(id: &str, yaml: &str) -> CodingAgentSpec {
    let file: serde_yaml::Value = serde_yaml::from_str(yaml).expect("the spec is YAML");
    let run: RunSection = serde_yaml::from_value(file["run"].clone()).expect("its run section");
    let agent: CodingAgentSection =
        serde_yaml::from_value(file["extensions"]["coding_agent"].clone())
            .expect("its coding_agent section");
    CodingAgentSpec {
        id: id.to_string(),
        run,
        agent,
    }
}

fn capabilities(of: &[Capability]) -> BTreeSet<Capability> {
    of.iter().copied().collect()
}

#[test]
fn the_registry_lists_every_builtin_agent_by_id() {
    // Given
    let (_home, registry) = a_registry_with_tools(&[]);

    // When
    let ids: Vec<&str> = registry.all().iter().map(|spec| spec.id.as_str()).collect();

    // Then
    assert_eq!(
        ids,
        vec![
            "claude",
            "claude-acp",
            "codex",
            "codex-acp",
            "cursor",
            "muse",
            "opencode",
            "oz",
            "warp",
        ]
    );
}

#[test]
fn for_session_type_names_the_agent_of_every_builtin_cli_session_type() {
    // Given
    let (_home, registry) = a_registry_with_tools(&[]);

    // When
    let resolved: Vec<Option<&str>> = [
        "claude-cli",
        "cursor-cli",
        "opencode-cli",
        "muse-cli",
        "warp-cli",
    ]
    .into_iter()
    .map(|session_type| {
        registry
            .for_session_type(session_type)
            .map(|spec| spec.id.as_str())
    })
    .collect();

    // Then
    assert_eq!(
        resolved,
        vec![
            Some("claude"),
            Some("cursor"),
            Some("opencode"),
            Some("muse"),
            Some("warp"),
        ]
    );
}

#[test]
fn a_session_type_no_pty_agent_stands_for_resolves_to_none() {
    // Given
    let (_home, registry) = a_registry_with_tools(&[]);

    // When
    let resolved = (
        registry.for_session_type("tool").map(|s| s.id.clone()),
        registry.for_session_type("workspace").map(|s| s.id.clone()),
        registry.for_session_type("opencode").map(|s| s.id.clone()),
        registry
            .for_session_type("claude-acp-cli")
            .map(|s| s.id.clone()),
        registry
            .for_session_type("opencode-cli")
            .map(|s| s.id.clone()),
    );

    // Then — a bare agent id is not a session type, and `claude-acp` runs only over ACP
    assert_eq!(
        resolved,
        (None, None, None, None, Some("opencode".to_string()))
    );
}

#[test]
fn every_builtin_agent_declares_the_capabilities_its_old_gates_granted() {
    // Given
    let (_home, registry) = a_registry_with_tools(&[]);

    // When
    let declared: Vec<(&str, BTreeSet<Capability>)> = registry
        .all()
        .into_iter()
        .map(|spec| (spec.id.as_str(), spec.capabilities().clone()))
        .collect();

    // Then — claude-cli held every gate, cursor-cli the hook, model, notify and worktree ones
    assert_eq!(
        declared,
        vec![
            (
                "claude",
                capabilities(&[
                    Capability::Hooks,
                    Capability::AskUserQuestion,
                    Capability::SplitHost,
                    Capability::ProjectHomeMain,
                    Capability::RequiresModel,
                    Capability::BranchNameRequired,
                    Capability::OperatorNotify,
                ])
            ),
            ("claude-acp", capabilities(&[])),
            ("codex", capabilities(&[])),
            ("codex-acp", capabilities(&[])),
            (
                "cursor",
                capabilities(&[
                    Capability::Hooks,
                    Capability::RequiresModel,
                    Capability::KeepWorktreeOnDelete,
                    Capability::OperatorNotify,
                ])
            ),
            ("muse", capabilities(&[])),
            ("opencode", capabilities(&[])),
            ("oz", capabilities(&[])),
            ("warp", capabilities(&[])),
        ]
    );
}

#[test]
fn has_answers_from_the_declared_capabilities() {
    // Given
    let cursor = the_spec_in("cursor", the_builtin_yaml("cursor"));

    // When
    let answers = (
        cursor.has(Capability::Hooks),
        cursor.has(Capability::AskUserQuestion),
    );

    // Then
    assert_eq!(answers, (true, false));
}

#[test]
fn an_unknown_capability_is_refused_at_load_naming_it() {
    // Given — an override of claude.yaml that declares a word no code reads
    let claude = the_builtin_yaml("claude")
        .replace("capabilities: [hooks,", "capabilities: [teleport, hooks,");
    let home = tempfile::tempdir().unwrap();
    let tools = home.path().join("tools");
    std::fs::create_dir_all(&tools).unwrap();
    std::fs::write(tools.join("claude.yaml"), claude).unwrap();

    // When
    let refused = CodingAgentRegistry::load(Path::new(&tools))
        .map(|registry| registry.all().len())
        .expect_err("an unknown capability must not load");

    // Then
    let message = refused.to_string();
    assert!(
        message.contains("claude.yaml") && message.contains("teleport"),
        "the refusal names the file and the word: {message}"
    );
}

#[test]
fn route_support_reads_the_providers_and_shape_of_the_route_section() {
    // Given — opencode.yaml's route, declaring what OpenCode can be routed to
    let opencode = the_builtin_yaml("opencode").replace(
        "      key_env: TDDY_OPENCODE_PROVIDER_KEY\n",
        "      key_env: TDDY_OPENCODE_PROVIDER_KEY\n      providers: [openai_responses, openai_chat]\n      shape: config_file\n",
    );
    let spec = the_spec_in("opencode", &opencode);

    // When
    let support = spec.route_support();

    // Then
    assert_eq!(
        support,
        RouteSupport {
            providers: vec![ProviderKind::OpenaiResponses, ProviderKind::OpenaiChat],
            shape: RouteShape::ConfigFile,
        }
    );
}

#[test]
fn an_agent_without_a_route_section_cannot_be_routed() {
    // Given — cursor.yaml declares no route: Cursor always runs on its own vendor
    let cursor = the_spec_in("cursor", the_builtin_yaml("cursor"));

    // When
    let support = cursor.route_support();

    // Then
    assert_eq!(
        (support.clone(), support.is_routable()),
        (RouteSupport::default(), false)
    );
}
