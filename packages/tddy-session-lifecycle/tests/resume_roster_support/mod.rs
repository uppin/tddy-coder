//! The fixture `resume_tab_roster_acceptance` builds on: a daemon over one **stopped** `claude-cli`
//! session on a private tmux server, whose Claude Code, Cursor and OpenCode are stub scripts that log
//! every launch's argv (Cursor's also mints a fixed chat for `create-chat`), and a restart of it over
//! the same sessions directory.
//!
//! The shape is `support::tab_daemon`'s (`#session-liveness` 2/4), with the harnesses a resume
//! distinguishes: one that resumes a given conversation by id (Claude), one that resumes a minted chat
//! (Cursor), and one that cannot resume a given conversation at all (OpenCode).
//!
//! The session's identity maps to the OS user running the test, so its tmux server is this test's
//! private one. Every suite using it is serial: the tmux socket directory is process-wide.

#![allow(dead_code)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tddy_agent_launch::tab_target_shell::TabTargetShell;
use tddy_core::output::SESSIONS_SUBDIR;
use tddy_core::tab_roster::{read_tab_roster, write_tab_roster, TabEntry, TabRoster};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_sandbox::tab_jails::TabJailSpawn;
use tddy_rpc::Request;
use tddy_service::proto::session::{
    ResumeSessionRequest, ResumeSessionResponse, SessionService as SessionServiceTrait,
};
use tddy_session_lifecycle::claude_cli_session::CliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_terminal_rpc::proto::terminal_session::{
    ListTerminalSessionsRequest, StartTerminalSessionRequest,
    TerminalSessionService as TerminalSessionServiceTrait,
};
use tddy_testing_commons::stub_scripts::{a_stub_agent_script, appended_invocation_count};
use tddy_testing_commons::wait::eventually_awaiting;
use tddy_testing_commons::{a_session_metadata, fs::write_session_yaml};

use crate::support::{a_config_mapping_to_this_user, PrivateTmux, OBSERVE, VALID_TOKEN};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// The stopped session every test resumes.
pub const RESUMED_SESSION_ID: &str = "resume-roster-session";

/// The chat the stub `agent create-chat` mints.
pub const MINTED_CHAT_ID: &str = "c0ffee00-e154-41d0-ae72-312bdf6d4d80";

/// A stub harness binary, and the log every launch of it appends its argv to (one tab-separated line
/// per launch).
pub struct StubHarness {
    pub binary: PathBuf,
    pub launches: PathBuf,
}

impl StubHarness {
    /// Every launch's argv, oldest first.
    pub fn launches(&self) -> Vec<Vec<String>> {
        std::fs::read_to_string(&self.launches)
            .unwrap_or_default()
            .lines()
            .map(|line| {
                if line.is_empty() {
                    Vec::new()
                } else {
                    line.split('\t').map(str::to_string).collect()
                }
            })
            .collect()
    }

    pub fn launch_count(&self) -> usize {
        appended_invocation_count(&self.launches)
    }

    /// How many launches carried `arg` anywhere in their argv.
    pub fn launches_carrying(&self, arg: &str) -> usize {
        self.launches()
            .iter()
            .filter(|argv| argv.iter().any(|a| a == arg))
            .count()
    }

    /// Wait until a launch carries `flag` followed by `value`, and return that launch's argv.
    pub async fn until_launched_with(&self, flag: &str, value: &str) -> Vec<String> {
        eventually_awaiting(
            &format!("a launch with `{flag} {value}`"),
            OBSERVE,
            || async {
                let launches = self.launches();
                launches
                    .iter()
                    .find(|argv| argv.windows(2).any(|pair| pair == [flag, value]))
                    .cloned()
                    .ok_or_else(|| format!("launches so far: {launches:?}"))
            },
        )
        .await
    }

    /// Wait until the harness has been launched `count` times, and return the last launch's argv.
    pub async fn until_launched_times(&self, count: usize) -> Vec<String> {
        eventually_awaiting(&format!("{count} launch(es)"), OBSERVE, || async {
            let launches = self.launches();
            (launches.len() == count)
                .then(|| launches.last().cloned().unwrap_or_default())
                .ok_or_else(|| format!("launches so far: {launches:?}"))
        })
        .await
    }
}

fn a_stub_harness(dir: &Path, name: &str, prelude: Option<String>) -> StubHarness {
    let launches = dir.join(format!("{name}-launches.log"));
    let script = a_stub_agent_script(dir, &format!("{name}.sh"));
    let script = match prelude {
        Some(prelude) => script.with_prelude(&prelude),
        None => script,
    };
    let binary = script
        .appending_argv_to(&launches)
        .then_reading_stdin()
        .build();
    StubHarness { binary, launches }
}

