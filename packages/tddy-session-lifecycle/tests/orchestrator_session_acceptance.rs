//! Acceptance: a person starts any number of **orchestrator** sessions with `StartSession`
//! (`session_role: "orchestrator"`), and the daemon places each exactly as it places main — on the
//! project's home worktree, fast-forwarded to the main branch ref, managed, launched read-only with
//! main's tool lists and `tddy-tools --mcp` as its only MCP server. A placement, a sandbox and an
//! agent that cannot be held read-only are refused naming why; `project-home` itself is still the
//! daemon's to grant (`#orchestrators` 1/11).
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-10-orchestrator-sessions.md
//! Changeset: docs/dev/1-WIP/2026-11-10-orchestrator-role.md

// Both modules are shared with the `EnsureProjectHomeSession` suites; this suite uses part of each.
#[allow(dead_code)]
mod ensure_project_home_world;
#[allow(dead_code)]
mod project_home_support;

use std::path::{Path, PathBuf};

use ensure_project_home_world::{a_claude_cli_spec, a_daemon_with_a_project, World};
use project_home_support::{a_caller_token, git, PROJECT_ID};
use tddy_rpc::{Code, Request};
use tddy_service::proto::session::{SessionService as SessionServiceTrait, StartSessionRequest};
use tddy_testing_commons::wait::{a_capture_showing, PTY_STUB_OUTPUT};
use tddy_worktree_service::home_worktree::home_worktree_path;

const ORCHESTRATOR_ROLE: &str = "orchestrator";

/// Main's `--disallowedTools`: every native shell and file tool, so its only reach is the daemon's.
const MAINS_WITHDRAWN_TOOLS: [&str; 11] = [
    "Read",
    "Write",
    "Edit",
    "MultiEdit",
    "NotebookEdit",
    "Bash",
    "BashOutput",
    "KillShell",
    "Glob",
    "Grep",
    "LS",
];

/// The New-session form's start for a Claude CLI orchestrator in the project.
fn an_orchestrator_start() -> StartSessionRequest {
    StartSessionRequest {
        session_role: ORCHESTRATOR_ROLE.to_string(),
        ..a_claude_cli_spec()
    }
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|e| panic!("{} resolves: {e}", path.display()))
}

impl World {
    /// `StartSession` as the person at the browser.
    async fn a_person_starts(
        &self,
        start: StartSessionRequest,
    ) -> Result<String, tddy_rpc::Status> {
        self.home
            .service
            .start_session(Request::direct(StartSessionRequest {
                session_token: a_caller_token().to_string(),
                project_id: PROJECT_ID.to_string(),
                ..start
            }))
            .await
            .map(|started| started.into_inner().session_id)
    }

    async fn a_running_orchestrator(&self) -> String {
        self.a_person_starts(an_orchestrator_start())
            .await
            .expect("a person starts an orchestrator")
    }

    /// Where the project's home worktree is, whether or not one has been made yet.
    fn the_home_worktree(&self) -> PathBuf {
        home_worktree_path(&tddy_core::worktree_dir(self.repo.path()), PROJECT_ID)
    }

    fn worktree_of(&self, session_id: &str) -> PathBuf {
        PathBuf::from(
            self.metadata_of(session_id)
                .repo_path
                .expect("the session has a worktree"),
        )
    }

    /// What `session_id`'s agent was launched with, once the stub `claude` has echoed all of it.
    async fn launch_of(&self, session_id: &str) -> ALaunch {
        let handle = self
            .home
            .manager
            .get(session_id)
            .await
            .expect("the agent is running");
        ALaunch {
            shown: a_capture_showing(
                &handle.capture,
                "ENVDUMP TDDY_REMOTE_DAEMON_SOCKET=[",
                PTY_STUB_OUTPUT,
            )
            .await,
        }
    }
}

/// One launch of the stub `claude`, as it echoed its argv.
struct ALaunch {
    shown: String,
}

impl ALaunch {
    fn argv(&self) -> &str {
        let start = self.shown.find("ARGV: ").expect("the stub echoed its argv") + "ARGV: ".len();
        let rest = &self.shown[start..];
        rest[..rest.find(['\r', '\n']).unwrap_or(rest.len())].trim_end()
    }

    fn values_of(&self, flag: &str) -> Vec<&str> {
        self.argv()
            .split(' ')
            .collect::<Vec<_>>()
            .windows(2)
            .filter(|pair| pair[0] == flag)
            .map(|pair| pair[1])
            .collect()
    }

    /// The MCP config the agent was told to load.
    fn mcp_config(&self) -> serde_json::Value {
        let path = self.values_of("--mcp-config");
        let path = path.first().expect("the agent was given an MCP config");
        serde_json::from_str(&std::fs::read_to_string(path).expect("the MCP config exists"))
            .expect("the MCP config is JSON")
    }
}

