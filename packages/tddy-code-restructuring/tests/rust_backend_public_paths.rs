//! The Rust backend's public paths resolve where they did before its split (`#reshape` 17/19).
//!
//! Every move of the split keeps these paths: what other crates and the CLI reach through
//! `backends::rust` and the crate root. A move that dropped one fails this file at compile time.

use std::time::Duration;

use serde_json::Value;
use tddy_code_restructuring::backends::rust::{
    client_capabilities, discard, server_settings, ProgressSink, RustBackend, ServerChatter,
    WAIT_HEARTBEAT,
};
use tddy_code_restructuring::backends::RustBackend as RustBackendThroughBackends;

/// The position encoding the handshake asks the server for.
fn the_encoding_asked_for(capabilities: &Value) -> &str {
    capabilities["general"]["positionEncodings"][0]
        .as_str()
        .expect("a position encoding")
}

#[test]
fn the_backends_public_paths_resolve_where_they_did() {
    // Given the handshake reached through the module and through the crate root
    let through_the_module = client_capabilities();
    let through_the_root = tddy_code_restructuring::client_capabilities();

    // Then both ask for byte offsets, and the server settings agree whichever path is taken
    assert_eq!(the_encoding_asked_for(&through_the_module), "utf-8");
    assert_eq!(the_encoding_asked_for(&through_the_root), "utf-8");
    assert_eq!(
        server_settings(),
        tddy_code_restructuring::server_settings()
    );
}

#[test]
fn the_backend_type_and_its_companions_are_reachable_from_outside_the_crate() {
    // Given the progress sink, the heartbeat and the chatter, named through `backends::rust`
    let sink: ProgressSink = discard();
    let chatter = ServerChatter::default();

    // When the sink is written to and the type is named through both of its paths
    sink("a line nobody reads");
    let through_the_module = std::any::type_name::<RustBackend>();
    let through_backends = std::any::type_name::<RustBackendThroughBackends>();

    // Then the heartbeat keeps its period, the chatter has heard no loading, and both paths name one type
    assert_eq!(WAIT_HEARTBEAT, Duration::from_secs(30));
    assert!(!chatter.loading());
    assert_eq!(through_the_module, through_backends);
}
