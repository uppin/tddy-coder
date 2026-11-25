//! Where a tmux session lives and how a terminal reaches it (`#target-reconnect` 1/3).
//!
//! Defined here, in the consumer's crate, in its own types: this crate does not depend on
//! `tddy-remote-shell` and must not start to. The host adapter wraps today's tmux helpers; the
//! target adapter (node 2/3) lives in `tddy-agent-launch` over `TabTargetShell`.

#[cfg(any(test, feature = "test-support"))]
use std::collections::HashSet;
use std::sync::Arc;
#[cfg(any(test, feature = "test-support"))]
use std::sync::Mutex;
use std::time::Duration;

use crate::tmux::{kill_tmux_session, tmux_attach_argv, tmux_binary, tmux_session_lives};

/// Why a location cannot be asked: never read as "gone".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmuxUnreachable(pub String);

impl std::fmt::Display for TmuxUnreachable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TmuxUnreachable {}

/// The local process that is a client attached to exactly one tmux session (no create).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmuxClient {
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
    /// The OS user the local client runs as: `Some(os_user)` on the host, `None` for a transport
    /// client (it runs as the daemon, carrying the daemon's ssh environment).
    pub run_as: Option<String>,
    pub exit_code_in_output: Option<tddy_pty::ExitCodeInOutput>,
}

/// Where a tmux session lives and how a terminal of this host reaches it.
#[async_trait::async_trait]
pub trait TmuxLocation: Send + Sync {
    /// For logs and re-attach reasons: "the tmux of dev", "tmux on buildbox".
    fn describe(&self) -> String;

    /// Whether `name` lives there. `Err` = could not tell (never read as gone).
    async fn session_lives(&self, name: &str) -> Result<bool, TmuxUnreachable>;

    /// The local process that is a client attached to exactly `name` (no create).
    /// `Err` when the location cannot even be resolved (vault locked, no target in the record).
    async fn attach_client(&self, name: &str) -> Result<TmuxClient, TmuxUnreachable>;

    /// Kill `name`; a session or server already gone is `Ok`.
    async fn kill(&self, name: &str) -> Result<(), TmuxUnreachable>;
}

/// The host's own tmux, reached as `os_user`.
#[derive(Debug, Clone)]
pub struct HostTmux {
    os_user: String,
}

impl HostTmux {
    pub fn new(os_user: &str) -> Self {
        Self {
            os_user: os_user.to_string(),
        }
    }

    pub fn os_user(&self) -> &str {
        &self.os_user
    }
}

#[async_trait::async_trait]
impl TmuxLocation for HostTmux {
    fn describe(&self) -> String {
        format!("the tmux of {}", self.os_user)
    }

    async fn session_lives(&self, name: &str) -> Result<bool, TmuxUnreachable> {
        let (user, name) = (self.os_user.clone(), name.to_string());
        tokio::task::spawn_blocking(move || tmux_session_lives(&user, &name))
            .await
            .map_err(|error| TmuxUnreachable(format!("the tmux probe did not finish: {error}")))?
            .map_err(|error| TmuxUnreachable(error.to_string()))
    }

    async fn attach_client(&self, name: &str) -> Result<TmuxClient, TmuxUnreachable> {
        Ok(TmuxClient {
            argv: tmux_attach_argv(&tmux_binary(), name),
            env: Vec::new(),
            run_as: Some(self.os_user.clone()),
            exit_code_in_output: None,
        })
    }

    async fn kill(&self, name: &str) -> Result<(), TmuxUnreachable> {
        let (user, name) = (self.os_user.clone(), name.to_string());
        tokio::task::spawn_blocking(move || kill_tmux_session(&user, &name))
            .await
            .map_err(|error| TmuxUnreachable(format!("the tmux kill did not finish: {error}")))?
            .map_err(|error| TmuxUnreachable(error.to_string()))
    }
}

/// An in-memory location for watcher and classifier tests.
#[cfg(any(test, feature = "test-support"))]
pub struct FakeTmuxLocation {
    state: Mutex<FakeState>,
}

#[cfg(any(test, feature = "test-support"))]
struct FakeState {
    alive: HashSet<String>,
    reachable: bool,
    probes: Vec<tokio::time::Instant>,
    kills: Vec<String>,
}

