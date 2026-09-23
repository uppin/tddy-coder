//! Acceptance: a credential is readable in-session and unreadable from the file alone.
//!
//! These are the properties an operator is entitled to, stated against the store's own API rather
//! than against a wired daemon, because none of them depends on how the daemon is configured: a
//! backup of the data directory must not contain live credentials or the passphrase, a wrong
//! passphrase must not silently discard the vault, a forgotten one must not destroy it, and
//! altering a record must be detected rather than absorbed.
//!
//! The rules this store *inherits* from the trait it replaces — a login whose credential cannot be
//! retained is reported, and a secret never reaches an RPC response path — are login-level, and
//! live in `tddy-daemon-auth`'s acceptance tests.

use tddy_credentials::{
    AccountId, CredentialRecord, CredentialStore, ProviderId, SecretString, VaultError,
};

const THE_OPERATOR: &str = "operator";
const THE_PASSPHRASE: &str = "correct horse battery staple";
const A_WRONG_PASSPHRASE: &str = "incorrect horse battery staple";
const A_NEW_PASSPHRASE: &str = "a passphrase chosen after forgetting";
const THE_SECRET: &str = "gho_a_live_repo_scoped_credential";
const THE_LABEL: &str = "Work account";
const THE_METADATA_VALUE: &str = "repo,read:user";

#[test]
fn a_credential_written_in_one_session_opens_in_the_next_with_the_same_passphrase() {
    // Given an operator who stored a credential and then signed out
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = CredentialStore::path_in(dir.path(), THE_OPERATOR);
    let first_session = CredentialStore::create(&path, &the_passphrase(), THE_OPERATOR)
        .expect("a fresh vault is created");
    first_session
        .put(a_github_credential())
        .expect("a credential is retained");
    drop(first_session);

    // When they open it again with the passphrase they chose
    let next_session =
        CredentialStore::open_with_passphrase(&path, &the_passphrase(), THE_OPERATOR)
            .expect("the same passphrase opens the same vault");

    // Then the credential is there, exactly as it was written
    assert_eq!(
        next_session.get(&github(), &the_account()),
        Ok(Some(a_github_credential())),
        "the passphrase, not whatever token a login happened to receive, opens the vault"
    );
}

#[test]
fn the_file_on_disk_holds_no_plaintext_secret_label_metadata_or_passphrase() {
    // Given a vault holding one credential
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = CredentialStore::path_in(dir.path(), THE_OPERATOR);
    CredentialStore::create(&path, &the_passphrase(), THE_OPERATOR)
        .expect("a fresh vault is created")
        .put(a_github_credential())
        .expect("a credential is retained");

    // When whoever holds a backup of the data directory reads the bytes
    let on_disk =
        String::from_utf8_lossy(&std::fs::read(&path).expect("the vault is on disk")).to_string();

    // Then none of what the operator stored, nor the passphrase that opens it, is legible in them
    let leaked: Vec<&str> = [THE_SECRET, THE_LABEL, THE_METADATA_VALUE, THE_PASSPHRASE]
        .into_iter()
        .filter(|plaintext| on_disk.contains(plaintext))
        .collect();
    assert_eq!(
        leaked,
        Vec::<&str>::new(),
        "a backup of the data directory must not contain live credentials or the passphrase"
    );
}

#[test]
fn altering_a_sealed_record_is_detected_rather_than_absorbed() {
    // Given a vault holding one credential, whose sealed record someone edits on disk
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = CredentialStore::path_in(dir.path(), THE_OPERATOR);
    CredentialStore::create(&path, &the_passphrase(), THE_OPERATOR)
        .expect("a fresh vault is created")
        .put(a_github_credential())
        .expect("a credential is retained");
    flip_the_last_ciphertext_byte(&path);

    // When the operator opens the vault and reads the account
    let reopened = CredentialStore::open_with_passphrase(&path, &the_passphrase(), THE_OPERATOR)
        .expect("the vault itself still opens; only the record was altered");

    // Then the alteration is reported, never read past
    assert_eq!(
        reopened.get(&github(), &the_account()),
        Err(VaultError::Crypto),
        "an altered record must be reported, not silently treated as absent"
    );
}

