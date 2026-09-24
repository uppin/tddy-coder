//! A vault passphrase never prints: the two `auth` requests that carry one format it redacted.
//!
//! prost derives `Debug` for every message, and a derived `Debug` prints every field — so a
//! `{:?}` of an `UnlockVaultRequest` anywhere (a log line, a panic, a test failure) would print
//! the passphrase. These requests are generated without that derive, and format it redacted.

use tddy_service::proto::auth::{ResetVaultRequest, UnlockVaultRequest};

const THE_PASSPHRASE: &str = "correct horse battery staple";

#[test]
fn an_unlock_request_prints_without_its_passphrase() {
    // Given
    let request = UnlockVaultRequest {
        session_token: "v2.the-session".to_string(),
        passphrase: THE_PASSPHRASE.to_string(),
        create: true,
    };

    // When
    let printed = format!("{request:?}");

    // Then
    assert!(
        !printed.contains(THE_PASSPHRASE) && printed.contains("create: true"),
        "expected the passphrase redacted and the other fields shown, got: {printed}"
    );
}

#[test]
fn a_reset_request_prints_without_its_new_passphrase() {
    // Given
    let request = ResetVaultRequest {
        session_token: "v2.the-session".to_string(),
        new_passphrase: THE_PASSPHRASE.to_string(),
    };

    // When
    let printed = format!("{request:?}");

    // Then
    assert!(
        !printed.contains(THE_PASSPHRASE),
        "expected the new passphrase redacted, got: {printed}"
    );
}
