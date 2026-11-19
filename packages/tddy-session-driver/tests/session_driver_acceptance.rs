//! Acceptance: `tddy-session-driver` runs one agent session on the target its arguments name and is
//! driven over MCP on its stdin/stdout.
//!
//! Each test spawns the compiled binary and speaks the real newline-delimited JSON-RPC wire. The
//! agent is `acme`, a tool file in the driver's tddy home whose binary is a stub script; a Coder
//! target is reached through a stand-in `coder` that runs the workspace's command on this machine.
//! No deployment is reached.
//!
//! Docs: packages/tddy-session-driver/docs/architecture.md.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tddy_testing_commons::remote_target::write_executable;

use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};

/// Bounds one read of the wire. Far above what a step needs: it only turns a hang into a failure.
const IO_TIMEOUT: Duration = Duration::from_secs(30);
/// The variable the `acme` tool file names for its binary.
const ACME_BINARY_VARIABLE: &str = "TDDY_ACME_BINARY";
const DEPLOYMENT_URL: &str = "https://coder.example.com";
const SESSION_TOKEN: &str = "adas-coder-session-token";

/// The `acme` coding agent's tool file: its binary is whatever [`ACME_BINARY_VARIABLE`] names.
const ACME_TOOL_FILE: &str = r#"schema_version: 1
id: acme
summary: a coding agent
binary:
  command: acme
  env: TDDY_ACME_BINARY
deps: []
run:
  transport: pty
  argv:
    - "{binary}"
extensions:
  coding_agent:
    mcp: { kind: file, flag: --mcp-config, format: claude-mcp-json }
    tool_allow: { flag: --allowedTools }
    tool_deny: { flag: --disallowedTools }
    permission_prompt: { flag: --permission-prompt-tool }
    instructions: { files: [CLAUDE.md] }
    hooks: { adapter: claude-settings-local }
    models: { default: acme-1, curated: [acme-1] }
"#;

/// An agent that announces itself, then answers every line it reads.
const AN_AGENT_THAT_ANSWERS: &str = "#!/bin/sh\n\
     printf 'ACME READY in %s\\n' \"$(/bin/pwd -P)\"\n\
     while IFS= read -r line; do printf 'acme heard: %s\\n' \"$line\"; done\n";

/// An agent that ignores the hang-up of a terminal that went away, as an agent on a remote
/// workspace never sees its client stop, and records it when it is told to terminate.
const AN_AGENT_THAT_OUTLIVES_ITS_CLIENT: &str = "#!/bin/sh\n\
     trap '' HUP\n\
     trap 'printf terminated > \"$(dirname \"$0\")/agent-ended\"; exit 0' TERM\n\
     printf 'ACME READY in %s\\n' \"$(/bin/pwd -P)\"\n\
     while :; do /bin/sleep 1; done\n";

/// An agent that refuses to run and says why.
const AN_AGENT_THAT_EXITS: &str = "#!/bin/sh\nprintf 'acme: no licence for this host\\n'\nexit 3\n";

/// A machine for one test: a tddy home holding the `acme` tool file, a worktree, the agent's stub,
/// and a stand-in `coder` whose every workspace is this machine.
struct AMachine {
    root: PathBuf,
    _dir: tempfile::TempDir,
}

impl AMachine {
    fn with_agent(agent_script: &str) -> Self {
        Self::on_workspace(agent_script, "exec ")
    }

    /// A machine whose stand-in `coder` runs the workspace's command as its own child instead of
    /// becoming it, so stopping the CLI here leaves the agent there running — as dropping a real
    /// `coder ssh` leaves the agent on the workspace.
    fn with_agent_outliving_its_client(agent_script: &str) -> Self {
        Self::on_workspace(agent_script, "")
    }

