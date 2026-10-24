//! Acceptance: `EnsureProjectHomeSession` sets a project's **main** (project-home session) up from
//! a New-session start spec — a managed claude-cli session on a dedicated home worktree detached at
//! the project's main branch ref — records it on the project, and afterwards reaches, resumes or
//! replaces that recorded session. Types that cannot be held read-only, sandbox placements and a
//! caller-set role are refused; a recorded session that no longer exists is cleared.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home session.
//! PRD: docs/ft/web/1-WIP/PRD-2026-10-24-project-home-from-new-session-form.md

mod ensure_project_home_world;
mod project_home_support;

use std::path::Path;

use ensure_project_home_world::{
    a_claude_cli_spec, a_daemon_with_a_project, a_daemon_with_a_project_recording, World,
};
use project_home_support::{git, PROJECT_ID, VALID_TOKEN};
use tddy_rpc::{Code, Request};
use tddy_service::proto::session::{
    AgentClonePlacement, ListSessionsRequest, SessionEntry, SessionService as SessionServiceTrait,
    SplitAgentPlacement, StartSessionRequest,
};
use tddy_testing_commons::wait::{a_capture_showing, PTY_STUB_OUTPUT};

const GONE_SESSION_ID: &str = "gone-0000-0000-0000-000000000000";
const PROJECT_HOME_ROLE: &str = "project-home";

/// The commit `worktree` has checked out.
fn head_of(worktree: &Path) -> String {
    git(worktree, &["rev-parse", "HEAD"])
}

/// Whether `worktree` is detached (on no branch).
fn is_detached(worktree: &Path) -> bool {
    git(worktree, &["rev-parse", "--abbrev-ref", "HEAD"]) == "HEAD"
}

impl World {
    async fn listed(&self, session_id: &str) -> SessionEntry {
        self.home
            .service
            .list_sessions(Request::direct(ListSessionsRequest {
                session_token: VALID_TOKEN.to_string(),
            }))
            .await
            .expect("ListSessions")
            .into_inner()
            .sessions
            .into_iter()
            .find(|s| s.session_id == session_id)
            .expect("the session is listed")
    }

    /// Write a harness profile named `name` running `harness` into the daemon's agents directory.
    fn with_harness_profile(&self, name: &str, harness: &str) {
        let agents = self.home.data.path().join("agents");
        std::fs::create_dir_all(&agents).unwrap();
        std::fs::write(
            agents.join(format!("{name}.yaml")),
            format!("name: {name}\nharness: {harness}\nmodel: auto\n"),
        )
        .unwrap();
    }

    async fn argv_of(&self, session_id: &str) -> String {
        let handle = self
            .home
            .manager
            .get(session_id)
            .await
            .expect("the agent is running");
        a_capture_showing(&handle.capture, "ARGV", PTY_STUB_OUTPUT).await
    }
}

#[tokio::test]
async fn setting_up_main_starts_a_managed_claude_cli_session_on_the_home_worktree_at_the_main_ref()
{
    // Given
    let world = a_daemon_with_a_project();

    // When
    let set_up = world
        .set_up_main(a_claude_cli_spec())
        .await
        .expect("setting up main succeeds");

    // Then
    assert_eq!(set_up.outcome, "started");
    let session = set_up.session.expect("main is returned");
    assert_eq!(session.session_role, PROJECT_HOME_ROLE);
    assert!(session.managed_codebase, "main is managed");
    assert_eq!(
        world
            .metadata_of(&session.session_id)
            .session_role
            .as_deref(),
        Some(PROJECT_HOME_ROLE)
    );
    let home = world.home_worktree_of(&session.session_id);
    assert_ne!(
        home,
        world.repo.path(),
        "the home worktree is not the main checkout"
    );
    assert_eq!(head_of(&home), world.origin_main());
    assert!(is_detached(&home), "the home worktree is on no branch");
}

#[tokio::test]
async fn setting_up_main_records_the_session_on_the_project() {
    // Given
    let world = a_daemon_with_a_project();

    // When
    let main = world.a_running_main().await;

    // Then
    assert_eq!(world.recorded_main(), Some(main.session_id));
}

