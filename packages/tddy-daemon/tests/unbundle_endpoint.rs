//! The endpoint of the whole `#unbundle` effort, expressed as tests rather than claimed in a
//! changeset.
//!
//! The brief was *"move most of the code from tddy-daemon and tddy-tools and leave them only for
//! high-level wiring"*. Nodes 1–8 moved 73 of 90 RPC methods and left ≈21,500 lines, of which 3,699
//! was wiring — and left `DaemonSessionHost` intact, because family C is precisely what needed
//! its 60 fields and its `self_arc` handle. Node 9 is what makes the brief true, and these are the
//! assertions that stop it being true only in prose.

use std::path::{Path, PathBuf};

fn daemon_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rs_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn non_blank_lines(path: &Path) -> usize {
    std::fs::read_to_string(path)
        .map(|text| text.lines().filter(|l| !l.trim().is_empty()).count())
        .unwrap_or(0)
}

/// The brief, as a number.
///
/// ≈21,500 after nodes 1–8; ≈4,900 after node 9. 6,000 is the ceiling with room for the wiring to
/// grow a little without the test becoming a tripwire on every edit.
#[test]
fn the_daemon_is_wiring_and_nothing_more() {
    // Given
    let mut files = Vec::new();
    rs_files(&daemon_src(), &mut files);

    // When
    let total: usize = files.iter().map(|f| non_blank_lines(f)).sum();

    // Then
    assert!(
        total < 6_000,
        "tddy-daemon is {total} non-blank source lines across {} files; the endpoint is wiring, \
         config and its own settings service",
        files.len()
    );
}

/// `connection_service.rs` and its 59-file directory are deleted, not thinned.
#[test]
fn the_connection_service_module_is_gone() {
    assert!(
        !daemon_src().join("connection_service.rs").exists(),
        "connection_service.rs still exists"
    );
    assert!(
        !daemon_src().join("connection_service").exists(),
        "the connection_service/ directory still exists"
    );
}

/// `self_arc` exists only because `DaemonSessionHost` exists: a `&self` handler had to produce an
/// `Arc<Self>` for `tddy_sandbox_runner::HostRpcHandler`. With that bridge in `tddy-daemon-sandbox`,
/// nothing needs it.
///
/// This is also the end of the pre-existing failure every changeset in this stack forwarded —
/// `self_arc called before set_self_handle`. It is not fixed; its subject is deleted.
#[test]
fn the_self_handle_that_only_the_god_object_needed_is_gone() {
    let mut files = Vec::new();
    rs_files(&daemon_src(), &mut files);

    let mut hits = Vec::new();
    for f in &files {
        if let Ok(text) = std::fs::read_to_string(f) {
            if text.contains("self_arc") || text.contains("set_self_handle") {
                hits.push(f.display().to_string());
            }
        }
    }

    assert!(
        hits.is_empty(),
        "self_arc / set_self_handle survive in {hits:?}; the pre-existing \
         'self_arc called before set_self_handle' failure ends by deleting its subject, not by \
         fixing the test"
    );
}

/// Every module left in the daemon is wiring, configuration, or its own settings service.
///
/// A whitelist rather than a line count, because "under 6,000 lines" would still pass if a session
/// module stayed and something else left.
///
/// Paths relative to `src/`, not bare file names. A bare-name whitelist that had to admit
/// `index_daemon/registry.rs` would have admitted *any* `registry.rs` anywhere under `src/` —
/// including a session module reintroduced under that name, which is the one thing this test
/// exists to catch. Qualifying the three `index_daemon/` submodules by their directory keeps the
/// set exact.
#[test]
fn every_module_left_in_the_daemon_is_one_of_the_endpoint_set() {
    const ENDPOINT: [&str; 18] = [
        "main.rs",
        "lib.rs",
        "server.rs",
        "startup.rs",
        "runtime.rs",
        "config.rs",
        "daemon_settings.rs",
        "daemon_config_service.rs",
        "local_socket_server.rs",
        "user_sessions_path.rs",
        "tddy_user_config.rs",
        "relay_idle.rs",
        // Lifecycle of the `tddy-index-daemon` child this endpoint spawns: lazy get-or-spawn,
        // readiness, restart on death, idle stop, cancellation on shutdown. Wiring by this test's
        // criterion — it implements no RPC method, holds no session state, touches no
        // `SessionHost`, and its only caller is `runtime.rs`, which builds it from the
        // `index_daemon:` config section, ticks its idle reaper and shuts it down. It is the
        // startup/shutdown code `runtime.rs` would otherwise carry inline, for one child process.
        "index_daemon.rs",
        "index_daemon_body.rs",
        "index_daemon/error.rs",
        "index_daemon/registry.rs",
        "index_daemon/spawn.rs",
        // The unix socket an embedded daemon serves to co-located agents — the only channel a
        // process spawned beside a jailed checkout has to this daemon, since `RuntimeHost::Embedded`
        // runs no HTTP listener. Wiring by this test's criterion: it implements no RPC method (it
        // serves the roster `runtime.rs` hands it), holds no session state, touches no
        // `SessionHost`, and its only caller is `runtime.rs`, which binds it at startup and
        // removes the socket on the way out.
        "agent_tool_socket.rs",
    ];

    let mut files = Vec::new();
    rs_files(&daemon_src(), &mut files);

    let mut unexpected: Vec<String> = files
        .iter()
        .map(|f| {
            f.strip_prefix(daemon_src())
                .unwrap_or(f)
                .display()
                .to_string()
        })
        .filter(|p| !ENDPOINT.contains(&p.as_str()))
        .collect();
    unexpected.sort();

    assert!(
        unexpected.is_empty(),
        "these are still in tddy-daemon but are not wiring: {unexpected:?}"
    );
}
