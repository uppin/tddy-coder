//! Acceptance: a split session's agent process is given no `TDDY_*` variable. Its tools reach the
//! codebase daemon through the `tddy-tools --mcp` server the agent is registered with, and that
//! server alone carries the `TDDY_REMOTE_*` transport and the roster seed (`#agent-tools 25/28`,
//! `docs/ft/daemon/remote-managed-worktree.md`).
//!
//! In memory: [`prepare_split_agent_wiring`] — what a split resume relaunches its agent with — over
//! a split-room port that issues joins for any room, and a codebase with no guidance.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_coding_agent_specs::{CodingAgentRegistry, CodingAgentSpec};
use tddy_daemon_auth::{DaemonSigningKey, SessionTokens, StandaloneKeyDirectory, SIGNING_KEY_FILE};
use tddy_daemon_extensions::{
    ReachableSessions, RemoteSnapshotSource, SessionTokenMinter, SplitAgentJoin, SplitRoomHosting,
};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::Status;
use tddy_service::proto::session_agents_svc::{SessionAgentEntry, SessionAgentRoster};
use tddy_session_files::context_sync::LocalWorktreeSource;
use tddy_session_split::split_session::{
    prepare_split_agent_wiring, roster_seed_env_pairs, split_agent_spec, wire_roster_withdrawals,
    SplitAgentWiring, SplitSpawnTarget,
};

const FACILITATING_HOST: &str = "workstation-a";
const CODEBASE_HOST: &str = "workstation-b";
const AGENT_SESSION: &str = "0199aaaa-0000-7000-8000-00000000000a";
const CODEBASE_SESSION: &str = "0199bbbb-0000-7000-8000-00000000000b";

#[test]
fn a_split_agent_process_carries_no_tddy_env() {
    // Given a facilitating daemon wiring a split agent to a checkout on another host
    let daemon = AFacilitatingDaemon::new();

    // When the agent's wiring is prepared
    let wiring = daemon.wires_a_split_agent_with_an_explorer_attached();

    // Then the agent process itself is told nothing of the session: no TDDY_* variable at all
    assert_eq!(
        tddy_names_in(&wiring.env),
        Vec::<String>::new(),
        "a split agent's process env must carry no TDDY_* variable"
    );
}

#[test]
fn a_split_agents_tools_reach_the_remote_through_their_tool_server() {
    // Given a facilitating daemon wiring a split agent to a checkout on another host
    let daemon = AFacilitatingDaemon::new();

    // When the agent's wiring is prepared
    let wiring = daemon.wires_a_split_agent_with_an_explorer_attached();

    // Then its tddy-tools server is registered with the route to the codebase session — this
    // daemon's room, addressed at this daemon, naming the checkout's session — and the roster seed
    let registered = tool_server_env_of(&wiring);
    assert_eq!(
        (
            registered.get("TDDY_REMOTE_SESSION_ID").cloned(),
            registered.get("TDDY_REMOTE_DAEMON_INSTANCE_ID").cloned(),
            registered.get("TDDY_REMOTE_LIVEKIT_ROOM").cloned(),
            registered.get("TDDY_REMOTE_SERVER_IDENTITY").cloned(),
            registered.contains_key("TDDY_REMOTE_LIVEKIT_TOKEN"),
            registered.contains_key("TDDY_SESSION_AGENT_ROSTER_SEED"),
        ),
        (
            Some(CODEBASE_SESSION.to_string()),
            Some(CODEBASE_HOST.to_string()),
            Some(tddy_daemon_kernel::peer_forwarding::session_room_name(
                AGENT_SESSION
            )),
            Some(tddy_daemon_kernel::peer_forwarding::daemon_rpc_identity(
                FACILITATING_HOST
            )),
            true,
            true,
        )
    );
}

