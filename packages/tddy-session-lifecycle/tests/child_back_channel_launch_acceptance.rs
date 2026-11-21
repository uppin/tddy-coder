//! Acceptance: an orchestrator and its Claude children are launched under their native peer names,
//! and a child's system prompt names its orchestrator and the back channel it reports over; a
//! session with no orchestrator is launched as before (`#orchestra` 2/9).
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Session messages.

// Both modules are shared with the `EnsureProjectHomeSession` suites; this suite uses part of each.
#[allow(dead_code)]
mod ensure_project_home_world;
#[allow(dead_code)]
mod project_home_support;

use ensure_project_home_world::{
    a_claude_cli_spec, a_daemon_with_a_project, a_daemon_with_a_project_and_coding_agents, World,
};
use project_home_support::{a_caller_token, PROJECT_ID};
use tddy_agent_launch::back_channel::native_peer_name_of;
use tddy_rpc::Request;
use tddy_service::proto::session::{
    ResumeSessionRequest, SessionService as SessionServiceTrait, StartSessionRequest,
};
use tddy_testing_commons::wait::{a_capture_showing, PTY_STUB_OUTPUT};

impl World {
    async fn a_person_starts(&self, start: StartSessionRequest) -> String {
        self.home
            .service
            .start_session(Request::direct(StartSessionRequest {
                session_token: a_caller_token().to_string(),
                project_id: PROJECT_ID.to_string(),
                ..start
            }))
            .await
            .expect("the session starts")
            .into_inner()
            .session_id
    }

    async fn a_running_orchestrator(&self) -> String {
        self.a_person_starts(StartSessionRequest {
            session_role: "orchestrator".to_string(),
            ..a_claude_cli_spec()
        })
        .await
    }

    async fn a_claude_child_of(&self, orchestrator: &str) -> String {
        self.a_person_starts(StartSessionRequest {
            orchestrator_session_id: orchestrator.to_string(),
            ..a_claude_cli_spec()
        })
        .await
    }

    /// What `session_id`'s terminal has shown once it shows `needle`.
    async fn terminal_of(&self, session_id: &str, needle: &str) -> String {
        let handle = self
            .home
            .manager
            .get(session_id)
            .await
            .expect("the agent is running");
        a_capture_showing(&handle.capture, needle, PTY_STUB_OUTPUT).await
    }

    /// The argv the stub `claude` echoed for `session_id`.
    async fn argv_of(&self, session_id: &str) -> String {
        let handle = self
            .home
            .manager
            .get(session_id)
            .await
            .expect("the agent is running");
        let shown = a_capture_showing(
            &handle.capture,
            "ENVDUMP TDDY_REMOTE_DAEMON_SOCKET=[",
            PTY_STUB_OUTPUT,
        )
        .await;
        let start = shown.find("ARGV: ").expect("the stub echoed its argv") + "ARGV: ".len();
        let rest = &shown[start..];
        rest[..rest.find(['\r', '\n']).unwrap_or(rest.len())]
            .trim_end()
            .to_string()
    }

