//! Closing a session of a [`TmuxAgentDaemon`] — `PreviewCloseSession` and `CloseSession`
//! (docs/ft/daemon/session-liveness.md § Closing a session) — and what a test reads around it: the
//! events a close streamed, the session's listing, its tmux-wrapped tabs, and the project's git
//! state.

use std::path::{Path, PathBuf};

use tddy_cli_sessions::tmux::agent_tab_tmux_session_name;
use tddy_rpc::{Request, Status};
use tddy_service::proto::session::{
    CloseSessionPreview, CloseSessionRequest, DeleteSessionRequest, ListSessionsRequest,
    PreviewCloseSessionRequest, ResumeSessionRequest, SessionEntry,
    SessionService as SessionServiceTrait,
};
use tddy_session_lifecycle::CloseSessionEventStream;
use tddy_terminal_rpc::proto::terminal_session::{
    LaunchNode as WireLaunchNode, StartTerminalSessionRequest,
    TerminalSessionService as TerminalSessionServiceTrait,
};
use tddy_testing_commons::remote_target::write_executable;
use tddy_testing_commons::wait::eventually;

use super::close_events::CloseEvents;
use super::TmuxAgentDaemon;
use crate::support::{OBSERVE, VALID_TOKEN};

/// The marker a close leaves in the session directory once it removed the session's worktrees.
pub const WORKTREE_REMOVED_MARKER: &str = "worktree-removed";

/// A close request for `session_id`: unconfirmed, or confirming the preview whose fingerprint is
/// `fingerprint`.
pub fn a_close_request(
    session_id: &str,
    confirmed_fingerprint: Option<&str>,
) -> CloseSessionRequest {
    CloseSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        session_id: session_id.to_string(),
        confirmed: confirmed_fingerprint.is_some(),
        confirmed_fingerprint: confirmed_fingerprint.unwrap_or_default().to_string(),
    }
}

