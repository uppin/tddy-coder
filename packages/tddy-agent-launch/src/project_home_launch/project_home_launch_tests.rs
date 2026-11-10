//! Unit tests: a main launched from the `claude` spec is handed its tool set through the spec's
//! slots, and gets what the hand-built launch gave it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_actions::tool::{ProcessToolHost, ToolHost};
use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_testing_commons::wait::{a_capture_showing, PTY_STUB_OUTPUT};

use super::*;
use crate::cli_agent_steps::{render_host_launch_slots, HostLaunchSlots};
use crate::harness_from_spec::{harness_template_params, HarnessLaunch};
use crate::spec_launch::{launch_host_pty, HostCodingAgents, HostPtySession};

const A_MAIN: &str = "01a16dc6-ab4c-7dc3-844e-baca4514f86e";
/// What the stub `claude` prints once it has recorded its argv.
const LAUNCHED: &str = "STUB LAUNCHED";

/// A host whose whole `PATH` is `dir`, so the `claude` spec and its `tddy-tools` dep resolve to the
/// stubs there and nothing of the real machine.
struct AStubPathHost {
    dir: PathBuf,
}

impl ToolHost for AStubPathHost {
    fn env_var(&self, _name: &str) -> Option<String> {
        None
    }

    fn path_dirs(&self) -> Vec<PathBuf> {
        vec![self.dir.clone()]
    }

    fn is_executable_file(&self, path: &Path) -> bool {
        ProcessToolHost.is_executable_file(path)
    }

    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        ProcessToolHost.canonicalize(path)
    }
}

fn an_executable(path: &Path, script: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, script).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A stub `claude` that writes its argv (one entry per line, `$0` first) to `argv_file`, and a
/// stub `tddy-tools`, in `dir`.
fn the_stub_binaries(dir: &Path, argv_file: &Path) {
    an_executable(
        &dir.join("claude"),
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$0\" \"$@\" > '{}'\necho '{LAUNCHED}'\ncat\n",
            argv_file.display()
        ),
    );
    an_executable(&dir.join("tddy-tools"), "#!/bin/sh\nexit 0\n");
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[tokio::test]
async fn a_main_launched_from_the_claude_spec_gets_its_tool_flags_and_mcp_file_from_the_slots() {
    // Given — a main's wiring, and a host whose coding agents resolve to stubs
    let stubs = tempfile::tempdir().unwrap();
    let stub_dir = std::fs::canonicalize(stubs.path()).unwrap();
    let records = tempfile::tempdir().unwrap();
    let argv_file = records.path().join("argv");
    the_stub_binaries(&stub_dir, &argv_file);
    let tddy_tools = stub_dir.join("tddy-tools");
    let empty_tools_dir = tempfile::tempdir().unwrap();
    let agents = HostCodingAgents::new(
        tddy_coding_agent_specs::load_agent_registry(empty_tools_dir.path()).unwrap(),
        Arc::new(AStubPathHost {
            dir: stub_dir.clone(),
        }),
    );
    let config = DaemonConfig::default();
    let data_dir = tempfile::tempdir().unwrap();
    let session_dir = tempfile::tempdir().unwrap();
    let worktree = tempfile::tempdir().unwrap();
    let wiring = project_home_wiring(ProjectHomeLaunch {
        config: &config,
        agent_tool_socket: &data_dir.path().join("agent.sock"),
        session_dir: session_dir.path(),
        session_id: A_MAIN,
        agent_session_token: "minted-for-the-agent",
        tddy_tools_path: &tddy_tools,
    })
    .expect("a main's wiring is built");
    let (extra_args, toolset) = spec_role_launch(wiring.main.as_ref());
    let manager = Arc::new(CliSessionManager::new());

    // When the main is launched through the generic host launch, from the `claude` spec
    let agent = agents
        .resolve("claude", None)
        .expect("the claude agent resolves");
    let params = harness_template_params(&HarnessLaunch {
        model: "opus",
        session_id: Some(A_MAIN),
        ..HarnessLaunch::default()
    });
    let mut slots = render_host_launch_slots(
        &agent,
        worktree.path(),
        session_dir.path(),
        HostLaunchSlots {
            toolset,
            ..HostLaunchSlots::default()
        },
    )
    .expect("the main's slots render");
    slots.argv_extra.extend(extra_args);
    let handle = launch_host_pty(
        &agent,
        &params,
        &slots,
        worktree.path(),
        HostPtySession {
            manager: &manager,
            session_id: A_MAIN,
            session_type: "claude-cli",
            model: "opus",
            env: wiring.env,
            os_user: None,
        },
        None,
    )
    .await
    .expect("the main is launched");
    a_capture_showing(&handle.capture, LAUNCHED, PTY_STUB_OUTPUT).await;

    // Then — the allow, deny and strict MCP flags are the slots', in slot order, before the role's
    // own prompt flag
    let mcp_config = session_dir.path().join("claude-mcp-config.json");
    let prompt_file = session_dir.path().join(PROJECT_HOME_PROMPT_BASENAME);
    let mut expected = strings(&[
        stub_dir.join("claude").to_str().unwrap(),
        "--model",
        "opus",
        "--session-id",
        A_MAIN,
        "--permission-mode",
        "auto",
    ]);
    for tool in tddy_tool_engine::read_only::project_home_allowed_tools() {
        expected.extend(["--allowedTools".to_string(), tool]);
    }
    for tool in tddy_session_split::split_session::NATIVE_FILESYSTEM_TOOLS {
        expected.extend(["--disallowedTools".to_string(), (*tool).to_string()]);
    }
    expected.extend(strings(&[
        "--mcp-config",
        mcp_config.to_str().unwrap(),
        "--strict-mcp-config",
        "--append-system-prompt-file",
        prompt_file.to_str().unwrap(),
    ]));
    let argv: Vec<String> = std::fs::read_to_string(&argv_file)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(argv, expected);

    // And the MCP file the slot wrote is the one a split launch with no transport writes, byte for byte
    let from_the_slot = std::fs::read(&mcp_config).expect("the slot wrote the MCP config");
    let split_agent = tddy_session_split::agent_argv::split_agent_spec(
        &tddy_coding_agent_specs::CodingAgentRegistry::builtin()
            .expect("the built-in coding agents load"),
    )
    .expect("the built-in registry holds the split agent");
    tddy_session_split::agent_argv::split_claude_extra_args(
        &split_agent,
        session_dir.path(),
        &tddy_tools.to_string_lossy(),
        &[],
        &[],
    )
    .expect("the split launch writes its MCP config");
    assert_eq!(
        String::from_utf8(from_the_slot).unwrap(),
        std::fs::read_to_string(&mcp_config).unwrap()
    );
}