#[tokio::test]
async fn setting_up_main_runs_the_agent_with_the_specs_model_permission_mode_and_initial_prompt() {
    // Given
    let world = a_daemon_with_a_project();
    let spec = StartSessionRequest {
        model: "sonnet".to_string(),
        permission_mode: "plan".to_string(),
        initial_prompt: "Watch the stack".to_string(),
        ..a_claude_cli_spec()
    };

    // When
    let main = world
        .set_up_main(spec)
        .await
        .expect("setting up main succeeds")
        .session
        .unwrap();

    // Then
    let argv = world.argv_of(&main.session_id).await;
    assert!(argv.contains("--model sonnet"), "model: {argv:?}");
    assert!(
        argv.contains("--permission-mode plan"),
        "permission mode: {argv:?}"
    );
    assert!(argv.contains("Watch the stack"), "initial prompt: {argv:?}");
}

#[tokio::test]
async fn ensuring_main_while_it_runs_returns_the_recorded_session() {
    // Given
    let world = a_daemon_with_a_project();
    let main = world.a_running_main().await;

    // When
    let again = world.ensure().await.expect("ensure reaches main");

    // Then
    assert_eq!(again.outcome, "running");
    assert_eq!(again.session.unwrap().session_id, main.session_id);
    assert_eq!(world.session_dirs(), 1);
}

#[tokio::test]
async fn ensuring_a_stopped_main_fast_forwards_and_resumes_the_same_conversation() {
    // Given main has stopped, and the main branch moved on since
    let world = a_daemon_with_a_project();
    let main = world.a_running_main().await;
    world.stop(&main.session_id).await;
    let moved_to = world.advance_origin_main();

    // When
    let resumed = world.ensure().await.expect("ensure resumes");

    // Then
    assert_eq!(resumed.outcome, "resumed");
    assert_eq!(resumed.session.unwrap().session_id, main.session_id);
    assert_eq!(head_of(&world.home_worktree_of(&main.session_id)), moved_to);
    let argv = world.argv_of(&main.session_id).await;
    assert!(
        argv.contains(&format!("--resume {}", main.session_id)),
        "the agent resumes its own conversation; got: {argv:?}"
    );
}

#[tokio::test]
async fn ensuring_a_project_with_no_main_is_not_found_and_starts_nothing() {
    // Given
    let world = a_daemon_with_a_project();

    // When
    let refusal = world.ensure().await.expect_err("no main is not found");

    // Then
    assert_eq!(refusal.code(), Code::NotFound);
    assert_eq!(world.session_dirs(), 0);
}

#[tokio::test]
async fn ensuring_a_recorded_main_that_no_longer_exists_clears_it_and_is_not_found() {
    // Given a project recording a main session that has been deleted
    let world = a_daemon_with_a_project_recording(Some(GONE_SESSION_ID));

    // When
    let refusal = world.ensure().await.expect_err("a gone main is not found");

    // Then
    assert_eq!(refusal.code(), Code::NotFound);
    assert_eq!(world.recorded_main(), None);
    assert_eq!(world.session_dirs(), 0);
}

#[tokio::test]
async fn setting_up_main_as_cursor_cli_is_refused_naming_the_type_and_starts_nothing() {
    // Given
    let world = a_daemon_with_a_project();
    let spec = StartSessionRequest {
        session_type: "cursor-cli".to_string(),
        model: "auto".to_string(),
        ..Default::default()
    };

    // When
    let refusal = world
        .set_up_main(spec)
        .await
        .expect_err("cursor-cli is refused");

    // Then
    assert_eq!(refusal.code(), Code::FailedPrecondition);
    assert!(
        refusal.message().contains("cursor-cli"),
        "names the type: {}",
        refusal.message()
    );
    assert_eq!(world.session_dirs(), 0);
    assert_eq!(world.recorded_main(), None);
}

#[tokio::test]
async fn setting_up_main_as_a_tool_session_is_refused_and_starts_nothing() {
    // Given the form's tool-session request: no session type, an agent
    let world = a_daemon_with_a_project();
    let spec = StartSessionRequest {
        agent: "claude".to_string(),
        model: "opus".to_string(),
        ..Default::default()
    };

    // When
    let refusal = world
        .set_up_main(spec)
        .await
        .expect_err("a tool session is refused");

    // Then
    assert_eq!(refusal.code(), Code::FailedPrecondition);
    assert!(
        refusal.message().contains("a tool session"),
        "names the type: {}",
        refusal.message()
    );
    assert_eq!(world.session_dirs(), 0);
}

