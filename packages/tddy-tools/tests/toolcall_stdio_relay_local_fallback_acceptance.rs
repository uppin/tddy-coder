//! Acceptance: local (non-relay) behavior stays unchanged by the toolcall stdio-RPC migration —
//! see `toolcall_stdio_relay_submit_acceptance.rs` for the migration context.

/// **without_tddy_socket_set_the_toolcall_client_falls_back_to_local_behavior_unchanged**: when
/// no session-owning process is relaying (no `TDDY_SOCKET`), `tddy-tools`' CLI commands must keep
/// running their existing local (non-relay) code paths unchanged — the stdio-RPC migration only
/// replaces the *relay* transport, never the local-mode fallback.
#[test]
fn without_tddy_socket_set_the_toolcall_client_falls_back_to_local_behavior_unchanged() {
    // Given no TDDY_SOCKET in the environment, and an empty repo to list build targets in
    let repo = tempfile::tempdir().unwrap();

    // When running `tddy-tools build-list` without a relay socket configured
    let cmd = std::process::Command::new(env!("CARGO_BIN_EXE_tddy-tools"))
        .env_remove("TDDY_SOCKET")
        .args(["build-list", "--repo-dir", &repo.path().to_string_lossy()])
        .output()
        .expect("run tddy-tools build-list locally");

    // Then it runs the local (non-relay) code path directly, exactly as today
    assert!(
        cmd.status.success(),
        "local build-list must succeed without TDDY_SOCKET; stderr={}",
        String::from_utf8_lossy(&cmd.stderr)
    );
    let stdout = String::from_utf8_lossy(&cmd.stdout);
    assert!(
        stdout.contains("\"targets\":[]"),
        "expected an empty targets array in local mode; got: {stdout}"
    );
}