#[test]
fn a_split_agents_context_dir_pointer_names_its_transport() {
    // Given a facilitating daemon wiring a split agent to a checkout on another host
    let daemon = AFacilitatingDaemon::new();

    // When the agent's wiring is prepared
    let wiring = daemon.wires_a_split_agent_with_an_explorer_attached();

    // Then the pointer its context dir is given names the same route as its tool server, and this
    // daemon on this host as its owner
    let pointer = wiring
        .session_pointer
        .as_ref()
        .expect("a split agent's context dir is given a session pointer");
    let remote = &pointer.remote;
    assert_eq!(
        (
            pointer.repo_dir.clone(),
            remote.get("TDDY_REMOTE_SESSION_ID").cloned(),
            remote.get("TDDY_REMOTE_DAEMON_INSTANCE_ID").cloned(),
            remote.get("TDDY_REMOTE_LIVEKIT_ROOM").cloned(),
            remote.get("TDDY_REMOTE_SERVER_IDENTITY").cloned(),
            remote.get("TDDY_REMOTE_LIVEKIT_TOKEN").cloned(),
            pointer.owner.clone(),
        ),
        (
            wiring.context_dir.clone(),
            Some(CODEBASE_SESSION.to_string()),
            Some(CODEBASE_HOST.to_string()),
            Some(tddy_daemon_kernel::peer_forwarding::session_room_name(
                AGENT_SESSION
            )),
            Some(tddy_daemon_kernel::peer_forwarding::daemon_rpc_identity(
                FACILITATING_HOST
            )),
            tool_server_env_of(&wiring)
                .get("TDDY_REMOTE_LIVEKIT_TOKEN")
                .cloned(),
            Some(tddy_core::session_pointer::SessionPointerOwner::this_process()),
        )
    );
}

#[test]
fn a_split_agents_flags_still_withdraw_what_its_roster_took_over() {
    // Given a facilitating daemon wiring a split agent whose roster gave Grep to an explorer
    let daemon = AFacilitatingDaemon::new();

    // When the agent's wiring is prepared
    let wiring = daemon.wires_a_split_agent_with_an_explorer_attached();

    // Then the proxied Grep is denied — moving the transport did not move the withdrawal
    assert!(
        flag_values(&wiring.extra_args, "--disallowedTools")
            .contains(&"mcp__tddy-tools__Grep".to_string()),
        "the explorer's Grep must stay withdrawn; flags were {:?}",
        wiring.extra_args
    );
}

// ─── the facilitating daemon ────────────────────────────────────────────────────

struct AFacilitatingDaemon {
    config: DaemonConfig,
    tokens: SessionTokens,
    rooms: AnyRoomJoins,
    session_dir: tempfile::TempDir,
    codebase: tempfile::TempDir,
    _identity: tempfile::TempDir,
}

impl AFacilitatingDaemon {
    fn new() -> Self {
        let identity = tempfile::tempdir().expect("identity dir");
        let key = DaemonSigningKey::load_or_generate(&identity.path().join(SIGNING_KEY_FILE))
            .expect("the facilitating daemon generates its keypair");
        Self {
            config: DaemonConfig {
                daemon_instance_id: Some(FACILITATING_HOST.to_string()),
                ..DaemonConfig::default()
            },
            tokens: SessionTokens::new(&key, Arc::new(StandaloneKeyDirectory)),
            rooms: AnyRoomJoins,
            session_dir: tempfile::tempdir().expect("session dir"),
            codebase: tempfile::tempdir().expect("codebase"),
            _identity: identity,
        }
    }

