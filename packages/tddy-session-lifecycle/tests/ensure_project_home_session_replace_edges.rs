//! Acceptance: the edges of `EnsureProjectHomeSession` around an existing main — a replace the
//! start refuses (recipe, caller-chosen id, provider account), a recorded main this daemon no
//! longer serves, a home worktree that cannot be fast-forwarded, a stopped main with no worktree
//! recorded, an ensure addressed to a daemon this one does not know, and a main whose id cannot be
//! recorded on the project.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home session.

mod ensure_project_home_world;
mod project_home_support;

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use ensure_project_home_world::{a_claude_cli_spec, a_daemon_with_a_project, World};
use project_home_support::{git, PROJECT_ID, VALID_TOKEN};
use tddy_rpc::{Code, Request};
use tddy_service::proto::session::{
    EnsureProjectHomeSessionRequest, EnsureProjectHomeSessionResponse, SessionEntry,
    SessionService as SessionServiceTrait, StartSessionRequest,
};

const UNKNOWN_DAEMON_ID: &str = "a-daemon-nobody-knows";

/// The projects directory made read-only for as long as this lives, so the project's record cannot
/// be rewritten; writable again on drop, so the fixture's temp dir can be removed.
struct ReadOnlyProjectsDir(PathBuf);

impl Drop for ReadOnlyProjectsDir {
    fn drop(&mut self) {
        std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755))
            .expect("the projects dir is made writable again");
    }
}

impl World {
    /// Set main up from `spec`, addressed to the daemon `daemon_instance_id` names.
    async fn set_up_main_on(
        &self,
        daemon_instance_id: &str,
        spec: StartSessionRequest,
    ) -> Result<EnsureProjectHomeSessionResponse, tddy_rpc::Status> {
        self.home
            .service
            .ensure_project_home_session(Request::direct(EnsureProjectHomeSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                project_id: PROJECT_ID.to_string(),
                daemon_instance_id: daemon_instance_id.to_string(),
                start: Some(spec),
            }))
            .await
            .map(|r| r.into_inner())
    }

    /// Commit on `session_id`'s home worktree, so it no longer fast-forwards to the main ref once
    /// that moves on.
    fn commit_on_the_home_worktree_of(&self, session_id: &str) {
        git(
            &self.home_worktree_of(session_id),
            &[
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "a commit of main's own",
            ],
        );
    }

    /// Drop the worktree `session_id`'s metadata records.
    fn forget_the_worktree_of(&self, session_id: &str) {
        let mut metadata = self.metadata_of(session_id);
        metadata.repo_path = None;
        self.rewrite_metadata_of(session_id, &metadata);
    }

    fn with_a_read_only_projects_dir(&self) -> ReadOnlyProjectsDir {
        let projects = self.home.data.path().join("projects");
        std::fs::set_permissions(&projects, std::fs::Permissions::from_mode(0o555))
            .expect("the projects dir is made read-only");
        ReadOnlyProjectsDir(projects)
    }

    /// The id of the one session the daemon has created.
    fn the_only_session_id(&self) -> String {
        let mut ids = std::fs::read_dir(self.home.data.path().join("sessions"))
            .expect("the sessions dir reads")
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned());
        let id = ids.next().expect("a session was created");
        assert_eq!(ids.next(), None, "exactly one session was created");
        id
    }

    /// `old` is still the project's main: its agent runs, the project records it, and no other
    /// session was created.
    async fn still_has_main(&self, old: &SessionEntry) {
        assert!(
            self.is_served(&old.session_id).await,
            "the old main still runs"
        );
        assert_eq!(self.recorded_main(), Some(old.session_id.clone()));
        assert_eq!(self.session_dirs(), 1, "no other session was created");
    }
}

#[tokio::test]
async fn a_replace_refused_for_an_unknown_recipe_leaves_the_running_main_running_and_recorded() {
    // Given a running main
    let world = a_daemon_with_a_project();
    let old = world.a_running_main().await;

    // When main is set up again on a recipe no daemon knows
    let refusal = world
        .set_up_main(StartSessionRequest {
            recipe: "no-such-recipe".to_string(),
            ..a_claude_cli_spec()
        })
        .await
        .expect_err("an unknown recipe is refused");

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
    world.still_has_main(&old).await;
}

#[tokio::test]
async fn a_replace_refused_for_a_caller_chosen_session_id_leaves_the_running_main_running_and_recorded(
) {
    // Given a running main
    let world = a_daemon_with_a_project();
    let old = world.a_running_main().await;

    // When main is set up again under an id the caller chose (only workspace sessions take one)
    let refusal = world
        .set_up_main(StartSessionRequest {
            requested_session_id: "a-caller-chosen-id".to_string(),
            ..a_claude_cli_spec()
        })
        .await
        .expect_err("a caller-chosen session id is refused");

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert!(
        refusal.message().contains("requested_session_id"),
        "names the refused field: {}",
        refusal.message()
    );
    world.still_has_main(&old).await;
}

