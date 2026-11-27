//! Unit tests: a jailed agent tab's jail plan ([`TabJailAgentPlanner::plan`]) — granted what the
//! session main's jail is for the same agent, with every file and tool link its own, its exec plan
//! in the tab's conversation, and none of the session's records written.

// Test-only: included via `mod` under `#[cfg(test)]`, so the filesystem-port guard reads
// the whole file as test code and the real-disk fixtures below are the tests' fixtures, not
// production I/O.
#![cfg(test)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tddy_test_hosts::a_tool_host;

use pretty_assertions::{assert_eq, assert_ne};
use tddy_actions::tool::ResolvedTool;
use tddy_daemon_sandbox::tab_jails::{TabJailAgent, TabJailLaunch, TabJailPaths};
use tddy_daemon_sandbox::tool_jail::{lower_tool_jail, ToolJailGrants, ToolJailInputs};
use tddy_rpc::Code;
use tddy_sandbox_runner::ExecPlan;
use tddy_testing_commons::stub_scripts::a_stub_agent_script;
use tddy_testing_commons::{a_session_metadata, fs::write_session_yaml};
use tddy_usage::{write_usage_context, UsageContextFile, USAGE_CONTEXT_FILE};

use super::{TabJailAgentPlan, TabJailAgentPlanner};
use crate::cli_harness_jail::closure_defs;
use crate::spec_launch::HostCodingAgents;

const SESSION_ID: &str = "019d3a10-7c2e-7b40-9e51-2a6c0f4d8b11";
const TAB: &str = "019d3a10-0000-7000-8000-0000000a6e17";
/// The conversation the tab holds: its own, never the session's.
const TAB_CONVERSATION: &str = "5f3c0000-0000-7000-8000-00000000a6e1";

/// A sandboxed claude-cli session on a fixture host whose Claude Code and `tddy-tools` are stubs.
struct ASandboxedSession {
    session_dir: PathBuf,
    worktree: PathBuf,
    tddy_home: PathBuf,
    host_home: PathBuf,
    coding_agents: HostCodingAgents,
    _dirs: Vec<tempfile::TempDir>,
}

/// The built-in coding agents, with `claude` and `tddy-tools` answered by stubs in `dir`.
fn coding_agents_with_stubs(dir: &Path) -> HostCodingAgents {
    let registry = tddy_coding_agent_specs::load_agent_registry(Path::new("/nonexistent/tools"))
        .expect("the built-in coding agents load");
    let env = ["claude", "tddy-tools"]
        .into_iter()
        .map(|id| {
            let var = registry
                .get(id)
                .and_then(|def| def.binary.env.clone())
                .unwrap_or_else(|| panic!("{id}'s spec names its binary's variable"));
            let stub = a_stub_agent_script(dir, &format!("{id}.sh")).build();
            (var, stub.to_string_lossy().into_owned())
        })
        .collect::<Vec<(String, String)>>();
    HostCodingAgents::new(registry, Arc::new(a_tool_host().with_envs(env).build()))
}

fn a_sandboxed_claude_session() -> ASandboxedSession {
    let dirs: Vec<tempfile::TempDir> = (0..5).map(|_| tempfile::tempdir().unwrap()).collect();
    let path = |i: usize| dirs[i].path().canonicalize().unwrap();
    let session_dir = path(0).join(SESSION_ID);
    std::fs::create_dir_all(&session_dir).unwrap();
    let worktree = path(1);
    let mut metadata = a_session_metadata()
        .with_session_id(SESSION_ID)
        .with_session_type("claude-cli")
        .with_status("active")
        .with_repo_path(worktree.to_str().unwrap())
        .without_pid()
        .build();
    metadata.sandbox = Some(true);
    metadata.session_role = Some("project-home".to_string());
    write_session_yaml(&session_dir, &metadata);
    ASandboxedSession {
        session_dir,
        worktree,
        tddy_home: path(2),
        host_home: path(3),
        coding_agents: coding_agents_with_stubs(&path(4)),
        _dirs: dirs,
    }
}