    /// The wiring a split agent is (re)launched with, on a roster holding one explorer that took
    /// over `Grep`.
    fn wires_a_split_agent_with_an_explorer_attached(&self) -> SplitAgentWiring {
        let spec = the_split_agent();
        let roster = a_roster_with_an_explorer_replacing("Grep");
        let caller_token = self.tokens.signer().mint_access(&tddy_github::GitHubUser {
            id: 4242,
            login: "operator".to_string(),
            avatar_url: String::new(),
            name: "Operator".to_string(),
        });
        prepare_split_agent_wiring(
            &spec,
            &self.rooms,
            &self.config,
            &self.tokens,
            self.session_dir.path(),
            "/usr/bin/tddy-tools",
            &SplitSpawnTarget {
                session_id: AGENT_SESSION,
                codebase_instance_id: CODEBASE_HOST,
                codebase_session_id: CODEBASE_SESSION,
                session_token: &caller_token,
            },
            &wire_roster_withdrawals(&roster.agents),
            &roster_seed_env_pairs(&roster),
            &context_globs_of(&spec),
            &a_codebase_with_no_guidance(self.codebase.path(), &spec),
        )
        .expect("a split agent's wiring is prepared")
    }
}

fn the_split_agent() -> CodingAgentSpec {
    split_agent_spec(&CodingAgentRegistry::builtin().expect("the built-in coding agents load"))
        .expect("the built-in registry holds the split agent")
}

fn context_globs_of(spec: &CodingAgentSpec) -> Vec<&str> {
    spec.agent
        .context_globs
        .iter()
        .map(String::as_str)
        .collect()
}

fn a_codebase_with_no_guidance(root: &Path, spec: &CodingAgentSpec) -> LocalWorktreeSource {
    LocalWorktreeSource::new(
        root.to_path_buf(),
        spec.agent.context_globs.clone(),
        spec.agent.instructions.files.clone(),
        64 * 1024 * 1024,
    )
}

fn a_roster_with_an_explorer_replacing(tool: &str) -> SessionAgentRoster {
    SessionAgentRoster {
        agents: vec![SessionAgentEntry {
            name: "explorer".to_string(),
            replaces: vec![tool.to_string()],
            ..Default::default()
        }],
        ..Default::default()
    }
}

/// The `TDDY_*` names among `env`, sorted.
fn tddy_names_in(env: &[(String, String)]) -> Vec<String> {
    let mut names: Vec<String> = env
        .iter()
        .filter(|(name, _)| name.starts_with("TDDY_"))
        .map(|(name, _)| name.clone())
        .collect();
    names.sort();
    names
}

/// The `env` block `wiring`'s MCP config registers `tddy-tools` with.
fn tool_server_env_of(wiring: &SplitAgentWiring) -> BTreeMap<String, String> {
    let config_path: PathBuf = flag_values(&wiring.extra_args, "--mcp-config")
        .first()
        .map(PathBuf::from)
        .expect("the wiring names an --mcp-config");
    let config: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&config_path).expect("the MCP config is written"),
    )
    .expect("the MCP config is JSON");
    serde_json::from_value(config["mcpServers"]["tddy-tools"]["env"].clone())
        .expect("tddy-tools is registered with a string env")
}

fn flag_values(args: &[String], flag: &str) -> Vec<String> {
    args.windows(2)
        .filter(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
        .collect()
}

// ─── the split-room port, in memory ─────────────────────────────────────────────

/// Issues a join for any room and hosts none: preparing the wiring asks for a join only.
struct AnyRoomJoins;

#[async_trait::async_trait]
impl SplitRoomHosting for AnyRoomJoins {
    async fn host_remote_worktree(
        &self,
        _session_id: &str,
        _session_dir: &Path,
        _codebase_session_id: &str,
        _codebase_instance_id: &str,
        _snapshots: Arc<dyn RemoteSnapshotSource>,
        _minter: Arc<dyn SessionTokenMinter>,
        _services: Arc<dyn ReachableSessions>,
    ) -> Result<(), Status> {
        panic!("preparing a split agent's wiring hosts no room")
    }

    fn agent_join(
        &self,
        room: &str,
        identity: &str,
        _ttl: Duration,
    ) -> Result<SplitAgentJoin, Status> {
        Ok(SplitAgentJoin {
            url: "ws://livekit.test:7880".to_string(),
            room: room.to_string(),
            token: format!("a-join-for-{identity}"),
        })
    }
}