#[tokio::test]
async fn a_replace_refused_for_a_provider_account_with_no_vault_leaves_the_running_main_running_and_recorded(
) {
    // Given a running main, on a daemon with no credential vault configured
    let world = a_daemon_with_a_project();
    let old = world.a_running_main().await;

    // When main is set up again on a provider account
    let refusal = world
        .set_up_main(StartSessionRequest {
            provider_account_id: "an-account".to_string(),
            ..a_claude_cli_spec()
        })
        .await
        .expect_err("an account no vault can hold is refused");

    // Then
    assert_eq!(refusal.code(), Code::FailedPrecondition);
    world.still_has_main(&old).await;
}

#[tokio::test]
async fn replacing_a_recorded_main_this_daemon_no_longer_serves_starts_and_records_the_new_one() {
    // Given a recorded main whose agent has stopped
    let world = a_daemon_with_a_project();
    let old = world.a_running_main().await;
    world.stop(&old.session_id).await;

    // When main is set up again
    let replaced = world
        .set_up_main(a_claude_cli_spec())
        .await
        .expect("replacing main succeeds");

    // Then
    assert_eq!(replaced.outcome, "replaced");
    let new = replaced.session.expect("the new main is returned");
    assert_ne!(new.session_id, old.session_id);
    assert_eq!(world.recorded_main(), Some(new.session_id.clone()));
    assert!(world.is_served(&new.session_id).await, "the new main runs");
}

#[tokio::test]
async fn a_replace_whose_home_worktree_cannot_fast_forward_is_refused_and_leaves_the_running_main()
{
    // Given a running main whose home worktree has a commit of its own, and a main branch that
    // moved on since
    let world = a_daemon_with_a_project();
    let old = world.a_running_main().await;
    world.commit_on_the_home_worktree_of(&old.session_id);
    world.advance_origin_main();

    // When main is set up again
    let refusal = world
        .set_up_main(a_claude_cli_spec())
        .await
        .expect_err("a diverged home worktree is refused");

    // Then
    assert_eq!(refusal.code(), Code::FailedPrecondition);
    assert!(
        refusal.message().contains("home worktree"),
        "names the home worktree: {}",
        refusal.message()
    );
    world.still_has_main(&old).await;
}

#[tokio::test]
async fn ensuring_a_stopped_main_with_no_recorded_worktree_is_refused_and_does_not_resume() {
    // Given a stopped main whose metadata records no worktree
    let world = a_daemon_with_a_project();
    let main = world.a_running_main().await;
    world.stop(&main.session_id).await;
    world.forget_the_worktree_of(&main.session_id);

    // When
    let refusal = world
        .ensure()
        .await
        .expect_err("a main with no worktree is not resumed");

    // Then
    assert_eq!(refusal.code(), Code::FailedPrecondition);
    assert!(
        refusal.message().contains("no worktree"),
        "says what is missing: {}",
        refusal.message()
    );
    assert!(
        !world.is_served(&main.session_id).await,
        "main stays stopped"
    );
    assert_eq!(world.recorded_main(), Some(main.session_id));
}

#[tokio::test]
async fn setting_up_main_on_a_daemon_this_one_does_not_know_is_refused_and_starts_nothing() {
    // Given
    let world = a_daemon_with_a_project();

    // When main is set up on a daemon that is neither this one nor a peer of it
    let refusal = world
        .set_up_main_on(UNKNOWN_DAEMON_ID, a_claude_cli_spec())
        .await
        .expect_err("an unknown daemon is refused");

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert_eq!(world.session_dirs(), 0);
    assert_eq!(world.recorded_main(), None);
}

#[tokio::test]
async fn a_main_whose_id_cannot_be_recorded_is_stopped_and_the_set_up_fails() {
    // Given a project whose record cannot be rewritten
    let world = a_daemon_with_a_project();
    let _read_only = world.with_a_read_only_projects_dir();

    // When
    let failure = world
        .set_up_main(a_claude_cli_spec())
        .await
        .expect_err("a main that cannot be recorded fails the set-up");

    // Then the main that started is stopped again, and the project records none
    assert_eq!(failure.code(), Code::Internal);
    assert!(
        failure.message().contains("recording project"),
        "fails on recording main: {}",
        failure.message()
    );
    let started = world.the_only_session_id();
    assert!(
        !world.is_served(&started).await,
        "the unrecorded main is stopped"
    );
    assert_eq!(world.recorded_main(), None);
}
