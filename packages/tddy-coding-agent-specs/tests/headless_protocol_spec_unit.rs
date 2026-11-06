//! Unit: the workflow-turn protocol fields of `coding_agent.headless` (`#agent-tools 15/19`) —
//! what each built-in declares, that an unknown word is refused at load naming the file and the
//! word, and the model listing a renderer hands the backend.
//!
//! Changeset: `docs/dev/changesets/2026-11-05-agent-tools-headless-driver.md`. The argv goldens
//! (`headless_render_golden.rs`, `headless_others_render_golden.rs`) stay unchanged: these fields
//! change no rendered argv.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_actions::tool::ToolHost;
use tddy_agent_backend::adapters::AdapterRegistry;
use tddy_coding_agent_specs::headless::{
    FreshSession, GoalParams, HeadlessProtocol, RouteModel, ScratchDir, SpecHeadlessRenderer,
    SystemPromptDelivery,
};
use tddy_coding_agent_specs::{
    builtin_specs, load_agent_registry, CodingAgentRegistry, CodingAgentSpec,
};

fn params(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect()
}

/// The built-in registry, loaded from an empty `<tddyhome>/tools`.
fn the_builtin_registry() -> (tempfile::TempDir, CodingAgentRegistry) {
    let home = tempfile::tempdir().expect("home");
    let tools = home.path().join("tools");
    std::fs::create_dir_all(&tools).expect("tools");
    let registry = CodingAgentRegistry::load(&tools).expect("the built-ins load");
    (home, registry)
}

/// The protocol `id`'s built-in spec declares.
fn the_builtin_protocol(id: &str) -> HeadlessProtocol {
    let (_home, registry) = the_builtin_registry();
    registry
        .get(id)
        .unwrap_or_else(|| panic!("{id} is built in"))
        .headless()
        .expect("a headless section")
        .protocol()
}

fn claudes_protocol() -> HeadlessProtocol {
    HeadlessProtocol {
        system_prompt: SystemPromptDelivery::Flag,
        goal_params: GoalParams {
            read_only_plan: params(&[("permission_mode", "plan")]),
            accept_edits: params(&[("permission_mode", "acceptEdits")]),
            ..GoalParams::default()
        },
        approval_tool: true,
        workflow_mcp: true,
        route_model: RouteModel::ReplacedIfRouteHas {
            flag: "--model".to_string(),
        },
        fresh_session: FreshSession::Caller,
        scratch: ScratchDir::SessionDir,
    }
}

fn cursors_protocol() -> HeadlessProtocol {
    HeadlessProtocol {
        system_prompt: SystemPromptDelivery::Prepend,
        goal_params: GoalParams {
            always: params(&[("force", "1")]),
            plan_mode: params(&[("plan", "1")]),
            ..GoalParams::default()
        },
        approval_tool: false,
        workflow_mcp: true,
        route_model: RouteModel::Kept,
        fresh_session: FreshSession::Agent,
        scratch: ScratchDir::Workdir,
    }
}

fn codexs_protocol() -> HeadlessProtocol {
    HeadlessProtocol {
        system_prompt: SystemPromptDelivery::Prepend,
        goal_params: GoalParams {
            read_only_plan: params(&[("sandbox_mode", "read-only")]),
            ..GoalParams::default()
        },
        approval_tool: false,
        workflow_mcp: false,
        route_model: RouteModel::Replaced,
        fresh_session: FreshSession::Agent,
        scratch: ScratchDir::SessionDir,
    }
}

fn ozs_protocol() -> HeadlessProtocol {
    HeadlessProtocol {
        system_prompt: SystemPromptDelivery::Prepend,
        goal_params: GoalParams::default(),
        approval_tool: false,
        workflow_mcp: true,
        route_model: RouteModel::Kept,
        fresh_session: FreshSession::Agent,
        scratch: ScratchDir::SessionDir,
    }
}

#[test]
fn claude_declares_the_protocol_its_bespoke_backend_ran_with() {
    // When
    let protocol = the_builtin_protocol("claude");

    // Then
    assert_eq!(protocol, claudes_protocol());
}

