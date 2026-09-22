//! Acceptance: a credential is readable in-session and unreadable from the file alone.
//!
//! These are the properties an operator is entitled to, stated against the store's own API rather
//! than against a wired daemon, because none of them depends on how the daemon is configured: a
//! backup of the data directory must not contain live credentials, a rotated login must not
//! silently discard the vault, and altering a record must be detected rather than absorbed.
//!
//! The two rules this store *inherits* from the trait it replaces — a failed write fails the login,
//! and a secret never reaches an RPC response path — are login-level, and live in
//! `tddy-daemon-auth`'s `login_opens_the_credential_store_acceptance.rs`.

use tddy_credentials::{AccountId, CredentialRecord, CredentialStore, ProviderId, VaultError};

const THE_OPERATOR: &str = "operator";
const THE_LOGIN_CREDENTIAL: &[u8] = b"gho_the_token_this_login_granted";
const ANOTHER_LOGIN_CREDENTIAL: &[u8] = b"gho_a_token_from_a_later_authorisation";
const THE_SECRET: &str = "gho_a_live_repo_scoped_credential";
const THE_LABEL: &str = "Work account";
const THE_METADATA_VALUE: &str = "repo,read:user";

#[test]
fn a_credential_written_in_one_session_opens_in_the_next() {
    // Given an operator who stored a credential and then signed out
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = CredentialStore::path_in(dir.path(), THE_OPERATOR);
    let first_session = CredentialStore::open_or_create(&path, THE_LOGIN_CREDENTIAL, THE_OPERATOR)
        .expect("a fresh vault opens");
    first_session
        .put(a_github_credential())
        .expect("a credential is retained");
    drop(first_session);

    // When they sign in again, deriving the same key from the same login credential
    let next_session = CredentialStore::open_or_create(&path, THE_LOGIN_CREDENTIAL, THE_OPERATOR)
        .expect("the same login credential opens the same vault");

    // Then the credential is there, exactly as it was written
    assert_eq!(
        next_session.get(&github(), &the_account()),
        Ok(Some(a_github_credential())),
        "a second login by the same user must open the same vault"
    );
}

#[test]
fn the_file_on_disk_holds_no_plaintext_secret_label_or_metadata() {
    // Given a vault holding one credential
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = CredentialStore::path_in(dir.path(), THE_OPERATOR);
    let vault = CredentialStore::open_or_create(&path, THE_LOGIN_CREDENTIAL, THE_OPERATOR)
        .expect("a fresh vault opens");
    vault
        .put(a_github_credential())
        .expect("a credential is retained");

    // When whoever holds a backup of the data directory reads the bytes
    let bytes = std::fs::read(&path).expect("the vault is on disk");
    let on_disk = String::from_utf8_lossy(&bytes).to_string();

    // Then none of what the operator stored is legible in them
    let leaked: Vec<&str> = [THE_SECRET, THE_LABEL, THE_METADATA_VALUE]
        .into_iter()
        .filter(|plaintext| on_disk.contains(plaintext))
        .collect();
    assert_eq!(
        leaked,
        Vec::<&str>::new(),
        "a backup of the data directory must not contain live credentials"
    );
}

#[test]
fn altering_a_stored_records_label_is_detected_rather_than_absorbed() {
    // Given a vault holding one credential, whose label someone edits on disk
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = CredentialStore::path_in(dir.path(), THE_OPERATOR);
    let vault = CredentialStore::open_or_create(&path, THE_LOGIN_CREDENTIAL, THE_OPERATOR)
        .expect("a fresh vault opens");
    vault
        .put(a_github_credential())
        .expect("a credential is retained");
    drop(vault);
    flip_the_last_ciphertext_byte(&path);

    // When the operator signs in and reads the account
    let reopened = CredentialStore::open_or_create(&path, THE_LOGIN_CREDENTIAL, THE_OPERATOR)
        .expect("the vault itself still opens; only the record was altered");

    // Then the alteration is reported, never read past — the label and metadata are inside the
    // seal, which is what a vault holding them in cleartext beside the secret cannot promise
    assert_eq!(
        reopened.get(&github(), &the_account()),
        Err(VaultError::Crypto),
        "an altered record must be reported, not silently treated as absent"
    );
}

#[test]
fn a_different_login_credential_locks_the_vault_and_changes_nothing_in_it() {
    // Given a vault sealed under one login credential
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = CredentialStore::path_in(dir.path(), THE_OPERATOR);
    let vault = CredentialStore::open_or_create(&path, THE_LOGIN_CREDENTIAL, THE_OPERATOR)
        .expect("a fresh vault opens");
    vault
        .put(a_github_credential())
        .expect("a credential is retained");
    drop(vault);
    let before = std::fs::read(&path).expect("the vault is on disk");

    // When the user's credential has rotated and the derived key no longer unwraps the data key
    let refused =
        CredentialStore::open_or_create(&path, ANOTHER_LOGIN_CREDENTIAL, THE_OPERATOR).err();

    // Then the vault is locked, and it is still all there — no re-initialisation, no second key
    assert_eq!(
        (refused, std::fs::read(&path).ok()),
        (Some(VaultError::Locked), Some(before)),
        "a vault that will not open must be reported, never replaced with an empty one"
    );
}

