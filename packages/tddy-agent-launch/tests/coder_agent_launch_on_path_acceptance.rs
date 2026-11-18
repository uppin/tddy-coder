//! Acceptance: a Coder target is reached through the `coder` the daemon finds on its own `PATH`,
//! as `shell_for_coder_target` names it — a bare program, not a path — although the session's
//! terminal is spawned with a cleared environment.
//!
//! A binary of its own: it puts the stand-in `coder` on this process's `PATH`, which no other
//! suite may observe. The stand-in runs the workspace's command on this machine; no deployment is
//! reached.
//!
//! PRD: docs/dev/1-WIP/PRD-2026-11-18-coder-interactive-launch.md.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tddy_testing_commons::remote_target::write_executable;

use pretty_assertions::assert_eq;
use tddy_actions::tool::{ResolvedTool, TemplateParams, ToolDef};
use tddy_agent_launch::coder_target::shell_for_coder_target;
use tddy_agent_launch::remote_agent_launch::{launch_on_target, RemoteLaunch};
use tddy_agent_launch::spec_launch::{HostAgent, HostPtySession};
use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_coding_agent_specs::slots::RenderedSlots;
use tddy_coding_agent_specs::CodingAgentSpec;
use tddy_core::SessionTarget;
use tddy_credentials::SecretString;
use tddy_host_service::coder_workspaces::{
    CoderCredentialError, CoderCredentialSource, CoderCredentials,
};
use tddy_testing_commons::wait::eventually;

const SESSION_ID: &str = "019d2c7e-51aa-7c40-8f0e-2b1d6a9c3e11";
/// How long the agent must stay up for a launch to count as started.
const STARTUP_WINDOW: Duration = Duration::from_millis(400);
/// A safety net for the stub to report, not a prediction of how long it takes.
const OBSERVE: Duration = Duration::from_secs(15);

/// The one account every read answers with.
struct AdasVault;

impl CoderCredentialSource for AdasVault {
    fn credentials_for(
        &self,
        _subject: &str,
        _account_id: &str,
    ) -> Result<CoderCredentials, CoderCredentialError> {
        Ok(CoderCredentials::new(
            "https://coder.example.com",
            SecretString::new("adas-coder-session-token"),
        ))
    }
}

/// The `acme` agent, resolved to `program`.
fn the_acme_agent(program: &Path) -> HostAgent {
    let def: ToolDef = serde_yaml::from_str(
        r#"schema_version: 1
id: acme
summary: a coding agent
binary:
  command: acme
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
"#,
    )
    .expect("the tool file deserializes");
    HostAgent {
        spec: CodingAgentSpec::of(&def).expect("a valid coding agent"),
        tool: ResolvedTool {
            id: "acme".to_string(),
            command: "acme".to_string(),
            binary: program.to_path_buf(),
            reads: Vec::new(),
            network: Default::default(),
        },
        deps: Vec::new(),
        env_base: tddy_coding_agent_specs::AgentEnvBase::builtin(),
    }
}

#[tokio::test]
async fn a_coder_found_only_on_the_daemons_path_carries_the_agents_terminal() {
    // Given a stand-in `coder` reachable only through this process's PATH, a workspace directory
    // holding the session's worktree, and an agent there that records where it runs
    let dir = tempfile::tempdir().expect("a directory standing for the workspace");
    let root = std::fs::canonicalize(dir.path()).expect("the workspace's directory resolves");
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).expect("a directory on PATH for the stand-in coder");
    write_executable(
        &bin.join("coder"),
        "#!/bin/sh\n\
         while [ \"$#\" -gt 0 ] && [ \"$1\" != \"--\" ]; do shift; done\n\
         shift\n\
         SHELL=\"$(dirname \"$0\")/login-shell\"\n\
         export SHELL\n\
         exec /bin/sh -c \"$*\"\n",
    );
    // The workspace's login shell finds its agents on a `PATH` of its own.
    let agents = root.join("opt/agents");
    std::fs::create_dir_all(&agents).expect("the directory the workspace's login finds agents in");
    write_executable(
        &bin.join("login-shell"),
        &format!(
            "#!/bin/sh\n[ \"$1\" = -lc ] || exit 64\nHOME='{}'\nPATH='{}:/usr/bin:/bin'\n\
             export HOME PATH\nexec /bin/sh -c \"$2\"\n",
            root.join("home/coder").display(),
            agents.display()
        ),
    );
    let worktree: PathBuf = root.join("home/coder/repo/.worktrees").join(SESSION_ID);
    std::fs::create_dir_all(&worktree).expect("the session's worktree in the workspace");
    let agent = agents.join("acme");
    write_executable(
        &agent,
        &format!(
            "#!/bin/sh\n/bin/pwd -P > '{}'\nexec /bin/cat\n",
            root.join("agent-cwd").display()
        ),
    );
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut dirs = vec![bin];
    dirs.extend(std::env::split_paths(&path));
    std::env::set_var("PATH", std::env::join_paths(dirs).expect("a joinable PATH"));

    // When a session is started on a Coder target through the daemon's own `coder`
    let shell = shell_for_coder_target(
        &SessionTarget::coder("coder-3f1c9a52@coder.example.com", "ada", "devbox"),
        "ada-on-github",
        &AdasVault,
    )
    .expect("a Coder target whose vault is open resolves to a shell");
    let session_dir = tempfile::tempdir().expect("a session directory");
    let manager = Arc::new(CliSessionManager::new());
    let launched = launch_on_target(
        &the_acme_agent(&agent),
        &TemplateParams::new(),
        &RenderedSlots::default(),
        RemoteLaunch {
            shell,
            worktree: &worktree,
            session_dir: &std::fs::canonicalize(session_dir.path()).expect("it resolves"),
            pointer: None,
            startup_window: STARTUP_WINDOW,
            clone_root: None,
        },
        HostPtySession {
            manager: &manager,
            session_id: SESSION_ID,
            session_type: "acme-cli",
            model: "acme-1",
            env: Vec::new(),
            os_user: None,
        },
        None,
    )
    .await
    .map(|_launched| ());

    // Then the terminal's client was found, and the agent runs in the session's worktree
    assert_eq!(launched, Ok(()));
    let agent_runs_in = eventually("the agent to record where it runs", OBSERVE, || {
        std::fs::read_to_string(root.join("agent-cwd"))
            .map(|cwd| PathBuf::from(cwd.trim_end()))
            .map_err(|_| "the agent has not run yet".to_string())
    })
    .await;
    assert_eq!(agent_runs_in, worktree);
}