#[test]
fn cursor_declares_the_protocol_its_bespoke_backend_ran_with() {
    // When
    let protocol = the_builtin_protocol("cursor");

    // Then
    assert_eq!(protocol, cursors_protocol());
}

#[test]
fn codex_declares_the_protocol_its_bespoke_backend_ran_with() {
    // When
    let protocol = the_builtin_protocol("codex");

    // Then
    assert_eq!(protocol, codexs_protocol());
}

#[test]
fn oz_declares_the_protocol_its_headless_conversations_run_with() {
    // When
    let protocol = the_builtin_protocol("oz");

    // Then
    assert_eq!(protocol, ozs_protocol());
}

#[test]
fn a_headless_section_declaring_no_protocol_field_runs_with_the_documented_defaults() {
    // Given — a YAML-only agent whose headless section names only its argv and adapter
    let yaml = a_lark_yaml_with_headless("");

    // When
    let protocol =
        loading_lark_as(&yaml).map(|spec| spec.headless().expect("a headless section").protocol());

    // Then
    assert_eq!(
        protocol,
        Ok(HeadlessProtocol {
            system_prompt: SystemPromptDelivery::Prepend,
            goal_params: GoalParams::default(),
            approval_tool: false,
            workflow_mcp: false,
            route_model: RouteModel::Kept,
            fresh_session: FreshSession::Agent,
            scratch: ScratchDir::SessionDir,
        })
    );
}

/// `lark.yaml`: a headless agent no Rust names, its headless section carrying `extra` lines
/// (indented six spaces).
fn a_lark_yaml_with_headless(extra: &str) -> String {
    format!(
        r#"schema_version: 1
id: lark
summary: Lark Code — a headless agent declared only in YAML
binary:
  command: lark
  env: TDDY_LARK_BINARY
deps: [tddy-tools]
reads: []
run:
  transport: pty
  argv: ["{{binary}}"]
  env: {{}}
extensions:
  coding_agent:
    mcp: {{ kind: file, flag: --mcp-config, format: claude-mcp-json }}
    tool_allow: unsupported
    tool_deny: unsupported
    permission_prompt: unsupported
    instructions:
      files: [AGENTS.md]
    hooks: {{ adapter: claude-settings-local }}
    models:
      default: lark-1
      curated: [lark-1]
    headless:
      argv: ["{{binary}}", -p, "{{prompt}}", {{ splice: slots }}]
      adapter: claude-stream-json
{extra}"#
    )
}

/// `<tddyhome>/tools/lark.yaml` as `yaml`, loaded: the `lark` spec, or the load's error text.
fn loading_lark_as(yaml: &str) -> Result<CodingAgentSpec, String> {
    let home = tempfile::tempdir().expect("home");
    let tools = home.path().join("tools");
    std::fs::create_dir_all(&tools).expect("tools");
    std::fs::write(tools.join("lark.yaml"), yaml).expect("lark.yaml");
    let registry = CodingAgentRegistry::load(Path::new(&tools)).map_err(|e| e.to_string())?;
    registry
        .get("lark")
        .cloned()
        .ok_or_else(|| "lark is not loaded".to_string())
}

/// Loading `lark.yaml` with `line` in its headless section is refused, naming the file and `word`.
fn assert_refused_naming_the_file_and(line: &str, word: &str) {
    // When
    let refused = loading_lark_as(&a_lark_yaml_with_headless(line))
        .map(|spec| spec.id)
        .expect_err("must not load");

    // Then
    assert_eq!(
        (refused.contains("lark.yaml"), refused.contains(word)),
        (true, true),
        "the refusal names the file and the word: {refused}"
    );
}

#[test]
fn an_unknown_system_prompt_delivery_is_refused_at_load_naming_the_file_and_the_word() {
    assert_refused_naming_the_file_and("      system_prompt: telepathy\n", "telepathy");
}

#[test]
fn an_unknown_route_model_is_refused_at_load_naming_the_file_and_the_word() {
    assert_refused_naming_the_file_and("      route_model: { kind: sometimes }\n", "sometimes");
}