#[tokio::test]
async fn setting_up_main_in_a_sandbox_is_refused_and_starts_nothing() {
    // Given
    let world = a_daemon_with_a_project();
    let spec = StartSessionRequest {
        sandbox: true,
        ..a_claude_cli_spec()
    };

    // When
    let refusal = world
        .set_up_main(spec)
        .await
        .expect_err("a sandbox is refused");

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert_eq!(world.session_dirs(), 0);
}

#[tokio::test]
async fn setting_up_main_with_a_requested_role_is_refused_and_starts_nothing() {
    // Given
    let world = a_daemon_with_a_project();
    let spec = StartSessionRequest {
        session_role: PROJECT_HOME_ROLE.to_string(),
        ..a_claude_cli_spec()
    };

    // When
    let refusal = world
        .set_up_main(spec)
        .await
        .expect_err("a requested role is refused");

    // Then
    assert_eq!(refusal.code(), Code::PermissionDenied);
    assert!(
        refusal.message().contains("session_role"),
        "names the refused field: {}",
        refusal.message()
    );
    assert_eq!(world.session_dirs(), 0);
}

#[tokio::test]
async fn replacing_a_running_main_stops_it_and_records_the_new_session() {
    // Given a running main
    let world = a_daemon_with_a_project();
    let old = world.a_running_main().await;

    // When main is set up again
    let replaced = world
        .set_up_main(StartSessionRequest {
            model: "sonnet".to_string(),
            ..a_claude_cli_spec()
        })
        .await
        .expect("replacing main succeeds");

    // Then
    assert_eq!(replaced.outcome, "replaced");
    let new = replaced.session.expect("the new main is returned");
    assert_ne!(new.session_id, old.session_id);
    assert_eq!(world.recorded_main(), Some(new.session_id));
    assert!(
        !world.is_served(&old.session_id).await,
        "the replaced main's agent is stopped"
    );
}

#[tokio::test]
async fn a_replaced_main_is_still_listed_with_its_role() {
    // Given main was replaced
    let world = a_daemon_with_a_project();
    let old = world.a_running_main().await;
    world
        .set_up_main(a_claude_cli_spec())
        .await
        .expect("replace");

    // When
    let listed = world.listed(&old.session_id).await;

    // Then it keeps its record and role, so it stays read-only
    assert_eq!(listed.session_role, PROJECT_HOME_ROLE);
}

#[tokio::test]
async fn a_project_home_session_is_listed_with_its_role() {
    // Given
    let world = a_daemon_with_a_project();
    let main = world.a_running_main().await;

    // When
    let listed = world.listed(&main.session_id).await;

    // Then
    assert_eq!(listed.session_role, PROJECT_HOME_ROLE);
}

#[tokio::test]
async fn an_ensured_main_is_pre_approved_the_project_home_tool_set() {
    // Given a main the ensure has started
    let world = a_daemon_with_a_project();
    let main = world.a_running_main().await;

    // When its agent's terminal is read
    let handle = world
        .home
        .manager
        .get(&main.session_id)
        .await
        .expect("main is running");

    // Then the command line the stub `claude` echoes pre-approves the role's tool set, a
    // cross-session tool among it (the wait panics with what the agent did print)
    a_capture_showing(
        &handle.capture,
        "--allowedTools mcp__tddy-tools__ListProjectSessions",
        PTY_STUB_OUTPUT,
    )
    .await;
}

#[tokio::test]
async fn a_start_session_naming_a_role_is_refused_and_starts_nothing() {
    // Given a daemon a client can start sessions on
    let world = a_daemon_with_a_project();

    // When a client asks StartSession for the project-home role itself
    let refusal = world
        .home
        .service
        .start_session(Request::direct(StartSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            project_id: PROJECT_ID.to_string(),
            session_role: PROJECT_HOME_ROLE.to_string(),
            ..a_claude_cli_spec()
        }))
        .await
        .expect_err("a requested role is refused");

    // Then the role is refused as not the caller's to grant, and no session exists
    assert_eq!(refusal.code(), Code::PermissionDenied);
    assert!(
        refusal.message().contains("session_role"),
        "names the refused field: {}",
        refusal.message()
    );
    assert_eq!(world.session_dirs(), 0);
}