#[tokio::test]
async fn a_person_starts_an_orchestrator_on_the_home_worktree_read_only() {
    // Given a project with no main
    let world = a_daemon_with_a_project();

    // When a person starts a Claude CLI session asking for the orchestrator role
    let orchestrator = world.a_running_orchestrator().await;

    // Then its record holds the role, managed, on the project's home worktree
    let recorded = world.metadata_of(&orchestrator);
    assert_eq!(
        (
            recorded.session_role.as_deref(),
            recorded.managed_codebase,
            recorded.project_id.as_str(),
        ),
        (Some(ORCHESTRATOR_ROLE), true, PROJECT_ID)
    );
    assert_eq!(
        canonical(&world.worktree_of(&orchestrator)),
        canonical(&world.the_home_worktree())
    );
    // And its agent was launched read-only, as main is: the role was on record before it spawned
    let launch = world.launch_of(&orchestrator).await;
    assert!(
        launch
            .values_of("--allowedTools")
            .contains(&"mcp__tddy-tools__ListProjectSessions"),
        "pre-approved main's tool set; argv: {}",
        launch.argv()
    );
    assert_eq!(launch.values_of("--disallowedTools"), MAINS_WITHDRAWN_TOOLS);
    assert!(
        launch.argv().contains("--strict-mcp-config"),
        "loads no MCP config but its own; argv: {}",
        launch.argv()
    );
    let servers = launch.mcp_config()["mcpServers"].clone();
    assert_eq!(
        servers
            .as_object()
            .map(|s| s.keys().cloned().collect::<Vec<_>>()),
        Some(vec!["tddy-tools".to_string()])
    );
    assert_eq!(servers["tddy-tools"]["args"], serde_json::json!(["--mcp"]));
}

#[tokio::test]
async fn two_orchestrators_and_main_run_on_one_home_worktree() {
    // Given a running main
    let world = a_daemon_with_a_project();
    let main = world.a_running_main().await.session_id;

    // When a person starts two orchestrators beside it
    let first = world.a_running_orchestrator().await;
    let second = world.a_running_orchestrator().await;

    // Then all three run, on one worktree, and main is still the project's main
    let worktrees: Vec<PathBuf> = [&main, &first, &second]
        .into_iter()
        .map(|id| canonical(&world.worktree_of(id)))
        .collect();
    assert_eq!(
        worktrees,
        vec![canonical(&world.the_home_worktree()); 3],
        "main, then the two orchestrators"
    );
    for session_id in [&main, &first, &second] {
        assert!(world.is_served(session_id).await, "{session_id} runs");
    }
    assert_eq!(world.recorded_main(), Some(main));
}

#[tokio::test]
async fn an_orchestrator_naming_a_placement_or_sandbox_is_refused_naming_the_field() {
    // Given
    let world = a_daemon_with_a_project();
    let placements = [
        (
            "sandbox",
            StartSessionRequest {
                sandbox: true,
                ..an_orchestrator_start()
            },
        ),
        (
            "sandboxed_codebase",
            StartSessionRequest {
                sandboxed_codebase: true,
                ..an_orchestrator_start()
            },
        ),
        (
            "codebase_daemon_instance_id",
            StartSessionRequest {
                codebase_daemon_instance_id: "another-host".to_string(),
                ..an_orchestrator_start()
            },
        ),
        (
            "ssh_config_host",
            StartSessionRequest {
                ssh_config_host: "build-box".to_string(),
                ..an_orchestrator_start()
            },
        ),
    ];

    // When each is asked for
    let mut refusals = Vec::new();
    for (field, start) in placements {
        let refusal = world
            .a_person_starts(start)
            .await
            .expect_err("a placement is refused for an orchestrator");
        refusals.push((field, refusal.code(), refusal.message().contains(field)));
    }

    // Then each is refused as a malformed request naming its field, and nothing was started
    assert_eq!(
        refusals,
        vec![
            ("sandbox", Code::InvalidArgument, true),
            ("sandboxed_codebase", Code::InvalidArgument, true),
            ("codebase_daemon_instance_id", Code::InvalidArgument, true),
            ("ssh_config_host", Code::InvalidArgument, true),
        ]
    );
    assert_eq!(world.session_dirs(), 0);
}

#[tokio::test]
async fn an_orchestrator_on_an_agent_without_project_home_main_is_refused() {
    // Given
    let world = a_daemon_with_a_project();

    // When a person asks for an orchestrator on Cursor CLI, whose spec declares no project_home_main
    let refusal = world
        .a_person_starts(StartSessionRequest {
            session_type: "cursor-cli".to_string(),
            model: "composer-2.5".to_string(),
            ..an_orchestrator_start()
        })
        .await
        .expect_err("an agent that cannot be held read-only is refused");

    // Then it is refused naming the type, and nothing was started
    assert_eq!(
        refusal.code(),
        Code::FailedPrecondition,
        "{}",
        refusal.message()
    );
    assert!(
        refusal.message().contains("cursor-cli"),
        "names the type: {}",
        refusal.message()
    );
    assert_eq!(world.session_dirs(), 0);
}

#[tokio::test]
async fn starting_an_orchestrator_fast_forwards_the_home_worktree() {
    // Given a running main on the home worktree, and the main branch moved on since
    let world = a_daemon_with_a_project();
    world.a_running_main().await;
    let moved_to = world.advance_origin_main();

    // When a person starts an orchestrator
    let orchestrator = world.a_running_orchestrator().await;

    // Then the home worktree it runs on is at the main branch's new tip
    assert_eq!(
        git(&world.worktree_of(&orchestrator), &["rev-parse", "HEAD"]),
        moved_to
    );
}