#[test]
fn a_sealed_record_moved_under_another_accounts_id_does_not_open() {
    // Given a vault holding two accounts, and someone who swaps their sealed records' ids on disk
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = CredentialStore::path_in(dir.path(), THE_OPERATOR);
    let vault = CredentialStore::create(&path, &the_passphrase(), THE_OPERATOR)
        .expect("a fresh vault is created");
    vault.put(a_github_credential()).expect("the first account");
    vault
        .put(a_second_github_credential())
        .expect("the second account");
    swap_the_two_record_ids(&path);

    // When the first account is read
    let read = vault.get(&github(), &the_account());

    // Then the identity sealed inside the record no longer matches the slot it was found in
    assert_eq!(
        read,
        Err(VaultError::Crypto),
        "a record must not open under another account's id"
    );
}

#[test]
fn a_wrong_passphrase_locks_the_vault_and_changes_nothing_in_it() {
    // Given a vault sealed under one passphrase
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = CredentialStore::path_in(dir.path(), THE_OPERATOR);
    CredentialStore::create(&path, &the_passphrase(), THE_OPERATOR)
        .expect("a fresh vault is created")
        .put(a_github_credential())
        .expect("a credential is retained");
    let before = std::fs::read(&path).expect("the vault is on disk");

    // When somebody presents a different one
    let refused = CredentialStore::open_with_passphrase(
        &path,
        &SecretString::new(A_WRONG_PASSPHRASE),
        THE_OPERATOR,
    )
    .err();

    // Then the vault is locked, and it is still all there — no re-initialisation, no second key
    assert_eq!(
        (refused, std::fs::read(&path).ok()),
        (Some(VaultError::Locked), Some(before)),
        "a vault that will not open must be reported, never replaced with an empty one"
    );
}

#[test]
fn a_vault_sealed_for_one_subject_does_not_open_for_another_with_the_same_passphrase() {
    // Given a vault sealed for one operator
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = CredentialStore::path_in(dir.path(), THE_OPERATOR);
    CredentialStore::create(&path, &the_passphrase(), THE_OPERATOR)
        .expect("a fresh vault is created");

    // When a second operator presents the same passphrase against that file
    let refused =
        CredentialStore::open_with_passphrase(&path, &the_passphrase(), "somebody-else").err();

    // Then the subject is bound into the derivation
    assert_eq!(refused, Some(VaultError::Locked));
}

#[test]
fn opening_a_vault_that_was_never_created_reports_it_uninitialized_and_creates_nothing() {
    // Given no vault
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = CredentialStore::path_in(dir.path(), THE_OPERATOR);

    // When it is opened
    let refused =
        CredentialStore::open_with_passphrase(&path, &the_passphrase(), THE_OPERATOR).err();

    // Then the answer says a passphrase has to be chosen, and still no file exists
    assert_eq!(
        (refused, path.exists()),
        (Some(VaultError::Uninitialized), false)
    );
}

#[test]
fn creating_over_an_existing_vault_is_refused_and_leaves_it_as_it_was() {
    // Given a vault that already holds a credential
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = CredentialStore::path_in(dir.path(), THE_OPERATOR);
    CredentialStore::create(&path, &the_passphrase(), THE_OPERATOR)
        .expect("a fresh vault is created")
        .put(a_github_credential())
        .expect("a credential is retained");
    let before = std::fs::read(&path).expect("the vault is on disk");

    // When a second create is attempted under another passphrase
    let refused =
        CredentialStore::create(&path, &SecretString::new(A_NEW_PASSPHRASE), THE_OPERATOR).err();

    // Then it is refused by name, and nothing was overwritten
    assert_eq!(
        (refused, std::fs::read(&path).ok()),
        (Some(VaultError::AlreadyInitialized), Some(before))
    );
}

#[test]
fn a_reset_sets_the_old_vault_aside_intact_and_opens_a_fresh_one_under_the_new_passphrase() {
    // Given a vault whose passphrase has been forgotten
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = CredentialStore::path_in(dir.path(), THE_OPERATOR);
    CredentialStore::create(&path, &the_passphrase(), THE_OPERATOR)
        .expect("a fresh vault is created")
        .put(a_github_credential())
        .expect("a credential is retained");
    let old_bytes = std::fs::read(&path).expect("the vault is on disk");

    // When the operator resets it under a new passphrase
    let (fresh, set_aside) =
        CredentialStore::reset(&path, &SecretString::new(A_NEW_PASSPHRASE), THE_OPERATOR)
            .expect("a reset succeeds");
    let set_aside = set_aside.expect("an existing vault is set aside");

    // Then the old file sits beside the new one byte-for-byte, and the new one is empty
    assert_eq!(
        (
            std::fs::read(&set_aside).ok(),
            fresh.list(None),
            set_aside
                .file_name()
                .map(|name| name.to_string_lossy().contains(".locked-"))
        ),
        (Some(old_bytes), Ok(Vec::new()), Some(true)),
        "a reset renames the old vault aside — it never deletes it"
    );
}

