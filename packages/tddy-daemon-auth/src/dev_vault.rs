//! `github.dev_vault_without_passphrase` — the development exclusion from vault passphrases, and
//! the fence around it.
//!
//! With it on, the auth service creates and opens every credential vault under the empty passphrase
//! itself, so `./web-dev` and `./desktop-dev` never prompt for one (see `tddy-github`'s
//! `AuthServiceImpl::with_vaults_without_passphrase`). Anyone can derive the key such a vault is
//! sealed under, so whatever is linked into it — provider accounts on the Accounts screen — is as
//! good as plaintext on the daemon's disk. That is acceptable only where no real GitHub token can
//! ever be sealed: a daemon whose provider is the stub. Any other daemon **does not start** with it.

use tddy_daemon_kernel::config::GitHubConfig;

use crate::AUTH_LOG_TARGET;

/// Whether this daemon's vaults open without a passphrase. Refused — the daemon does not start —
/// when the switch is on without `stub: true`; said at startup, as a warning, when it is on.
pub fn vaults_without_passphrase(github: &GitHubConfig) -> anyhow::Result<bool> {
    if github.dev_vault_without_passphrase != Some(true) {
        return Ok(false);
    }
    if github.stub != Some(true) {
        anyhow::bail!(
            "github.dev_vault_without_passphrase is a development setting and needs `stub: true`: \
             a vault opened without a passphrase is sealed under a key anyone can derive, and this \
             daemon's logins carry real GitHub tokens. Remove the setting."
        );
    }
    log::warn!(
        target: AUTH_LOG_TARGET,
        "github.dev_vault_without_passphrase is on: credential vaults are created and opened \
         under the empty passphrase, so anything linked into them is readable by whoever can read \
         auth_storage. For development only."
    );
    Ok(true)
}
