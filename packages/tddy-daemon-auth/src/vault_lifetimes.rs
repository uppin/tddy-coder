//! How long a credential vault's secrets stay in memory with nobody using them, and the sweep that
//! drops them when nobody asks.
//!
//! Two things are held in `SessionVaults`, and each has a lifetime from the daemon's config:
//!
//! - **a pending sign-in's token** — a sign-in over a closed vault holds its GitHub token unsealed
//!   until the passphrase opens the vault, and that waiting token is what permits a first
//!   passphrase or a reset. It expires after `github.pending_login_ttl_seconds`;
//! - **an open vault's data key** — kept so PR-status reads can use the stored token, and closed
//!   once nothing has used the vault for `github.open_vault_idle_ttl_seconds`. The next refresh
//!   presenting an unlock key reopens it, exactly as after a restart.
//!
//! **What each setting means is decided here** ([`VaultLifetimes::of`]), not in the daemon's config,
//! which reads both as plain optional seconds: this crate already depends on `tddy-github`, whose
//! refresh-token lifetime is the default and the ceiling, and `tddy-daemon-kernel` must not.
//!
//! | Setting | absent | `0` | at most |
//! |---|---|---|---|
//! | `pending_login_ttl_seconds` | [`PENDING_LOGIN_LIFETIME`], ten minutes | never | [`REFRESH_TOKEN_TTL`] |
//! | `open_vault_idle_ttl_seconds` | [`REFRESH_TOKEN_TTL`], seven days | never | [`REFRESH_TOKEN_TTL`] |
//!
//! A value past its ceiling stops the daemon at startup, naming the setting — a longer lifetime
//! would outlive every session that could still refresh.
//!
//! `SessionVaults` drops either whenever it looks at it; the sweep here covers the one nobody looks
//! at again. Kept out of `auth.rs` and `runtime.rs`, both over their size budget: each calls one
//! function.

use std::path::Path;
use std::sync::{Arc, Weak};
use std::time::Duration;

use tddy_credentials::{SessionVaults, PENDING_LOGIN_LIFETIME};
use tddy_daemon_kernel::config::GitHubConfig;
use tddy_github::REFRESH_TOKEN_TTL;

use crate::AUTH_LOG_TARGET;

/// The longest the sweep waits between looks, whatever the lifetimes: a token or a data key
/// outlives its lifetime by at most this much, even with nobody signing in.
const LONGEST_SWEEP_PERIOD: Duration = Duration::from_secs(60);

/// How long the credential vaults hold each thing, resolved from the `github:` block — `None` for
/// never.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VaultLifetimes {
    /// How long a sign-in's token waits for its vault (`github.pending_login_ttl_seconds`).
    pub pending: Option<Duration>,
    /// How long an open vault may go unused (`github.open_vault_idle_ttl_seconds`).
    pub idle: Option<Duration>,
}

impl VaultLifetimes {
    /// The lifetimes `github` configures, with the defaults for what it leaves out — or, for a
    /// value past its ceiling, the error that stops the daemon, naming the setting.
    pub fn of(github: &GitHubConfig) -> anyhow::Result<Self> {
        Ok(Self {
            pending: resolved(
                "github.pending_login_ttl_seconds",
                github.pending_login_ttl_seconds,
                PENDING_LOGIN_LIFETIME,
            )?,
            idle: resolved(
                "github.open_vault_idle_ttl_seconds",
                github.open_vault_idle_ttl_seconds,
                REFRESH_TOKEN_TTL,
            )?,
        })
    }
}

/// `configured` seconds as a lifetime: `default` when absent, `None` for `0`, refused past
/// [`REFRESH_TOKEN_TTL`].
fn resolved(
    setting: &str,
    configured: Option<u64>,
    default: Duration,
) -> anyhow::Result<Option<Duration>> {
    let ceiling = REFRESH_TOKEN_TTL.as_secs();
    match configured {
        None => Ok(Some(default)),
        Some(0) => Ok(None),
        Some(seconds) if seconds <= ceiling => Ok(Some(Duration::from_secs(seconds))),
        Some(seconds) => Err(anyhow::anyhow!(
            "{setting} is at most {ceiling} (seven days, the refresh-token lifetime); got \
             {seconds}. 0 means never"
        )),
    }
}

/// The vaults over `dir`, holding a pending sign-in's token and an unused open vault for
/// `lifetimes` — and said so, once, at startup: an `info` naming each lifetime, and a `warn` for
/// each that is never.
pub fn credential_vaults_in(dir: &Path, lifetimes: &VaultLifetimes) -> SessionVaults {
    match lifetimes.pending {
        Some(pending) => log::info!(
            target: AUTH_LOG_TARGET,
            "a sign-in's GitHub token waits in memory for its credential vault for {} s \
             (github.pending_login_ttl_seconds) before it is dropped",
            pending.as_secs()
        ),
        None => log::warn!(
            target: AUTH_LOG_TARGET,
            "github.pending_login_ttl_seconds is 0: a sign-in's pending GitHub token is held in \
             memory until the vault is unlocked, its lineage logs out, or the daemon restarts — \
             and it permits choosing the vault's passphrase as long"
        ),
    }
    match lifetimes.idle {
        Some(idle) => log::info!(
            target: AUTH_LOG_TARGET,
            "an open credential vault nothing uses is closed, and its data key dropped, after {} \
             s (github.open_vault_idle_ttl_seconds)",
            idle.as_secs()
        ),
        None => log::warn!(
            target: AUTH_LOG_TARGET,
            "github.open_vault_idle_ttl_seconds is 0: an open credential vault — its data key in \
             memory — is held until its last lineage logs out or the daemon restarts, even with \
             nobody signed in"
        ),
    }
    SessionVaults::new(dir)
        .with_pending_lifetime(lifetimes.pending)
        .with_idle_lifetime(lifetimes.idle)
}

/// How often the sweep looks: once per shortest lifetime, and at least once a minute — `None`
/// when neither kind of thing ever expires.
#[must_use]
pub fn sweep_period(pending: Option<Duration>, idle: Option<Duration>) -> Option<Duration> {
    [pending, idle]
        .into_iter()
        .flatten()
        .min()
        .map(|shortest| shortest.min(LONGEST_SWEEP_PERIOD))
}

/// Start the sweep over `vaults` on the current tokio runtime — `None`, and no task at all, when
/// neither pending tokens nor open vaults ever expire. Each look skips the kind whose lifetime is
/// `0`.
///
/// The task holds the vaults weakly and ends once the daemon drops them, so it keeps nothing
/// alive. Dropping the returned handle detaches it; it does not stop it.
pub fn spawn_credential_sweep(vaults: &Arc<SessionVaults>) -> Option<tokio::task::JoinHandle<()>> {
    let period = sweep_period(vaults.pending_lifetime(), vaults.idle_lifetime())?;
    // Scheduled from now, not from whenever the task is first polled, so the first look is one
    // period after the sweep was asked for.
    let mut ticks = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    Some(tokio::spawn(sweep(Arc::downgrade(vaults), ticks)))
}

async fn sweep(vaults: Weak<SessionVaults>, mut ticks: tokio::time::Interval) {
    loop {
        ticks.tick().await;
        let Some(vaults) = vaults.upgrade() else {
            return;
        };
        if vaults.pending_lifetime().is_some() {
            vaults.expire_pending();
        }
        if vaults.idle_lifetime().is_some() {
            vaults.evict_idle();
        }
    }
}
