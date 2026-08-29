//! Acceptance: a `tddy-session-sync` mirror kept equal to a live session's worktree, end to end —
//! `docs/ft/daemon/session-worktree-sync.md` AC31-AC36.
//!
//! Nothing here is stubbed. A real LiveKit server carries the room; a real daemon starts a real
//! `claude-cli` session, cuts a real `git worktree` for it and runs the real poll loop over it; the
//! real `connection.ConnectionService` serves `StreamAgentActivityDelta` inside the room; the real
//! `remote_git.RemoteGitService` serves the project as a git remote, reached by the real
//! `tddy-remote-git-repo` binary as git's `GIT_SSH_COMMAND`; and the client under test is
//! `tddy_session_sync::sync::run` itself, attached through `tddy_session_sync::attach`.
//!
//! **Every assertion is on the mirror's resulting bytes**, never on patch text — the changeset is
//! explicit that pinning `git diff --binary` output would pin git's formatting rather than the
//! behaviour. That also makes each test indifferent to *how* the mirror caught up: applying the
//! tick's patch and reconciling from the WIP ref are both correct outcomes of the same contract,
//! and which one a run takes depends on where a 200ms poll tick happened to land.
//!
//! ⚠️ **Slow on purpose, and far outside the ordinary integration-test budget.** Every test starts
//! (or reuses) a LiveKit container, starts a session, waits for a poll tick that shells out to git,
//! clones a repository over LiveKit and then waits for further ticks. `#[serial]`, because the
//! container and the process environment are shared.
//!
//! Requires Docker or `LIVEKIT_TESTKIT_WS_URL`, and a built `tddy-remote-git-repo` — the git
//! transport is looked up on `PATH` by name, exactly as an operator's shell would find it.
//!
//! Placed in `tddy-daemon` rather than in `tddy-session-sync`, where the changeset first proposed
//! it: `tddy-daemon` already depends on `tddy-session-sync` as a library, so the client is reachable
//! from here, while the reverse would make the whole daemon a dev-dependency of the client.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use serial_test::serial;
use tddy_core::agent_activity::{append_agent_activity, AgentActivityRecord, STATUS_COMPLETED};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::project_storage::{write_projects, ProjectData};
use tddy_daemon::remote_git_service::{ProjectsDirResolver, RemoteGitServiceImpl, UserResolver};
use tddy_livekit::{LiveKitParticipant, RoomOptions};
use tddy_livekit_testkit::LiveKitTestkit;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, StartSessionRequest,
};
use tddy_session_sync::{Credentials, DaemonToken, LiveKitCredentials};
use tddy_testing_commons::stub_scripts::a_stub_agent_script;
use tddy_testing_commons::wait::eventually;

/// The daemon under test: it runs the session, hosts its room, serves the deltas and serves the
/// project as a git remote. One instance id for all four, because a participant addresses one
/// daemon by one name wherever it meets it.
const INSTANCE_ID: &str = "session-sync-e2e-host";

/// The lobby this daemon serves `remote_git.RemoteGitService` in — `MintLiveKitToken` grants the
/// common room and only the common room, which is why the git transport meets the daemon here and
/// the syncer meets it in the session room.
const COMMON_ROOM: &str = "session-sync-e2e-lobby";

const LK_API_KEY: &str = "devkey";

/// The one secret this deployment shares: LiveKit's API secret *and* the key session tokens are
/// signed with. The syncer holds it because no minted token admits it to a session room — recorded
/// in the PRD as a real widening of the trust surface.
const FLEET_SECRET: &str = "secret";

const GITHUB_USER: &str = "testuser";
const PROJECT_NAME: &str = "session-sync-app";
const PROJECT_ID: &str = "0198f1b0-0000-7000-8000-0000000053c0";

/// Short enough that a tick lands inside a test's patience, long enough that a loaded machine is
/// not spending its whole slice shelling out to git. The configured floor is 100ms.
const POLL_INTERVAL_MS: u64 = 200;

/// A tracked file, committed before the session's worktree is cut, so every checkout carries it
/// from the moment its room opens and an edit to it is a change with a real pre-image.
const SEEDED_FILE: &str = "notes.md";
const SEEDED_CONTENTS: &str = "one\n";
const EDITED_CONTENTS: &str = "one\ntwo\n";

