//! Golden: the built-in `claude.yaml`'s `coding_agent.headless` section renders the argv the
//! hand-built paths render today — `build_claude_args` for the workflow backend and
//! `headless_driver/argv.rs::claude()` for a headless conversation — byte for byte.
//!
//! The spec is read straight from the embedded file (no registry, no host resolution), so this
//! pins the declaration and the renderer alone.
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-headless-agents-from-specs.md` (`#agent-tools 6/12`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_actions::tool::{NetworkSection, ResolvedTool, RunSection};
use tddy_agent_backend::adapters::{
    AdapterRegistry, HeadlessMcpServer, HeadlessRenderer, HeadlessTurn,
};
use tddy_agent_backend::SessionMode;
use tddy_coding_agent_specs::headless::{HeadlessSpecError, SpecHeadlessRenderer};
use tddy_coding_agent_specs::{builtin_specs, CodingAgentSection, CodingAgentSpec};

const CLAUDE: &str = "/opt/homebrew/bin/claude";
const TDDY_TOOLS: &str = "/opt/tddy/bin/tddy-tools";
const APPROVAL_TOOL: &str = "mcp__tddy-tools__approval_prompt";
const SCRATCH: &str = "/sessions/0190-a";

/// The built-in claude spec, read from its embedded YAML without the registry.
fn the_builtin_claude_spec() -> CodingAgentSpec {
    let (_, yaml) = builtin_specs()
        .into_iter()
        .find(|(path, _)| path.ends_with("claude.yaml"))
        .expect("claude.yaml is built in");
    let file: serde_yaml::Value = serde_yaml::from_str(yaml).expect("claude.yaml is YAML");
    let run: RunSection = serde_yaml::from_value(file["run"].clone()).expect("its run section");
    let agent: CodingAgentSection =
        serde_yaml::from_value(file["extensions"]["coding_agent"].clone())
            .expect("its coding_agent section");
    CodingAgentSpec {
        id: "claude".to_string(),
        run,
        agent,
    }
}

fn a_resolved(id: &str, binary: &str) -> ResolvedTool {
    ResolvedTool {
        id: id.to_string(),
        command: id.to_string(),
        binary: PathBuf::from(binary),
        reads: Vec::new(),
        network: NetworkSection::default(),
    }
}

fn the_renderer_for(spec: CodingAgentSpec) -> SpecHeadlessRenderer {
    SpecHeadlessRenderer::new(
        spec,
        a_resolved("claude", CLAUDE),
        &[a_resolved("tddy-tools", TDDY_TOOLS)],
        AdapterRegistry::builtin(),
    )
    .expect("the built-in claude spec renders headless")
}

fn the_tool_server() -> HeadlessMcpServer {
    HeadlessMcpServer {
        name: "tddy-tools".to_string(),
        command: PathBuf::from(TDDY_TOOLS),
        args: vec!["--mcp".to_string()],
        env: BTreeMap::new(),
    }
}

/// A workflow-backend turn as `ClaudeCodeBackend` hands it over.
fn a_backend_turn(permission_mode: Option<&str>, session: SessionMode) -> HeadlessTurn {
    HeadlessTurn {
        prompt: "Plan the feature".to_string(),
        model: Some("opus".to_string()),
        session: Some(session),
        permission_mode: permission_mode.map(str::to_string),
        append_system_prompt_file: Some(PathBuf::from("/sessions/0190-a/system-prompt.md")),
        allowed_tools: vec!["Read".to_string(), "Glob".to_string()],
        denied_tools: Vec::new(),
        permission_prompt_tool: Some(APPROVAL_TOOL.to_string()),
        mcp_server: Some(the_tool_server()),
        workdir: PathBuf::from("/home/dev/repo"),
        scratch_dir: PathBuf::from(SCRATCH),
    }
}

