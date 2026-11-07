//! Unit tests: the split agent's PTY starts from the allow-listed environment (`#agent-tools 28/28`).
//!
//! The daemon's environment is this test process's. The agent is a shell script that writes the
//! environment it was started with to a file.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use tddy_cli_sessions::cli_session_manager::CliSessionManager;

use super::{spawn_split_agent_pty, SplitAgentPty};

/// How long the agent has to write its environment file.
const ENV_FILE_DEADLINE: Duration = Duration::from_secs(15);

const SESSION_MARKER: &str = "CLAUDE_CODE_CHILD_SESSION";
const INHERITED_TDDY: &str = "TDDY_ALLOW_LIST_PROBE";
/// A provider key the daemon holds that the `claude` spec does not declare.
const UNDECLARED_KEY: &str = "OTHER_VENDOR_API_KEY";

fn a_daemon_holding_a_marker_tddy_config_and_an_undeclared_key() {
    static PLANTED: std::sync::Once = std::sync::Once::new();
    PLANTED.call_once(|| {
        std::env::set_var(SESSION_MARKER, "1");
        std::env::set_var(INHERITED_TDDY, "/etc/tddy/probe");
        std::env::set_var(UNDECLARED_KEY, "sk-other");
    });
}

/// A stub agent that writes its environment to `env.txt` and then waits on its terminal.
struct AnEnvRecordingAgent {
    dir: tempfile::TempDir,
}

fn an_env_recording_agent() -> AnEnvRecordingAgent {
    let dir = tempfile::tempdir().expect("agent dir");
    let program = dir.path().join("claude");
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\n/usr/bin/env > '{}.tmp' && mv '{0}.tmp' '{0}'\nexec cat\n",
            dir.path().join("env.txt").display()
        ),
    )
    .expect("write agent");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    AnEnvRecordingAgent { dir }
}

impl AnEnvRecordingAgent {
    fn program(&self) -> PathBuf {
        self.dir.path().join("claude")
    }

    async fn environment(&self) -> BTreeMap<String, String> {
        let file = self.dir.path().join("env.txt");
        let started = Instant::now();
        while !file.exists() {
            assert!(
                started.elapsed() < ENV_FILE_DEADLINE,
                "the agent wrote no environment within {ENV_FILE_DEADLINE:?}"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        std::fs::read_to_string(&file)
            .expect("read env")
            .lines()
            .filter_map(|line| line.split_once('='))
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect()
    }
}

#[tokio::test]
async fn the_split_agents_pty_sees_no_unlisted_daemon_variable() {
    // Given — the split agent's spec, and a daemon holding what no agent may see
    a_daemon_holding_a_marker_tddy_config_and_an_undeclared_key();
    let spec = tddy_coding_agent_specs::embedded_spec("claude").expect("claude is built in");
    let stub = an_env_recording_agent();
    let context_dir = tempfile::tempdir().expect("context dir");
    let manager = CliSessionManager::new();

    // When
    let _handle = spawn_split_agent_pty(
        &manager,
        &spec,
        &tddy_coding_agent_specs::AgentEnvBase::builtin(),
        SplitAgentPty {
            session_id: "019d105b-ac0f-78d3-9a89-409731145a60",
            session_type: "claude-cli",
            context_dir: context_dir.path().to_path_buf(),
            model: "opus",
            argv: vec![stub.program().to_string_lossy().into_owned()],
            os_user: None,
            on_exit: None,
        },
    )
    .await
    .expect("the split agent is spawned");
    let env = stub.environment().await;

    // Then
    for dropped in [SESSION_MARKER, INHERITED_TDDY, UNDECLARED_KEY] {
        assert!(
            !env.contains_key(dropped),
            "{dropped} reached the split agent: {:?}",
            env.keys().collect::<Vec<_>>()
        );
    }
    assert!(env.contains_key("PATH"), "PATH is a base variable");
}