/// The file an agent writes that no commit ever mentions — AC31's whole point.
const A_DRAFTED_FILE: &str = "draft.md";
const DRAFTED_CONTENTS: &str = "a draft the agent has not committed\n";

/// A file whose bytes cover the whole 0-255 range, including NUL: git calls it binary and diffs it
/// as a literal rather than as text, which is the path AC34 is about.
const A_BINARY_FILE: &str = "payload.bin";

/// What a hand-corrupted mirror holds instead of the file it is supposed to (AC36). Deliberately
/// nothing like [`SEEDED_CONTENTS`], so no patch cut against the real pre-image could apply onto it.
const CORRUPTED_CONTENTS: &str = "somebody edited the mirror by hand\n";

/// Call ids, one per test, fixed rather than generated: a call id is the key the whole delta path
/// is looked up by, and a generated one would read as though the value mattered.
const A_WRITE_CALL: &str = "0199c7a4-0001-7c9a-9d1e-3f0a1b2c3d4e";
const AN_EDIT_CALL: &str = "0199c7a4-0002-7c9a-9d1e-3f0a1b2c3d4e";
const A_DELETE_CALL: &str = "0199c7a4-0003-7c9a-9d1e-3f0a1b2c3d4e";
const A_BINARY_WRITE_CALL: &str = "0199c7a4-0004-7c9a-9d1e-3f0a1b2c3d4e";
const A_COMMITTED_EDIT_CALL: &str = "0199c7a4-0005-7c9a-9d1e-3f0a1b2c3d4e";
const A_REPAIRING_EDIT_CALL: &str = "0199c7a4-0006-7c9a-9d1e-3f0a1b2c3d4e";

/// Files used only to move the checkout past the first two ticks of its room, so a delta the mirror
/// can act on exists before any test does anything. See [`AMirroredSession::warm_the_room`].
const A_SCRATCH_FILE: &str = "scratch-1.txt";
const ANOTHER_SCRATCH_FILE: &str = "scratch-2.txt";

/// A cold LiveKit container has to admit every participant, the daemon shells out to git on a
/// blocking pool for every tick, and the mirror clones a repository over the room before it can
/// answer anything — so a run legitimately spans many poll intervals on a machine that is also
/// compiling. Well past the integration budget by design (see the module note); the condition
/// decides when to stop, and this only decides when to give up.
const MIRROR_TIMEOUT: Duration = Duration::from_secs(90);

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;

// ---------------------------------------------------------------------------
// The fixture
// ---------------------------------------------------------------------------

/// A live session with a `tddy-session-sync` mirror of its worktree running beside it.
struct AMirroredSession {
    session_id: String,
    /// The agent's checkout — what the mirror is kept equal to.
    worktree: PathBuf,
    /// Where the agent's `agent-activity.jsonl` is written, which is the only place the daemon
    /// reads tool calls from.
    session_dir: PathBuf,
    /// The project's repository. The session's worktree is a linked worktree of it, so this is
    /// where the WIP ref lands and what the git transport serves.
    repo: PathBuf,
    /// The directory the syncer owns and mirrors into.
    mirror: PathBuf,
    /// The syncer's failure, if it stopped. `run` returns only on error, so anything in here ends
    /// the test immediately rather than as a timeout with no explanation.
    syncer_failure: Arc<OnceLock<String>>,
    /// `None` until [`AMirroredSession::with_a_syncer_attached`] has run; the room has to be warm
    /// before a mirror is built from it.
    _syncer: Option<AbortOnDrop>,
    _daemon_http: AbortOnDrop,
    _lobby: AbortOnDrop,
    _agent: tempfile::TempDir,
    _testkit: LiveKitTestkit,
    _home: tempfile::TempDir,
    _config_dir: tempfile::TempDir,
}

