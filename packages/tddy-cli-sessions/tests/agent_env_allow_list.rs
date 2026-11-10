//! An agent main spawned through `spawn_rendered_main` starts from the allow-listed environment
//! (`#agent-tools 28/28`), and a user's terminal still inherits the daemon's.
//!
//! The daemon's environment is this test process's. Each child is a shell script that writes the
//! environment it was started with to a file.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use tddy_cli_sessions::cli_session_manager::CliSessionManager;

const ENV_FILE_DEADLINE: Duration = Duration::from_secs(15);

const SESSION_MARKER: &str = "CLAUDE_CODE_CHILD_SESSION";
const INHERITED_TDDY: &str = "TDDY_ALLOW_LIST_PROBE";
/// What the daemon computed for the agent on purpose (a `pass_env` value, a route key).
const LAUNCH_SET: &str = "ROUTE_BASE_URL";

fn a_daemon_holding_a_marker_and_tddy_config() {
    static PLANTED: std::sync::Once = std::sync::Once::new();
    PLANTED.call_once(|| {
        std::env::set_var(SESSION_MARKER, "1");
        std::env::set_var(INHERITED_TDDY, "/etc/tddy/probe");
    });
}

struct AnEnvRecordingChild {
    dir: tempfile::TempDir,
}

fn an_env_recording_child() -> AnEnvRecordingChild {
    let dir = tempfile::tempdir().expect("child dir");
    let program = dir.path().join("child");
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\n/usr/bin/env > '{}.tmp' && mv '{0}.tmp' '{0}'\nexec cat\n",
            dir.path().join("env.txt").display()
        ),
    )
    .expect("write child");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    AnEnvRecordingChild { dir }
}

impl AnEnvRecordingChild {
    fn program(&self) -> PathBuf {
        self.dir.path().join("child")
    }

    async fn environment(&self) -> BTreeMap<String, String> {
        let file = self.dir.path().join("env.txt");
        let started = Instant::now();
        while !file.exists() {
            assert!(
                started.elapsed() < ENV_FILE_DEADLINE,
                "the child wrote no environment within {ENV_FILE_DEADLINE:?}"
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
async fn an_agent_main_spawned_through_spawn_rendered_main_sees_no_unlisted_daemon_variable() {
    // Given
    a_daemon_holding_a_marker_and_tddy_config();
    let child = an_env_recording_child();
    let worktree = tempfile::tempdir().expect("worktree");
    let manager = CliSessionManager::new();

    // When
    let _handle = manager
        .spawn_rendered_main(
            "019d105b-ac0f-78d3-9a89-409731145a70",
            "lark-cli",
            worktree.path().to_path_buf(),
            "opus",
            vec![child.program().to_string_lossy().into_owned()],
            // The agent's whole environment, as the daemon computed it.
            vec![
                ("PATH".to_string(), "/usr/bin:/bin".to_string()),
                (LAUNCH_SET.to_string(), "https://route.example".to_string()),
            ],
            None,
            None,
        )
        .await
        .expect("the main is spawned");
    let env = child.environment().await;

    // Then
    for dropped in [SESSION_MARKER, INHERITED_TDDY] {
        assert!(
            !env.contains_key(dropped),
            "{dropped} reached the agent: {:?}",
            env.keys().collect::<Vec<_>>()
        );
    }
    assert_eq!(
        env.get(LAUNCH_SET).map(String::as_str),
        Some("https://route.example")
    );
    assert_eq!(env.get("PATH").map(String::as_str), Some("/usr/bin:/bin"));
}

#[tokio::test]
async fn a_user_terminal_still_inherits_the_daemon_environment() {
    // Given
    a_daemon_holding_a_marker_and_tddy_config();
    let child = an_env_recording_child();
    let worktree = tempfile::tempdir().expect("worktree");
    let manager = CliSessionManager::new();

    // When — a Bash tool, its shell being the recording child
    let _handle = manager
        .start_terminal(
            "019d105b-ac0f-78d3-9a89-409731145a71",
            worktree.path().to_path_buf(),
            &child.program().to_string_lossy(),
        )
        .await
        .expect("the terminal is started");
    let env = child.environment().await;

    // Then — guard: a user's shell is not an agent
    assert_eq!(env.get(SESSION_MARKER).map(String::as_str), Some("1"));
}
