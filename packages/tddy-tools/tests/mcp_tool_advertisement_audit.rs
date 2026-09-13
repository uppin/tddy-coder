//! Acceptance test: the whole advertised MCP tool surface, enumerated over the real
//! `tddy-tools --mcp` stdio wire.
//!
//! Changeset: docs/dev/1-WIP/2026-09-09-unbundle-tools-thinning.md § Testing Plan.
//!
//! The tools this server advertises are now assembled from six crates — `tddy-tools` itself
//! (`approval_prompt`, the 14 `pr_*` and 2 `github_*` tools, `spawn_conversation`, the six
//! `subagent_*` tools), `tddy-tool-engine` (the ten exec tools), `tddy-lsp-executor` (the five
//! `Lsp*` tools) and `tddy-core` (the action surface's gate). An omission in any one of them
//! would otherwise surface as a capability an agent silently no longer has, at that agent's
//! runtime rather than in CI. So the audit pins the **names**, not a count: a count that still
//! matched while a name changed would pass, which is the exact failure this file exists to catch.
//!
//! Two transports, two answers, and that is deliberate. `request_action`, `list_actions` and
//! `invoke_action` used to be advertised on every daemon-hosted session and implemented on none —
//! every call answered `{"error":"unknown tool: ListActions","is_error":true}`. They are now
//! merged only where the host claims to serve them, with
//! `tddy_core::session_actions::SESSION_ACTION_TOOLS_ENV`, which `tddy-sandbox-app`'s spawn path
//! sets and the daemon path does not. The surface is therefore **43** where they are served and
//! **40** where they are not, differing in exactly those three.

use std::collections::BTreeSet;
use std::process::Stdio;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};

const IO_TIMEOUT: Duration = Duration::from_secs(10);

/// Every tool a session with all four of its optional surfaces present advertises, sorted.
///
/// Held as names rather than a length so that a tool renamed, dropped or re-homed during a crate
/// move fails here by name. The count is the array's own length — `[&str; 43]` is the "exactly 43"
/// half of the assertion, checked by the compiler.
const EVERY_ADVERTISED_TOOL: [&str; 43] = [
    "Await",
    "Delete",
    "Glob",
    "Grep",
    "LspDefinition",
    "LspDiagnostics",
    "LspHover",
    "LspReferences",
    "LspSymbols",
    "Read",
    "ReadLints",
    "SemanticSearch",
    "Shell",
    "StrReplace",
    "Write",
    "approval_prompt",
    "github_create_pull_request",
    "github_update_pull_request",
    "invoke_action",
    "list_actions",
    "pr_add_planned",
    "pr_adopt",
    "pr_close",
    "pr_comments",
    "pr_delete_planned",
    "pr_merge",
    "pr_read",
    "pr_repoint",
    "pr_resolve_conflicts",
    "pr_search",
    "pr_set_parents",
    "pr_set_status",
    "pr_spawn_child",
    "pr_stack_status",
    "pr_update_planned",
    "request_action",
    "spawn_conversation",
    "subagent_await",
    "subagent_cancel",
    "subagent_list",
    "subagent_new_session",
    "subagent_prompt",
    "subagent_status",
];

/// The three tools a host must claim before they are advertised at all.
const ACTION_TOOLS: [&str; 3] = ["invoke_action", "list_actions", "request_action"];

// ─── The session under audit ──────────────────────────────────────────────────

/// Every variable that decides what `tools/list` answers. Cleared before each spawn so a test
/// states its own environment — one leaked from the developer's shell would otherwise change the
/// advertised set.
const ADVERTISEMENT_ENV_KEYS: [&str; 10] = [
    "TDDY_SANDBOX_TOOL_IPC",
    "TDDY_REMOTE_LIVEKIT_URL",
    "TDDY_REMOTE_LIVEKIT_ROOM",
    "TDDY_REMOTE_LIVEKIT_TOKEN",
    "TDDY_REMOTE_SESSION_ID",
    "TDDY_REMOTE_DAEMON_URL",
    "TDDY_SESSION_ACTION_TOOLS",
    "TDDY_SUBAGENTS_JSON",
    "TDDY_SUBAGENT_ROSTER_STATIC",
    "TDDY_LSP_TOOLS",
];

/// The environment of a session that has every optional tool surface: a reachable session-tool
/// transport (the exec catalog), an agent on its roster (the conversation tools), and a language
/// server (the `Lsp*` tools).
///
/// The socket is never connected to and the subagent's `base_url` never answers — advertisement is
/// decided by what the environment declares, not by what replies, which is what makes this audit a
/// deterministic enumeration rather than an integration test.
struct FullyConfiguredSession {
    env: Vec<(&'static str, String)>,
}

fn a_fully_configured_session() -> FullyConfiguredSession {
    let explorer = json!([{
        "name": "explorer",
        "model": "qwen2.5-coder:7b",
        "base_url": "http://127.0.0.1:1",
        "tools": ["READ", "GLOB", "GREP"],
        "max_turns": 6,
        "replaces": []
    }]);
    FullyConfiguredSession {
        env: vec![
            ("TDDY_SANDBOX_TOOL_IPC", unconnected_socket_path()),
            ("TDDY_SUBAGENTS_JSON", explorer.to_string()),
            ("TDDY_SUBAGENT_ROSTER_STATIC", "1".to_string()),
            ("TDDY_LSP_TOOLS", "1".to_string()),
        ],
    }
}

impl FullyConfiguredSession {
    /// A host that routes `EstablishAction` / `ListActions` / `InvokeAction` to something which
    /// implements them — what `tddy-sandbox-app` declares for the one placement that answers them.
    fn whose_host_serves_the_action_tools(mut self) -> Self {
        self.env
            .push(("TDDY_SESSION_ACTION_TOOLS", "1".to_string()));
        self
    }

