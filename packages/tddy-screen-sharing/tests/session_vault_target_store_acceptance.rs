//! The production target store, over a real sealed credential vault.
//!
//! The service acceptance suite fakes storage so that it can hold the *service* still; this one
//! holds the storage still. Nothing here is faked: a target goes through [`SessionVaults`] into the
//! file `tddy-credentials` seals, and comes back out of it.

use std::sync::Arc;

use tddy_credentials::{CredentialStore, SecretString, SessionVaults};
use tddy_screen_sharing::screen_sharing_records::{ScreenSharingTargetStore, TargetError};
use tddy_screen_sharing::session_vault_target_store::SessionVaultTargetStore;
use tddy_service::proto::screen_sharing::{Protocol, ScreenSharingTarget};

const AN_OPERATOR: &str = "an-operator";
const THE_FIRST_SESSION: &str = "the-first-session-token";
const THE_NEXT_SESSION: &str = "the-next-session-token";
const A_DESKTOP_PASSWORD: &str = "the-desktop-password";
const A_DESKTOP_HOST: &str = "desktop-host.internal.example";

fn a_passphrase() -> SecretString {
    SecretString::new("the-passphrase-the-operator-chose")
}

fn a_desktop() -> ScreenSharingTarget {
    ScreenSharingTarget {
        id: String::new(),
        label: "dev box".to_string(),
        host: A_DESKTOP_HOST.to_string(),
        port: 5900,
        protocol: Protocol::Vnc as i32,
        username: "ada".to_string(),
    }
}

/// A daemon's vaults with the operator's store created on disk; open only when `unlocked`.
struct ADaemon {
    /// Held so the directory outlives the test; dropping it deletes the vault.
    _storage: tempfile::TempDir,
    vaults: Arc<SessionVaults>,
}

impl ADaemon {
    fn whose_operator_has_unlocked_the_store() -> Self {
        let daemon = Self::with_a_sealed_store();
        daemon
            .vaults
            .unlock(AN_OPERATOR, &a_passphrase())
            .expect("the operator's passphrase opens the store");
        daemon
    }

    fn with_a_sealed_store() -> Self {
        let storage = tempfile::tempdir().expect("a storage directory");
        let vaults = Arc::new(SessionVaults::new(storage.path()));
        CredentialStore::create(&vaults.path_for(AN_OPERATOR), &a_passphrase(), AN_OPERATOR)
            .expect("a fresh store is created");
        Self {
            _storage: storage,
            vaults,
        }
    }

    /// Both session tokens belong to the operator; any other token names no session.
    fn store(&self) -> SessionVaultTargetStore {
        SessionVaultTargetStore::new(
            Arc::clone(&self.vaults),
            Arc::new(|token: &str| {
                matches!(token, THE_FIRST_SESSION | THE_NEXT_SESSION)
                    .then(|| AN_OPERATOR.to_string())
            }),
        )
    }

    fn sealed_file(&self) -> String {
        std::fs::read_to_string(self.vaults.path_for(AN_OPERATOR)).expect("the sealed file")
    }
}

#[test]
fn a_desktop_added_in_one_session_is_listed_in_the_next() {
    // Given a desktop added through the first session
    let daemon = ADaemon::whose_operator_has_unlocked_the_store();
    let store = daemon.store();
    let added = store
        .add(THE_FIRST_SESSION, &a_desktop(), A_DESKTOP_PASSWORD)
        .expect("the desktop is added");

    // When the next session lists
    let listed = store.list(THE_NEXT_SESSION).expect("a listing");

    // Then
    assert_eq!(listed, vec![added]);
}

#[test]
fn a_desktops_password_is_read_back_from_the_vault() {
    // Given
    let daemon = ADaemon::whose_operator_has_unlocked_the_store();
    let store = daemon.store();
    let added = store
        .add(THE_FIRST_SESSION, &a_desktop(), A_DESKTOP_PASSWORD)
        .expect("the desktop is added");

    // When
    let password = store.password_for(THE_NEXT_SESSION, &added.id);

    // Then
    assert_eq!(password, Ok(A_DESKTOP_PASSWORD.to_string()));
}

#[test]
fn a_removed_desktop_is_no_longer_listed() {
    // Given
    let daemon = ADaemon::whose_operator_has_unlocked_the_store();
    let store = daemon.store();
    let added = store
        .add(THE_FIRST_SESSION, &a_desktop(), A_DESKTOP_PASSWORD)
        .expect("the desktop is added");

    // When
    store
        .remove(THE_FIRST_SESSION, &added.id)
        .expect("the desktop is removed");

    // Then
    assert_eq!(store.list(THE_FIRST_SESSION), Ok(vec![]));
}

#[test]
fn what_a_desktop_is_stays_out_of_the_sealed_file_in_the_clear() {
    // Given a desktop added to the store
    let daemon = ADaemon::whose_operator_has_unlocked_the_store();
    daemon
        .store()
        .add(THE_FIRST_SESSION, &a_desktop(), A_DESKTOP_PASSWORD)
        .expect("the desktop is added");

    // When someone reads the file without opening it
    let on_disk = daemon.sealed_file();

    // Then neither the machine's address nor its password is there to read
    assert!(
        !on_disk.contains(A_DESKTOP_HOST) && !on_disk.contains(A_DESKTOP_PASSWORD),
        "the sealed file leaked a desktop's details: {on_disk}"
    );
}

#[test]
fn a_store_nobody_has_unlocked_is_locked_rather_than_empty() {
    // Given a store on disk that this daemon has not opened
    let daemon = ADaemon::with_a_sealed_store();

    // When
    let listed = daemon.store().list(THE_FIRST_SESSION);

    // Then
    assert_eq!(listed, Err(TargetError::Locked));
}

#[test]
fn a_token_naming_no_session_reaches_no_store() {
    // Given
    let daemon = ADaemon::whose_operator_has_unlocked_the_store();

    // When
    let listed = daemon.store().list("a-token-nobody-issued");

    // Then
    assert_eq!(listed, Err(TargetError::NoSuchSession));
}