fn the_mcp_config() -> String {
    Path::new(SCRATCH)
        .join("claude-mcp-config.json")
        .to_string_lossy()
        .into_owned()
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn a_plan_goals_fresh_turn_renders_the_argv_build_claude_args_builds_today() {
    // Given
    let renderer = the_renderer_for(the_builtin_claude_spec());
    let turn = a_backend_turn(Some("plan"), SessionMode::Fresh("sess-1".to_string()));

    // When
    let invocation = renderer.render(&turn).expect("the turn renders");

    // Then
    assert_eq!(
        (invocation.program, invocation.argv),
        (
            PathBuf::from(CLAUDE),
            strings(&[
                "-p",
                "Plan the feature",
                "--output-format",
                "stream-json",
                "--verbose",
                "--permission-mode",
                "plan",
                "--model",
                "opus",
                "--session-id",
                "sess-1",
                "--append-system-prompt-file",
                "/sessions/0190-a/system-prompt.md",
                "--allowedTools",
                "Read",
                "--allowedTools",
                "Glob",
                "--permission-prompt-tool",
                APPROVAL_TOOL,
                "--mcp-config",
                &the_mcp_config(),
            ])
        )
    );
}

#[test]
fn an_accept_edits_resumed_turn_renders_the_argv_build_claude_args_builds_today() {
    // Given
    let renderer = the_renderer_for(the_builtin_claude_spec());
    let turn = a_backend_turn(
        Some("acceptEdits"),
        SessionMode::Resume("sess-1".to_string()),
    );

    // When
    let invocation = renderer.render(&turn).expect("the turn renders");

    // Then
    assert_eq!(
        invocation.argv,
        strings(&[
            "-p",
            "Plan the feature",
            "--output-format",
            "stream-json",
            "--verbose",
            "--permission-mode",
            "acceptEdits",
            "--model",
            "opus",
            "--resume",
            "sess-1",
            "--append-system-prompt-file",
            "/sessions/0190-a/system-prompt.md",
            "--allowedTools",
            "Read",
            "--allowedTools",
            "Glob",
            "--permission-prompt-tool",
            APPROVAL_TOOL,
            "--mcp-config",
            &the_mcp_config(),
        ])
    );
}

#[test]
fn a_conversations_resumed_turn_renders_the_argv_the_headless_driver_builds_today() {
    // Given — a headless conversation's second turn: no permission mode, no system prompt
    let renderer = the_renderer_for(the_builtin_claude_spec());
    let turn = HeadlessTurn {
        prompt: "Now add a test".to_string(),
        model: Some("sonnet".to_string()),
        session: Some(SessionMode::Resume("cl-1".to_string())),
        permission_mode: None,
        append_system_prompt_file: None,
        allowed_tools: Vec::new(),
        denied_tools: Vec::new(),
        permission_prompt_tool: Some(APPROVAL_TOOL.to_string()),
        mcp_server: Some(the_tool_server()),
        workdir: PathBuf::from("/home/dev/worktrees/parser-fix"),
        scratch_dir: PathBuf::from(SCRATCH),
    };

    // When
    let invocation = renderer.render(&turn).expect("the turn renders");

    // Then
    assert_eq!(
        invocation.argv,
        strings(&[
            "-p",
            "Now add a test",
            "--output-format",
            "stream-json",
            "--verbose",
            "--model",
            "sonnet",
            "--resume",
            "cl-1",
            "--permission-prompt-tool",
            APPROVAL_TOOL,
            "--mcp-config",
            &the_mcp_config(),
        ])
    );
}

#[test]
fn a_turn_renders_the_mcp_registration_as_its_one_file_and_names_its_adapter() {
    // Given
    let renderer = the_renderer_for(the_builtin_claude_spec());
    let turn = a_backend_turn(Some("plan"), SessionMode::Fresh("sess-1".to_string()));

    // When
    let invocation = renderer.render(&turn).expect("the turn renders");

    // Then
    let expected_config = serde_json::json!({
        "mcpServers": { "tddy-tools": { "command": TDDY_TOOLS, "args": ["--mcp"] } }
    });
    assert_eq!(
        (
            invocation.files,
            invocation.adapter.name().to_string(),
            invocation.exit_ok_if_structured,
        ),
        (
            vec![(
                PathBuf::from(the_mcp_config()),
                expected_config.to_string().into_bytes()
            )],
            "claude-stream-json".to_string(),
            true,
        )
    );
}

#[test]
fn the_renderers_default_tool_server_is_its_tddy_tools_dep_with_mcp() {
    // Given
    let renderer = the_renderer_for(the_builtin_claude_spec());

    // When
    let server = renderer.tool_server();

    // Then
    assert_eq!(server, Some(the_tool_server()));
}

#[test]
fn a_spec_without_a_headless_section_is_refused_naming_the_agent() {
    // Given
    let mut spec = the_builtin_claude_spec();
    spec.agent.headless = None;

    // When
    let refused = SpecHeadlessRenderer::new(
        spec,
        a_resolved("claude", CLAUDE),
        &[a_resolved("tddy-tools", TDDY_TOOLS)],
        AdapterRegistry::builtin(),
    )
    .map(|_| ());

    // Then
    assert_eq!(
        refused,
        Err(HeadlessSpecError::NoHeadlessSection {
            agent: "claude".to_string()
        })
    );
}
