//! The coding-agent registry (`#agent-tools 12/12`): one enumeration of the agents a host offers,
//! each with the capabilities and route support its spec declares.
//!
//! Feature doc: `docs/ft/daemon/session-agent-roster.md`.
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
                    Capability::ManagedWorkflow,
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
                    Capability::ManagedWorkflow,
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
    // Given — opencode.yaml's route, which declares what OpenCode can be routed to
    let spec = the_spec_in("opencode", the_builtin_yaml("opencode"));

    // When
    let support = spec.route_support();

    // Then
    assert_eq!(
        support,
        RouteSupport {
            providers: vec![ProviderKind::OpenaiResponses, ProviderKind::OpenaiChat],
            shape: RouteShape::InlineConfig,
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

/// The built-in `id`'s YAML with its declared session type replaced by `session_type`.
fn the_builtin_declaring(id: &str, session_type: &str) -> String {
    let yaml = the_builtin_yaml(id);
    let declared = format!("\n    session_type: {id}-cli\n");
    assert!(yaml.contains(&declared), "{id}.yaml declares {id}-cli");
    yaml.replacen(
        &declared,
        &format!("\n    session_type: {session_type}\n"),
        1,
    )
}

#[test]
fn every_builtin_session_type_is_declared_with_its_launch_and_tool() {
    // Given
    let (_home, registry) = a_registry_with_tools(&[]);

    // When
    let declared: Vec<(String, String, String)> = registry
        .session_types()
        .into_iter()
        .map(|declared| {
            (
                declared.session_type.clone(),
                declared.launch.to_string(),
                declared.tool_id.clone(),
            )
        })
        .collect();

    // Then — tddy-coder declares `tool`; no file declares `workspace`
    assert_eq!(
        declared,
        [
            ("claude-cli", "agent_cli", "claude"),
            ("codex-cli", "agent_cli", "codex"),
            ("cursor-cli", "agent_cli", "cursor"),
            ("muse-cli", "agent_cli", "muse"),
            ("opencode-cli", "agent_cli", "opencode"),
            ("oz-cli", "agent_cli", "oz"),
            ("tool", "tddy_coder", "tddy-coder"),
            ("warp-cli", "agent_cli", "warp"),
        ]
        .map(|(a, b, c)| (a.to_string(), b.to_string(), c.to_string()))
        .to_vec()
    );
}

#[test]
fn two_tool_files_declaring_one_session_type_refuse_the_registry_naming_both() {
    // Given — an override of cursor.yaml that declares claude's session type
    let home = tempfile::tempdir().unwrap();
    let tools = home.path().join("tools");
    std::fs::create_dir_all(&tools).unwrap();
    std::fs::write(
        tools.join("cursor.yaml"),
        the_builtin_declaring("cursor", "claude-cli"),
    )
    .unwrap();

    // When
    let refused = CodingAgentRegistry::load(&tools).map(|_| ());

    // Then
    assert_eq!(
        refused.map_err(|error| error.to_string()),
        Err("session type \"claude-cli\" is declared by both claude and cursor".to_string())
    );
}

#[test]
fn a_tool_file_declaring_the_no_agent_session_type_is_refused() {
    // Given
    let home = tempfile::tempdir().unwrap();
    let tools = home.path().join("tools");
    std::fs::create_dir_all(&tools).unwrap();
    std::fs::write(
        tools.join("cursor.yaml"),
        the_builtin_declaring("cursor", "workspace"),
    )
    .unwrap();

    // When
    let refused = CodingAgentRegistry::load(&tools).map(|_| ());

    // Then
    let message = refused.map_err(|error| error.to_string()).unwrap_err();
    assert!(
        message.contains("\"workspace\" is reserved for a session with no agent"),
        "{message}"
    );
}

#[test]
fn an_undeclared_session_type_is_refused_naming_it_and_every_declared_one() {
    // Given
    let (_home, registry) = a_registry_with_tools(&[]);

    // When
    let message = registry.undeclared_session_type_message("claude");

    // Then
    assert_eq!(
        message,
        "session_type \"claude\" is not a session type this host declares; it offers claude-cli, \
         codex-cli, cursor-cli, muse-cli, opencode-cli, oz-cli, tool, warp-cli, workspace"
    );
}

/// The built-in `id`'s YAML with its `extensions.session` section struck out.
fn the_builtin_without_its_session_section(id: &str) -> String {
    let yaml = the_builtin_yaml(id);
    let start = yaml
        .find("  # The session type a session of this agent")
        .unwrap_or_else(|| panic!("{id}.yaml documents its session section"));
    let end = yaml[start..]
        .find("  coding_agent:\n")
        .map(|offset| start + offset)
        .expect("the coding_agent section follows the session section");
    format!("{}{}", &yaml[..start], &yaml[end..])
}

#[test]
fn an_override_of_a_builtin_that_drops_its_session_section_is_refused_naming_the_file_and_the_builtins_values(
) {
    // Given — an operator's cursor.yaml written before session types were declared
    let home = tempfile::tempdir().unwrap();
    let tools = home.path().join("tools");
    std::fs::create_dir_all(&tools).unwrap();
    let override_file = tools.join("cursor.yaml");
    std::fs::write(
        &override_file,
        the_builtin_without_its_session_section("cursor"),
    )
    .unwrap();

    // When
    let refused = CodingAgentRegistry::load(&tools).map(|_| ());

    // Then
    assert_eq!(
        refused.map_err(|error| error.to_string()),
        Err(format!(
            "tool file {}: extension session: this file overrides the built-in cursor, which \
             declares a session type, but declares none itself; add `extensions.session: {{ \
             session_type: cursor-cli, launch: agent_cli, label: Cursor }}` (the built-in's \
             values) or another session type of its own",
            override_file.display()
        ))
    );
}

#[test]
fn a_new_agent_dropped_in_without_a_session_section_loads_and_declares_no_session_type() {
    // Given — cursor.yaml's content under a new id, with no session section
    let yaml = the_builtin_without_its_session_section("cursor").replacen(
        "\nid: cursor\n",
        "\nid: acme\n",
        1,
    );

    // When
    let (_home, registry) = a_registry_with_tools(&[("acme.yaml", &yaml)]);

    // Then
    assert_eq!(
        (
            registry.get("acme").is_some(),
            registry.session_type_of("acme"),
            registry.session_type_of("cursor"),
        ),
        (true, None, Some("cursor-cli"))
    );
}
