//! Which of ssh's askpass requests the helper may relay to an operator.
//!
//! ssh calls its helper for more than passphrases — host-key confirmation, account passwords — and
//! hands it only the question as one argument. Only a key passphrase request may be relayed; relaying
//! anything else would put "type yes to trust this host" in front of an operator as though it were a
//! password prompt. The wording below is ssh's own, as `clone_askpass_acceptance` observes it.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-10-17-clone-passphrase-prompt.md (AC1, AC6)

use tddy_askpass::prompt_text::{classify, AskpassPrompt};

fn key_passphrase_for(key: &str) -> AskpassPrompt {
    AskpassPrompt::KeyPassphrase {
        key: key.to_string(),
    }
}

#[test]
fn a_key_passphrase_request_names_the_key() {
    assert_eq!(
        classify("Enter passphrase for key '/home/ada/.ssh/id_rsa': "),
        key_passphrase_for("/home/ada/.ssh/id_rsa")
    );
}

#[test]
fn a_key_path_with_spaces_is_kept_whole() {
    assert_eq!(
        classify("Enter passphrase for key '/home/ada/my keys/id ed25519': "),
        key_passphrase_for("/home/ada/my keys/id ed25519")
    );
}

#[test]
fn a_key_path_containing_a_quote_is_kept_whole() {
    assert_eq!(
        classify("Enter passphrase for key '/home/ada/ada's keys/id_rsa': "),
        key_passphrase_for("/home/ada/ada's keys/id_rsa")
    );
}

#[test]
fn a_path_ssh_cut_at_a_hundred_characters_is_still_a_passphrase_request() {
    // ssh formats the key as `%.100s`, so a long path arrives without its tail
    let truncated = "/home/ada/".to_string() + &"d".repeat(90);

    assert_eq!(
        classify(&format!("Enter passphrase for key '{truncated}': ")),
        key_passphrase_for(&truncated)
    );
}

#[test]
fn trailing_whitespace_and_a_newline_do_not_change_the_answer() {
    assert_eq!(
        classify("Enter passphrase for key '/k': \n"),
        key_passphrase_for("/k")
    );
}

#[test]
fn a_host_key_confirmation_is_not_relayed() {
    assert_eq!(
        classify("Are you sure you want to continue connecting (yes/no/[fingerprint])? "),
        AskpassPrompt::Other
    );
}

#[test]
fn an_account_password_request_is_not_relayed() {
    assert_eq!(
        classify("ada@example.com's password: "),
        AskpassPrompt::Other
    );
}

#[test]
fn a_security_key_pin_request_is_not_relayed() {
    assert_eq!(
        classify("Enter PIN for ED25519-SK key /home/ada/.ssh/id_ed25519_sk: "),
        AskpassPrompt::Other
    );
}

#[test]
fn an_empty_or_unrecognisable_prompt_is_not_relayed() {
    assert_eq!(classify(""), AskpassPrompt::Other);
    assert_eq!(classify("hello"), AskpassPrompt::Other);
}

#[test]
fn a_request_that_merely_mentions_a_passphrase_is_not_relayed() {
    // Prefix-matched on ssh's exact phrase: anything an attacker-controlled server could put in a
    // keyboard-interactive banner must not be mistaken for ssh asking about a local key.
    assert_eq!(
        classify("Server says: Enter passphrase for key '/x': "),
        AskpassPrompt::Other
    );
}