#[test]
fn a_vault_sealed_for_one_subject_does_not_open_for_another() {
    // Given a vault sealed for one operator
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = CredentialStore::path_in(dir.path(), THE_OPERATOR);
    CredentialStore::open_or_create(&path, THE_LOGIN_CREDENTIAL, THE_OPERATOR)
        .expect("a fresh vault opens");

    // When a second operator presents the same bytes as their own login credential
    let refused =
        CredentialStore::open_or_create(&path, THE_LOGIN_CREDENTIAL, "somebody-else").err();

    // Then the subject is bound into the derivation, so identical input keying material is not
    // enough — one user's credential never opens another's vault
    assert_eq!(refused, Some(VaultError::Locked));
}

#[test]
fn rewrapping_moves_the_vault_onto_the_new_key_and_off_the_old_one() {
    // Given a vault holding a credential, re-wrapped as a successful login would re-wrap it
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = CredentialStore::path_in(dir.path(), THE_OPERATOR);
    let vault = CredentialStore::open_or_create(&path, THE_LOGIN_CREDENTIAL, THE_OPERATOR)
        .expect("a fresh vault opens");
    vault
        .put(a_github_credential())
        .expect("a credential is retained");

    // When the login credential rotates while a session is still open
    vault
        .rewrap(ANOTHER_LOGIN_CREDENTIAL)
        .expect("a live session can move the vault onto a new key");
    drop(vault);

    // Then the new credential opens it and the old one no longer does
    let under_the_new =
        CredentialStore::open_or_create(&path, ANOTHER_LOGIN_CREDENTIAL, THE_OPERATOR)
            .and_then(|vault| vault.get(&github(), &the_account()));
    let under_the_old =
        CredentialStore::open_or_create(&path, THE_LOGIN_CREDENTIAL, THE_OPERATOR).err();
    assert_eq!(
        (under_the_new, under_the_old),
        (Ok(Some(a_github_credential())), Some(VaultError::Locked)),
        "re-wrapping must carry the records over and leave the old key with nothing"
    );
}

#[test]
fn a_removed_credential_is_gone_and_the_others_are_not() {
    // Given a vault holding two accounts at the same provider
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = CredentialStore::path_in(dir.path(), THE_OPERATOR);
    let vault = CredentialStore::open_or_create(&path, THE_LOGIN_CREDENTIAL, THE_OPERATOR)
        .expect("a fresh vault opens");
    vault.put(a_github_credential()).expect("the first account");
    vault
        .put(a_second_github_credential())
        .expect("the second account");

    // When the operator unlinks one of them
    vault
        .remove(&github(), &the_account())
        .expect("an account is unlinked");

    // Then only that one is gone
    assert_eq!(
        (
            vault.get(&github(), &the_account()),
            vault.list(Some(&github()))
        ),
        (Ok(None), Ok(vec![a_second_github_credential()])),
        "unlinking one account must not disturb the others"
    );
}

#[test]
fn listing_is_scoped_to_the_provider_it_names() {
    // Given a vault holding accounts at two providers
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = CredentialStore::path_in(dir.path(), THE_OPERATOR);
    let vault = CredentialStore::open_or_create(&path, THE_LOGIN_CREDENTIAL, THE_OPERATOR)
        .expect("a fresh vault opens");
    vault.put(a_github_credential()).expect("a github account");
    vault
        .put(a_cloudflare_credential())
        .expect("a cloudflare account");

    // When the Accounts screen asks for one provider's accounts
    let github_accounts = vault.list(Some(&github()));

    // Then it is handed that provider's and no other — the dimension a login-keyed map never had
    assert_eq!(github_accounts, Ok(vec![a_github_credential()]));
}

fn github() -> ProviderId {
    ProviderId::new("github")
}

fn the_account() -> AccountId {
    AccountId::new("operator")
}

fn a_github_credential() -> CredentialRecord {
    CredentialRecord {
        provider: github(),
        account: the_account(),
        label: THE_LABEL.to_string(),
        secret: THE_SECRET.to_string(),
        metadata: [("scopes".to_string(), THE_METADATA_VALUE.to_string())].into(),
        updated_at: 1_758_240_000,
    }
}

fn a_second_github_credential() -> CredentialRecord {
    CredentialRecord {
        provider: github(),
        account: AccountId::new("operator-personal"),
        label: "Personal account".to_string(),
        secret: "gho_the_other_one".to_string(),
        metadata: Default::default(),
        updated_at: 1_758_240_001,
    }
}

fn a_cloudflare_credential() -> CredentialRecord {
    CredentialRecord {
        provider: ProviderId::new("cloudflare"),
        account: AccountId::new("the-zone-account"),
        label: "Zone admin".to_string(),
        secret: "cf_an_api_token".to_string(),
        metadata: Default::default(),
        updated_at: 1_758_240_002,
    }
}

/// Alter one byte of the sealed record, standing in for anyone who can write the file.
///
/// The last hex digit of the file is inside the final record's ciphertext, because the records are
/// the last thing the format writes. Flipping it is the smallest edit that must not pass.
fn flip_the_last_ciphertext_byte(path: &std::path::Path) {
    let contents = std::fs::read_to_string(path).expect("the vault is on disk");
    let altered_at = contents
        .rfind(|c: char| c.is_ascii_hexdigit())
        .expect("the sealed records are hex");
    let altered_digit = match &contents[altered_at..altered_at + 1] {
        "0" => "1",
        _ => "0",
    };
    let altered = format!(
        "{}{}{}",
        &contents[..altered_at],
        altered_digit,
        &contents[altered_at + 1..]
    );
    std::fs::write(path, altered).expect("the vault is writable");
}
