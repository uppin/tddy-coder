//! Acceptance: an ordinary, locally-launched CLI-agent session (no role, no managed workflow) is
//! given the **session tool transport** in its agent process's environment — the `TDDY_REMOTE_*`
//! variables a `tddy-tools` run from the agent's own shell (`tddy-tools monitor`, `tddy-tools send`,
//! under Claude Code's `Monitor`) resolves its route to this daemon from, authenticated by a
//! credential of the agent's own (`#activity-wake` 4/5, 5/5).
//!
//! Before this the variables reached only an MCP config's `env` block, and an ordinary session
//! registers no `tddy-tools` MCP server at all — so neither route existed in-session and every
//! shell-run `tddy-tools monitor` died "remote toolset not configured".
//!
//! Defect: live-tested `tddy-tools monitor` on daemon LT-R1VXTH2V6H (cursor-cli and claude-cli).

#[allow(dead_code)]
mod ensure_project_home_world;
#[allow(dead_code)]
mod project_home_support;

use ensure_project_home_world::{a_claude_cli_spec, a_daemon_with_a_project, World};
use project_home_support::{
    a_caller_token, current_os_user, this_daemons_session_tokens, PROJECT_ID,
};
use std::path::{Path, PathBuf};
use tddy_daemon_kernel::daemon_hook_urls;
use tddy_rpc::Request;
use tddy_service::proto::session::StartSessionRequest;
use tddy_service::proto::session::{ResumeSessionRequest, SessionService as SessionServiceTrait};
use tddy_testing_commons::wait::{a_capture_showing, PTY_STUB_OUTPUT};

impl World {
    /// `StartSession` as the person at the browser, of an ordinary session (no role).
    async fn a_person_starts_an_ordinary_session(&self) -> String {
        self.home
            .service
            .start_session(Request::direct(StartSessionRequest {
                session_token: a_caller_token().to_string(),
                project_id: PROJECT_ID.to_string(),
                ..a_claude_cli_spec()
            }))
            .await
            .expect("a person starts an ordinary claude-cli session")
            .into_inner()
            .session_id
    }

    /// What `session_id`'s agent was launched with, once the stub has dumped its environment.
    async fn launch_of(&self, session_id: &str) -> ALaunch {
        let handle = self
            .home
            .manager
            .get(session_id)
            .await
            .expect("the agent is running");
        // The stub's last dump line, so everything before it is in the capture too.
        let shown = a_capture_showing(
            &handle.capture,
            "ENVDUMPENV TDDY_REMOTE_DAEMON_SOCKET=[",
            PTY_STUB_OUTPUT,
        )
        .await;
        ALaunch { shown }
    }

    /// Stop and resume `session_id` through the session service, as a person reopens it.
    async fn stop_and_resume(&self, session_id: &str) {
        self.stop(session_id).await;
        self.home
            .service
            .resume_session(Request::direct(ResumeSessionRequest {
                session_token: a_caller_token().to_string(),
                session_id: session_id.to_string(),
            }))
            .await
            .expect("the session resumes");
    }
}

/// One launch of the stub `claude`, as it dumped its own process environment.
struct ALaunch {
    shown: String,
}

impl ALaunch {
    /// The environment variable `key` as the agent process itself saw it; empty when unset.
    fn env(&self, key: &str) -> &str {
        let needle = format!("ENVDUMPENV {key}=[");
        let start = self
            .shown
            .find(&needle)
            .unwrap_or_else(|| panic!("the stub echoed {key}"))
            + needle.len();
        let rest = &self.shown[start..];
        &rest[..rest.find(']').expect("the dumped value is closed")]
    }
}

/// This daemon's local URL, as a daemon with no `listen:` port of its own is reached on.
fn this_daemons_local_url() -> String {
    daemon_hook_urls::local_daemon_hook_url(&tddy_daemon_kernel::config::DaemonConfig::default())
}

/// This daemon's socket for the session's OS user — what `tddy-tools` dials over the UDS transport.
fn this_users_socket(data_dir: &Path) -> PathBuf {
    tddy_daemon_kernel::agent_tool_sockets::user_socket_path_for_user(data_dir, &current_os_user())
        .expect("the user has a socket path")
}

#[tokio::test]
async fn an_ordinary_sessions_agent_carries_the_session_tool_transport_in_its_own_environment() {
    // Given a daemon with a project, and an ordinary claude-cli session started in it
    let world = a_daemon_with_a_project();
    let session_id = world.a_person_starts_an_ordinary_session().await;

    // When its launch is read
    let launch = world.launch_of(&session_id).await;

    // Then the agent process's own environment names this session to this daemon
    assert_eq!(launch.env("TDDY_REMOTE_SESSION_ID"), session_id);
    assert_eq!(
        launch.env("TDDY_REMOTE_DAEMON_SOCKET"),
        this_users_socket(world.home.data.path()).to_string_lossy()
    );
    assert_eq!(
        launch.env("TDDY_REMOTE_DAEMON_URL"),
        this_daemons_local_url()
    );

    // And it holds a credential of its own — not the caller's short-lived one — that this daemon
    // verifies as the caller, which is how its tool calls are authorized
    let agent_token = launch.env("TDDY_REMOTE_SESSION_TOKEN");
    assert_ne!(agent_token, "");
    assert_ne!(agent_token, a_caller_token());
    let claims = this_daemons_session_tokens()
        .verifier()
        .verify_now(agent_token)
        .expect("the session's agent token verifies under this daemon's key");
    let caller = this_daemons_session_tokens()
        .verifier()
        .verify_now(a_caller_token())
        .expect("the caller's token verifies");
    assert_eq!(claims.login, caller.login);
}

#[tokio::test]
async fn a_resumed_sessions_agent_is_given_the_transport_again_under_a_fresh_credential() {
    // Given an ordinary session that ran and was stopped
    let world = a_daemon_with_a_project();
    let session_id = world.a_person_starts_an_ordinary_session().await;
    world.launch_of(&session_id).await; // the agent is up before it is stopped

    // When it is resumed
    world.stop_and_resume(&session_id).await;
    let launch = world.launch_of(&session_id).await;

    // Then the resumed agent is wired to this daemon as the session again, under a credential
    // minted now. Nothing persists the previous process's token, so a resume can only mint.
    assert_eq!(launch.env("TDDY_REMOTE_SESSION_ID"), session_id);
    let agent_token = launch.env("TDDY_REMOTE_SESSION_TOKEN");
    assert_ne!(agent_token, a_caller_token());
    this_daemons_session_tokens()
        .verifier()
        .verify_now(agent_token)
        .expect("the resumed agent's token verifies under this daemon's key");
}