#[test]
fn an_unknown_fresh_session_minter_is_refused_at_load_naming_the_file_and_the_word() {
    assert_refused_naming_the_file_and("      fresh_session: nobody\n", "nobody");
}

#[test]
fn an_unknown_scratch_dir_is_refused_at_load_naming_the_file_and_the_word() {
    assert_refused_naming_the_file_and("      scratch: attic\n", "attic");
}

#[test]
fn an_unknown_goal_param_hint_is_refused_at_load_naming_the_file_and_the_word() {
    assert_refused_naming_the_file_and(
        "      goal_params: { when_tired: { nap: \"1\" } }\n",
        "when_tired",
    );
}

// --- The model listing a renderer hands the backend ----------------------------------------------

/// A host whose env names a stub `agent` and `tddy-tools`; nothing of the real machine resolves.
struct AFixtureHost {
    env: BTreeMap<String, String>,
}

impl ToolHost for AFixtureHost {
    fn env_var(&self, name: &str) -> Option<String> {
        self.env.get(name).cloned()
    }

    fn path_dirs(&self) -> Vec<PathBuf> {
        Vec::new()
    }

    fn is_executable_file(&self, path: &Path) -> bool {
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }

    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }
}

/// A renderer over the built-in `id`, its binary and `tddy-tools` stubs in `bin`.
fn a_renderer_for(id: &str, binary_env: &str, bin: &Path) -> SpecHeadlessRenderer {
    for name in ["agent-stub", "tddy-tools"] {
        let path = bin.join(name);
        std::fs::write(&path, "#!/bin/sh\n").expect("stub");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    let host = AFixtureHost {
        env: BTreeMap::from([
            (
                binary_env.to_string(),
                bin.join("agent-stub").to_string_lossy().into_owned(),
            ),
            (
                "TDDY_TOOLS_BINARY".to_string(),
                bin.join("tddy-tools").to_string_lossy().into_owned(),
            ),
        ]),
    };
    let home = tempfile::tempdir().expect("home");
    let registry = load_agent_registry(home.path()).expect("the built-ins load");
    let spec = CodingAgentSpec::of(registry.get(id).expect("built in")).expect("a coding agent");
    let mut closure = registry.resolve_closure(&[id], &host).expect("resolves");
    let tool = closure.pop().expect("the agent's own tool");
    SpecHeadlessRenderer::new(spec, tool, &closure, AdapterRegistry::builtin()).expect("renders")
}

#[test]
fn cursors_model_listing_runs_its_spec_resolved_binary_and_reads_through_cursor_models() {
    // Given
    let bin = tempfile::tempdir().expect("bin");
    let renderer = a_renderer_for("cursor", "TDDY_CURSOR_AGENT", bin.path());

    // When
    let listing = renderer
        .model_listing()
        .expect("the listing renders")
        .map(|listing| {
            (
                listing.program,
                listing.argv,
                listing.adapter.name().to_string(),
            )
        });

    // Then
    assert_eq!(
        listing,
        Some((
            std::fs::canonicalize(bin.path().join("agent-stub")).expect("stub"),
            vec!["--list-models".to_string()],
            "cursor-models".to_string()
        ))
    );
}

#[test]
fn a_spec_declaring_no_model_listing_hands_the_backend_none() {
    // Given
    let bin = tempfile::tempdir().expect("bin");
    let renderer = a_renderer_for("claude", "TDDY_CLAUDE_BINARY", bin.path());

    // When
    let listing = renderer.model_listing().expect("nothing to render");

    // Then
    assert_eq!(listing.map(|listing| listing.argv), None);
}

#[test]
fn the_builtin_specs_this_file_reads_are_the_embedded_ones() {
    // Given — the positive control that the four agents above are built in
    let ids: Vec<bool> = ["claude", "cursor", "codex", "oz"]
        .iter()
        .map(|id| {
            builtin_specs()
                .into_iter()
                .any(|(path, _)| path.ends_with(&format!("/{id}.yaml")))
        })
        .collect();

    // Then
    assert_eq!(ids, vec![true, true, true, true]);
}
