//! `Debug` for the `auth` requests that carry a vault passphrase, which `build.rs` generates
//! without prost's derive: a derived `Debug` prints every field, so any `{:?}` of one — a log line,
//! a panic message, a failing assertion — would print the passphrase. The access token beside it
//! is a bearer credential too, and is redacted with it.

use std::fmt;

use crate::proto::auth::{ResetVaultRequest, UnlockVaultRequest};

const REDACTED: &str = "[redacted]";

impl fmt::Debug for UnlockVaultRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnlockVaultRequest")
            .field("session_token", &REDACTED)
            .field("passphrase", &REDACTED)
            .field("create", &self.create)
            .finish()
    }
}

impl fmt::Debug for ResetVaultRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResetVaultRequest")
            .field("session_token", &REDACTED)
            .field("new_passphrase", &REDACTED)
            .finish()
    }
}