#[tokio::test]
async fn setting_up_main_over_ssh_is_refused_and_starts_nothing() {
    // Given
    let world = a_daemon_with_a_project();
    let spec = StartSessionRequest {
        ssh_config_host: "build-box".to_string(),
        ..a_claude_cli_spec()
    };

    // When
    let refusal = world
        .set_up_main(spec)
        .await
        .expect_err("an SSH exec target is refused");

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert!(
        refusal.message().contains("ssh_config_host"),
        "names the refused field: {}",
        refusal.message()
    );
    assert_eq!(world.session_dirs(), 0);
    assert_eq!(world.recorded_main(), None);
}

#[tokio::test]
async fn setting_up_main_with_a_profile_that_runs_cursor_cli_is_refused() {
    // Given a harness profile that runs Cursor CLI, named as a claude-cli spec's agent on the
    // harness's own vendor
    let world = a_daemon_with_a_project();
    world.with_harness_profile("cursor-on-vendor", "cursor-cli");
    let spec = StartSessionRequest {
        agent: "cursor-on-vendor".to_string(),
        provider_built_in: true,
        ..a_claude_cli_spec()
    };

    // When
    let refusal = world
        .set_up_main(spec)
        .await
        .expect_err("the profile's cursor-cli is refused");

    // Then
    assert_eq!(refusal.code(), Code::FailedPrecondition);
    assert!(
        refusal.message().contains("cursor-cli"),
        "names the type the profile resolves to: {}",
        refusal.message()
    );
    assert_eq!(world.session_dirs(), 0);
    assert_eq!(world.recorded_main(), None);
}

#[tokio::test]
async fn setting_up_main_as_an_agent_clone_is_refused() {
    // Given
    let world = a_daemon_with_a_project();
    let spec = StartSessionRequest {
        agent_clone: Some(AgentClonePlacement::default()),
        ..a_claude_cli_spec()
    };

    // When
    let refusal = world
        .set_up_main(spec)
        .await
        .expect_err("an agent clone is refused");

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert_eq!(world.session_dirs(), 0);
}

#[tokio::test]
async fn setting_up_main_as_a_split_agent_is_refused() {
    // Given
    let world = a_daemon_with_a_project();
    let spec = StartSessionRequest {
        split_agent: Some(SplitAgentPlacement::default()),
        ..a_claude_cli_spec()
    };

    // When
    let refusal = world
        .set_up_main(spec)
        .await
        .expect_err("a split agent is refused");

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert_eq!(world.session_dirs(), 0);
}

#[tokio::test]
async fn a_refused_replace_leaves_the_running_main_running_and_recorded() {
    // Given a running main
    let world = a_daemon_with_a_project();
    let old = world.a_running_main().await;

    // When main is set up again from a spec the start refuses (claude-cli needs a model)
    let refusal = world
        .set_up_main(StartSessionRequest {
            model: String::new(),
            ..a_claude_cli_spec()
        })
        .await
        .expect_err("a start without a model is refused");

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert!(
        world.is_served(&old.session_id).await,
        "the old main still runs"
    );
    assert_eq!(world.recorded_main(), Some(old.session_id));
    assert_eq!(world.session_dirs(), 1);
}

#[tokio::test]
async fn two_concurrent_set_ups_record_one_main_and_the_other_is_replaced() {
    // Given
    let world = a_daemon_with_a_project();

    // When main is set up twice at once
    let (first, second) = tokio::join!(
        world.set_up_main(a_claude_cli_spec()),
        world.set_up_main(a_claude_cli_spec())
    );

    // Then whichever took its turn second replaces the first, which is stopped (the order the two
    // reach the lock in is the runtime's, so the test does not assume it)
    let (first, second) = (
        first.expect("the first set-up succeeds"),
        second.expect("the second set-up succeeds"),
    );
    let (earlier, later) = if first.outcome == "started" {
        (first, second)
    } else {
        (second, first)
    };
    assert_eq!(
        (earlier.outcome.as_str(), later.outcome.as_str()),
        ("started", "replaced")
    );
    let earlier = earlier.session.unwrap().session_id;
    let later = later.session.unwrap().session_id;
    assert_eq!(world.recorded_main(), Some(later));
    assert!(
        !world.is_served(&earlier).await,
        "the replaced main is no longer served"
    );
}