/// A task that lives exactly as long as the fixture that started it.
struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Start a session on a real daemon, then attach a real syncer to it and wait until the mirror is
/// standing.
///
/// `suffix` names this test's own session, and therefore its own room, so one test's ticks and
/// broadcasts can never be mistaken for another's on a shared server.
async fn a_mirrored_session(suffix: &str) -> AMirroredSession {
    the_git_transport_is_on_path();

    let testkit = LiveKitTestkit::start()
        .await
        .expect("LiveKit testkit (Docker, or LIVEKIT_TESTKIT_WS_URL)");
    let ws_url = testkit.get_ws_url();

    let home = tempfile::tempdir().expect("tempdir");
    let repo = home.path().join("repos").join(PROJECT_NAME);
    a_project_repository(&repo);

    let data_dir = home.path().join("tddy-data");
    let projects_dir = data_dir.join("projects");
    std::fs::create_dir_all(&projects_dir).expect("create the projects directory");
    write_projects(&projects_dir, &[a_project_at(&repo)]).expect("write projects.yaml");

    let agent = tempfile::tempdir().expect("tempdir");
    let agent_binary = a_stub_agent_script(agent.path(), "stub-claude.sh")
        .then_reading_stdin()
        .build();

    let config_dir = tempfile::tempdir().expect("tempdir");
    let config_path = config_dir.path().join("daemon.yaml");
    std::fs::write(&config_path, a_daemon_yaml(&ws_url, &agent_binary)).expect("write daemon.yaml");
    let config = DaemonConfig::load(&config_path).expect("daemon.yaml must load");

    // The daemon's real auth wiring: one resolver verifies the session token everywhere, as in
    // production — the RPCs in the room, the git transport, and the mint the transport calls.
    let auth = tddy_daemon::auth::build_auth_entries(&config, "127.0.0.1", 0)
        .expect("the daemon's auth must build");
    let user_resolver: UserResolver = auth
        .user_resolver
        .clone()
        .expect("a daemon configured with github: stub must produce a resolver");

    let sessions_base = data_dir.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let connections = ConnectionServiceImpl::new(
        config.clone(),
        sessions_base_resolver,
        data_dir.clone(),
        user_resolver.clone(),
        None,
        None,
        None,
        Arc::new(tddy_daemon::claude_cli_session::ClaudeCliSessionManager::new()),
    );

    // The daemon's Connect-HTTP surface: `attach` lists sessions on it, and the git transport
    // exchanges its token and mints its room JWT on it.
    let mut entries = auth.entries;
    entries.push(tddy_rpc::ServiceEntry {
        name: "connection.ConnectionService",
        service: Arc::new(tddy_service::ConnectionServiceServer::new(
            connections.clone(),
        )) as Arc<dyn tddy_rpc::RpcService>,
    });
    let router = tddy_connectrpc::connect_router(tddy_rpc::RpcBridge::new(
        tddy_rpc::MultiRpcService::new(entries),
    ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the daemon's HTTP surface");
    let daemon_url = format!("http://{}", listener.local_addr().expect("local addr"));
    let daemon_http = AbortOnDrop(tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    }));

    // The lobby participant serving the project as a git remote — where `tddy-remote-git-repo`
    // meets this daemon.
    let lobby = a_git_remote_in_the_lobby(
        &testkit,
        &ws_url,
        &config,
        user_resolver,
        projects_dir.clone(),
    )
    .await;

    let session_id = a_started_session(&connections, suffix).await;
    let session_dir = unified_session_dir_path(&data_dir, &session_id);
    let worktree = worktree_of(&session_dir);

    let session = AMirroredSession {
        session_id,
        worktree,
        session_dir,
        repo,
        mirror: home.path().join("mirror"),
        syncer_failure: Arc::new(OnceLock::new()),
        _syncer: None,
        _daemon_http: daemon_http,
        _lobby: lobby,
        _agent: agent,
        _testkit: testkit,
        _home: home,
        _config_dir: config_dir,
    };
    session.warm_the_room().await;
    session.with_a_syncer_attached(&ws_url, &daemon_url).await
}

impl AMirroredSession {
    /// Move the checkout past the two ticks a mirror cannot act on, before it exists.
    ///
    /// A room's first tick writes the WIP tree it will diff the next one against and records no
    /// delta; the tick after that records the room's *first* delta, and that one is numbered 0 —
    /// which is also the wire's "no tick has covered this call yet" sentinel, so a record stamped
    /// with it is indistinguishable from an unmeasured one and the mirror ignores it. Spending both
    /// on scratch files here means the first delta any test provokes is numbered 1, follows the
    /// sequence the mirror's first restore left it on, and applies.
    async fn warm_the_room(&self) {
        self.write_in_worktree(A_SCRATCH_FILE, b"scratch\n");
        let seeded = self.await_published_wip_ref().await;
        self.write_in_worktree(ANOTHER_SCRATCH_FILE, b"scratch again\n");
        self.await_published_wip_ref_beyond(&seeded).await;
    }

