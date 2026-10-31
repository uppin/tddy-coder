//! Golden: the built-in `cursor.yaml`'s, `codex.yaml`'s and `oz.yaml`'s `coding_agent.headless`
//! sections render the argv the hand-built paths render today — `build_cursor_cli_args` and
//! `build_codex_exec_argv` for the workflow backends, `headless_driver/argv.rs`'s `cursor()`,
//! `codex()` and `oz()` for a headless conversation — and each spec's MCP slot writes what the
//! hand-built launch writes (`#agent-tools 9/12`).
//!
//! One deliberate difference: a headless Codex conversation's `-m` now follows `--json`, as the
//! backend's does — one template for both, same meaning.
//!
//! The specs are read straight from the embedded files (no registry, no host resolution).
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-headless-cursor-codex-oz-from-specs.md`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_actions::tool::{NetworkSection, ResolvedTool, RunSection};
use tddy_agent_backend::adapters::{
    AdapterRegistry, HeadlessMcpServer, HeadlessRenderer, HeadlessTurn,
};
use tddy_agent_backend::SessionMode;
use tddy_coding_agent_specs::headless::SpecHeadlessRenderer;
use tddy_coding_agent_specs::slots::headless_mcp::{
    mcp_written_paths, render_inline_mcp, ARGV_MCP_FILE,
};
use tddy_coding_agent_specs::slots::McpServerCommand;
use tddy_coding_agent_specs::{
    builtin_specs, CodingAgentSection, CodingAgentSpec, McpSlot, ToolSlot, Unsupported,
};

const TDDY_TOOLS: &str = "/opt/tddy/bin/tddy-tools";
const SCRATCH: &str = "/data/headless/conv-1";
const REPO: &str = "/home/dev/repo";
const WORKTREE: &str = "/home/dev/worktrees/parser-fix";
const TOKEN: &str = "minted-agent-token";

/// The built-in spec `id`, read from its embedded YAML without the registry.
fn the_builtin_spec(id: &str) -> CodingAgentSpec {
    let file_name = format!("/{id}.yaml");
    let (_, yaml) = builtin_specs()
        .into_iter()
        .find(|(path, _)| path.ends_with(&file_name))
        .unwrap_or_else(|| panic!("{id}.yaml is built in"));
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

fn a_resolved(id: &str, binary: &str) -> ResolvedTool {
    ResolvedTool {
        id: id.to_string(),
        command: id.to_string(),
        binary: PathBuf::from(binary),
        reads: Vec::new(),
        network: NetworkSection::default(),
    }
}

fn the_renderer_for(id: &str, binary: &str) -> SpecHeadlessRenderer {
    SpecHeadlessRenderer::new(
        the_builtin_spec(id),
        a_resolved(id, binary),
        &[a_resolved("tddy-tools", TDDY_TOOLS)],
        AdapterRegistry::builtin(),
    )
    .unwrap_or_else(|e| panic!("the built-in {id} spec renders headless: {e}"))
}

/// The `tddy-tools` server a headless conversation is handed, its credential in its environment.
fn the_conversations_tool_server() -> HeadlessMcpServer {
    HeadlessMcpServer {
        name: "tddy-tools".to_string(),
        command: PathBuf::from(TDDY_TOOLS),
        args: vec!["--mcp".to_string()],
        env: BTreeMap::from([
            ("TDDY_CONVERSATION_ID".to_string(), "conv-1".to_string()),
            ("TDDY_REMOTE_SESSION_TOKEN".to_string(), TOKEN.to_string()),
        ]),
    }
}

/// A turn with nothing but `prompt`, `model` and `session`, on `workdir`.
fn a_turn(prompt: &str, model: &str, session: Option<SessionMode>, workdir: &str) -> HeadlessTurn {
    HeadlessTurn {
        prompt: prompt.to_string(),
        model: Some(model.to_string()),
        session,
        permission_mode: None,
        append_system_prompt_file: None,
        allowed_tools: Vec::new(),
        denied_tools: Vec::new(),
        permission_prompt_tool: None,
        mcp_server: None,
        workdir: PathBuf::from(workdir),
        scratch_dir: PathBuf::from(SCRATCH),
        params: BTreeMap::new(),
        route_args: Vec::new(),
    }
}

fn params(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect()
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

// --- Cursor -----------------------------------------------------------------------------------

#[test]
fn a_cursor_planning_goals_resumed_turn_renders_the_argv_build_cursor_cli_args_builds_today() {
    // Given — the workflow backend: `force` always, `plan` for a planning goal
    let renderer = the_renderer_for("cursor", "/usr/local/bin/agent");
    let turn = HeadlessTurn {
        params: params(&[("plan", "1"), ("force", "1")]),
        ..a_turn(
            "You plan.\n\nPlan the feature",
            "composer-2.5",
            Some(SessionMode::Resume("chat-1".to_string())),
            REPO,
        )
    };

    // When
    let invocation = renderer.render(&turn).expect("the turn renders");

    // Then
    assert_eq!(
        (invocation.program, invocation.argv),
        (
            PathBuf::from("/usr/local/bin/agent"),
            strings(&[
                "--plan",
                "--resume",
                "chat-1",
                "-p",
                "You plan.\n\nPlan the feature",
                "--model",
                "composer-2.5",
                "--output-format",
                "stream-json",
                "--stream-partial-output",
                "--force",
                "--trust",
                "--approve-mcps",
            ])
        )
    );
}

#[test]
fn a_cursor_conversations_first_turn_renders_the_argv_the_headless_driver_builds_today() {
    // Given — a headless conversation binds neither `plan` nor `force`, and hands its server
    let renderer = the_renderer_for("cursor", "/usr/local/bin/agent");
    let turn = HeadlessTurn {
        mcp_server: Some(the_conversations_tool_server()),
        ..a_turn("Fix the parser", "composer-2.5", None, WORKTREE)
    };

    // When
    let invocation = renderer.render(&turn).expect("the turn renders");

    // Then — no `--force`: a conversation runs read-only unless an operator opts in
    assert_eq!(
        invocation.argv,
        strings(&[
            "-p",
            "Fix the parser",
            "--model",
            "composer-2.5",
            "--output-format",
            "stream-json",
            "--stream-partial-output",
            "--trust",
            "--approve-mcps",
        ])
    );
}

#[test]
fn a_cursor_turn_writes_its_registration_into_the_worktrees_cursor_mcp_json_and_names_cursor_stream(
) {
    // Given
    let renderer = the_renderer_for("cursor", "/usr/local/bin/agent");
    let turn = HeadlessTurn {
        mcp_server: Some(the_conversations_tool_server()),
        ..a_turn("Fix the parser", "composer-2.5", None, WORKTREE)
    };

    // When
    let invocation = renderer.render(&turn).expect("the turn renders");

    // Then
    let expected = serde_json::json!({
        "mcpServers": {
            "tddy-tools": {
                "command": TDDY_TOOLS,
                "args": ["--mcp"],
                "env": { "TDDY_CONVERSATION_ID": "conv-1", "TDDY_REMOTE_SESSION_TOKEN": TOKEN },
            }
        }
    });
    assert_eq!(
        (invocation.files, invocation.adapter.name().to_string()),
        (
            vec![(
                Path::new(WORKTREE).join(".cursor/mcp.json"),
                expected.to_string().into_bytes()
            )],
            "cursor-stream".to_string(),
        )
    );
}

#[test]
fn cursors_headless_mcp_slot_writes_the_worktree_file_while_its_pty_slot_stays_in_home() {
    // Given
    let cursor = the_builtin_spec("cursor");
    let headless_slot = cursor
        .headless()
        .and_then(|headless| headless.mcp.clone())
        .expect("cursor.yaml declares a headless MCP slot");

    // When
    let written = (
        mcp_written_paths(
            &headless_slot,
            Path::new(WORKTREE),
            Path::new(SCRATCH),
            None,
        ),
        mcp_written_paths(
            &cursor.agent.mcp,
            Path::new(WORKTREE),
            Path::new(SCRATCH),
            Some(Path::new("/home/dev")),
        ),
    );

    // Then
    assert_eq!(
        written,
        (
            Ok(vec![Path::new(WORKTREE).join(".cursor/mcp.json")]),
            Ok(vec![PathBuf::from("/home/dev/.cursor/mcp.json")]),
        )
    );
}

// --- Codex ------------------------------------------------------------------------------------

#[test]
fn a_codex_read_only_planning_turn_renders_the_argv_build_codex_exec_argv_builds_today() {
    // Given
    let renderer = the_renderer_for("codex", "/usr/local/bin/codex");
    let turn = HeadlessTurn {
        params: params(&[("sandbox_mode", "read-only")]),
        ..a_turn(
            "You plan.\n\nPlan the feature",
            "gpt-5.3-codex",
            Some(SessionMode::Fresh("unused".to_string())),
            REPO,
        )
    };

    // When
    let invocation = renderer.render(&turn).expect("the turn renders");

    // Then
    assert_eq!(
        (invocation.program, invocation.argv),
        (
            PathBuf::from("/usr/local/bin/codex"),
            strings(&[
                "exec",
                "-C",
                REPO,
                "-s",
                "read-only",
                "--json",
                "-m",
                "gpt-5.3-codex",
                "You plan.\n\nPlan the feature",
            ])
        )
    );
}

#[test]
fn a_codex_resumed_turn_puts_resume_before_the_model_and_the_thread_before_the_prompt() {
    // Given — no `sandbox_mode`: `workspace-write`, as for every goal that is not read-only
    let renderer = the_renderer_for("codex", "/usr/local/bin/codex");
    let turn = a_turn(
        "Now add a test",
        "gpt-5.3-codex",
        Some(SessionMode::Resume("th-1".to_string())),
        REPO,
    );

    // When
    let invocation = renderer.render(&turn).expect("the turn renders");

    // Then
    assert_eq!(
        invocation.argv,
        strings(&[
            "exec",
            "-C",
            REPO,
            "-s",
            "workspace-write",
            "--json",
            "resume",
            "-m",
            "gpt-5.3-codex",
            "th-1",
            "Now add a test",
        ])
    );
}

#[test]
fn a_codex_turn_on_a_provider_route_takes_the_routes_argv_in_the_models_place() {
    // Given — on a route the backend passes no model of its own
    let renderer = the_renderer_for("codex", "/usr/local/bin/codex");
    let turn = HeadlessTurn {
        model: None,
        route_args: strings(&["-c", "model_provider=\"acme\"", "-m", "acme-coder"]),
        ..a_turn("Go", "unused", None, REPO)
    };

    // When
    let invocation = renderer.render(&turn).expect("the turn renders");

    // Then
    assert_eq!(
        invocation.argv,
        strings(&[
            "exec",
            "-C",
            REPO,
            "-s",
            "workspace-write",
            "--json",
            "-c",
            "model_provider=\"acme\"",
            "-m",
            "acme-coder",
            "Go",
        ])
    );
}

#[test]
fn a_codex_conversations_resumed_turn_carries_its_server_inline_with_its_model_after_json() {
    // Given
    let renderer = the_renderer_for("codex", "/usr/local/bin/codex");
    let turn = HeadlessTurn {
        mcp_server: Some(the_conversations_tool_server()),
        ..a_turn(
            "Go on",
            "gpt-5.3-codex",
            Some(SessionMode::Resume("th-1".to_string())),
            WORKTREE,
        )
    };

    // When
    let invocation = renderer.render(&turn).expect("the turn renders");

    // Then — `-m` follows `--json` (the driver put it before): one template, same meaning
    assert_eq!(
        (invocation.argv, invocation.files),
        (
            strings(&[
                "exec",
                "-c",
                "mcp_servers.tddy-tools.command=\"/opt/tddy/bin/tddy-tools\"",
                "-c",
                "mcp_servers.tddy-tools.args=[\"--mcp\"]",
                "-c",
                "mcp_servers.tddy-tools.env_vars=[\"TDDY_CONVERSATION_ID\",\"TDDY_REMOTE_SESSION_TOKEN\"]",
                "-C",
                WORKTREE,
                "-s",
                "workspace-write",
                "--json",
                "resume",
                "-m",
                "gpt-5.3-codex",
                "th-1",
                "Go on",
            ]),
            Vec::new(),
        )
    );
}

#[test]
fn codexs_mcp_slot_renders_its_server_inline_and_writes_no_file() {
    // Given
    let codex = the_builtin_spec("codex");
    let server = McpServerCommand {
        name: "tddy-tools".to_string(),
        command: PathBuf::from(TDDY_TOOLS),
        args: strings(&["--mcp"]),
        env: BTreeMap::from([("TDDY_REMOTE_SESSION_TOKEN".to_string(), TOKEN.to_string())]),
    };

    // When
    let rendered = (
        codex.agent.mcp.clone(),
        render_inline_mcp("-c", "mcp_servers", &server),
        mcp_written_paths(&codex.agent.mcp, Path::new(REPO), Path::new(SCRATCH), None),
    );

    // Then
    assert_eq!(
        rendered,
        (
            McpSlot::Inline {
                flag: "-c".to_string(),
                key_prefix: "mcp_servers".to_string(),
            },
            strings(&[
                "-c",
                "mcp_servers.tddy-tools.command=\"/opt/tddy/bin/tddy-tools\"",
                "-c",
                "mcp_servers.tddy-tools.args=[\"--mcp\"]",
                "-c",
                "mcp_servers.tddy-tools.env_vars=[\"TDDY_REMOTE_SESSION_TOKEN\"]",
            ]),
            Ok(Vec::new()),
        )
    );
}

#[test]
fn codex_declares_it_cannot_withdraw_its_native_tools_and_reads_through_codex_jsonl() {
    // Given — the refusal of a managed Codex launch reads `tool_deny`; the positive control is that
    // the spec does declare a headless adapter
    let codex = the_builtin_spec("codex");

    // When
    let declared = (
        codex.agent.tool_deny.clone(),
        codex
            .headless()
            .map(|headless| headless.adapter.clone())
            .unwrap_or_default(),
        mcp_written_paths(&codex.agent.mcp, Path::new(REPO), Path::new(SCRATCH), None)
            .map(|paths| paths.len()),
    );

    // Then
    assert_eq!(
        declared,
        (
            ToolSlot::Unsupported(Unsupported::Unsupported),
            "codex-jsonl".to_string(),
            Ok(0),
        )
    );
}

// --- Oz ---------------------------------------------------------------------------------------

#[test]
fn an_oz_conversations_resumed_turn_renders_the_argv_the_headless_driver_builds_today() {
    // Given
    let renderer = the_renderer_for("oz", "/usr/local/bin/oz");
    let turn = HeadlessTurn {
        mcp_server: Some(the_conversations_tool_server()),
        params: params(&[("profile", "tddy-managed")]),
        ..a_turn(
            "Go on",
            "claude-4-sonnet",
            Some(SessionMode::Resume("oz-conv-1".to_string())),
            WORKTREE,
        )
    };

    // When
    let invocation = renderer.render(&turn).expect("the turn renders");

    // Then
    let mcp_file = Path::new(SCRATCH).join(ARGV_MCP_FILE);
    assert_eq!(
        (invocation.program, invocation.argv),
        (
            PathBuf::from("/usr/local/bin/oz"),
            strings(&[
                "agent",
                "run",
                "-p",
                "Go on",
                "--output-format",
                "ndjson",
                "-C",
                WORKTREE,
                "--conversation",
                "oz-conv-1",
                "--model",
                "claude-4-sonnet",
                "--mcp",
                &mcp_file.to_string_lossy(),
                "--profile",
                "tddy-managed",
            ])
        )
    );
}

#[test]
fn an_oz_turn_writes_its_servers_by_name_file_under_the_conversation_and_names_oz_ndjson() {
    // Given
    let renderer = the_renderer_for("oz", "/usr/local/bin/oz");
    let turn = HeadlessTurn {
        mcp_server: Some(the_conversations_tool_server()),
        ..a_turn("Go", "claude-4-sonnet", None, WORKTREE)
    };

    // When
    let invocation = renderer.render(&turn).expect("the turn renders");

    // Then
    let expected = serde_json::json!({
        "tddy-tools": {
            "command": TDDY_TOOLS,
            "args": ["--mcp"],
            "env": { "TDDY_CONVERSATION_ID": "conv-1", "TDDY_REMOTE_SESSION_TOKEN": TOKEN },
        }
    });
    assert_eq!(
        (invocation.files, invocation.adapter.name().to_string()),
        (
            vec![(
                Path::new(SCRATCH).join(ARGV_MCP_FILE),
                expected.to_string().into_bytes()
            )],
            "oz-ndjson".to_string(),
        )
    );
}

#[test]
fn ozs_mcp_slot_is_an_argv_file_under_the_conversations_scratch() {
    // Given
    let oz = the_builtin_spec("oz");

    // When
    let written = mcp_written_paths(&oz.agent.mcp, Path::new(WORKTREE), Path::new(SCRATCH), None);

    // Then
    assert_eq!(
        (oz.agent.mcp.clone(), written),
        (
            McpSlot::Argv {
                flag: "--mcp".to_string()
            },
            Ok(vec![Path::new(SCRATCH).join(ARGV_MCP_FILE)]),
        )
    );
}
