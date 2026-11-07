//! Acceptance: a spec routes an agent's provider configuration inline with `route.shape:
//! inline_config`, naming the variable in `route.config_env`; OpenCode declares it, and a slot that
//! cannot be rendered as declared fails loading, naming the spec (`#agent-tools 29/32`).
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-11-07-opencode-route-inline-from-spec.md`.

use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_actions::tool::ToolError;
use tddy_coding_agent_specs::{
    load_agent_registry, CodingAgentSpec, RouteShape, CODING_AGENT_EXTENSION, OPENCODE_SPEC_YAML,
};

/// OpenCode's built-in route slot, as `opencode.yaml` declares it.
const OPENCODE_ROUTE_SLOT: &str = "      shape: inline_config
      config_env: OPENCODE_CONFIG_CONTENT
";

/// A `<tddyhome>/tools` directory.
struct AToolsDir {
    dir: tempfile::TempDir,
}

fn a_tools_dir() -> AToolsDir {
    AToolsDir {
        dir: tempfile::tempdir().expect("a tools dir"),
    }
}

impl AToolsDir {
    /// An operator override of `opencode.yaml` whose route slot's shape lines are `slot`.
    fn with_opencode_routed_by(self, slot: &str) -> Self {
        assert!(
            OPENCODE_SPEC_YAML.contains(OPENCODE_ROUTE_SLOT),
            "opencode.yaml declares its route inline"
        );
        std::fs::write(
            self.file("opencode.yaml"),
            OPENCODE_SPEC_YAML.replace(OPENCODE_ROUTE_SLOT, slot),
        )
        .expect("a tool file");
        self
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn file(&self, file: &str) -> PathBuf {
        self.dir.path().join(file)
    }
}

/// The rejection `load_agent_registry` answers for `tools`, which must be a `coding_agent` one.
fn the_refusal_of(tools: &AToolsDir) -> (PathBuf, String) {
    let refused = match load_agent_registry(tools.path()) {
        Ok(_) => panic!("the registry loads, but the override should be refused"),
        Err(refused) => refused,
    };
    let ToolError::ExtensionRejected {
        file,
        owner,
        message,
    } = refused
    else {
        panic!("expected a coding_agent refusal, got {refused:?}");
    };
    assert_eq!(owner, CODING_AGENT_EXTENSION);
    (file, message)
}

#[test]
fn opencode_declares_its_route_inline_in_opencode_config_content() {
    // Given
    let tools = a_tools_dir();

    // When
    let registry = load_agent_registry(tools.path()).expect("the built-ins load");
    let opencode =
        CodingAgentSpec::of(registry.get("opencode").expect("opencode is built in")).expect("spec");

    // Then — guard: the shape and the variable; no file directory, no route-file adapter
    let route = opencode.agent.route.as_ref().expect("opencode is routable");
    assert_eq!(
        (
            opencode.route_support().shape,
            route.config_env.as_deref(),
            route.files_dir.as_deref(),
            opencode.acp().and_then(|acp| acp.hooks_adapter.as_deref()),
        ),
        (
            RouteShape::InlineConfig,
            Some("OPENCODE_CONFIG_CONTENT"),
            None,
            None,
        )
    );
}

#[test]
fn an_inline_config_route_without_config_env_fails_loading_naming_the_spec() {
    // Given — an override routing inline with no variable to carry the configuration in
    let tools = a_tools_dir().with_opencode_routed_by("      shape: inline_config\n");

    // When
    let (file, message) = the_refusal_of(&tools);

    // Then
    assert_eq!(file, tools.file("opencode.yaml"));
    assert!(message.contains("config_env"), "{message}");
}

#[test]
fn an_inline_config_route_naming_a_tddy_variable_fails_loading_naming_the_spec() {
    // Given — an override that would hand the configuration over in a daemon variable
    let tools = a_tools_dir().with_opencode_routed_by(
        "      shape: inline_config\n      config_env: TDDY_OPENCODE_CONFIG\n",
    );

    // When
    let (file, message) = the_refusal_of(&tools);

    // Then
    assert_eq!(file, tools.file("opencode.yaml"));
    assert!(message.contains("TDDY_OPENCODE_CONFIG"), "{message}");
}

#[test]
fn an_inline_config_route_with_a_files_dir_fails_loading_naming_the_spec() {
    // Given — an override that declares a directory for files an inline route never writes
    let tools = a_tools_dir().with_opencode_routed_by(
        "      shape: inline_config\n      config_env: OPENCODE_CONFIG_CONTENT\n      files_dir: provider-route\n",
    );

    // When
    let (file, message) = the_refusal_of(&tools);

    // Then
    assert_eq!(file, tools.file("opencode.yaml"));
    assert!(message.contains("files_dir"), "{message}");
}

#[test]
fn a_json_config_file_route_still_loads_with_its_variable_and_directory() {
    // Given — guard: the file shape keeps its variable and directory
    let tools = a_tools_dir().with_opencode_routed_by(
        "      shape: json_config_file\n      config_env: OPENCODE_CONFIG\n      files_dir: provider-route\n",
    );

    // When
    let registry = load_agent_registry(tools.path()).expect("a file route loads");

    // Then
    let opencode = CodingAgentSpec::of(registry.get("opencode").expect("opencode")).expect("spec");
    assert_eq!(opencode.route_support().shape, RouteShape::JsonConfigFile);
}