impl ASandboxedSession {
    fn planner(&self) -> TabJailAgentPlanner<'_> {
        TabJailAgentPlanner {
            coding_agents: &self.coding_agents,
            tddy_home: &self.tddy_home,
            host_home: &self.host_home,
        }
    }

    /// The launch of tab [`TAB`]'s jail, its files under `<session>/sandbox/tabs/<tab>/`.
    fn a_tab_launch(&self) -> TabJailLaunch {
        let root = self.session_dir.join("sandbox").join("tabs").join(TAB);
        TabJailLaunch {
            session_id: SESSION_ID.to_string(),
            terminal_id: TAB.to_string(),
            session_dir: self.session_dir.clone(),
            worktree: self.worktree.clone(),
            paths: TabJailPaths {
                root: root.clone(),
                ipc_socket: PathBuf::from("/tmp/tddy-tab-test.sock"),
                ready_marker: root.join("sandbox.ready"),
                exec_plan: root.join("exec-plan.json"),
                profile: root.join("sandbox.sb"),
                egress_dir: root.join("egress"),
                scratch_dir: root.join(".work"),
            },
            os_user: "tddy".to_string(),
            login_shell: "/bin/zsh".to_string(),
            subject: "ada".to_string(),
        }
    }

    /// The main's jail directory: `<session>/sandbox`.
    fn the_mains_sandbox_root(&self) -> PathBuf {
        self.session_dir.join("sandbox")
    }

    /// What the session main's jail is granted for Claude: lowered as `launch_jailed` lowers it,
    /// over the main's own scratch tree.
    fn the_mains_grants_for_claude(&self) -> ToolJailGrants {
        let agent = self
            .coding_agents
            .resolve("claude", None)
            .expect("the stub claude resolves");
        let defs = closure_defs(self.coding_agents.registry(), &agent).expect("claude's defs");
        let closure: Vec<ResolvedTool> = agent.deps.iter().chain([&agent.tool]).cloned().collect();
        lower_tool_jail(&ToolJailInputs {
            tool: &agent.tool,
            closure: &closure,
            defs: &defs,
            tddy_home: &self.tddy_home,
            host_home: &self.host_home,
            scratch_dir: &self.the_mains_sandbox_root().join(".work"),
            host: self.coding_agents.host(),
            route: None,
        })
        .expect("the main's grants for claude lower")
    }

    /// The main has recorded the session's usage context, as its launch does.
    fn has_a_main_usage_context(&self) {
        write_usage_context(
            &self.session_dir,
            &UsageContextFile {
                spec_id: "claude".to_string(),
                agent_home: Some(self.tddy_home.join("homes").join("claude")),
                conversation_id: Some(SESSION_ID.to_string()),
                worktree: Some(self.worktree.clone()),
                model: Some("claude-opus-4-8".to_string()),
            },
        )
        .expect("the main's usage context is written");
    }

    /// The session's records, as files: its `.session.yaml` and its usage context.
    fn session_records(&self) -> (String, String) {
        let read = |name: &str| {
            std::fs::read_to_string(self.session_dir.join(name))
                .unwrap_or_else(|e| panic!("the session's {name} reads: {e}"))
        };
        (read(".session.yaml"), read(USAGE_CONTEXT_FILE))
    }
}

/// A Claude tab in [`TAB_CONVERSATION`]: started fresh, or resumed.
fn a_claude_tab(resume: bool) -> TabJailAgent {
    TabJailAgent {
        harness: "claude-cli".to_string(),
        conversation_id: TAB_CONVERSATION.to_string(),
        resume,
    }
}

fn planned(session: &ASandboxedSession, agent: &TabJailAgent) -> TabJailAgentPlan {
    session
        .planner()
        .plan(&session.a_tab_launch(), agent)
        .expect("the jailed agent tab is planned")
}

/// Whether `argv` carries `flag` immediately followed by `value`.
fn carries(argv: &[String], flag: &str, value: &str) -> bool {
    argv.windows(2).any(|pair| pair == [flag, value])
}

// ---------------------------------------------------------------------------
// Grants and files
// ---------------------------------------------------------------------------

#[test]
fn a_jailed_agent_tab_is_granted_what_the_mains_jail_is_for_the_same_agent() {
    // Given a sandboxed claude-cli session, and what its main's jail is granted
    let session = a_sandboxed_claude_session();
    let mains = session.the_mains_grants_for_claude();

    // When a Claude tab's jail is planned
    let tabs = planned(&session, &a_claude_tab(false)).grants;

    // Then the tab's jail has the same home — the persistent one, used as is — mounts, secrets,
    // egress and policy
    assert_eq!(
        (
            &tabs.home,
            &tabs.mounts,
            &tabs.secrets,
            &tabs.egress,
            tabs.oauth_inbound,
            format!("{:?}", tabs.policy),
        ),
        (
            &mains.home,
            &mains.mounts,
            &mains.secrets,
            &mains.egress,
            mains.oauth_inbound,
            format!("{:?}", mains.policy),
        )
    );
}

