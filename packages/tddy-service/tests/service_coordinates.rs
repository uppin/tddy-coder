//! The published coordinates are the ones their schema declares.
//!
//! [`tddy_service::SESSION_FILES_SERVICE`] is a routing address two crates consume: one registers a
//! service entry under it, the other addresses a forward at it. Both now read the same constant, so
//! they cannot disagree with *each other* — what is still free to drift is the constant against the
//! `.proto` every other client is generated from. A rename there and not here would leave the
//! workspace addressing a coordinate nothing serves, which fails at runtime as "unknown service"
//! and at no point during compilation.
//!
//! Reading the `.proto` text rather than the generated Rust is deliberate, for the reason
//! `unbundle_service_split.rs` gives: the schema is what a client in any language is generated
//! from.

use std::path::Path;

/// The `package X;` and `service Y {` a `.proto` declares, joined the way a coordinate is spelled.
fn the_coordinate_declared_in(proto_file: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("proto")
        .join(proto_file);
    let proto = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} is readable: {e}", path.display()));

    let package = proto
        .lines()
        .find_map(|line| line.strip_prefix("package ")?.strip_suffix(';'))
        .unwrap_or_else(|| panic!("{proto_file} declares a package"))
        .trim()
        .to_string();
    let service = proto
        .lines()
        .find_map(|line| line.strip_prefix("service ")?.strip_suffix(" {"))
        .unwrap_or_else(|| panic!("{proto_file} declares a service"))
        .trim()
        .to_string();

    format!("{package}.{service}")
}

#[test]
fn the_session_files_coordinate_is_the_one_its_schema_declares() {
    // Given the schema the session-file methods are generated from
    // When reading the coordinate it declares
    let declared = the_coordinate_declared_in("session_files.proto");

    // Then it is the coordinate this crate publishes for both ends of the wire to consume
    assert_eq!(
        declared,
        tddy_service::SESSION_FILES_SERVICE,
        "session_files.proto declares {declared}, which is not the coordinate \
         tddy_service::SESSION_FILES_SERVICE publishes — the hosts serving it and the forwarders \
         addressing it would both be one rename behind the schema"
    );
}
