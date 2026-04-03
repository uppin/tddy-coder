//! Test-only stderr markers for Red-phase instrumentation (`cargo test` only).
//! `cfg!(test)` ensures release TUI binaries never emit these lines (NFR1).

pub(crate) fn tddy_marker(marker_id: &'static str, scope: &'static str) {
    if cfg!(test) {
        eprintln!(
            r#"{{"tddy":{{"marker_id":"{}","scope":"{}","data":{{}}}}}}"#,
            marker_id, scope
        );
    }
}