#[test]
fn a_jailed_agent_tabs_tool_links_and_scratch_are_its_own() {
    // Given a sandboxed claude-cli session
    let session = a_sandboxed_claude_session();
    let launch = session.a_tab_launch();

    // When a Claude tab's jail is planned
    let plan = planned(&session, &a_claude_tab(false));

    // Then its tool links live in the tab's scratch tree, which its agent's PATH searches first
    let tab_bin = launch.paths.scratch_dir.join("bin");
    assert_eq!(plan.grants.bin_dir, tab_bin);
    let path = plan.exec_plan.env.get("PATH").cloned().unwrap_or_default();
    assert!(
        path.starts_with(&format!("{}:", tab_bin.display())),
        "the agent's PATH starts at the tab's tool links: {path}"
    );
}

#[test]
fn a_jailed_agent_tabs_exec_plan_is_written_to_its_own_files_not_the_mains() {
    // Given a sandboxed claude-cli session
    let session = a_sandboxed_claude_session();
    let launch = session.a_tab_launch();

    // When a Claude tab's jail is planned
    let plan = planned(&session, &a_claude_tab(false));

    // Then the plan the tab's runner execs is the tab's file, and the main's is not written
    let written: ExecPlan = serde_json::from_slice(
        &std::fs::read(&launch.paths.exec_plan).expect("the tab's exec plan is written"),
    )
    .expect("the tab's exec plan parses");
    assert_eq!(written, plan.exec_plan);
    assert!(
        !session
            .the_mains_sandbox_root()
            .join(crate::jail_launch::EXEC_PLAN_FILE)
            .exists(),
        "the main's exec plan is the main's"
    );
}

// ---------------------------------------------------------------------------
// The tab's conversation
// ---------------------------------------------------------------------------

#[test]
fn a_new_jailed_claude_tab_starts_its_own_conversation() {
    // Given a sandboxed claude-cli session
    let session = a_sandboxed_claude_session();

    // When a new Claude tab's jail is planned
    let argv = planned(&session, &a_claude_tab(false)).exec_plan.argv;

    // Then its Claude starts the tab's conversation, never the session's
    assert!(carries(&argv, "--session-id", TAB_CONVERSATION), "{argv:?}");
    assert!(!argv.iter().any(|arg| arg == "--resume"), "{argv:?}");
    assert!(!argv.iter().any(|arg| arg == SESSION_ID), "{argv:?}");
}

#[test]
fn a_restored_jailed_claude_tab_resumes_its_own_conversation() {
    // Given a sandboxed claude-cli session
    let session = a_sandboxed_claude_session();

    // When a restored Claude tab's jail is planned
    let argv = planned(&session, &a_claude_tab(true)).exec_plan.argv;

    // Then its Claude resumes the tab's conversation
    assert!(carries(&argv, "--resume", TAB_CONVERSATION), "{argv:?}");
}

// ---------------------------------------------------------------------------
// The session's records stay the main's
// ---------------------------------------------------------------------------

#[test]
fn planning_a_jailed_agent_tab_writes_none_of_the_sessions_records() {
    // Given a sandboxed claude-cli session whose main recorded its role and its usage context
    let session = a_sandboxed_claude_session();
    session.has_a_main_usage_context();
    let recorded_by_the_main = session.session_records();

    // When a Claude tab's jail is planned
    let plan = planned(&session, &a_claude_tab(false));

    // Then the session's records are the main's, byte for byte
    assert_eq!(session.session_records(), recorded_by_the_main);
    assert_ne!(plan.exec_plan.argv, Vec::<String>::new());
}

// ---------------------------------------------------------------------------
// What is refused
// ---------------------------------------------------------------------------

#[test]
fn a_tab_of_a_harness_no_coding_agent_runs_is_refused_naming_it() {
    // Given a sandboxed claude-cli session
    let session = a_sandboxed_claude_session();
    let unknown = TabJailAgent {
        harness: "nonesuch-cli".to_string(),
        ..a_claude_tab(false)
    };

    // When a tab of a harness nothing runs is planned
    let refused = session
        .planner()
        .plan(&session.a_tab_launch(), &unknown)
        .expect_err("a harness no coding agent runs is refused");

    // Then it is refused naming the harness, and nothing is written for the tab
    assert_eq!(refused.code(), Code::InvalidArgument);
    assert!(
        refused.message().contains("\"nonesuch-cli\""),
        "{}",
        refused.message()
    );
    assert!(!session.a_tab_launch().paths.exec_plan.exists());
}