#[cfg(any(test, feature = "test-support"))]
impl FakeTmuxLocation {
    pub fn new(alive: &[&str]) -> Self {
        Self {
            state: Mutex::new(FakeState {
                alive: alive.iter().map(|name| name.to_string()).collect(),
                reachable: true,
                probes: Vec::new(),
                kills: Vec::new(),
            }),
        }
    }

    pub fn set_reachable(&self, reachable: bool) {
        self.state.lock().unwrap().reachable = reachable;
    }

    pub fn set_alive(&self, name: &str, alive: bool) {
        let mut state = self.state.lock().unwrap();
        if alive {
            state.alive.insert(name.to_string());
        } else {
            state.alive.remove(name);
        }
    }

    pub fn unreachable_reason(&self) -> &'static str {
        "the fake location cannot be asked"
    }

    pub fn probe_count(&self) -> usize {
        self.state.lock().unwrap().probes.len()
    }

    pub fn probe_instants(&self) -> Vec<tokio::time::Instant> {
        self.state.lock().unwrap().probes.clone()
    }

    pub fn kills(&self) -> Vec<String> {
        self.state.lock().unwrap().kills.clone()
    }
}

#[cfg(any(test, feature = "test-support"))]
#[async_trait::async_trait]
impl TmuxLocation for FakeTmuxLocation {
    fn describe(&self) -> String {
        "fake tmux".to_string()
    }

    async fn session_lives(&self, name: &str) -> Result<bool, TmuxUnreachable> {
        let mut state = self.state.lock().unwrap();
        state.probes.push(tokio::time::Instant::now());
        if !state.reachable {
            return Err(TmuxUnreachable(self.unreachable_reason().to_string()));
        }
        Ok(state.alive.contains(name))
    }

    async fn attach_client(&self, name: &str) -> Result<TmuxClient, TmuxUnreachable> {
        let state = self.state.lock().unwrap();
        if !state.reachable {
            return Err(TmuxUnreachable(self.unreachable_reason().to_string()));
        }
        let _ = name;
        Ok(TmuxClient {
            argv: vec![
                "/bin/sh".to_string(),
                "-c".to_string(),
                "sleep 300".to_string(),
            ],
            env: Vec::new(),
            run_as: None,
            exit_code_in_output: None,
        })
    }

    async fn kill(&self, name: &str) -> Result<(), TmuxUnreachable> {
        let mut state = self.state.lock().unwrap();
        if !state.reachable {
            return Err(TmuxUnreachable(self.unreachable_reason().to_string()));
        }
        state.kills.push(name.to_string());
        state.alive.remove(name);
        Ok(())
    }
}

/// The probe cadence of one detach watcher: the base interval while the location answers,
/// doubling to the cap while it cannot be asked, and the hold grace that cadence accumulates.
#[derive(Debug, Clone)]
pub struct WatchBackoff {
    base: Duration,
    cap: Duration,
    grace: Duration,
    next: Duration,
    unreachable_for: Duration,
    grace_expired: bool,
}

impl WatchBackoff {
    pub fn new(base: Duration) -> Self {
        Self::with_limits(base, Duration::from_secs(300), Duration::from_secs(900))
    }

    pub fn with_limits(base: Duration, cap: Duration, grace: Duration) -> Self {
        Self {
            base,
            cap,
            grace,
            next: base,
            unreachable_for: Duration::ZERO,
            grace_expired: false,
        }
    }

    /// The wait before the next probe.
    pub fn delay(&self) -> Duration {
        self.next
    }

    /// The location answered alive: the next probe is due after the base interval again.
    pub fn on_alive(&mut self) {
        self.next = self.base;
        self.unreachable_for = Duration::ZERO;
        self.grace_expired = false;
    }

    /// The location could not be asked: back off, and expire the hold once the grace is spent.
    pub fn on_unreachable(&mut self) {
        self.unreachable_for += self.next;
        if self.unreachable_for >= self.grace {
            self.grace_expired = true;
        }
        self.next = self.next.saturating_mul(2).min(self.cap);
    }

    pub fn grace_expired(&self) -> bool {
        self.grace_expired
    }
}

/// A location held behind the port, as watchers and runs carry it.
pub type SharedTmuxLocation = Arc<dyn TmuxLocation>;