    async fn advertised_tool_names(self) -> BTreeSet<String> {
        let mut child = self.spawn();
        let mut stdin = child.stdin.take().expect("child stdin");
        let mut stdout = BufReader::new(child.stdout.take().expect("child stdout"));
        initialize_mcp_session(&mut stdin, &mut stdout).await;
        send_json_line(
            &mut stdin,
            json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}}),
        )
        .await;
        let response = read_json_line(&mut stdout).await;
        let _ = child.kill().await;
        response["result"]["tools"]
            .as_array()
            .unwrap_or_else(|| panic!("tools/list must return a tools array, got: {response}"))
            .iter()
            .map(|tool| {
                tool["name"]
                    .as_str()
                    .unwrap_or_else(|| panic!("every advertised tool must be named: {tool}"))
                    .to_string()
            })
            .collect()
    }

    fn spawn(&self) -> Child {
        let mut cmd = tokio::process::Command::new(env!("CARGO_BIN_EXE_tddy-tools"));
        cmd.arg("--mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for key in ADVERTISEMENT_ENV_KEYS {
            cmd.env_remove(key);
        }
        for (key, value) in &self.env {
            cmd.env(key, value);
        }
        cmd.spawn().expect("spawn tddy-tools --mcp")
    }
}

/// A socket path nothing listens on. `detect_session_tool_transport` reads the variable and never
/// connects, so this is enough to give the session a transport for advertisement purposes.
fn unconnected_socket_path() -> String {
    tddy_sandbox::SandboxSpec::short_ipc_socket_path("mcpaudit")
        .to_str()
        .expect("socket path must be utf8")
        .to_string()
}

fn names(of: &[&str]) -> BTreeSet<String> {
    of.iter().map(|name| name.to_string()).collect()
}

// ─── The MCP stdio wire, exactly as Claude Code speaks it ─────────────────────

async fn send_json_line(stdin: &mut ChildStdin, message: Value) {
    let mut line = message.to_string();
    line.push('\n');
    tokio::time::timeout(IO_TIMEOUT, stdin.write_all(line.as_bytes()))
        .await
        .expect("write to tddy-tools stdin timed out")
        .expect("write to tddy-tools stdin");
}

async fn read_json_line(stdout: &mut BufReader<ChildStdout>) -> Value {
    let mut line = String::new();
    tokio::time::timeout(IO_TIMEOUT, stdout.read_line(&mut line))
        .await
        .expect("read from tddy-tools stdout timed out")
        .expect("read from tddy-tools stdout");
    serde_json::from_str(&line).unwrap_or_else(|e| panic!("invalid JSON-RPC line {line:?}: {e}"))
}

async fn initialize_mcp_session(stdin: &mut ChildStdin, stdout: &mut BufReader<ChildStdout>) {
    send_json_line(
        stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "tddy-mcp-advertisement-audit", "version": "0.0.1"}
            }
        }),
    )
    .await;
    read_json_line(stdout).await;
    send_json_line(
        stdin,
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
    )
    .await;
}

// ─── The audit ────────────────────────────────────────────────────────────────

/// The headline number of the `tddy-tools` thinning: with the tool bodies now living in six
/// crates, a session that has every optional surface still advertises the same 43 names it did
/// when they all lived in one.
#[tokio::test]
async fn advertises_all_forty_three_tool_names_where_the_host_serves_the_action_tools() {
    // Given
    let session = a_fully_configured_session().whose_host_serves_the_action_tools();

    // When
    let advertised = session.advertised_tool_names().await;

    // Then
    assert_eq!(
        advertised,
        names(&EVERY_ADVERTISED_TOOL),
        "the fully configured MCP surface must be exactly these 43 tools"
    );
}

/// The daemon path advertises 40, and the audit records that rather than reconciling it: the
/// daemon's tool handler has no arm for any of the three action tools, so advertising them there
/// was a false advertisement an agent could only discover by calling one.
#[tokio::test]
async fn advertises_forty_tool_names_on_the_daemon_path_which_serves_no_action_tool() {
    // Given
    let session = a_fully_configured_session();

    // When
    let advertised = session.advertised_tool_names().await;

    // Then
    let without_the_action_tools: BTreeSet<String> = names(&EVERY_ADVERTISED_TOOL)
        .difference(&names(&ACTION_TOOLS))
        .cloned()
        .collect();
    assert_eq!(
        advertised, without_the_action_tools,
        "the daemon path must advertise the same surface minus the three action tools"
    );
}

/// The whole of the difference between the two transports, asserted as a difference rather than as
/// two constants: a tool added to both paths leaves this test green, while one added to only the
/// claiming path fails it.
#[tokio::test]
async fn withholds_exactly_the_three_action_tools_where_the_host_does_not_claim_them() {
    // Given
    let claiming_host = a_fully_configured_session().whose_host_serves_the_action_tools();
    let silent_host = a_fully_configured_session();

    // When
    let with_the_claim = claiming_host.advertised_tool_names().await;
    let without_the_claim = silent_host.advertised_tool_names().await;

    // Then
    let withheld: BTreeSet<String> = with_the_claim
        .difference(&without_the_claim)
        .cloned()
        .collect();
    assert_eq!(
        withheld,
        names(&ACTION_TOOLS),
        "the host's claim must decide exactly the three action tools and nothing else"
    );
    assert!(
        without_the_claim.is_subset(&with_the_claim),
        "the claim may only add tools, never change or remove one; \
         got {without_the_claim:?} against {with_the_claim:?}"
    );
}