/// A daemon over [`RESUMED_SESSION_ID`], recorded as a stopped `claude-cli` session with a worktree.
pub struct ResumeRosterDaemon {
    pub host: Arc<DaemonSessionHost>,
    pub tmux: PrivateTmux,
    pub worktree: PathBuf,
    pub claude: StubHarness,
    pub cursor: StubHarness,
    pub opencode: StubHarness,
    create_chats: PathBuf,
    config: DaemonConfig,
    agents: Arc<tddy_agent_launch::spec_launch::HostCodingAgents>,
    sessions_base: PathBuf,
    /// Where a jailed tab's jail starts, when a test gave one (`#session-liveness` 5/8).
    tab_jails: Option<Arc<dyn TabJailSpawn>>,
    /// Where a target tab's target resolves, when a test gave one (`#session-liveness` 6/8).
    target_shells: Option<Arc<dyn TabTargetShell>>,
    _dirs: Vec<tempfile::TempDir>,
}

/// A daemon serving the stopped session, with none of its terminals running.
pub fn a_stopped_session() -> ResumeRosterDaemon {
    a_stopped_session_with(None)
}

/// [`a_stopped_session`], whose jailed tabs' jails start through `tab_jails` — on this daemon and
/// on every restart of it.
pub fn a_stopped_session_whose_tab_jails_start_through(
    tab_jails: Arc<dyn TabJailSpawn>,
) -> ResumeRosterDaemon {
    a_stopped_session_with(Some(tab_jails))
}

/// [`a_stopped_session`], whose target tabs' targets resolve through `target_shells` — on this
/// daemon and on every restart of it. The session itself records no target, so its main resumes on
/// this host: what is under test is the tab's own target, which the double answers for.
pub fn a_stopped_session_whose_target_shells_resolve_through(
    target_shells: Arc<dyn TabTargetShell>,
) -> ResumeRosterDaemon {
    ResumeRosterDaemon {
        target_shells: Some(target_shells),
        ..a_stopped_session_with(None)
    }
    .with_its_host_rebuilt()
}

fn a_stopped_session_with(tab_jails: Option<Arc<dyn TabJailSpawn>>) -> ResumeRosterDaemon {
    let tmux = PrivateTmux::started();
    let stub_dir = tempfile::tempdir().unwrap();
    let create_chats = stub_dir.path().join("create-chat.log");
    let claude = a_stub_harness(stub_dir.path(), "claude", None);
    let cursor = a_stub_harness(
        stub_dir.path(),
        "cursor",
        Some(format!(
            "if [ \"$1\" = \"create-chat\" ]; then\n  echo create-chat >> \"{}\"\n  echo \
             \"{MINTED_CHAT_ID}\"\n  exit 0\nfi",
            create_chats.display()
        )),
    );
    let opencode = a_stub_harness(stub_dir.path(), "opencode", None);
    let (config_dir, config) = a_config_mapping_to_this_user();
    let agents = tddy_session_lifecycle::test_util::coding_agents_with_binaries(&[
        ("claude", &claude.binary),
        ("cursor", &cursor.binary),
        ("opencode", &opencode.binary),
    ]);
    let worktree_dir = tempfile::tempdir().unwrap();
    let worktree = worktree_dir.path().canonicalize().unwrap();
    let sessions_dir = tempfile::tempdir().unwrap();
    let sessions_base = sessions_dir.path().to_path_buf();
    a_recorded_stopped_session(&sessions_base, &worktree);
    let host = a_session_host(
        config.clone(),
        agents.clone(),
        sessions_base.clone(),
        tab_jails.clone(),
        None,
    );
    ResumeRosterDaemon {
        host,
        tmux,
        worktree,
        claude,
        cursor,
        opencode,
        create_chats,
        config,
        agents,
        sessions_base,
        tab_jails,
        target_shells: None,
        _dirs: vec![config_dir, stub_dir, worktree_dir, sessions_dir],
    }
}

fn a_recorded_stopped_session(sessions_base: &Path, worktree: &Path) {
    let dir = sessions_base.join(SESSIONS_SUBDIR).join(RESUMED_SESSION_ID);
    std::fs::create_dir_all(&dir).unwrap();
    let mut metadata = a_session_metadata()
        .with_session_id(RESUMED_SESSION_ID)
        .with_session_type("claude-cli")
        .with_status("stopped")
        .with_repo_path(worktree.to_str().unwrap())
        .without_pid()
        .build();
    metadata.model = Some("claude-opus-4-8".to_string());
    write_session_yaml(&dir, &metadata);
}

fn a_session_host(
    config: DaemonConfig,
    agents: Arc<tddy_agent_launch::spec_launch::HostCodingAgents>,
    sessions_base: PathBuf,
    tab_jails: Option<Arc<dyn TabJailSpawn>>,
    target_shells: Option<Arc<dyn TabTargetShell>>,
) -> Arc<DaemonSessionHost> {
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver =
        Arc::new(|token| (token == VALID_TOKEN).then(|| "testuser".to_string()));
    let host = DaemonSessionHost::new(
        config.clone(),
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        tddy_session_lifecycle::peer_routing::PeerRouting::local_only(&config),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        Arc::new(CliSessionManager::new()),
        agents,
    );
    let host = match tab_jails {
        Some(spawn) => host.with_tab_jail_spawn(spawn),
        None => host,
    };
    Arc::new(match target_shells {
        Some(shells) => host.with_tab_target_shell(shells),
        None => host,
    })
}