    /// Attach the real client and run it until the mirror holds the session's committed state.
    async fn with_a_syncer_attached(mut self, ws_url: &str, daemon_url: &str) -> Self {
        let credentials = Credentials {
            session_id: self.session_id.clone(),
            dest: self.mirror.clone(),
            livekit: LiveKitCredentials {
                url: ws_url.to_string(),
                api_key: LK_API_KEY.to_string(),
                api_secret: FLEET_SECRET.to_string(),
            },
            daemon_url: daemon_url.to_string(),
            token: DaemonToken::Access(an_access_token_for(GITHUB_USER)),
            // The same budget the assertions wait on, because it bounds the same things: how long
            // the daemon may be absent from the room, and how long a delta stream may say nothing
            // before it is declared wedged.
            connect_timeout: MIRROR_TIMEOUT,
        };
        let attached = tddy_session_sync::attach(&credentials)
            .await
            .unwrap_or_else(|e| panic!("the syncer must attach to {}: {e}", self.session_id));

        let failure = Arc::clone(&self.syncer_failure);
        self._syncer = Some(AbortOnDrop(tokio::spawn(async move {
            if let Err(e) = tddy_session_sync::sync::run(&credentials, attached).await {
                let _ = failure.set(e.to_string());
            }
        })));

        // The first attach clones the project and restores the session's uncommitted state onto it.
        // Waiting for the seeded file is what makes every test below start from a mirror that is
        // standing rather than one that is still being built.
        self.assert_the_mirror_holds(SEEDED_FILE, SEEDED_CONTENTS.as_bytes())
            .await;
        self
    }

    // --- what the agent does -----------------------------------------------------------------

    /// The agent writes a file and records the call it did so in.
    ///
    /// In that order, which is both what an agent does — the tool writes, then its hook records the
    /// completed row — and the order that cannot leave the mirror stale. A tick landing between the
    /// two attributes the record to a *later* delta than the one carrying the change, which the
    /// mirror sees as a sequence gap and repairs by restoring from the WIP ref. Recording first
    /// would attribute it to an *earlier* one, and a change announced by nothing is a change the
    /// mirror never learns about.
    fn the_agent_writes(&self, call_id: &str, path: &str, contents: &[u8]) {
        self.write_in_worktree(path, contents);
        self.record_activity(&a_completed_call(call_id, "Write", path, &self.head()));
    }

    fn the_agent_edits(&self, call_id: &str, path: &str, contents: &[u8]) {
        self.write_in_worktree(path, contents);
        self.record_activity(&a_completed_call(call_id, "Edit", path, &self.head()));
    }

    fn the_agent_deletes(&self, call_id: &str, path: &str) {
        std::fs::remove_file(self.worktree.join(path)).expect("remove the file from the checkout");
        self.record_activity(&a_completed_deletion(call_id, path, &self.head()));
    }

    /// The agent commits what it has written, and the commit it made.
    fn the_agent_commits(&self, message: &str) -> String {
        git_ok(&self.worktree, &["add", "-A"]);
        git_ok(&self.worktree, &["commit", "-m", message]);
        self.head()
    }

    /// Replace a file's contents in one step no poll can catch half-done.
    ///
    /// `std::fs::write` truncates before it writes, so a tick landing in between measures a state
    /// nobody asked for and diffs the next one against it. Writing beside the target and renaming
    /// makes the change atomic.
    fn write_in_worktree(&self, path: &str, contents: &[u8]) {
        let staged = self.worktree.join(format!(".{path}.partial"));
        std::fs::write(&staged, contents).expect("write the replacement beside the target");
        std::fs::rename(&staged, self.worktree.join(path))
            .expect("swap the replacement into place");
    }

    /// Record one of the agent's tool calls the way the agent itself does — by appending to the
    /// session's activity log, which is the only thing the daemon reads them from.
    fn record_activity(&self, record: &AgentActivityRecord) {
        append_agent_activity(&self.session_dir, record).expect("append to agent-activity.jsonl");
    }

