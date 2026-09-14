//! A tool a specialized agent `replaces` stays withdrawn — SSH execution does not re-open it.
//!
//! `replaces` is enforced in `tddy-tools` before dispatch. RemoteShell lives on the code-managing
//! daemon; this process must never open SSH, and a replaced name must not become reachable just
//! because the session has `ssh_config_host` set.

use serial_test::serial;
use tddy_tools::server::PermissionServer;

const IPC_SOCKET_ENV: &str = "TDDY_SANDBOX_TOOL_IPC";

fn with_subagent_replacing<R>(replaced: &[&str], f: impl FnOnce() -> R) -> R {
    let defs = format!(
        r#"[{{"name":"explorer","model":"m","base_url":"http://127.0.0.1:1","replaces":[{}]}}]"#,
        replaced
            .iter()
            .map(|t| format!("\"{t}\""))
            .collect::<Vec<_>>()
            .join(",")
    );
    std::env::set_var(IPC_SOCKET_ENV, "/tmp/tddy-test-ipc.sock");
    std::env::set_var("TDDY_SUBAGENT", "explorer");
    std::env::set_var("TDDY_SUBAGENTS_JSON", defs);
    std::env::set_var("TDDY_SSH_CONFIG_HOST", "buildbox");
    let result = f();
    std::env::remove_var(IPC_SOCKET_ENV);
    std::env::remove_var("TDDY_SUBAGENT");
    std::env::remove_var("TDDY_SUBAGENTS_JSON");
    std::env::remove_var("TDDY_SSH_CONFIG_HOST");
    result
}

#[test]
#[serial]
fn a_replaced_exec_tool_is_still_refused_and_is_not_dispatched_over_ssh() {
    // Given — Read is withdrawn, and the session names an SSH Host alias
    let tools = with_subagent_replacing(&["Read"], || PermissionServer::new().tool_names());

    // Then — the MCP server still omits Read; an SSH alias is not a way around replaces
    assert!(
        !tools.contains(&"Read".to_string()),
        "replaced Read must stay withdrawn when ssh_config_host is set; got: {tools:?}"
    );
}
