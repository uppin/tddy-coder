//! Fixtures the driver's acceptance suites share: a machine standing for this host and a Coder
//! workspace (a tddy home holding the `acme` tool files, a worktree, the agent's stub and a stand-in
//! `coder`), and an MCP client on a driver's stdin/stdout.

#![allow(dead_code)]

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use serde_json::{json, Value};
use tddy_testing_commons::remote_target::write_executable;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};

/// Bounds one read of the wire. Far above what a step needs: it only turns a hang into a failure.
pub const IO_TIMEOUT: Duration = Duration::from_secs(30);
/// The variable the `acme` tool file names for its binary.
pub const ACME_BINARY_VARIABLE: &str = "TDDY_ACME_BINARY";
pub const DEPLOYMENT_URL: &str = "https://coder.example.com";
pub const SESSION_TOKEN: &str = "adas-coder-session-token";

/// The `acme` coding agent's tool file: its binary is whatever [`ACME_BINARY_VARIABLE`] names.
pub const ACME_TOOL_FILE: &str = r#"schema_version: 1
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

/// `acme-hooked`: the `acme` agent with the `hooks` capability, so a start wires its lifecycle hooks
/// into the session's own hooks file in the worktree, as a daemon start does for Claude Code.
pub const ACME_HOOKED_TOOL_FILE: &str = r#"schema_version: 1
id: acme-hooked
summary: a coding agent with lifecycle hooks
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
    capabilities: [hooks]
    models: { default: acme-1, curated: [acme-1] }
"#;

/// A machine for one test: a tddy home holding the `acme` tool file, a worktree, the agent's stub,
/// and a stand-in `coder` whose every workspace is this machine.
pub struct AMachine {
    pub root: PathBuf,
    _dir: tempfile::TempDir,
}

impl AMachine {
    pub fn with_agent(agent_script: &str) -> Self {
        Self::on_workspace(agent_script, "exec ", "")
    }

    /// A machine whose stand-in `coder` runs the workspace's command as its own child instead of
    /// becoming it, so stopping the CLI here leaves the agent there running — as dropping a real
    /// `coder ssh` leaves the agent on the workspace.
    pub fn with_agent_outliving_its_client(agent_script: &str) -> Self {
        Self::on_workspace(agent_script, "", "")
    }

    /// A machine whose stand-in `coder` ends as the real CLI does when the workspace's command
    /// failed: it says the command's status last, `error: run command: Process exited with status
    /// <N>`, and exits 1 itself, whatever that status was.
    pub fn with_agent_behind_a_cli_that_exits_1_on_failure(agent_script: &str) -> Self {
        Self::on_workspace(
            agent_script,
            "",
            "status=$?\n\
             if [ \"$status\" -ne 0 ]; then\n\
               printf 'Encountered an error running \"coder ssh\", see \"coder ssh --help\" for more information\\r\\n'\n\
               printf 'error: run command: Process exited with status %s\\r\\n' \"$status\"\n\
               exit 1\n\
             fi\n",
        )
    }

