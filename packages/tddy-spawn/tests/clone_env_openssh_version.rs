//! Whether the host's OpenSSH can be told to ask for a passphrase with no terminal.
//!
//! `SSH_ASKPASS_REQUIRE=force` arrived in OpenSSH 8.4. Before it, ssh calls its askpass helper only
//! when `DISPLAY` is set, and a daemon has none — so on an older host the clone would hang on a
//! prompt nobody can see, which is exactly the failure this feature exists to end. Better to say so
//! up front than to find out by waiting.
//!
//! Feature: docs/ft/daemon/clone-ssh-passphrase.md

use tddy_spawn::clone_env::OpenSshVersion;

#[test]
fn openssh_8_3_cannot_be_told_to_ask_without_a_display() {
    let version =
        OpenSshVersion::parse("OpenSSH_8.3p1 Ubuntu-1ubuntu0.2, OpenSSL 1.1.1f  31 Mar 2020")
            .expect("a real banner parses");

    assert!(!version.supports_forced_askpass());
}

#[test]
fn openssh_8_4_is_the_first_that_can() {
    let version = OpenSshVersion::parse("OpenSSH_8.4p1 Debian-5, OpenSSL 1.1.1k  25 Mar 2021")
        .expect("a real banner parses");

    assert!(version.supports_forced_askpass());
}

#[test]
fn a_two_digit_major_version_is_newer_not_smaller() {
    // "10.3" compared as text sorts before "8.4"; compared as numbers it is the newer release
    let version =
        OpenSshVersion::parse("OpenSSH_10.3p1, LibreSSL 3.3.6").expect("a real banner parses");

    assert!(version.supports_forced_askpass());
}

#[test]
fn a_banner_that_is_not_openssh_is_not_guessed_at() {
    assert_eq!(OpenSshVersion::parse("dropbear_2022.83"), None);
}
