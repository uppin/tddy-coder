//! The `acme` agent and the daemon end `coder_agent_launch_acceptance` and `remote_hooks_acceptance`
//! both launch a target session against: the agent's tool file (its hooks are Claude Code's
//! settings file) and its host resolution, the session's activity as a delivered hook sets it, and a
//! daemon holding one session's terminals and directory.
//!
//! Each suite reaches its target its own way (a Coder workspace, an ssh machine), so the target and
//! the launch stay in the suite; everything here is used by both, so neither test binary carries
//! dead code.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tddy_actions::tool::{ResolvedTool, ToolDef};
use tddy_agent_launch::remote_hook_follower::{HookLine, HookSink};
use tddy_agent_launch::spec_launch::HostAgent;
use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_coding_agent_specs::slots::RenderedSlots;
use tddy_coding_agent_specs::CodingAgentSpec;
use tddy_core::{activity_status_from_hook_with_tool, SessionActivityStatus};

pub const MODEL: &str = "acme-1";

/// The `acme` coding agent's tool file: its argv carries the model, its hooks are Claude Code's
/// settings file.
pub fn the_acme_spec() -> CodingAgentSpec {
    let def: ToolDef = serde_yaml::from_str(&format!(
        r#"schema_version: 1
id: acme
summary: a coding agent
binary:
  command: acme
deps: []
run:
  transport: pty
  argv:
    - "{{binary}}"
    - {{ if: model, then: ["--model", "{{model}}"] }}
extensions:
  coding_agent:
    mcp: {{ kind: file, flag: --mcp-config, format: claude-mcp-json }}
    tool_allow: {{ flag: --allowedTools }}
    tool_deny: {{ flag: --disallowedTools }}
    permission_prompt: {{ flag: --permission-prompt-tool }}
    instructions: {{ files: [CLAUDE.md] }}
    hooks: {{ adapter: claude-settings-local }}
    models: {{ default: {MODEL}, curated: [{MODEL}] }}
"#
    ))
    .expect("the tool file deserializes");
    CodingAgentSpec::of(&def).expect("a valid coding agent")
}

/// The `acme` agent as this host resolves it: its binary is `program`.
pub fn the_acme_agent(program: &Path) -> HostAgent {
    HostAgent {
        spec: the_acme_spec(),
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

/// The session's activity as the daemon keeps it: every status a delivered hook set, and every
/// hook delivered (its event and payload), in order.
#[derive(Default)]
pub struct TheSessionsActivity {
    pub statuses: Mutex<Vec<SessionActivityStatus>>,
    pub delivered: Mutex<Vec<HookLine>>,
}

#[async_trait]
impl HookSink for TheSessionsActivity {
    async fn deliver(&self, line: HookLine) {
        let status = activity_status_from_hook_with_tool(
            &line.event,
            line.payload["notification_type"].as_str(),
            line.payload["tool_name"].as_str(),
        );
        self.statuses.lock().unwrap().extend(status);
        self.delivered.lock().unwrap().push(line);
    }
}

/// The hooks of a target session, installed: the slots the launch renders and where they log.
pub struct InstalledHooks {
    pub slots: RenderedSlots,
    pub log: String,
}

/// A daemon with one session: its terminals, the session's own directory and its activity.
pub struct ADaemon {
    pub manager: Arc<CliSessionManager>,
    pub activity: Arc<TheSessionsActivity>,
    pub session_dir: PathBuf,
    _session_dir: tempfile::TempDir,
}

pub fn a_daemon() -> ADaemon {
    let dir = tempfile::tempdir().expect("a session directory");
    ADaemon {
        manager: Arc::new(CliSessionManager::new()),
        activity: Arc::default(),
        session_dir: std::fs::canonicalize(dir.path()).expect("the session directory resolves"),
        _session_dir: dir,
    }
}