impl ResumeRosterDaemon {
    pub fn session_dir(&self) -> PathBuf {
        self.sessions_base
            .join(SESSIONS_SUBDIR)
            .join(RESUMED_SESSION_ID)
    }

    /// Record `tabs` as the session's roster, as an earlier run of the daemon left it.
    pub fn has_recorded_tabs(&self, tabs: Vec<TabEntry>) {
        write_tab_roster(
            &self.session_dir(),
            &TabRoster {
                tabs,
                ..TabRoster::default()
            },
        )
        .expect("the session's tabs.yaml is written");
    }

    /// The session's roster, as the file beside its `.session.yaml` holds it.
    pub fn roster(&self) -> TabRoster {
        read_tab_roster(&self.session_dir()).expect("the session's tabs.yaml reads")
    }

    /// The roster's entry for `terminal_id`.
    pub fn roster_entry(&self, terminal_id: &str) -> TabEntry {
        self.roster()
            .tabs
            .into_iter()
            .find(|tab| tab.terminal_id == terminal_id)
            .unwrap_or_else(|| panic!("the roster records terminal {terminal_id}"))
    }

    pub fn roster_records(&self, terminal_id: &str) -> bool {
        self.roster()
            .tabs
            .iter()
            .any(|tab| tab.terminal_id == terminal_id)
    }

    /// How many times the stub `agent create-chat` ran.
    pub fn minted_chats(&self) -> usize {
        appended_invocation_count(&self.create_chats)
    }

    /// Start the stub Claude in `conversation_id` in a detached tmux session `name` in the session's
    /// worktree — a tmux-wrapped tab's agent outliving its daemon — and wait for it to have launched.
    pub async fn runs_claude_in_tmux_session(&self, name: &str, conversation_id: &str) {
        let started = self.tmux.tmux(&[
            "new-session",
            "-d",
            "-s",
            name,
            "-c",
            self.worktree.to_str().unwrap(),
            "--",
            self.claude.binary.to_str().unwrap(),
            "--session-id",
            conversation_id,
        ]);
        assert!(started.status.success(), "tmux new-session must succeed");
        self.claude
            .until_launched_with("--session-id", conversation_id)
            .await;
    }

    /// Open a tab of `kind` on the session through the daemon.
    pub async fn opens_a_tab(&self, kind: &str) -> String {
        self.host
            .terminal_session_service()
            .start_terminal_session(Request::direct(StartTerminalSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: RESUMED_SESSION_ID.to_string(),
                control_token: String::new(),
                kind: kind.to_string(),
                tmux_session: String::new(),
                wrappers: Vec::new(),
            }))
            .await
            .expect("StartTerminalSession must succeed")
            .into_inner()
            .terminal_id
    }

    /// Stop the daemon — every terminal it runs ends, as at a shutdown — and start a fresh one over the
    /// same sessions directory and tmux server, which serves no terminal yet.
    pub async fn restarts(self) -> Self {
        self.host.shut_down_children().await;
        self.with_its_host_rebuilt()
    }

    /// This daemon with a fresh host over the same configuration, sessions and doubles.
    fn with_its_host_rebuilt(self) -> Self {
        let host = a_session_host(
            self.config.clone(),
            self.agents.clone(),
            self.sessions_base.clone(),
            self.tab_jails.clone(),
            self.target_shells.clone(),
        );
        ResumeRosterDaemon { host, ..self }
    }

    /// Resume the session, as the web's Resume does.
    pub async fn resumes(&self) -> ResumeSessionResponse {
        self.host
            .resume_session(Request::direct(ResumeSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: RESUMED_SESSION_ID.to_string(),
            }))
            .await
            .expect("ResumeSession must succeed")
            .into_inner()
    }

    /// The terminal ids `ListTerminalSessions` reports for the session.
    pub async fn listed_terminal_ids(&self) -> BTreeSet<String> {
        self.host
            .terminal_session_service()
            .list_terminal_sessions(Request::direct(ListTerminalSessionsRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: RESUMED_SESSION_ID.to_string(),
            }))
            .await
            .expect("ListTerminalSessions must succeed")
            .into_inner()
            .terminals
            .into_iter()
            .map(|terminal| terminal.terminal_id)
            .collect()
    }

    /// Wait until `ListTerminalSessions` reports exactly `expected`, and return what it reports.
    pub async fn until_listed_terminals_are(&self, expected: &[&str]) -> BTreeSet<String> {
        let expected: BTreeSet<String> = expected.iter().map(|id| id.to_string()).collect();
        eventually_awaiting("the session's terminals to settle", OBSERVE, || async {
            let listed = self.listed_terminal_ids().await;
            (listed == expected)
                .then(|| listed.clone())
                .ok_or_else(|| format!("listed {listed:?}, waiting for {expected:?}"))
        })
        .await
    }
}

/// A safety net for a tmux client to attach, not a prediction of how long it takes.
pub const ATTACH_OBSERVE: Duration = OBSERVE;