    /// The machine, its stand-in `coder` handing the workspace's command to a shell with `run`
    /// before it (`exec ` to become that shell, empty to wait on it as a child).
    fn on_workspace(agent_script: &str, run: &str) -> Self {
        let dir = tempfile::tempdir().expect("a directory for the machine");
        let root = std::fs::canonicalize(dir.path()).expect("it resolves");
        std::fs::create_dir_all(root.join("tddy-home/tools")).expect("the tools directory");
        std::fs::write(root.join("tddy-home/tools/acme.yaml"), ACME_TOOL_FILE)
            .expect("the acme tool file");
        std::fs::create_dir_all(root.join("worktree")).expect("the worktree");
        write_executable(&root.join("acme"), agent_script);
        // The workspace's login shell finds the agent on a `PATH` of its own, by its bare name.
        write_executable(
            &root.join("login-shell"),
            &format!(
                "#!/bin/sh\n[ \"$1\" = -lc ] || exit 64\nPATH='{}:/usr/bin:/bin'\nexport PATH\n\
                 exec /bin/sh -c \"$2\"\n",
                root.display()
            ),
        );
        write_executable(
            &root.join("coder"),
            &format!(
                "#!/bin/sh\n\
                 printf 'CODER_URL=%s CODER_SESSION_TOKEN=%s\\n' \"${{CODER_URL-}}\" \
                 \"${{CODER_SESSION_TOKEN-}}\" >> '{calls}'\n\
                 while [ \"$#\" -gt 0 ] && [ \"$1\" != \"--\" ]; do shift; done\n\
                 shift\n\
                 SHELL='{login}'\nexport SHELL\n\
                 {run}/bin/sh -c \"$*\"\n",
                calls = root.join("coder-calls").display(),
                login = root.join("login-shell").display()
            ),
        );
        Self { root, _dir: dir }
    }

    fn worktree(&self) -> PathBuf {
        self.root.join("worktree")
    }

    /// What the stand-in `coder` was given on each call, one line per call.
    fn coder_calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.root.join("coder-calls"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// The driver, started on `target` with `extra` arguments, its stdin and stdout piped.
    fn driver(&self, target: &str, extra: &[&str]) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_tddy-session-driver"));
        command
            .arg("--target")
            .arg(target)
            .arg("--worktree")
            .arg(self.worktree())
            .arg("--agent")
            .arg("acme")
            .arg("--tddy-home")
            .arg(self.root.join("tddy-home"))
            .arg("--coder")
            .arg(self.root.join("coder"))
            .args(extra)
            .env(ACME_BINARY_VARIABLE, self.root.join("acme"))
            .env("CODER_URL", DEPLOYMENT_URL)
            .env("CODER_SESSION_TOKEN", SESSION_TOKEN)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        command
    }
}

/// An MCP client on a driver's stdin/stdout.
struct ADriver {
    child: Child,
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
            child,
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
                    "clientInfo": { "name": "session-driver-acceptance", "version": "0" },
                }),
            )
            .await;
        assert!(
            initialized.get("result").is_some(),
            "the driver answers initialize: {initialized}"
        );
        driver
            .send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
            .await;
        driver
    }

    async fn send(&mut self, message: Value) {
        let mut line = message.to_string();
        line.push('\n');
        tokio::time::timeout(IO_TIMEOUT, self.stdin.write_all(line.as_bytes()))
            .await
            .expect("writing to the driver timed out")
            .expect("the driver reads its stdin");
    }

    /// One JSON-RPC request and its response. Every line the driver writes must be a JSON-RPC
    /// frame: anything else on stdout fails here.
    async fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
            .await;
        loop {
            let mut line = String::new();
            let read = tokio::time::timeout(IO_TIMEOUT, self.stdout.read_line(&mut line))
                .await
                .expect("the driver answered within the timeout")
                .expect("the driver's stdout is readable");
            assert_ne!(
                read, 0,
                "the driver closed its stdout before answering {method}"
            );
            let frame: Value = serde_json::from_str(&line).unwrap_or_else(|e| {
                panic!("the driver wrote a line that is not JSON-RPC: {line:?}: {e}")
            });
            if frame.get("id") == Some(&json!(id)) {
                return frame;
            }
        }
    }

    /// `tools/call` of `tool` with `arguments`: its text and whether it is an error.
    async fn call(&mut self, tool: &str, arguments: Value) -> (String, bool) {
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
        (text, result["isError"].as_bool().unwrap_or(false))
    }

    /// `tools/call` of `tool`, which must succeed, its text parsed as JSON.
    async fn answer(&mut self, tool: &str, arguments: Value) -> Value {
        let (text, is_error) = self.call(tool, arguments).await;
        assert!(!is_error, "{tool} failed: {text}");
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{tool} answered {text:?}: {e}"))
    }

    async fn tool_names(&mut self) -> Vec<String> {
        let listed = self.request("tools/list", json!({})).await;
        let mut names: Vec<String> = listed["result"]["tools"]
            .as_array()
            .expect("a tool list")
            .iter()
            .filter_map(|tool| tool["name"].as_str().map(str::to_string))
            .collect();
        names.sort();
        names
    }
}

