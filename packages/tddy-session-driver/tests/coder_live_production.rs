//! Production test: the Coder path, run for real against a Coder deployment.
//!
//! Every other test of the Coder target runs a stand-in `coder`. This one runs the real CLI
//! against a real workspace, to check the facts the stand-ins assume: what `coder version`
//! prints, what status a running workspace reports, that Claude and Cursor start there and reach
//! their prompts. (The hook log is created by the daemon's start path, not by this driver, so it
//! is not checked here.)
//!
//! Needs `TDDY_CODER_LIVE_WORKSPACE` (the workspace, `<workspace>` or `<owner>/<workspace>`),
//! `CODER_URL` and `CODER_SESSION_TOKEN`, exported or in the environment of the caller. A missing
//! variable fails the test; it never skips. The token is read from the environment only and
//! reaches the `coder` children by inheritance: it is never put on an argument list or printed.
//!
//! Run: `./test -p tddy-session-driver -- --ignored coder_live`
//!
//! Docs: docs/dev/guides/testing.md § Production Tests.

use std::process::Stdio;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};

const WORKSPACE_VARIABLE: &str = "TDDY_CODER_LIVE_WORKSPACE";
const DEPLOYMENT_VARIABLE: &str = "CODER_URL";
const TOKEN_VARIABLE: &str = "CODER_SESSION_TOKEN";
/// Bounds one read of the wire; a real agent takes a while to start.
const IO_TIMEOUT: Duration = Duration::from_secs(120);
const PROMPT_TIMEOUT_SECS: u64 = 90;

/// The value of a variable this test needs; a missing one fails naming it.
fn required(name: &str) -> String {
    match std::env::var(name) {
        Ok(value) if !value.is_empty() => value,
        _ => panic!("{name} is not set: this production test needs it (it never skips)"),
    }
}

/// The real workspace the test runs against, reached through the real `coder` CLI.
struct ALiveWorkspace {
    /// `<owner>/<workspace>`, the form the driver's `coder:` target takes.
    address: String,
}

impl ALiveWorkspace {
    fn from_the_environment() -> Self {
        required(DEPLOYMENT_VARIABLE);
        required(TOKEN_VARIABLE);
        let named = required(WORKSPACE_VARIABLE);
        let address = if named.contains('/') {
            named
        } else {
            format!("me/{named}")
        };
        Self { address }
    }

    fn coder(&self) -> tokio::process::Command {
        // The deployment and token are inherited from this process's environment.
        let mut command = tokio::process::Command::new("coder");
        command.stdin(Stdio::null()).kill_on_drop(true);
        command
    }

    /// What `coder <args>` printed on stdout; a failure fails with its stderr (never the token).
    async fn coder_output(&self, args: &[&str]) -> String {
        let output = self
            .coder()
            .args(args)
            .output()
            .await
            .expect("the coder CLI starts");
        assert!(
            output.status.success(),
            "coder {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// Runs `script` on the workspace and returns its stdout.
    async fn run(&self, script: &str) -> String {
        self.coder_output(&["ssh", "--disable-autostart", &self.address, "--", script])
            .await
    }

    /// A fresh directory on the workspace, for a session's worktree.
    async fn scratch_worktree(&self) -> String {
        self.run("mktemp -d").await.trim().to_string()
    }

    /// A small clone on the workspace with a session worktree at `<clone>/.worktrees/<id>`: the
    /// layout a target checkout uses, from which a Claude start derives the clone to pre-trust.
    /// Returns `(clone root, worktree)`.
    async fn cloned_worktree(&self, session_id: &str) -> (String, String) {
        let root = self.scratch_worktree().await;
        let worktree = format!("{root}/.worktrees/{session_id}");
        self.run(&format!(
            "cd {root} && git init -q && git -c user.name=t -c user.email=t@example.com \
             commit -q --allow-empty -m init && git worktree add -q {worktree} -b live-{session_id}"
        ))
        .await;
        (root, worktree)
    }

    fn driver(
        &self,
        agent: &str,
        worktree: &str,
        session_id: Option<&str>,
    ) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_tddy-session-driver"));
        command
            .arg("--target")
            .arg(format!("coder:{}", self.address))
            .arg("--worktree")
            .arg(worktree)
            .arg("--agent")
            .arg(agent)
            .args(
                session_id
                    .map(|id| ["--session-id", id])
                    .into_iter()
                    .flatten(),
            )
            .arg("--startup-window-secs")
            .arg("10")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true);
        command
    }
}

