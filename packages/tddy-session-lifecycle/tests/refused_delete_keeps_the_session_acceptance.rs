//! A session delete that one of its peer-held parts would refuse is refused before anything of the
//! session is torn down: its agent keeps running in its tmux session and its directory stays, so a
//! retry can still reach both (`#agent-tools 30/32`).
//!
//! The refusal is a reachable one: a split session, whose worktree half lives on another daemon,
//! deleted on a daemon that is configured for a common room but has not joined it. That refusal is
//! already pinned on its own (`remote_managed_worktree_acceptance.rs`,
//! `deleting_a_split_session_refuses_while_this_daemon_cannot_reach_the_common_room`); this suite
//! pins what is left standing when it is given — the agent the delete used to kill first.
//!
//! No LiveKit: the common-room slot is configured but holds no room, as in
//! `remote_managed_worktree_acceptance.rs`. The agent's tmux session runs on a private tmux server
//! (`support::PrivateTmux`), so no developer's tmux is touched. Only one test lives here, because
//! that server is selected through this process's environment.

#![cfg(unix)]

mod support;

use std::path::PathBuf;
use std::sync::Arc;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_livekit::livekit_peer_discovery::LiveKitDiscoveryHandles;
use tddy_host_service::multi_host::{DaemonInstanceId, EligibleDaemonInfo, EligibleDaemonSource};
use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::session::{DeleteSessionRequest, SessionService as SessionServiceTrait};
use tddy_session_lifecycle::claude_cli_session::CliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;

use support::{this_os_user, PrivateTmux};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const TOKEN: &str = "operator-token";
const THIS_DAEMON: &str = "laptop-a";
const CODEBASE_DAEMON: &str = "workstation-b";
const SESSION_ID: &str = "019d105b-ac0f-78d3-9a89-40973114cc01";
const CODEBASE_SESSION_ID: &str = "019d105b-ac0f-78d3-9a89-40973114cc02";
const AGENT_TMUX_SESSION: &str = "tddy-refused-delete-agent";

#[tokio::test]
async fn a_refused_delete_leaves_the_session_and_its_agent_running() {
    // Given a split session whose agent runs in its tmux session, on a daemon configured for a
    // common room it has not joined
    let tmux = PrivateTmux::started().with_session(AGENT_TMUX_SESSION, 1);
    let daemon = a_daemon_out_of_its_common_room();
    let session = daemon.a_split_session_whose_agent_runs_in(AGENT_TMUX_SESSION);

    // When its operator deletes it
    let refusal = daemon
        .delete(SESSION_ID)
        .await
        .expect_err("a delete that cannot reach the codebase daemon is refused");

    // Then it is refused over the half it cannot reach, and nothing of the session was torn down
    assert_eq!(
        refusal.code(),
        Code::FailedPrecondition,
        "refused over the unreachable codebase half; got {:?}: {}",
        refusal.code(),
        refusal.message()
    );
    assert!(
        refusal.message().contains(CODEBASE_SESSION_ID),
        "the refusal names the half it cannot reach: {}",
        refusal.message()
    );
    assert!(
        tmux.has_session(AGENT_TMUX_SESSION),
        "a refused delete leaves the session's agent running in its tmux session"
    );
    assert!(
        session.is_dir(),
        "a refused delete leaves the session's directory in place, so a retry can reach it"
    );
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// A daemon whose operator maps to the OS user running the test, which knows the codebase daemon
/// as a peer and is configured for a common room it holds no connection to.
struct ADaemon {
    host: Arc<DaemonSessionHost>,
    sessions: tempfile::TempDir,
}

fn a_daemon_out_of_its_common_room() -> ADaemon {
    let sessions = tempfile::tempdir().expect("a sessions directory");
    let config = a_config_mapping_the_operator_to_this_user();
    let sessions_base = sessions.path().to_path_buf();
    let resolver: SessionsBaseResolver = {
        let base = sessions_base.clone();
        Arc::new(move |_| Some(base.clone()))
    };
    let operator: UserResolver = Arc::new(|token| (token == TOKEN).then(|| "operator".to_string()));
    let discovery = LiveKitDiscoveryHandles {
        eligible_daemon_source: Arc::new(KnowsTheCodebaseDaemon) as Arc<dyn EligibleDaemonSource>,
        common_room_livekit_room: Arc::new(tokio::sync::RwLock::new(None)),
    };
    let host = DaemonSessionHost::new(
        config.clone(),
        resolver,
        sessions_base,
        operator,
        None,
        tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        tddy_daemon_livekit::peer_routing::peer_routing_over(&config, Some(discovery)),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        Arc::new(CliSessionManager::new()),
        tddy_session_lifecycle::test_util::builtin_coding_agents(),
    );
    ADaemon {
        host: Arc::new(host),
        sessions,
    }
}

impl ADaemon {
    /// A stopped-looking split session on disk: paired to its codebase half on
    /// [`CODEBASE_DAEMON`], with no repository of its own, and its agent recorded as running in
    /// `tmux_session`. Returns the session's directory.
    fn a_split_session_whose_agent_runs_in(&self, tmux_session: &str) -> PathBuf {
        let session_dir = tddy_core::session_lifecycle::unified_session_dir_path(
            self.sessions.path(),
            SESSION_ID,
        );
        std::fs::create_dir_all(&session_dir).expect("a session directory");
        let mut metadata = tddy_testing_commons::builders::a_session_metadata()
            .with_session_id(SESSION_ID)
            .with_status("running")
            .build();
        metadata.session_type = Some("claude-cli".to_string());
        metadata.repo_path = None;
        metadata.tmux_session = Some(tmux_session.to_string());
        metadata.codebase_daemon_instance_id = Some(CODEBASE_DAEMON.to_string());
        metadata.codebase_session_id = Some(CODEBASE_SESSION_ID.to_string());
        tddy_core::write_session_metadata(&session_dir, &metadata).expect("write the metadata");
        session_dir
    }

    async fn delete(&self, session_id: &str) -> Result<(), Status> {
        self.host
            .delete_session(Request::direct(DeleteSessionRequest {
                session_token: TOKEN.to_string(),
                session_id: session_id.to_string(),
            }))
            .await
            .map(|_| ())
    }
}

fn a_config_mapping_the_operator_to_this_user() -> DaemonConfig {
    let yaml = format!(
        "users:\n  - github_user: \"operator\"\n    os_user: \"{}\"\ndaemon_instance_id: \"{THIS_DAEMON}\"\n",
        this_os_user()
    );
    serde_yaml::from_str(&yaml).expect("the config parses")
}

struct KnowsTheCodebaseDaemon;

impl EligibleDaemonSource for KnowsTheCodebaseDaemon {
    fn list_eligible_daemons(&self) -> Vec<EligibleDaemonInfo> {
        vec![EligibleDaemonInfo {
            instance_id: DaemonInstanceId(CODEBASE_DAEMON.to_string()),
            label: CODEBASE_DAEMON.to_string(),
        }]
    }
}
