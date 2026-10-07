//! The production [`ScreenSharingTargetStore`]: the vaults a daemon holds, found by the session's
//! subject.
//!
//! Nothing here is screen-sharing-specific storage. A target is a `screen-sharing` record
//! ([`record_for`]) put into, listed from and removed from the same [`SessionVault`] a GitHub token
//! lives in — which is the claim this node exists to demonstrate.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tddy_credentials::{SessionVault, SessionVaults, VaultError, VaultState};
use tddy_service::proto::screen_sharing::ScreenSharingTarget;

use crate::screen_sharing_records::{
    account_for, record_for, screen_sharing_provider, target_from, ScreenSharingTargetStore,
    TargetError,
};

/// Resolve a `session_token` to the subject whose vault it may open, or `None` when the token names
/// no live session. Injected: verifying a session token is the daemon's job.
pub type SessionSubjectResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// What a person is told when the vault could not be read or written. Deliberately path-free.
const STORE_UNREADABLE: &str = "the credential store could not be read or written on this daemon";

/// A [`ScreenSharingTargetStore`] over the [`SessionVaults`] a daemon keeps open, one per
/// signed-in subject.
pub struct SessionVaultTargetStore {
    vaults: Arc<SessionVaults>,
    subject_of: SessionSubjectResolver,
}

impl SessionVaultTargetStore {
    #[must_use]
    pub fn new(vaults: Arc<SessionVaults>, subject_of: SessionSubjectResolver) -> Self {
        Self { vaults, subject_of }
    }

    /// The subject the token's session belongs to, and its vault, open on this daemon.
    ///
    /// A vault that is not open is never an empty listing: a locked one may hold targets its
    /// passphrase would show.
    fn vault_for(&self, session_token: &str) -> Result<(String, Arc<SessionVault>), TargetError> {
        let subject = (self.subject_of)(session_token).ok_or(TargetError::NoSuchSession)?;
        if let Some(refusal) = refusal_for_closed(self.vaults.state(&subject)) {
            return Err(refusal);
        }
        match self.vaults.use_open(&subject) {
            Some(vault) => Ok((subject, vault)),
            // Closed between the two look-ups: whatever it is now is the answer.
            None => Err(
                refusal_for_closed(self.vaults.state(&subject)).unwrap_or_else(|| {
                    TargetError::Unavailable(
                    "the credential vault closed and reopened while it was being read; try again"
                        .to_string(),
                )
                }),
            ),
        }
    }
}

fn refusal_for_closed(state: VaultState) -> Option<TargetError> {
    match state {
        VaultState::Open => None,
        VaultState::Locked => Some(TargetError::Locked),
        VaultState::Uninitialized => Some(TargetError::Unavailable(
            "no credential vault exists yet; choose a passphrase to create one".to_string(),
        )),
    }
}

/// `Locked` keeps its meaning. `Io` names server-side detail, so the client gets
/// [`STORE_UNREADABLE`] and the log gets the full error; every other variant's text is a fixed
/// sentence written for the person.
fn refusal_of(subject: &str, error: VaultError) -> TargetError {
    match error {
        VaultError::Locked => TargetError::Locked,
        VaultError::Io(_) => {
            log::error!(
                target: "tddy_screen_sharing",
                "credential store for {subject} could not be read or written: {error}"
            );
            TargetError::Unavailable(STORE_UNREADABLE.to_string())
        }
        other => TargetError::Unavailable(other.to_string()),
    }
}

fn now_unix_seconds() -> Result<u64, TargetError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .map_err(|e| TargetError::Unavailable(format!("the system clock is before 1970: {e}")))
}

impl ScreenSharingTargetStore for SessionVaultTargetStore {
    fn list(&self, session_token: &str) -> Result<Vec<ScreenSharingTarget>, TargetError> {
        let (subject, vault) = self.vault_for(session_token)?;
        vault
            .list(Some(&screen_sharing_provider()))
            .map_err(|error| refusal_of(&subject, error))?
            .iter()
            .map(target_from)
            .collect()
    }

    fn add(
        &self,
        session_token: &str,
        target: &ScreenSharingTarget,
        password: &str,
    ) -> Result<ScreenSharingTarget, TargetError> {
        let (subject, vault) = self.vault_for(session_token)?;
        let mut stored = target.clone();
        stored.id = format!("target-{}", uuid::Uuid::new_v4().simple());
        vault
            .put(record_for(&stored, password, now_unix_seconds()?))
            .map_err(|error| refusal_of(&subject, error))?;
        Ok(stored)
    }

    fn remove(&self, session_token: &str, target_id: &str) -> Result<(), TargetError> {
        let (subject, vault) = self.vault_for(session_token)?;
        vault
            .remove(&screen_sharing_provider(), &account_for(target_id))
            .map_err(|error| refusal_of(&subject, error))
    }

    fn password_for(&self, session_token: &str, target_id: &str) -> Result<String, TargetError> {
        let (subject, vault) = self.vault_for(session_token)?;
        vault
            .get(&screen_sharing_provider(), &account_for(target_id))
            .map_err(|error| refusal_of(&subject, error))?
            .map(|record| record.secret.expose().to_string())
            .ok_or_else(|| TargetError::Malformed(format!("no target {target_id}")))
    }
}
