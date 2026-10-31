//! The ACP slots of the built-in `claude-acp.yaml`, `codex-acp.yaml` and `opencode.yaml`'s
//! `coding_agent.acp` (`#agent-tools 11/12`): the tool server becomes `session/new` `mcp_servers`,
//! and a managed launch's native tools are withdrawn as each spec declares — `_meta` for Claude,
//! inline config for OpenCode, a refusal for Codex — matching what `withdrawal_session_meta` and
//! `opencode_withdrawal_config` build by hand today.
//!
//! The specs are read straight from the embedded files (no registry, no host resolution).
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-acp-agents-from-specs.md`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use pretty_assertions::assert_eq;
use tddy_actions::tool::RunSection;
use tddy_agent_backend::McpServerSpec;
use tddy_coding_agent_specs::slots::acp::{acp_mcp_servers, acp_withdrawal, AcpWithdrawal};
use tddy_coding_agent_specs::slots::{McpServerCommand, SlotError};
use tddy_coding_agent_specs::{builtin_specs, CodingAgentSection, CodingAgentSpec};

const TDDY_TOOLS: &str = "/opt/tddy/bin/tddy-tools";
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

/// `tddy-tools --mcp`, its credential in its environment.
fn the_tool_server() -> McpServerCommand {
    McpServerCommand {
        name: "tddy-tools".to_string(),
        command: PathBuf::from(TDDY_TOOLS),
        args: vec!["--mcp".to_string()],
        env: BTreeMap::from([
            ("TDDY_CONVERSATION_ID".to_string(), "conv-1".to_string()),
            ("TDDY_REMOTE_SESSION_TOKEN".to_string(), TOKEN.to_string()),
        ]),
    }
}

/// The native tools `spec` declares a managed launch withdraws.
fn its_native_tools(spec: &CodingAgentSpec) -> Vec<String> {
    spec.acp()
        .expect("the spec declares an acp section")
        .withdraw_native
        .clone()
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

#[test]
fn an_acp_session_slot_hands_the_tool_server_as_session_new_mcp_servers() {
    // Given — every ACP spec declares `mcp: { kind: acp-session }`, OpenCode's in its `acp` section
    let specs = [
        the_builtin_spec("claude-acp"),
        the_builtin_spec("codex-acp"),
        the_builtin_spec("opencode"),
    ];

    // When
    let servers: Vec<Vec<McpServerSpec>> = specs
        .iter()
        .map(|spec| acp_mcp_servers(spec, Some(&the_tool_server())).expect("rendered"))
        .collect();

    // Then — the server as the one `mcp_servers` entry, its env in name order
    let the_entry = || McpServerSpec {
        name: "tddy-tools".to_string(),
        command: PathBuf::from(TDDY_TOOLS),
        args: vec!["--mcp".to_string()],
        env: vec![
            ("TDDY_CONVERSATION_ID".to_string(), "conv-1".to_string()),
            ("TDDY_REMOTE_SESSION_TOKEN".to_string(), TOKEN.to_string()),
        ],
    };
    assert_eq!(
        servers,
        vec![vec![the_entry()], vec![the_entry()], vec![the_entry()]]
    );
}

#[test]
fn an_acp_launch_without_a_tool_server_opens_its_session_with_no_mcp_servers() {
    // Given
    let claude_acp = the_builtin_spec("claude-acp");

    // When
    let servers = acp_mcp_servers(&claude_acp, None);

    // Then
    assert_eq!(servers, Ok(Vec::new()));
}

#[test]
fn claude_acp_withdraws_its_native_tools_as_a_list_at_its_declared_meta_path() {
    // Given — `tool_deny: { acp_meta: claudeCode.options.disallowedTools }`
    let claude_acp = the_builtin_spec("claude-acp");
    let tools = its_native_tools(&claude_acp);

    // When
    let withdrawal = acp_withdrawal(&claude_acp, &tools);

    // Then — the `_meta` `withdrawal_session_meta("claude-acp")` builds today
    let meta = serde_json::json!({
        "claudeCode": { "options": { "disallowedTools": [
            "Read", "Write", "Edit", "MultiEdit", "NotebookEdit", "Bash", "BashOutput",
            "KillShell", "Glob", "Grep", "LS",
        ] } }
    });
    assert_eq!(
        withdrawal,
        Ok(Some(AcpWithdrawal::Meta(
            meta.as_object().expect("an object").clone()
        )))
    );
}

#[test]
fn opencode_acp_withdraws_its_native_tools_as_inline_config_in_its_declared_env() {
    // Given — `acp.tool_deny: { config_env: OPENCODE_CONFIG_CONTENT }`
    let opencode = the_builtin_spec("opencode");
    let tools = its_native_tools(&opencode);

    // When
    let withdrawal = acp_withdrawal(&opencode, &tools);

    // Then — the config `opencode_withdrawal_config("opencode")` builds today
    assert_eq!(
        withdrawal,
        Ok(Some(AcpWithdrawal::ConfigEnv {
            env: "OPENCODE_CONFIG_CONTENT".to_string(),
            config: serde_json::json!({ "tools": {
                "bash": false, "read": false, "write": false, "edit": false,
                "patch": false, "glob": false, "grep": false, "list": false,
            } }),
        }))
    );
}

#[test]
fn codex_acp_refuses_withdrawing_tools_through_its_unsupported_deny_slot_and_withdraws_none_unasked(
) {
    // Given — `tool_deny: unsupported`
    let codex_acp = the_builtin_spec("codex-acp");

    // When
    let withdrawals = (
        acp_withdrawal(&codex_acp, &strings(&["shell"])),
        acp_withdrawal(&codex_acp, &[]),
    );

    // Then
    assert_eq!(
        withdrawals,
        (
            Err(SlotError::SlotUnsupported {
                agent: "codex-acp".to_string(),
                slot: "tool_deny".to_string(),
            }),
            Ok(None),
        )
    );
}
