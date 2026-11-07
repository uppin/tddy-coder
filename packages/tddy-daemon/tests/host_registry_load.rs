//! A daemon loads its coding-agent registry — the built-in specs plus `<tddy home>/tools` — once at
//! start: a registry that does not load stops the start, and Telegram and the session host share
//! the one instance that did.
//!
//! Product: `docs/ft/daemon/session-agent-roster.md` § Coding agents.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_daemon::config::DaemonConfig;
use tddy_daemon::runtime::{self, DaemonRuntime, RuntimeOptions};
use tokio::net::TcpListener;

const THE_OPERATOR: &str = "operator";

#[tokio::test]
async fn a_daemon_with_an_unparseable_tool_spec_refuses_to_start_naming_the_file() {
    // Given a daemon whose Telegram bot offers coding agents, and an operator override of
    // `claude.yaml` that is not YAML
    let tddy_home = a_tddy_home().with_tool_file("claude.yaml", "id: claude\nrun: [unterminated\n");
    let config = a_daemon_config(tddy_home.path())
        .with_a_telegram_bot()
        .build()
        .await;

    // When the daemon starts
    let Err(refusal) = start(config).await else {
        panic!(
            "a daemon whose coding-agent registry does not load must not start — Telegram must \
             not quietly offer no agents"
        );
    };

    // Then the refusal names the file. Containment, not equality: the rest of the message is the
    // YAML parser's, which this crate does not own.
    let message = format!("{refusal:#}");
    let the_file = tddy_home.tool_file("claude.yaml");
    assert!(
        message.contains(&the_file.display().to_string()),
        "the refusal names {}: {message}",
        the_file.display()
    );
}

#[tokio::test]
async fn telegram_and_the_session_host_share_one_registry() {
    // Given a daemon that hosts sessions and runs a Telegram bot
    let tddy_home = a_tddy_home();
    let config = a_daemon_config(tddy_home.path())
        .with_a_signed_in_operator()
        .with_a_telegram_bot()
        .build()
        .await;

    // When the daemon starts
    let runtime = start(config).await.expect("the daemon starts");

    // Then Telegram offers the very registry the session host launches from — one load, one
    // instance
    assert_shares_one_registry(&runtime);
}

#[tokio::test]
async fn a_daemon_without_a_telegram_bot_offers_no_telegram_registry() {
    // Given a daemon that hosts sessions and runs no bot
    let tddy_home = a_tddy_home();
    let config = a_daemon_config(tddy_home.path())
        .with_a_signed_in_operator()
        .build()
        .await;

    // When the daemon starts
    let runtime = start(config).await.expect("the daemon starts");

    // Then there is no Telegram registry to report
    assert!(runtime.tasks.telegram_coding_agents().is_none());
}

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// A `<tddy home>` the daemon keeps its state in, with operator tool files under `tools/`.
struct ATddyHome {
    dir: tempfile::TempDir,
}

fn a_tddy_home() -> ATddyHome {
    ATddyHome {
        dir: tempfile::tempdir().expect("a tddy home"),
    }
}

impl ATddyHome {
    /// Write `<tddy home>/tools/<name>` with `contents`, as an operator's override is written.
    fn with_tool_file(self, name: &str, contents: &str) -> Self {
        let tools = self.dir.path().join("tools");
        std::fs::create_dir_all(&tools).expect("the tools directory");
        std::fs::write(tools.join(name), contents).expect("the tool file");
        self
    }

    fn tool_file(&self, name: &str) -> PathBuf {
        self.dir.path().join("tools").join(name)
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }
}

/// A daemon configuration keeping its state in a `<tddy home>`, on a free loopback port.
struct ADaemonConfig {
    tddy_home: PathBuf,
    signed_in_operator: bool,
    telegram_bot: bool,
}

fn a_daemon_config(tddy_home: &Path) -> ADaemonConfig {
    ADaemonConfig {
        tddy_home: tddy_home.to_path_buf(),
        signed_in_operator: false,
        telegram_bot: false,
    }
}

impl ADaemonConfig {
    /// GitHub sign-in (a stub) mapping one operator, so the daemon mounts its session host.
    fn with_a_signed_in_operator(mut self) -> Self {
        self.signed_in_operator = true;
        self
    }

    /// A Telegram bot with inbound session control. Building it dials nothing.
    fn with_a_telegram_bot(mut self) -> Self {
        self.telegram_bot = true;
        self
    }

    async fn build(self) -> DaemonConfig {
        let mut yaml = format!(
            "listen:\n  web_port: {port}\n  web_host: 127.0.0.1\ntddy_data_dir: \"{home}\"\n",
            port = a_free_tcp_port().await,
            home = self.tddy_home.display(),
        );
        if self.signed_in_operator {
            yaml.push_str(&format!(
                "users:\n  - github_user: {THE_OPERATOR}\n    os_user: {THE_OPERATOR}\n\
                 github:\n  stub: true\n  stub_codes: \"operator-code:{THE_OPERATOR}\"\n"
            ));
        }
        if self.telegram_bot {
            yaml.push_str(
                "telegram:\n  enabled: true\n  bot_token: \"123456:host-registry-test\"\n  \
                 chat_ids: [42]\n",
            );
        }
        serde_yaml::from_str(&yaml).expect("the config fixture did not parse")
    }
}

/// A port that is free at this moment, so the fixture competes for no fixed one.
async fn a_free_tcp_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("no loopback port was available")
        .local_addr()
        .expect("the bound listener has no address")
        .port()
}

// ---------------------------------------------------------------------------
// Actions and assertions
// ---------------------------------------------------------------------------

/// Assemble the daemon `config` describes, as an embedding host does. Assembling binds nothing.
async fn start(config: DaemonConfig) -> anyhow::Result<DaemonRuntime> {
    runtime::build(
        config,
        RuntimeOptions::for_embedded(
            tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        ),
    )
    .await
}

fn assert_shares_one_registry(runtime: &DaemonRuntime) {
    let telegram = runtime
        .tasks
        .telegram_coding_agents()
        .expect("the daemon runs Telegram's inbound session control");
    let host = runtime
        .session_host
        .as_ref()
        .expect("the daemon hosts sessions")
        .launch_coding_agents()
        .agents_shared();
    assert!(
        Arc::ptr_eq(telegram, &host),
        "Telegram offers a registry loaded apart from the one the session host launches from"
    );
}