#[test]
fn after_a_reset_only_the_new_passphrase_opens_the_vault() {
    // Given a vault reset under a new passphrase
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = CredentialStore::path_in(dir.path(), THE_OPERATOR);
    CredentialStore::create(&path, &the_passphrase(), THE_OPERATOR)
        .expect("a fresh vault is created");
    CredentialStore::reset(&path, &SecretString::new(A_NEW_PASSPHRASE), THE_OPERATOR)
        .expect("a reset succeeds");

    // When each passphrase is tried
    let opens = |passphrase: &str| {
        CredentialStore::open_with_passphrase(&path, &SecretString::new(passphrase), THE_OPERATOR)
            .is_ok()
    };

    // Then the forgotten one opens nothing
    assert_eq!(
        (opens(THE_PASSPHRASE), opens(A_NEW_PASSPHRASE)),
        (false, true)
    );
}

#[test]
fn a_removed_credential_is_gone_and_the_others_are_not() {
    // Given a vault holding two accounts at the same provider
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = CredentialStore::path_in(dir.path(), THE_OPERATOR);
    let vault = CredentialStore::create(&path, &the_passphrase(), THE_OPERATOR)
        .expect("a fresh vault is created");
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
    let vault = CredentialStore::create(&path, &the_passphrase(), THE_OPERATOR)
        .expect("a fresh vault is created");
    vault.put(a_github_credential()).expect("a github account");
    vault
        .put(a_cloudflare_credential())
        .expect("a cloudflare account");

    // When the Accounts screen asks for one provider's accounts
    let github_accounts = vault.list(Some(&github()));

    // Then it is handed that provider's and no other
    assert_eq!(github_accounts, Ok(vec![a_github_credential()]));
}

#[test]
fn a_record_prints_without_its_secret() {
    // Given a record somebody formats into a log line
    let record = a_github_credential();

    // When it is formatted
    let printed = format!("{record:?}");

    // Then the secret is not in the line
    assert!(
        !printed.contains(THE_SECRET),
        "a record's Debug must redact its secret, printed: {printed}"
    );
}

fn the_passphrase() -> SecretString {
    SecretString::new(THE_PASSPHRASE)
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
        secret: SecretString::new(THE_SECRET),
        metadata: [("scopes".to_string(), THE_METADATA_VALUE.to_string())].into(),
        updated_at: 1_758_240_000,
    }
}

fn a_second_github_credential() -> CredentialRecord {
    CredentialRecord {
        provider: github(),
        account: AccountId::new("operator-personal"),
        label: "Personal account".to_string(),
        secret: SecretString::new("gho_the_other_one"),
        metadata: Default::default(),
        updated_at: 1_758_240_001,
    }
}

fn a_cloudflare_credential() -> CredentialRecord {
    CredentialRecord {
        provider: ProviderId::new("cloudflare"),
        account: AccountId::new("the-zone-account"),
        label: "Zone admin".to_string(),
        secret: SecretString::new("cf_an_api_token"),
        metadata: Default::default(),
        updated_at: 1_758_240_002,
    }
}

/// Alter one byte of the sealed record, standing in for anyone who can write the file.
///
/// The last hex digit of the file is inside the final record's ciphertext (its Poly1305 tag),
/// because the records are the last thing the format writes. Flipping it is the smallest edit that
/// must not pass.
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

/// Exchange the `id`s of the file's two sealed records, leaving each ciphertext where it was —
/// someone moving one account's credential into another account's slot.
fn swap_the_two_record_ids(path: &std::path::Path) {
    let raw = std::fs::read_to_string(path).expect("the vault is on disk");
    let mut document: serde_json::Value =
        serde_json::from_str(&raw).expect("the vault is a json document");
    let records = document
        .get_mut("records")
        .and_then(serde_json::Value::as_array_mut)
        .expect("the vault holds records");
    let first = records[0]["id"].clone();
    records[0]["id"] = records[1]["id"].clone();
    records[1]["id"] = first;
    std::fs::write(
        path,
        serde_json::to_string_pretty(&document).expect("the document serialises"),
    )
    .expect("the vault is writable");
}
