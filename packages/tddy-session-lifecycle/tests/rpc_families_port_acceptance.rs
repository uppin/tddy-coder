//! The port session code reaches the RPC families through, now that they live above this crate.
//!
//! A session start routes its peer-owned stack-base and named-node questions through the PR-stack
//! handler, and a session room serves the four families' entries. Neither can name
//! `tddy-daemon-rpc`, so both read the port the composition root installs — and a host never given
//! it must say so, not quietly serve a room with four families missing.

use std::sync::Arc;

use tddy_rpc::{Code, ServiceEntry};
use tddy_session_lifecycle::test_util::test_service;
use tddy_session_lifecycle::{DaemonRpcFamilies, PrStackHandler};

/// Families that are only ever handed back, never called: what is under test is which ones the
/// host holds.
struct SomeRpcFamilies;

impl DaemonRpcFamilies for SomeRpcFamilies {
    fn pr_stack_handler(&self) -> Arc<dyn PrStackHandler> {
        unreachable!("the port is only inspected here, never routed through")
    }

    fn service_entries(&self) -> Vec<ServiceEntry> {
        unreachable!("the port is only inspected here, never served")
    }
}

#[test]
fn a_host_built_without_rpc_families_reports_the_missing_wiring() {
    // Given a host nobody installed the families on
    let sessions = tempfile::tempdir().expect("a sessions base");
    let daemon = test_service(sessions.path().to_path_buf());

    // When session code asks for them
    let refusal = daemon
        .connection()
        .rpc_families()
        .err()
        .expect("a host with no families installed handed some back");

    // Then it is refused as a wiring fault, naming what is missing
    assert_eq!(refusal.code(), Code::FailedPrecondition);
    assert!(
        refusal.message().contains("DaemonRpcFamilies"),
        "the refusal does not name the missing port: {:?}",
        refusal.message()
    );
}

#[test]
fn a_host_given_rpc_families_hands_back_the_ones_it_was_given() {
    // Given a host with families installed
    let sessions = tempfile::tempdir().expect("a sessions base");
    let families: Arc<dyn DaemonRpcFamilies> = Arc::new(SomeRpcFamilies);
    let host = test_service(sessions.path().to_path_buf())
        .connection()
        .clone()
        .with_rpc_families(Arc::clone(&families));

    // When session code asks for them
    let installed = host
        .rpc_families()
        .expect("a host with families installed refused them");

    // Then they are the very ones installed, not a copy or a default
    assert!(Arc::ptr_eq(installed, &families));
}