    /// A Claude child of `orchestrator` whose process has exited: its record, its changeset naming
    /// the orchestrator, and its worktree.
    fn an_exited_claude_child_of(&self, orchestrator: &str, worktree: &std::path::Path) -> String {
        let child = "01a1fe01-0000-7000-8000-0000000000aa".to_string();
        let session_dir = self.home.data.path().join("sessions").join(&child);
        std::fs::create_dir_all(&session_dir).unwrap();
        let mut meta = tddy_session_split::service_util::starting_session_metadata(
            &child,
            PROJECT_ID,
            "claude-cli",
        );
        meta.status = "inactive".to_string();
        meta.repo_path = Some(worktree.to_string_lossy().into_owned());
        meta.hook_token = Some("a-hook-token".to_string());
        tddy_core::write_session_metadata(&session_dir, &meta).unwrap();
        tddy_core::write_changeset(
            &session_dir,
            &tddy_core::Changeset {
                orchestrator_session_id: Some(orchestrator.to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        child
    }

    async fn a_person_resumes(&self, session_id: &str) {
        self.home
            .service
            .resume_session(Request::direct(ResumeSessionRequest {
                session_token: a_caller_token().to_string(),
                session_id: session_id.to_string(),
            }))
            .await
            .expect("the session resumes");
    }

    fn system_prompt_of(&self, session_id: &str) -> String {
        std::fs::read_to_string(
            self.home
                .data
                .path()
                .join("sessions")
                .join(session_id)
                .join("agent-system-prompt.md"),
        )
        .unwrap_or_default()
    }
}

#[tokio::test]
async fn an_orchestrator_is_launched_under_its_native_peer_name() {
    // Given a project
    let world = a_daemon_with_a_project();

    // When a person starts a Claude orchestrator
    let orchestrator = world.a_running_orchestrator().await;

    // Then its agent is launched under the name its Claude children message it by
    let argv = world.argv_of(&orchestrator).await;
    assert!(
        argv.contains(&format!("--name {}", native_peer_name_of(&orchestrator))),
        "argv: {argv}"
    );
}

#[tokio::test]
async fn a_claude_child_is_launched_under_its_peer_name_and_told_to_report_to_its_orchestrator() {
    // Given a running Claude orchestrator
    let world = a_daemon_with_a_project();
    let orchestrator = world.a_running_orchestrator().await;

    // When a Claude child of it starts on the same host
    let child = world.a_claude_child_of(&orchestrator).await;

    // Then the child is launched under its own peer name
    let argv = world.argv_of(&child).await;
    assert!(
        argv.contains(&format!("--name {}", native_peer_name_of(&child))),
        "argv: {argv}"
    );
    // And its system prompt tells it to report progress and status to the orchestrator's peer name
    let prompt = world.system_prompt_of(&child);
    assert!(
        prompt.contains(&native_peer_name_of(&orchestrator)) && prompt.contains("progress"),
        "prompt: {prompt}"
    );
}

#[tokio::test]
async fn a_session_without_an_orchestrator_is_launched_unnamed_and_told_of_none() {
    // Given a project
    let world = a_daemon_with_a_project();

    // When a person starts an ordinary Claude session
    let session = world.a_person_starts(a_claude_cli_spec()).await;

    // Then it gets no peer name and no back-channel guidance
    let argv = world.argv_of(&session).await;
    assert!(!argv.contains("--name "), "argv: {argv}");
    assert!(
        !world.system_prompt_of(&session).contains("orchestrator"),
        "prompt: {}",
        world.system_prompt_of(&session)
    );
}

#[tokio::test]
async fn a_resumed_claude_child_keeps_its_peer_name_and_back_channel_guidance() {
    // Given a running Claude orchestrator and an exited Claude child of it
    let world = a_daemon_with_a_project();
    let orchestrator = world.a_running_orchestrator().await;
    let worktree = tempfile::tempdir().unwrap();
    let child = world.an_exited_claude_child_of(&orchestrator, worktree.path());

    // When the child is resumed
    world.a_person_resumes(&child).await;

    // Then it is relaunched under its peer name
    let argv = world.argv_of(&child).await;
    assert!(
        argv.contains(&format!("--name {}", native_peer_name_of(&child))),
        "argv: {argv}"
    );
    // And its system prompt again tells it to report to its orchestrator
    let prompt = world.system_prompt_of(&child);
    assert!(
        prompt.contains(&native_peer_name_of(&orchestrator)) && prompt.contains("progress"),
        "prompt: {prompt}"
    );
}

/// The task a Codex child is started with.
const A_CHILDS_TASK: &str = "Fix the flaky retry test";

#[tokio::test]
async fn a_codex_child_is_told_in_its_first_prompt_to_report_to_its_orchestrator_with_tddy_tools_send(
) {
    // Given a running Claude orchestrator on a host with Codex installed
    let world = a_daemon_with_a_project_and_coding_agents();
    let orchestrator = world.a_running_orchestrator().await;

    // When a Codex child of it — an agent with no system-prompt slot — starts with a task
    let child = world
        .a_person_starts(StartSessionRequest {
            session_type: "codex-cli".to_string(),
            model: "gpt-5.3-codex".to_string(),
            orchestrator_session_id: orchestrator.clone(),
            initial_prompt: A_CHILDS_TASK.to_string(),
            ..Default::default()
        })
        .await;

    // Then its first prompt tells it to report to its orchestrator with `tddy-tools send`, ahead
    // of its task
    let shown = world.terminal_of(&child, A_CHILDS_TASK).await;
    let report = format!("tddy-tools send --to {orchestrator}");
    let told = shown
        .find(&report)
        .unwrap_or_else(|| panic!("the first prompt names {report:?}: {shown}"));
    assert!(
        told < shown.find(A_CHILDS_TASK).expect("the task is shown"),
        "the guidance comes before the task: {shown}"
    );
}