    /// Overwrite a file inside the mirror, standing in for whatever wrote into a managed directory
    /// it does not own.
    fn somebody_overwrites_in_the_mirror(&self, path: &str, contents: &str) {
        std::fs::write(self.mirror.join(path), contents).expect("corrupt the mirror by hand");
    }

    // --- what the mirror shows ---------------------------------------------------------------

    /// Assert the mirror holds `path` with exactly `expected` bytes, once it has caught up.
    async fn assert_the_mirror_holds(&self, path: &str, expected: &[u8]) {
        eventually(
            &format!("the mirror to hold {path} as the session has it"),
            MIRROR_TIMEOUT,
            || {
                self.no_syncer_failure();
                let observed = std::fs::read(self.mirror.join(path))
                    .map_err(|e| format!("the mirror could not be read: {e}"))?;
                (observed == expected)
                    .then_some(())
                    .ok_or_else(|| describing(&observed, expected))
            },
        )
        .await;
    }

    /// Assert the mirror no longer holds `path`, once it has caught up.
    async fn assert_the_mirror_has_dropped(&self, path: &str) {
        eventually(
            &format!("the mirror to drop {path} as the session has"),
            MIRROR_TIMEOUT,
            || {
                self.no_syncer_failure();
                let target = self.mirror.join(path);
                (!target.exists())
                    .then_some(())
                    .ok_or_else(|| format!("{} is still there", target.display()))
            },
        )
        .await;
    }

    /// Assert the mirror's `HEAD` is on `expected`, once it has caught up.
    async fn assert_the_mirror_is_on_commit(&self, expected: &str) {
        eventually(
            &format!("the mirror to follow the session onto {expected}"),
            MIRROR_TIMEOUT,
            || {
                self.no_syncer_failure();
                let observed = git_ok(&self.mirror, &["rev-parse", "HEAD"]);
                (observed == expected)
                    .then_some(())
                    .ok_or_else(|| format!("the mirror was on {observed}"))
            },
        )
        .await;
    }

    /// End the test now if the syncer has stopped. `run` returns only on failure, so waiting out a
    /// mirror that has no syncer behind it would report a timeout for a fault that already has a
    /// message.
    fn no_syncer_failure(&self) {
        if let Some(reason) = self.syncer_failure.get() {
            panic!("the syncer stopped: {reason}");
        }
    }

    // --- what the session is -------------------------------------------------------------------

    fn head(&self) -> String {
        head_commit_of(&self.worktree)
    }

    fn wip_ref(&self) -> String {
        format!("refs/tddy/session/{}/wip", self.session_id)
    }

    /// The commit the session's WIP ref points at. Read from the *project* repository: a linked
    /// worktree shares its ref store with the repository it was cut from, which is exactly why an
    /// ordinary `git fetch` of that repository can reach it.
    fn published_wip_commit(&self) -> String {
        git_ok(&self.repo, &["rev-parse", &self.wip_ref()])
    }

    async fn await_published_wip_ref(&self) -> String {
        let expected = self.wip_ref();
        eventually(
            &format!("a poll tick to publish {expected}"),
            MIRROR_TIMEOUT,
            || {
                let published = git_ok(
                    &self.repo,
                    &["for-each-ref", "--format=%(refname)", "refs/tddy/"],
                );
                (published == expected)
                    .then_some(())
                    .ok_or_else(|| format!("refs/tddy/ held {published:?}"))
            },
        )
        .await;
        self.published_wip_commit()
    }