impl TmuxAgentDaemon {
    /// What closing `session_id` would lose, as `PreviewCloseSession` answers.
    pub async fn preview_of_closing(
        &self,
        session_id: &str,
    ) -> Result<CloseSessionPreview, Status> {
        self.host
            .preview_close_session(Request::direct(PreviewCloseSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: session_id.to_string(),
            }))
            .await
            .map(|preview| preview.into_inner())
    }

    /// [`Self::preview_of_closing`], which must succeed.
    pub async fn previews_closing(&self, session_id: &str) -> CloseSessionPreview {
        self.preview_of_closing(session_id)
            .await
            .expect("PreviewCloseSession must succeed")
    }

    /// Start `request`'s close: the stream of its events, or the refusal of the call.
    pub async fn starts_closing_with(
        &self,
        request: CloseSessionRequest,
    ) -> Result<CloseSessionEventStream, Status> {
        self.host
            .close_session(Request::direct(request))
            .await
            .map(|stream| stream.into_inner())
    }

    /// Start an unconfirmed close of `session_id`.
    pub async fn starts_closing(
        &self,
        session_id: &str,
    ) -> Result<CloseSessionEventStream, Status> {
        self.starts_closing_with(a_close_request(session_id, None))
            .await
    }

    /// Run `request`'s close to its end, and answer every event it streamed.
    pub async fn closes_with(&self, request: CloseSessionRequest) -> CloseEvents {
        let stream = self
            .starts_closing_with(request)
            .await
            .expect("CloseSession must start");
        CloseEvents::read_to_end(stream).await
    }

    /// Close `session_id` without confirming anything.
    pub async fn closes(&self, session_id: &str) -> CloseEvents {
        self.closes_with(a_close_request(session_id, None)).await
    }

    /// Close `session_id`, confirming the preview whose fingerprint is `fingerprint`.
    pub async fn closes_confirming(&self, session_id: &str, fingerprint: &str) -> CloseEvents {
        self.closes_with(a_close_request(session_id, Some(fingerprint)))
            .await
    }

    /// Delete `session_id`, answering the refusal rather than expecting success.
    pub async fn delete_of(&self, session_id: &str) -> Result<(), Status> {
        self.host
            .delete_session(Request::direct(DeleteSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: session_id.to_string(),
            }))
            .await
            .map(|_| ())
    }

    /// `session_id`'s row in `ListSessions`.
    pub async fn listed(&self, session_id: &str) -> SessionEntry {
        self.host
            .list_sessions(Request::direct(ListSessionsRequest {
                session_token: VALID_TOKEN.to_string(),
            }))
            .await
            .expect("ListSessions must succeed")
            .into_inner()
            .sessions
            .into_iter()
            .find(|entry| entry.session_id == session_id)
            .unwrap_or_else(|| panic!("session {session_id} is listed"))
    }

    /// The worktree path `session_id` records, exactly as it records it.
    pub fn recorded_worktree_of(&self, session_id: &str) -> String {
        self.metadata_of(session_id)
            .repo_path
            .expect("the session records its worktree")
    }

    /// The branch checked out in `session_id`'s worktree.
    pub fn branch_of(&self, session_id: &str) -> String {
        git(
            &self.worktree_of(session_id),
            &["rev-parse", "--abbrev-ref", "HEAD"],
        )
    }

    /// The project's main clone, which the session's worktree was cut from.
    pub fn project_repo(&self) -> PathBuf {
        self._repo.path().canonicalize().unwrap()
    }

    /// Whether the project's clone still has `branch`.
    pub fn project_has_branch(&self, branch: &str) -> bool {
        std::process::Command::new("git")
            .arg("-C")
            .arg(self.project_repo())
            .args(["rev-parse", "--verify", "--quiet"])
            .arg(format!("refs/heads/{branch}"))
            .output()
            .expect("git runs")
            .status
            .success()
    }

    /// Whether the project's clone lists `worktree` among its worktrees.
    pub fn project_lists_worktree(&self, worktree: &Path) -> bool {
        git(&self.project_repo(), &["worktree", "list", "--porcelain"])
            .lines()
            .filter_map(|line| line.strip_prefix("worktree "))
            .any(|listed| {
                Path::new(listed).canonicalize().ok().as_deref() == Some(worktree)
                    || Path::new(listed) == worktree
            })
    }

    /// The agent left `file` in `session_id`'s worktree, uncommitted and untracked.
    pub fn agent_leaves_a_file(&self, session_id: &str, file: &str) {
        std::fs::write(
            self.worktree_of(session_id).join(file),
            "left by the agent\n",
        )
        .unwrap();
    }

    /// The agent committed `file` in `session_id`'s worktree with `subject`, pushing it nowhere.
    /// Answers the commit's sha.
    pub fn agent_commits(&self, session_id: &str, file: &str, subject: &str) -> String {
        let worktree = self.worktree_of(session_id);
        std::fs::write(worktree.join(file), format!("{subject}\n")).unwrap();
        git(&worktree, &["add", file]);
        git(&worktree, &["commit", "-q", "-m", subject]);
        git(&worktree, &["rev-parse", "HEAD"])
    }

    /// `session_id`'s worktree ignores `pattern`, through a `.gitignore` the agent left in it.
    pub fn worktree_ignores(&self, session_id: &str, pattern: &str) {
        std::fs::write(
            self.worktree_of(session_id).join(".gitignore"),
            format!("{pattern}\n"),
        )
        .unwrap();
    }

    /// The session's worktree was removed outside the daemon (`git worktree remove --force`).
    pub fn worktree_is_removed_outside_the_daemon(&self, session_id: &str) {
        let worktree = self.worktree_of(session_id);
        git(
            &self.project_repo(),
            &["worktree", "remove", "--force", worktree.to_str().unwrap()],
        );
    }

    /// Resume `session_id`, answering the refusal rather than expecting success.
    pub async fn resume_of(&self, session_id: &str) -> Result<(), Status> {
        self.host
            .resume_session(Request::direct(ResumeSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: session_id.to_string(),
            }))
            .await
            .map(|_| ())
    }

    /// Start an agent tab of `session_id` wrapped in tmux: its terminal id, or the refusal.
    pub async fn start_of_a_tmux_wrapped_agent_tab(
        &self,
        session_id: &str,
    ) -> Result<String, Status> {
        self.host
            .terminal_session_service()
            .start_terminal_session(Request::direct(StartTerminalSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: session_id.to_string(),
                control_token: String::new(),
                kind: "claude-cli".to_string(),
                tmux_session: String::new(),
                wrappers: vec![WireLaunchNode {
                    kind: "tmux".to_string(),
                    properties: Default::default(),
                }],
            }))
            .await
            .map(|started| started.into_inner().terminal_id)
    }

    /// `session_id`'s tab roster (`tabs.yaml`) can no longer be read: it was left half-written.
    pub fn tab_roster_is_left_half_written(&self, session_id: &str) {
        std::fs::write(
            self.session_dir(session_id).join("tabs.yaml"),
            "tabs: [half-written\n",
        )
        .unwrap();
    }

    /// Open an agent tab of `session_id` wrapped in tmux, and wait until its tmux session is up.
    /// Answers its terminal id and that tmux session.
    pub async fn opens_a_tmux_wrapped_agent_tab(&self, session_id: &str) -> (String, String) {
        let terminal_id = self
            .start_of_a_tmux_wrapped_agent_tab(session_id)
            .await
            .expect("a tmux-wrapped agent tab starts");
        let name = agent_tab_tmux_session_name(session_id, &terminal_id);
        eventually("the tab's tmux session to be up", OBSERVE, || {
            self.tmux
                .has_session(&name)
                .then_some(())
                .ok_or_else(|| format!("tmux has no session {name:?} yet"))
        })
        .await;
        (terminal_id, name)
    }

    /// This daemon's instance id, as a split session's record names its codebase daemon.
    pub fn instance_id(&self) -> String {
        tddy_daemon_kernel::daemon_identity::local_instance_id_for_config(self.host.config())
    }

    /// `session_id` records workspace session `codebase_session_id` on `daemon_instance_id` as its
    /// codebase half — a split session whose checkout lives there.
    pub fn records_its_codebase_half_as(
        &self,
        session_id: &str,
        daemon_instance_id: &str,
        codebase_session_id: &str,
    ) {
        let mut metadata = self.metadata_of(session_id);
        metadata.codebase_daemon_instance_id = Some(daemon_instance_id.to_string());
        metadata.codebase_session_id = Some(codebase_session_id.to_string());
        tddy_core::write_session_metadata(&self.session_dir(session_id), &metadata)
            .expect("the session's .session.yaml is rewritten");
    }

    /// `session_id`'s record names a roster agent on `daemon_instance_id` reading the clone that
    /// daemon holds as workspace session `codebase_session_id` — as an attach records one, and as
    /// a restarted daemon knows it: from the record alone.
    pub fn records_an_agent_reading_a_clone(
        &self,
        session_id: &str,
        daemon_instance_id: &str,
        codebase_session_id: &str,
    ) {
        let mut metadata = self.metadata_of(session_id);
        metadata.agents.push(tddy_core::SessionAgentRecord {
            agent_id: format!("explorer@{daemon_instance_id}"),
            name: "explorer".to_string(),
            daemon_instance_id: daemon_instance_id.to_string(),
            label: None,
            model: String::new(),
            replaces: Vec::new(),
            tools: Vec::new(),
            codebase_session_id: Some(codebase_session_id.to_string()),
            kind: Default::default(),
            harness: None,
            placement: Default::default(),
        });
        tddy_core::write_session_metadata(&self.session_dir(session_id), &metadata)
            .expect("the session's .session.yaml is rewritten");
    }

    /// The clones (their workspace sessions) `session_id`'s record names.
    pub fn clones_recorded_for(&self, session_id: &str) -> Vec<String> {
        self.metadata_of(session_id)
            .agents
            .into_iter()
            .filter_map(|agent| agent.codebase_session_id)
            .collect()
    }

    /// `session_id`'s record can no longer be read: its `.session.yaml` was left half-written.
    pub fn record_is_left_half_written(&self, session_id: &str) {
        std::fs::write(
            self.session_dir(session_id).join(".session.yaml"),
            "session_id: [half-written\n",
        )
        .unwrap();
    }

    /// Whether the project's repository still has `reference`.
    pub fn project_has_ref(&self, reference: &str) -> bool {
        std::process::Command::new("git")
            .arg("-C")
            .arg(self.project_repo())
            .args(["rev-parse", "--verify", "--quiet", reference])
            .output()
            .expect("git runs")
            .status
            .success()
    }

    /// From now on the daemon's tmux refuses to kill any session — every other tmux command runs
    /// as before — and the pane of `tmux_session` is kept after its process exits, so nothing
    /// signalling its agent ends the tmux session either. The tmux session survives a terminate.
    pub fn tmux_refuses_to_kill(&self, tmux_session: &str) {
        let kept = self.tmux.tmux(&[
            "set-option",
            "-t",
            &format!("={tmux_session}:"),
            "remain-on-exit",
            "on",
        ]);
        assert!(kept.status.success(), "remain-on-exit is set");
        let real_tmux = String::from_utf8(
            std::process::Command::new("sh")
                .args(["-c", "command -v tmux"])
                .output()
                .expect("sh runs")
                .stdout,
        )
        .unwrap();
        let refusing = self.sessions_base.join("refusing-tmux");
        write_executable(
            &refusing,
            &format!(
                "#!/bin/sh\nfor arg in \"$@\"; do\n  if [ \"$arg\" = kill-session ]; then echo 'kill-session refused' >&2; exit 1; fi\ndone\nexec '{}' \"$@\"\n",
                real_tmux.trim()
            ),
        );
        // Cleared with the rest of the private tmux's environment when it is dropped.
        std::env::set_var(
            tddy_cli_sessions::tmux::TMUX_BINARY_ENV,
            refusing.to_str().unwrap(),
        );
    }
}

/// What `git -C <dir> <args>` prints; the command must succeed.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "t@t.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "t@t.com")
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?} in {} failed: {}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}
