//! Shared test setup. Ensures TDDY_SESSIONS_DIR is set so tests never write to ~/.tddy.
//! Include via `mod common;` in each integration test file.

use ctor::ctor;

#[ctor]
fn set_tddy_sessions_dir_for_tests() {
    if std::env::var(tddy_core::output::TDDY_SESSIONS_DIR_ENV).is_err() {
        let dir = std::env::temp_dir().join("tddy-test-sessions");
        std::env::set_var(
            tddy_core::output::TDDY_SESSIONS_DIR_ENV,
            dir.to_str().unwrap_or("/tmp/tddy-test-sessions"),
        );
    }
}