    /// The machine, its stand-in `coder` handing the workspace's command to a shell with `run`
    /// before it (`exec ` to become that shell, empty to wait on it as a child), then running
    /// `after` once a waited-on shell has ended.
    pub fn on_workspace(agent_script: &str, run: &str, after: &str) -> Self {
        let dir = tempfile::tempdir().expect("a directory for the machine");
        let root = std::fs::canonicalize(dir.path()).expect("it resolves");
        std::fs::create_dir_all(root.join("tddy-home/tools")).expect("the tools directory");
        std::fs::write(root.join("tddy-home/tools/acme.yaml"), ACME_TOOL_FILE)
            .expect("the acme tool file");
        std::fs::write(
            root.join("tddy-home/tools/acme-hooked.yaml"),
            ACME_HOOKED_TOOL_FILE,
        )
        .expect("the acme-hooked tool file");
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
                 {run}/bin/sh -c \"$*\"\n\
                 {after}",
                calls = root.join("coder-calls").display(),
                login = root.join("login-shell").display()
            ),
        );
        // `CoderShell::exec_streamed` runs OpenSSH with `coder ssh --stdio` as its ProxyCommand: the
        // stand-in `ssh` (first on the driver's `PATH`) checks it was given that proxy, records the
        // call, and runs the command on this machine, as the proxied workspace would — as a child it
        // waits on, so killing the client leaves the command running, as a real ssh session does.
        std::fs::create_dir_all(root.join("bin")).expect("the stand-in ssh's directory");
        write_executable(
            &root.join("bin/ssh"),
            &format!(
                "#!/bin/sh\n\
                 proxied=no\n\
                 for arg; do case \"$arg\" in ProxyCommand=*' ssh --stdio '*) proxied=yes;; esac; done\n\
                 for command; do :; done\n\
                 printf '%s %s\\n' \"$proxied\" \"$command\" >> '{calls}'\n\
                 [ \"$proxied\" = yes ] || exit 255\n\
                 /bin/sh -c \"$command\" &\n\
                 wait \"$!\"\n",
                calls = root.join("ssh-calls").display()
            ),
        );
        Self { root, _dir: dir }
    }

    /// The processes on this machine still following the session's hook log.
    pub fn hook_log_followers(&self) -> Vec<String> {
        let log = self.hook_log().to_string_lossy().into_owned();
        let listed = std::process::Command::new("ps")
            .args(["-eo", "pid=,args="])
            .output()
            .expect("ps lists the processes");
        String::from_utf8_lossy(&listed.stdout)
            .lines()
            .filter(|line| line.contains("tail ") && line.contains(&log))
            .map(|line| line.trim().to_string())
            .collect()
    }

    /// Whether every follower of the hook log has ended, waiting up to `within`.
    pub async fn hook_log_followers_end_within(&self, within: Duration) -> Vec<String> {
        let deadline = std::time::Instant::now() + within;
        loop {
            let left = self.hook_log_followers();
            if left.is_empty() || std::time::Instant::now() >= deadline {
                return left;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    /// What the stand-in `ssh` was asked to run, one `<proxied through the CLI: yes|no> <command>`
    /// line per call.
    pub fn ssh_calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.root.join("ssh-calls"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    pub fn worktree(&self) -> PathBuf {
        self.root.join("worktree")
    }

    /// Where the session's hook log lives in the worktree.
    pub fn hook_log(&self) -> PathBuf {
        self.worktree().join(".tddy/hooks.log")
    }

    /// The command the launch installed for `event`, read from the session's hooks file in the
    /// worktree (`.tddy/hooks/<session_id>/claude-settings.json`, one session runs here) as Claude
    /// Code reads it; `None` when the file or the event's command is missing.
    pub fn installed_hook(&self, event: &str) -> Option<String> {
        let session_dir = std::fs::read_dir(self.worktree().join(".tddy/hooks"))
            .ok()?
            .filter_map(Result::ok)
            .find(|entry| entry.path().is_dir())?;
        let settings: Value = serde_json::from_slice(
            &std::fs::read(session_dir.path().join("claude-settings.json")).ok()?,
        )
        .ok()?;
        settings["hooks"][event][0]["hooks"][0]["command"]
            .as_str()
            .map(str::to_string)
    }

    /// Fire `event` as Claude Code fires a hook: its installed command under `sh -c`, in the
    /// worktree the agent runs in, with `payload` on its stdin.
    pub fn fire_hook(&self, event: &str, payload: &Value) {
        self.fire_hook_printing(event, payload);
    }

    /// [`Self::fire_hook`], returning what the hook printed once it ended: the decision Claude Code
    /// would read from a hook that waits for an answer.
    pub fn fire_hook_printing(&self, event: &str, payload: &Value) -> String {
        use std::io::Write as _;
        let command = self
            .installed_hook(event)
            .unwrap_or_else(|| panic!("the launch installed no {event} hook"));
        let mut hook = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(&command)
            .current_dir(self.worktree())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("the hook command runs");
        hook.stdin
            .take()
            .expect("the hook's stdin")
            .write_all(payload.to_string().as_bytes())
            .expect("the payload is written");
        let output = hook.wait_with_output().expect("the hook command ends");
        assert!(
            output.status.success(),
            "the {event} hook exited {}",
            output.status
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// Where the launch of `session_id` records its agent's pid in the worktree.
    pub fn agent_pid_file(&self, session_id: &str) -> PathBuf {
        self.worktree()
            .join(".tddy/agents")
            .join(format!("{session_id}.pid"))
    }

    /// The pid the launch of `session_id` recorded, once it has.
    pub async fn recorded_agent_pid(&self, session_id: &str) -> String {
        let file = self.agent_pid_file(session_id);
        let deadline = std::time::Instant::now() + IO_TIMEOUT;
        loop {
            if let Ok(pid) = std::fs::read_to_string(&file) {
                if !pid.trim().is_empty() {
                    return pid.trim().to_string();
                }
            }
            assert!(
                std::time::Instant::now() < deadline,
                "{} was never written",
                file.display()
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Whether the agent under `pid` recorded being told to terminate, waiting up to `within`.
    pub async fn agent_ended(&self, pid: &str, within: Duration) -> bool {
        let record = self.root.join(format!("agent-ended-{pid}"));
        let deadline = std::time::Instant::now() + within;
        while !record.exists() && std::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        record.exists()
    }

    /// What the stand-in `coder` was given on each call, one line per call.
    pub fn coder_calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.root.join("coder-calls"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// The driver, started on `target` with `extra` arguments, its stdin and stdout piped.
    pub fn driver(&self, target: &str, extra: &[&str]) -> tokio::process::Command {
        self.driver_of("acme", target, extra)
    }

    /// The driver of `agent` (`acme` or `acme-hooked`), started on `target` with `extra` arguments.
    pub fn driver_of(&self, agent: &str, target: &str, extra: &[&str]) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_tddy-session-driver"));
        command
            .arg("--target")
            .arg(target)
            .arg("--worktree")
            .arg(self.worktree())
            .arg("--agent")
            .arg(agent)
            .arg("--tddy-home")
            .arg(self.root.join("tddy-home"))
            .arg("--coder")
            .arg(self.root.join("coder"))
            .args(extra)
            .env(ACME_BINARY_VARIABLE, self.root.join("acme"))
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    self.root.join("bin").display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            )
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
pub struct ADriver {
    pub child: Child,
    pub stdin: ChildStdin,
    pub stdout: BufReader<ChildStdout>,
    pub next_id: u64,
}

impl ADriver {
    pub async fn started(mut command: tokio::process::Command) -> Self {
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

    pub async fn send(&mut self, message: Value) {
        let mut line = message.to_string();
        line.push('\n');
        tokio::time::timeout(IO_TIMEOUT, self.stdin.write_all(line.as_bytes()))
            .await
            .expect("writing to the driver timed out")
            .expect("the driver reads its stdin");
    }

    /// One JSON-RPC request and its response. Every line the driver writes must be a JSON-RPC
    /// frame: anything else on stdout fails here.
    pub async fn request(&mut self, method: &str, params: Value) -> Value {
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
    pub async fn call(&mut self, tool: &str, arguments: Value) -> (String, bool) {
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
    pub async fn answer(&mut self, tool: &str, arguments: Value) -> Value {
        let (text, is_error) = self.call(tool, arguments).await;
        assert!(!is_error, "{tool} failed: {text}");
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{tool} answered {text:?}: {e}"))
    }

    pub async fn tool_names(&mut self) -> Vec<String> {
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