    /// Wait until the WIP ref has moved off `previous` — one further tick that measured a change,
    /// which is also the tick that recorded a delta for it.
    async fn await_published_wip_ref_beyond(&self, previous: &str) {
        eventually(
            &format!(
                "a poll tick to republish {} past {previous}",
                self.wip_ref()
            ),
            MIRROR_TIMEOUT,
            || {
                let published = self.published_wip_commit();
                (published != previous)
                    .then_some(())
                    .ok_or_else(|| format!("the WIP ref was still on {published}"))
            },
        )
        .await;
    }
}

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// The completed row an agent's hook writes for a call it has just finished, stamped as its writer
/// stamps it: the commit the call ran upon and the worktree-relative path it declared.
///
/// `activity_seq` is the one field a row never arrives with — nothing knows which tick will cover
/// the call until a tick does, and stamping that is the daemon's job.
fn a_completed_call(
    call_id: &str,
    tool_name: &str,
    file_path: &str,
    head_commit: &str,
) -> AgentActivityRecord {
    AgentActivityRecord {
        call_id: call_id.to_string(),
        tool_name: tool_name.to_string(),
        input: serde_json::json!({ "file_path": file_path }),
        status: STATUS_COMPLETED.to_string(),
        result: serde_json::Value::Null,
        error_message: String::new(),
        started_unix_ms: 1_780_828_020_298,
        completed_unix_ms: 1_780_828_020_299,
        source: "claude-cli".to_string(),
        head_commit: head_commit.to_string(),
        activity_seq: 0,
        changed_paths: vec![file_path.to_string()],
    }
}

/// The completed row for a call that removed a file. A deletion is a shell command rather than a
/// writing tool, so it declares its path outright — which is exactly what `changed_paths` is for.
fn a_completed_deletion(call_id: &str, file_path: &str, head_commit: &str) -> AgentActivityRecord {
    AgentActivityRecord {
        input: serde_json::json!({ "command": format!("rm {file_path}") }),
        tool_name: "Bash".to_string(),
        ..a_completed_call(call_id, "Bash", file_path, head_commit)
    }
}

/// A repository whose `origin` points at itself, so the daemon's worktree setup can fetch with no
/// server, holding [`SEEDED_FILE`] from its first commit.
fn a_project_repository(repo: &Path) {
    std::fs::create_dir_all(repo).expect("create the project repository");
    git_ok(repo, &["init", "--initial-branch=main"]);
    git_ok(repo, &["config", "user.email", "agent@example.com"]);
    git_ok(repo, &["config", "user.name", "Agent"]);
    std::fs::write(repo.join(SEEDED_FILE), SEEDED_CONTENTS).expect("seed the repository");
    git_ok(repo, &["add", "."]);
    git_ok(repo, &["commit", "-m", "seed"]);
    git_ok(repo, &["remote", "add", "origin", &repo.to_string_lossy()]);
    git_ok(repo, &["push", "-u", "origin", "main"]);
}

fn a_project_at(repo: &Path) -> ProjectData {
    ProjectData {
        project_id: PROJECT_ID.to_string(),
        name: PROJECT_NAME.to_string(),
        git_url: String::new(),
        main_repo_path: repo.to_string_lossy().to_string(),
        main_branch_ref: None,
        remote_name: None,
        host_repo_paths: Default::default(),
    }
}

/// The `daemon.yaml` of a daemon that runs agents, hosts their rooms and serves its projects.
fn a_daemon_yaml(ws_url: &str, agent_binary: &Path) -> String {
    format!(
        "daemon_instance_id: {INSTANCE_ID}\n\
         users:\n  - github_user: \"{GITHUB_USER}\"\n    os_user: \"{}\"\n\
         github:\n  stub: true\n\
         claude_cli:\n  binary_path: {}\n\
         session_room:\n  poll_interval_ms: {POLL_INTERVAL_MS}\n\
         livekit:\n  url: {ws_url}\n  api_key: {LK_API_KEY}\n  api_secret: {FLEET_SECRET}\n  \
         common_room: {COMMON_ROOM}\n",
        serving_os_user(),
        agent_binary.display(),
    )
}

/// The access token a signed-in browser would present, signed with the secret this deployment
/// shares.
fn an_access_token_for(login: &str) -> String {
    tddy_github::SessionTokenSigner::new(FLEET_SECRET.as_bytes()).mint_access(
        &tddy_github::GitHubUser {
            id: 4242,
            login: login.to_string(),
            avatar_url: String::new(),
            name: login.to_string(),
        },
    )
}

/// The OS user the daemon serves the project as. It has to exist: the git transport resolves it
/// through passwd to drop privilege to it before spawning `git-upload-pack`.
fn serving_os_user() -> String {
    std::env::var("USER").expect("USER must be set")
}