/// Starts the session, waits for the agent's banner, has it answer a line, then stops it, checking
/// each step's answer.
async fn drive_a_conversation(driver: &mut ADriver, worktree: &Path) {
    let started = driver.answer("start_session", json!({})).await;
    assert_eq!(started["state"], json!("running"), "{started}");

    let banner = format!("ACME READY in {}", worktree.display());
    let ready = driver
        .answer(
            "wait_for_text",
            json!({ "text": banner, "timeout_secs": 20 }),
        )
        .await;
    assert_eq!(ready["matched"], json!(true), "{ready}");

    let sent = driver
        .answer(
            "send_keys",
            json!({ "text": "hello there", "keys": ["Enter"] }),
        )
        .await;
    assert_eq!(sent["sent_bytes"], json!("hello there".len() + 1));
    let heard = driver
        .answer(
            "wait_for_text",
            json!({ "text": "acme heard: hello there", "timeout_secs": 20 }),
        )
        .await;
    assert_eq!(heard["matched"], json!(true), "{heard}");

    let screen = driver.answer("read_terminal", json!({})).await;
    let lines: Vec<String> = screen["lines"]
        .as_array()
        .expect("the screen's lines")
        .iter()
        .filter_map(|line| line.as_str().map(str::to_string))
        .collect();
    assert!(
        lines.iter().any(|line| line == "acme heard: hello there"),
        "the screen shows the agent's answer: {lines:?}"
    );
    assert!(screen["cols"].as_u64().unwrap_or(0) > 0, "{screen}");

    let stopped = driver.answer("stop_session", json!({})).await;
    assert_eq!(stopped["state"], json!("exited"), "{stopped}");
    let status = driver.answer("session_status", json!({})).await;
    assert_eq!(status["state"], json!("exited"), "{status}");
}

#[tokio::test]
async fn the_driver_offers_the_session_tools() {
    // Given a driver on a local target
    let machine = AMachine::with_agent(AN_AGENT_THAT_ANSWERS);
    let mut driver = ADriver::started(machine.driver("local", &[])).await;

    // When its tools are listed
    let names = driver.tool_names().await;

    // Then they are the six session tools
    assert_eq!(
        names,
        [
            "read_terminal",
            "send_keys",
            "session_status",
            "start_session",
            "stop_session",
            "wait_for_text",
        ]
        .map(str::to_string)
        .to_vec()
    );
    // And no session runs until one is started
    let status = driver.answer("session_status", json!({})).await;
    assert_eq!(status["state"], json!("not_started"), "{status}");
}

#[tokio::test]
async fn a_local_session_is_started_read_typed_into_and_stopped_over_mcp() {
    // Given a driver on a local target whose agent answers what it reads
    let machine = AMachine::with_agent(AN_AGENT_THAT_ANSWERS);
    let mut driver = ADriver::started(machine.driver("local", &[])).await;

    // When / Then a conversation is driven through the session's terminal
    drive_a_conversation(&mut driver, &machine.worktree()).await;
}

#[tokio::test]
async fn a_coder_session_is_driven_through_the_workspace_as_the_cli_s_own_account() {
    // Given a driver on a Coder target, given the deployment and token in the CLI's own variables
    let machine = AMachine::with_agent(AN_AGENT_THAT_ANSWERS);
    let mut driver = ADriver::started(machine.driver("coder:ada/devbox.main", &[])).await;

    // When / Then a conversation is driven through the session's terminal on the workspace
    drive_a_conversation(&mut driver, &machine.worktree()).await;
    // And every call of the CLI was given that deployment and token
    let calls = machine.coder_calls();
    assert!(!calls.is_empty(), "the stand-in coder was used");
    assert_eq!(
        calls
            .iter()
            .filter(|call| *call
                != &format!("CODER_URL={DEPLOYMENT_URL} CODER_SESSION_TOKEN={SESSION_TOKEN}"))
            .collect::<Vec<_>>(),
        Vec::<&String>::new()
    );
}

