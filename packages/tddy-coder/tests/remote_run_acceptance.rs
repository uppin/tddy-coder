//! Acceptance tests: `run_remote` full bootstrap (Phase 5 follow-up).
//!
//! AC: `run_remote` shells out to `tddy-tools remote {start|connect}-session`, creates a
//! `RemoteContextDir`, builds the allowlist from `tddy-tools remote list-tools` output, and
//! passes a fully populated `InvokeRequest` (with `remote: Some(RemoteToolEnv)`) to the backend.

use tddy_coder::remote::run_remote_with_tools_output;
use tddy_coder::config::RemoteConfig;

/// Phase 5 AC: `run_remote_with_tools_output` builds an allowlist from a JSON tool-name array.
///
/// This is the core of the allowlist-construction step: given the JSON stdout from
/// `tddy-tools remote list-tools`, build `mcp__tddy-tools__<name>` entries + AskUserQuestion.
#[test]
fn run_remote_builds_allowlist_from_list_tools_json() {
    let tools_json = r#"["Read","Write","Grep","Shell","Await"]"#;

    let allowlist = run_remote_with_tools_output(tools_json)
        .expect("run_remote_with_tools_output must succeed with valid JSON");

    assert!(
        allowlist.contains(&"mcp__tddy-tools__Read".to_string()),
        "allowlist must contain mcp__tddy-tools__Read; got: {:?}",
        allowlist
    );
    assert!(
        allowlist.contains(&"mcp__tddy-tools__Shell".to_string()),
        "allowlist must contain mcp__tddy-tools__Shell; got: {:?}",
        allowlist
    );
    assert!(
        allowlist.contains(&"AskUserQuestion".to_string()),
        "allowlist must always include AskUserQuestion; got: {:?}",
        allowlist
    );

    // Native tools must NOT be in the allowlist.
    let forbidden = ["Bash", "Edit", "Read", "Write"];
    for tool in &forbidden {
        let bare = tool.to_string();
        assert!(
            !allowlist.contains(&bare),
            "allowlist must not contain bare native tool '{}'; got: {:?}",
            tool,
            allowlist
        );
    }
}

/// Phase 5 AC: `run_remote_with_tools_output` returns Err on invalid JSON.
#[test]
fn run_remote_returns_err_on_invalid_tools_json() {
    let result = run_remote_with_tools_output("not valid json");
    assert!(
        result.is_err(),
        "run_remote_with_tools_output must return Err on invalid JSON"
    );
}

/// Phase 5 AC: `RemoteConfig` can be constructed from CLI flags and converts to `RemoteToolEnv`.
///
/// The conversion populates all fields that `RemoteToolEnv.env_pairs()` will export.
#[test]
fn remote_config_converts_to_remote_tool_env() {
    use tddy_core::backend::RemoteToolEnv;

    let cfg = RemoteConfig {
        daemon_url: Some("http://relay.local:9001".to_string()),
        session_id: Some("sess-convert-test".to_string()),
        session_token: Some("tok-convert".to_string()),
        daemon_instance_id: Some("relay-id-42".to_string()),
    };

    let env: RemoteToolEnv = cfg
        .to_remote_tool_env()
        .expect("to_remote_tool_env must succeed when all required fields are set");

    assert_eq!(env.daemon_url, "http://relay.local:9001");
    assert_eq!(env.session_id, "sess-convert-test");
    assert_eq!(env.session_token, "tok-convert");
    assert_eq!(env.daemon_instance_id.as_deref(), Some("relay-id-42"));
}

/// Phase 5 AC: `RemoteConfig::to_remote_tool_env` returns Err when required fields are missing.
#[test]
fn remote_config_to_remote_tool_env_fails_on_incomplete_config() {
    let cfg = RemoteConfig {
        daemon_url: None,          // missing — required
        session_id: None,          // missing — required
        session_token: None,       // missing — required
        daemon_instance_id: None,
    };

    let result = cfg.to_remote_tool_env();
    assert!(
        result.is_err(),
        "to_remote_tool_env must return Err when daemon_url/session_id/token are absent"
    );
}

/// Phase 5 AC: `run_remote` appears as a public function in tddy_coder::run accepting `&Args`.
///
/// This test verifies the function signature exists (type-check only).
/// The actual integration (subprocess bootstrap) is covered by e2e tests.
#[test]
fn run_remote_public_function_accepts_args_ref() {
    use tddy_coder::run::run_remote;
    // Just ensure the symbol resolves — actual call would need a real relay.
    let _fn_ptr: fn(&tddy_coder::run::Args) -> anyhow::Result<()> = run_remote;
}