/// Join the lobby as this daemon, serving the project as a git remote.
async fn a_git_remote_in_the_lobby(
    testkit: &LiveKitTestkit,
    ws_url: &str,
    config: &DaemonConfig,
    user_resolver: UserResolver,
    projects_dir: PathBuf,
) -> AbortOnDrop {
    let projects_dir_resolver: ProjectsDirResolver =
        Arc::new(move |os_user| (os_user == serving_os_user()).then(|| projects_dir.clone()));
    let service = tddy_service::RemoteGitServiceServer::new(RemoteGitServiceImpl::new(
        user_resolver,
        projects_dir_resolver,
        Arc::new(config.clone()),
    ));
    let token = testkit
        .generate_token(COMMON_ROOM, &format!("daemon-{INSTANCE_ID}"))
        .expect("a LiveKit token for the daemon's lobby participant");
    let participant =
        LiveKitParticipant::connect(ws_url, &token, service, RoomOptions::default(), None, None)
            .await
            .expect("the daemon must join its lobby");
    AbortOnDrop(tokio::spawn(async move {
        let _ = participant.run().await;
    }))
}

/// Start the session whose worktree is mirrored, exactly as the web dashboard would.
async fn a_started_session(connections: &ConnectionServiceImpl, suffix: &str) -> String {
    let started = connections
        .start_session(tddy_rpc::Request::new(StartSessionRequest {
            session_token: an_access_token_for(GITHUB_USER),
            project_id: PROJECT_ID.to_string(),
            session_type: "claude-cli".to_string(),
            model: "claude-opus-5".to_string(),
            new_branch_name: format!("session-sync-{suffix}"),
            branch_worktree_intent: "new_branch_from_base".to_string(),
            ..Default::default()
        }))
        .await
        .expect("an agent session must start")
        .into_inner();
    started.session_id
}

/// The checkout the daemon cut for a session, as the session's own metadata records it.
fn worktree_of(session_dir: &Path) -> PathBuf {
    let metadata = tddy_core::read_session_metadata(session_dir)
        .unwrap_or_else(|e| panic!("session metadata at {session_dir:?} must be readable: {e}"));
    PathBuf::from(
        metadata
            .repo_path
            .expect("an agent session must record the checkout it runs in"),
    )
}

/// Every byte value, several times over — content no line-based diff can carry and no encoding can
/// survive being guessed at.
fn every_byte_value() -> Vec<u8> {
    (0..=255u8).cycle().take(4096).collect()
}

/// Put the git transport where git looks for it: `GIT_SSH_COMMAND` is the bare name
/// `tddy-remote-git-repo`, resolved on `PATH`, exactly as an operator's shell resolves it.
///
/// Mutating the process environment is safe here only because this whole suite is `#[serial]` and
/// every test wants the same value; it is done once and never undone.
fn the_git_transport_is_on_path() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let mut dir = std::env::current_exe().expect("test executable path");
        dir.pop(); // deps/
        if dir.ends_with("deps") {
            dir.pop();
        }
        assert!(
            dir.join("tddy-remote-git-repo").exists(),
            "tddy-remote-git-repo is not built at {}; build it before running this suite",
            dir.display()
        );
        let path = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", format!("{}:{path}", dir.display()));
    });
}

// ---------------------------------------------------------------------------
// Probes
// ---------------------------------------------------------------------------

/// What a mirror held where something else was expected, in a form a failure message can carry.
///
/// Binary content is summarised by length and by the first bytes that differ rather than printed:
/// a failure that dumps four kilobytes of every byte value tells nobody anything.
fn describing(observed: &[u8], expected: &[u8]) -> String {
    let first_difference = observed
        .iter()
        .zip(expected)
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| observed.len().min(expected.len()));
    format!(
        "the mirror held {} byte(s) where {} were expected, first differing at byte {first_difference}",
        observed.len(),
        expected.len()
    )
}

fn git_ok(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "Agent")
        .env("GIT_AUTHOR_EMAIL", "agent@example.com")
        .env("GIT_COMMITTER_NAME", "Agent")
        .env("GIT_COMMITTER_EMAIL", "agent@example.com")
        .output()
        .unwrap_or_else(|e| panic!("git must be on PATH: {e}"));
    assert!(
        output.status.success(),
        "git {args:?} failed in {cwd:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// The sha `HEAD` resolves to, checked rather than trusted: a git that could not run answers with
/// an empty string, and so does a measurement that failed — an unchecked helper would compare one
/// failure against another and call them equal.
fn head_commit_of(root: &Path) -> String {
    let sha = git_ok(root, &["rev-parse", "HEAD"]);
    // The sha is whatever git minted at commit time; only its shape can be pinned here.
    assert!(
        sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()),
        "git rev-parse HEAD in {root:?} answered {sha:?}, which is not a commit sha"
    );
    sha
}

