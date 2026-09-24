//! How long a credential vault's secrets stay in memory with nobody using them, and the sweep that
//! drops them when nobody asks.
//!
//! Two things are held in `SessionVaults`, and each has a lifetime from the daemon's config:
//!
//! - **a pending sign-in's token** — a sign-in over a closed vault holds its GitHub token unsealed
//!   until the passphrase opens the vault, and that waiting token is what permits a first
//!   passphrase or a reset. It expires after `github.pending_login_ttl_seconds`
//!   (`tddy_daemon_kernel::pending_login_ttl`);
//! - **an open vault's data key** — kept so PR-status reads can use the stored token, and closed
//!   once nothing has used the vault for `github.open_vault_idle_ttl_seconds`
//!   (`tddy_daemon_kernel::open_vault_idle_ttl`). The next refresh presenting an unlock key reopens
//!   it, exactly as after a restart.
//!
//! `SessionVaults` drops either whenever it looks at it; the sweep here covers the one nobody looks
//! at again. Kept out of `auth.rs` and `runtime.rs`, both over their size budget: each calls one
//! function.

use std::path::Path;
use std::sync::{Arc, Weak};
use std::time::Duration;

use tddy_credentials::SessionVaults;
use tddy_daemon_kernel::config::GitHubConfig;

use crate::AUTH_LOG_TARGET;

/// The longest the sweep waits between looks, whatever the lifetimes: a token or a data key
/// outlives its lifetime by at most this much, even with nobody signing in.
const LONGEST_SWEEP_PERIOD: Duration = Duration::from_secs(60);

/// The vaults over `dir`, holding a pending sign-in's token and an unused open vault for as long as
/// `github` says — and said so, once, at startup: an `info` naming each lifetime, and a `warn` for
/// each that is `0`.
pub fn credential_vaults_in(dir: &Path, github: &GitHubConfig) -> SessionVaults {
    let (pending, idle) = (
        github.pending_login_ttl_seconds,
        github.open_vault_idle_ttl_seconds,
    );
    match pending.lifetime() {
        Some(_) => log::info!(
            target: AUTH_LOG_TARGET,
            "a sign-in's GitHub token waits in memory for its credential vault for {pending} \
             (github.pending_login_ttl_seconds) before it is dropped"
        ),
        None => log::warn!(
            target: AUTH_LOG_TARGET,
            "github.pending_login_ttl_seconds is 0: a sign-in's pending GitHub token is held in \
             memory until the vault is unlocked, its lineage logs out, or the daemon restarts — \
             and it permits choosing the vault's passphrase as long"
        ),
    }
    match idle.lifetime() {
        Some(_) => log::info!(
            target: AUTH_LOG_TARGET,
            "an open credential vault nothing uses is closed, and its data key dropped, after \
             {idle} (github.open_vault_idle_ttl_seconds)"
        ),
        None => log::warn!(
            target: AUTH_LOG_TARGET,
            "github.open_vault_idle_ttl_seconds is 0: an open credential vault — its data key in \
             memory — is held until its last lineage logs out or the daemon restarts, even with \
             nobody signed in"
        ),
    }
    SessionVaults::new(dir)
        .with_pending_lifetime(pending.lifetime())
        .with_idle_lifetime(idle.lifetime())
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
