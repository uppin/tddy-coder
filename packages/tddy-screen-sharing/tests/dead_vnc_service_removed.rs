//! `#unbundle` node 2 deletes the VNC service, which was never reachable.
//!
//! `runtime.rs` registers `screen_sharing.ScreenSharingService` but **never** `vnc.VncService`, and
//! the only references anywhere under `packages/` were the daemon's own `lib.rs` declarations, the
//! two source files, and their two acceptance suites — 1,008 lines in total.
//!
//! The assertion is made against the daemon's source rather than its running service registry
//! because that is what the deletion actually is: a module that is declared, compiled, tested, and
//! reachable by nothing. A registry test would pass today, before the deletion, since the service
//! is already absent from the registry — which is precisely the problem.

use std::path::{Path, PathBuf};

fn daemon_src(file: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tddy-daemon/src")
        .join(file)
}

#[test]
fn the_unreachable_vnc_service_source_is_gone() {
    for file in ["vnc_service.rs", "vnc_vault.rs"] {
        assert!(
            !daemon_src(file).exists(),
            "packages/tddy-daemon/src/{file} is registered nowhere and must be deleted, not moved"
        );
    }
}

#[test]
fn the_daemon_no_longer_declares_the_vnc_modules() {
    // Given
    let lib =
        std::fs::read_to_string(daemon_src("lib.rs")).expect("the daemon's lib.rs is readable");

    // Then
    for decl in ["pub mod vnc_service;", "pub mod vnc_vault;"] {
        assert!(
            !lib.contains(decl),
            "packages/tddy-daemon/src/lib.rs still declares `{decl}`"
        );
    }
}

/// The suites go with the code. Leaving them would leave tests for a service that does not exist,
/// which fails to compile — so this is really a reminder that the deletion is four files, not two.
#[test]
fn the_vnc_acceptance_suites_are_gone() {
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tddy-daemon/tests");
    for file in ["vnc_service_acceptance.rs", "vnc_vault_acceptance.rs"] {
        assert!(
            !tests.join(file).exists(),
            "packages/tddy-daemon/tests/{file} tests a service that node 2 deletes"
        );
    }
}