/// An MCP client on a driver's stdin/stdout.
struct ADriver {
    _child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl ADriver {
    async fn started(mut command: tokio::process::Command) -> Self {
        let mut child = command.spawn().expect("the driver starts");
        let stdin = child.stdin.take().expect("its stdin");
        let stdout = BufReader::new(child.stdout.take().expect("its stdout"));
        let mut driver = Self {
            _child: child,
            stdin,
            stdout,
            next_id: 1,
        };
        let initialized = driver
            .request(
                "initialize",
                json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": { "name": "coder-live-production", "version": "0" },
                }),
            )
            .await;
        assert!(initialized.get("result").is_some(), "{initialized}");
        driver
            .send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
            .await;
        driver
    }

    async fn send(&mut self, message: Value) {
        let mut line = message.to_string();
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .await
            .expect("the driver reads its stdin");
    }

    async fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
            .await;
        loop {
            let mut line = String::new();
            let read = tokio::time::timeout(IO_TIMEOUT, self.stdout.read_line(&mut line))
                .await
                .unwrap_or_else(|_| panic!("the driver did not answer {method} in time"))
                .expect("the driver's stdout is readable");
            assert_ne!(
                read, 0,
                "the driver closed its stdout before answering {method}"
            );
            let frame: Value = serde_json::from_str(&line)
                .unwrap_or_else(|e| panic!("not JSON-RPC: {line:?}: {e}"));
            if frame.get("id") == Some(&json!(id)) {
                return frame;
            }
        }
    }

    /// `tools/call` of `tool`, which must succeed, its text parsed as JSON.
    async fn answer(&mut self, tool: &str, arguments: Value) -> Value {
        let response = self
            .request(
                "tools/call",
                json!({ "name": tool, "arguments": arguments }),
            )
            .await;
        let result = response
            .get("result")
            .unwrap_or_else(|| panic!("{tool} answered no result: {response}"));
        let text = result["content"]
            .as_array()
            .map(|content| {
                content
                    .iter()
                    .filter_map(|item| item["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        assert!(
            !result["isError"].as_bool().unwrap_or(false),
            "{tool} failed: {text}"
        );
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{tool} answered {text:?}: {e}"))
    }

    async fn reaches(&mut self, prompt_text: &str) {
        let waited = self
            .answer(
                "wait_for_text",
                json!({ "text": prompt_text, "timeout_secs": PROMPT_TIMEOUT_SECS }),
            )
            .await;
        assert_eq!(
            waited["matched"],
            json!(true),
            "never saw {prompt_text:?}: {waited}"
        );
    }
}

#[tokio::test]
#[ignore = "production test: needs a Coder deployment, a session token and a running workspace; run with --ignored"]
async fn the_deployment_reads_as_the_stand_ins_assume() {
    // Given the live workspace
    let workspace = ALiveWorkspace::from_the_environment();

    // When the CLI is asked its version and the deployment its workspaces
    let version = workspace.coder_output(&["version"]).await;
    let listed = workspace.coder_output(&["list", "--output", "json"]).await;

    // Then the version's first line is `Coder <version>[+build] …`, as the host probe parses it
    let first = version.lines().next().unwrap_or_default();
    let mut words = first.split_whitespace();
    assert_eq!(
        words.next(),
        Some("Coder"),
        "coder version printed {first:?}"
    );
    assert!(
        words.next().is_some_and(|v| v.starts_with('v')),
        "{first:?}"
    );
    // And the workspace is reported with the status the stand-ins assume a runnable one has
    let workspaces: Value = serde_json::from_str(&listed).expect("coder list -o json is JSON");
    let name = workspace.address.rsplit('/').next().unwrap_or_default();
    let status = workspaces
        .as_array()
        .and_then(|all| all.iter().find(|w| w["name"] == json!(name)))
        .map(|w| w["latest_build"]["status"].clone());
    assert_eq!(status, Some(json!("running")), "workspace {name}");
}

#[tokio::test]
#[ignore = "production test: needs a Coder deployment, a session token and a running workspace; run with --ignored"]
async fn a_claude_session_on_a_cloned_worktree_reaches_its_prompt_without_the_trust_dialog() {
    // Given the live workspace and a session worktree laid out in a clone on it, so the start
    // pre-trusts the clone
    let workspace = ALiveWorkspace::from_the_environment();
    // Claude takes the id as `--session-id`, which it requires to be a UUID.
    let session_id = workspace
        .run("cat /proc/sys/kernel/random/uuid")
        .await
        .trim()
        .to_string();
    let (clone, worktree) = workspace.cloned_worktree(&session_id).await;
    let mut driver =
        ADriver::started(workspace.driver("claude", &worktree, Some(&session_id))).await;

    // When Claude is started there
    let started = driver.answer("start_session", json!({})).await;
    assert_eq!(started["state"], json!("running"), "{started}");

    // Then it reaches its prompt without the folder-trust prompt in the way
    // (the prompt box shows its `❯ Try "…"` hint; `for shortcuts` is not on screen under a status
    // line, which the live run showed)
    driver.reaches("❯ Try").await;
    driver.answer("stop_session", json!({})).await;
    workspace.run(&format!("rm -rf {clone}")).await;
}

#[tokio::test]
#[ignore = "production test: needs a Coder deployment, a session token and a running workspace; run with --ignored"]
async fn a_cursor_session_on_the_workspace_mints_its_chat_and_reaches_its_prompt() {
    // Given the live workspace and an empty worktree on it
    let workspace = ALiveWorkspace::from_the_environment();
    let worktree = workspace.scratch_worktree().await;
    let mut driver = ADriver::started(workspace.driver("cursor", &worktree, None)).await;

    // When Cursor is started there
    let started = driver.answer("start_session", json!({})).await;
    assert_eq!(started["state"], json!("running"), "{started}");

    // Then it reaches its prompt
    driver.reaches("Cursor Agent").await;
    driver.answer("stop_session", json!({})).await;
    workspace.run(&format!("rm -rf {worktree}")).await;
}
