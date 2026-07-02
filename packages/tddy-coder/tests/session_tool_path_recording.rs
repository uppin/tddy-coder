//! Regression: every place `tddy-coder` writes its own initial session metadata must record the
//! *actual* path it was invoked with — never a hardcoded literal.
//!
//! `ResumeSession` (tddy-daemon) needs a real, resolvable path to respawn a session; it cannot
//! reconstruct one from a bare, hardcoded name like `"tddy-coder"` (that only resolves if the
//! binary happens to be on `PATH`, which it generally isn't in this project's dev toolchain
//! layout, e.g. `target/debug/tddy-coder`). `std::env::args().next()` is exactly the absolute
//! path the daemon's `spawn_as_user` invoked this process with (see
//! `tddy-daemon/src/spawner.rs::resolve_tool_path`, which always resolves to an absolute path
//! before spawning) — recording it verbatim means resume never needs to guess or fall back to a
//! config-driven default; it can just reuse the exact path that worked the first time.
//!
//! This mirrors the existing `run_daemon_presenter_loop_polls_tool_calls` pattern
//! (daemon_toolcall_poll_regression.rs) of asserting properties of `run.rs`'s source directly —
//! the alternative (spawning a real `--daemon` process end-to-end and waiting for it to write
//! its own `.session.yaml`) is exactly the kind of slow, flaky, real-process test this project
//! avoids for a property that's fully determined by which literal appears in the source.

fn run_rs_source() -> &'static str {
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/run.rs"))
}

#[test]
fn every_initial_tool_session_metadata_call_records_the_actual_invoked_path() {
    // Given
    let src = run_rs_source();

    // When — every call site that writes a session's own initial tool metadata
    let call_sites = src.matches("write_initial_tool_session_metadata(").count();

    // Then — each one records std::env::args().next(), the exact absolute path this process was
    // launched with, not a guess made after the fact
    let recorded_actual_path = src.matches("tool: std::env::args().next()").count();
    assert!(
        call_sites > 0,
        "expected to find at least one write_initial_tool_session_metadata call site to check"
    );
    assert_eq!(
        recorded_actual_path, call_sites,
        "every write_initial_tool_session_metadata call must set `tool: std::env::args().next()` \
         — found {} call site(s) but only {} recording the actual invoked path",
        call_sites, recorded_actual_path
    );
}
