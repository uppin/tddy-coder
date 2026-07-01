//! `RpcClientTransport` is the object-safe client abstraction that lets user code (hand-written
//! or generated) call RPCs without depending on a concrete transport (LiveKit `Room`, stdio
//! pipes, ...) — this is the "same user code over either transport" principle. Concrete
//! transports (implemented in `tddy-livekit`, `tddy-stdio`) are exercised by their own crates;
//! this file only asserts the trait exists and is usable as a trait object. Fails to compile
//! until `tddy_rpc::RpcClientTransport` exists. See `docs/dev/1-WIP/rpc-multi-transport.md`.

use tddy_rpc::RpcClientTransport;

/// Compile-time check: `dyn RpcClientTransport` must be usable as a trait object so a single
/// client handle can be passed around independent of which concrete transport implements it.
fn _accepts_any_transport_as_a_trait_object(_transport: &dyn RpcClientTransport) {}

#[test]
fn rpc_client_transport_is_object_safe() {
    // No runtime assertion needed — `_accepts_any_transport_as_a_trait_object` above only
    // type-checks if `RpcClientTransport` is object-safe, which is the property under test.
}