// ---------------------------------------------------------------------------
// AC31-AC34 — the agent's uncommitted work reaches the mirror
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn mirrors_a_file_the_agent_wrote_without_a_commit() {
    // Given a session whose worktree is being mirrored
    let session = a_mirrored_session("write").await;

    // When the agent writes a file it never commits
    session.the_agent_writes(A_WRITE_CALL, A_DRAFTED_FILE, DRAFTED_CONTENTS.as_bytes());

    // Then the mirror holds it. Nothing in git's own transport carries an untracked file, which is
    // the entire reason this feature exists.
    session
        .assert_the_mirror_holds(A_DRAFTED_FILE, DRAFTED_CONTENTS.as_bytes())
        .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn mirrors_an_edit_to_an_existing_file() {
    // Given a session whose worktree is being mirrored
    let session = a_mirrored_session("edit").await;

    // When the agent edits a file that was already tracked
    session.the_agent_edits(AN_EDIT_CALL, SEEDED_FILE, EDITED_CONTENTS.as_bytes());

    // Then the mirror holds the file as it now is, not as it was committed
    session
        .assert_the_mirror_holds(SEEDED_FILE, EDITED_CONTENTS.as_bytes())
        .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn removes_a_file_the_agent_deleted() {
    // Given a session whose worktree is being mirrored
    let session = a_mirrored_session("delete").await;

    // When the agent deletes a tracked file
    session.the_agent_deletes(A_DELETE_CALL, SEEDED_FILE);

    // Then the mirror drops it too. A mirror that only ever gains files is one that diverges the
    // moment an agent cleans up after itself.
    session.assert_the_mirror_has_dropped(SEEDED_FILE).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn mirrors_binary_content_byte_for_byte() {
    // Given a session whose worktree is being mirrored
    let session = a_mirrored_session("binary").await;
    let bytes = every_byte_value();

    // When the agent writes a file whose content is not text
    session.the_agent_writes(A_BINARY_WRITE_CALL, A_BINARY_FILE, &bytes);

    // Then the mirror holds every byte of it — the assertion no line-oriented delta can pass
    session.assert_the_mirror_holds(A_BINARY_FILE, &bytes).await;
}

// ---------------------------------------------------------------------------
// AC35 — the mirror follows the session's own commits
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn follows_the_session_head_when_the_agent_commits() {
    // Given a session whose worktree is being mirrored, with work in it to commit
    let session = a_mirrored_session("commit").await;
    session.the_agent_edits(
        A_COMMITTED_EDIT_CALL,
        SEEDED_FILE,
        EDITED_CONTENTS.as_bytes(),
    );

    // When the agent commits
    let committed = session.the_agent_commits("the agent's own commit");

    // Then the mirror is parked on that commit. Every delta afterwards is cut from the session's
    // HEAD, so a mirror left on the previous one would refuse all of them.
    session.assert_the_mirror_is_on_commit(&committed).await;
}

// ---------------------------------------------------------------------------
// AC36 — a mirror somebody wrote into is restored, not patched
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn restores_a_mirror_that_was_corrupted_by_hand() {
    // Given a mirror somebody has overwritten a tracked file in, so no patch cut against the
    // session's copy of it can apply there any more
    let session = a_mirrored_session("corrupted").await;
    session.somebody_overwrites_in_the_mirror(SEEDED_FILE, CORRUPTED_CONTENTS);

    // When the agent edits that same file in the session
    session.the_agent_edits(
        A_REPAIRING_EDIT_CALL,
        SEEDED_FILE,
        EDITED_CONTENTS.as_bytes(),
    );

    // Then the mirror holds the session's bytes again: a rejected patch is a divergence the syncer
    // recovers from by fetching the WIP ref and resetting onto it, not by patching harder.
    session
        .assert_the_mirror_holds(SEEDED_FILE, EDITED_CONTENTS.as_bytes())
        .await;
}