#[tokio::test]
async fn a_coder_target_without_a_session_token_is_refused_before_anything_runs() {
    // Given a driver on a Coder target with no session token in its environment
    let machine = AMachine::with_agent(AN_AGENT_THAT_ANSWERS);
    let mut command = machine.driver("coder:ada/devbox", &[]);
    command.env_remove("CODER_SESSION_TOKEN");

    // When it is started
    let mut child = command.spawn().expect("the driver starts");
    let status = tokio::time::timeout(IO_TIMEOUT, child.wait())
        .await
        .expect("the driver exits")
        .expect("its status");
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .expect("its stderr")
        .read_to_string(&mut stderr)
        .await
        .expect("its stderr is readable");

    // Then it exits non-zero naming the variable, and the CLI was never run
    assert!(!status.success(), "the driver refused the target: {status}");
    assert!(stderr.contains("CODER_SESSION_TOKEN"), "{stderr}");
    assert_eq!(machine.coder_calls(), Vec::<String>::new());
}

#[tokio::test]
async fn an_agent_that_exits_as_it_starts_fails_start_session_with_its_last_words() {
    // Given a driver on a local target whose agent refuses to run
    let machine = AMachine::with_agent(AN_AGENT_THAT_EXITS);
    let mut driver = ADriver::started(machine.driver("local", &[])).await;

    // When the session is started
    let (text, is_error) = driver.call("start_session", json!({})).await;

    // Then the start fails with what the agent said
    assert!(is_error, "the start failed: {text}");
    assert!(text.contains("acme: no licence for this host"), "{text}");
    let status = driver.answer("session_status", json!({})).await;
    assert_eq!(status["state"], json!("exited"), "{status}");
}

#[tokio::test]
async fn with_start_the_session_runs_before_the_first_call() {
    // Given a driver started with --start on a local target
    let machine = AMachine::with_agent(AN_AGENT_THAT_ANSWERS);
    let mut driver = ADriver::started(machine.driver("local", &["--start"])).await;

    // When its status is asked for at once
    let status = driver.answer("session_status", json!({})).await;

    // Then the session is already running, and a second start is refused
    assert_eq!(status["state"], json!("running"), "{status}");
    let (text, is_error) = driver.call("start_session", json!({})).await;
    assert!(is_error, "a second start is refused: {text}");
    let _ = driver.answer("stop_session", json!({})).await;
    let _ = driver.child.start_kill();
}

#[tokio::test]
async fn stopping_a_coder_session_stops_its_agent_on_the_workspace() {
    // Given a driver on a Coder target whose agent outlives the CLI that carries its terminal, as an
    // agent on a real workspace does, and records how it is ended
    let machine = AMachine::with_agent_outliving_its_client(AN_AGENT_THAT_OUTLIVES_ITS_CLIENT);
    let mut driver = ADriver::started(machine.driver("coder:ada/devbox", &[])).await;
    let _ = driver.answer("start_session", json!({})).await;
    let ready = driver
        .answer(
            "wait_for_text",
            json!({ "text": "ACME READY", "timeout_secs": 20 }),
        )
        .await;
    assert_eq!(ready["matched"], json!(true), "{ready}");

    // When the session is stopped
    let stopped = driver.answer("stop_session", json!({})).await;

    // Then the agent on the workspace was told to terminate, by the pid its launch recorded there,
    // and the session reads as exited
    let ended = machine.root.join("agent-ended");
    let deadline = std::time::Instant::now() + IO_TIMEOUT;
    while !ended.exists() && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let how = std::fs::read_to_string(&ended).unwrap_or_default();
    // An agent the stop missed keeps running; end it so the suite leaves nothing behind.
    if how.is_empty() {
        if let Ok(pid) = std::fs::read_to_string(machine.worktree().join(".tddy/agent.pid")) {
            let _ = std::process::Command::new("kill")
                .arg("-9")
                .arg(pid.trim())
                .status();
        }
    }
    assert_eq!(how, "terminated");
    assert_eq!(stopped["state"], json!("exited"), "{stopped}");
}
