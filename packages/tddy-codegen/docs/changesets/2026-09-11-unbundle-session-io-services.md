# 2026-09-11 — `generate_tonic_adapter` emits a real trait impl

The generator emitted an adapter struct and a `new()`. It now emits a complete implementation of
tonic-build's server trait, one method per rpc, in all four shapes: unary, server-streaming (with the
associated `…Stream` type the tonic trait declares beside it), client-streaming, and
**bidirectional**. Bodies delegate to the wrapped `tddy-rpc` implementation and convert refusals
with `tddy_service::to_tonic_status` — not configurable, because the hand-written adapters in
`tddy-daemon` use the same pair and an adapter with a conversion of its own is how one refusal
reaches two transports as two different gRPC codes.

`TddyServiceGenerator` gained `tonic_trait_path`. It names the `<service>_server` module
tonic-build itself emitted, and it is required rather than derived because both passes emit a trait
called `<Service>Service`: they collide in one namespace, so the tonic one lives in its own module
and cannot be imported unqualified. `None` keeps the struct-and-`new()` shape — which is what
`echo_service.proto` and `token.proto` need, since both set `generate_tonic_adapter: true` and
publicly re-export their `*TonicAdapter` type while having no tonic pass at all.

A server-streaming method's impl carries `T::<Method>Stream: 'static` in its `where` clause: the
`tddy-rpc` trait bounds its stream associated types `Send + Unpin` but not `'static`, which boxing
them into the tonic trait's `Pin<Box<dyn Stream + Send>>` requires.

The five tests for this were `cfg`'d out of every run. The gate is gone, so `cargo test -p
tddy-codegen` now runs them.

A declarative macro cannot substitute: `#[tonic::async_trait]` rewrites the signatures of the trait
it is applied to, and a macro cannot see through that rewrite to generate the bodies.

Docs: [tonic-adapter.md](../tonic-adapter.md).
Full record: [../../../../docs/dev/changesets/2026-09-11-unbundle-session-io-services.md](../../../../docs/dev/changesets/2026-09-11-unbundle-session-io-services.md).
