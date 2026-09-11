# The tonic adapter generator

A `.proto` in this repo can be served over two unrelated server stacks: `tddy-rpc`, which carries
LiveKit data channels and stdio, and tonic, which carries gRPC and Connect-HTTP. `tddy-codegen` emits
one async trait per service for the first, and — when asked — an **adapter** that implements the
tonic server trait by delegating to an implementation of that first trait.

So a service is written once and reached at the same coordinate on every transport.

## Why it has to be codegen

`#[tonic::async_trait]` rewrites the signatures of the trait it is applied to. A declarative macro
cannot see through that rewrite to produce the bodies, so there is no `macro_rules!` that stands in
for this. The alternative is a hand-written `async fn` per method that unwraps a `tonic::Request`,
calls the `tddy-rpc` implementation and maps the result back — which is what
`packages/tddy-daemon/src/host_tonic_adapter.rs` and `worktree_tonic_adapter.rs` still are.

## Turning it on

Two fields of `TddyServiceGenerator`, in a package's `build.rs`:

```rust
tddy_codegen::TddyServiceGenerator {
    generate_rpc_server: true,
    generate_tonic_adapter: true,
    rpc_crate_path: "tddy_rpc".to_string(),
    tonic_trait_path: Some(
        "crate::proto::tonic_terminal_session::terminal_session_service_server".to_string(),
    ),
}
```

`generate_tonic_adapter` alone emits the wrapper struct and its `new()` and nothing else.
`tonic_trait_path` is what makes it a real trait impl, and it names the module **tonic-build itself**
emitted — `<service>_server`, in whichever `OUT_DIR` sub-module the package's tonic pass wrote. The
path is required rather than derived because the two passes each emit a trait called
`<Service>Service`: they would collide in one namespace, so the tonic one lives in its own module and
the trait cannot be imported unqualified.

The split is the reason the field is optional rather than the adapter being unconditional. A `.proto`
with **no** tonic pass has no server trait to implement, and two of them —
`proto/test/echo_service.proto` and `proto/token.proto` — already set `generate_tonic_adapter: true`
and publicly re-export their `*TonicAdapter` type. Those keep the struct-and-`new()` shape, and their
builds and re-exports are unaffected.

`packages/tddy-terminal-rpc/build.rs` is the worked example of both passes over one proto.

## What it emits

A `<Service>TonicAdapter<T>` holding an `Arc<T>`, plus `impl<T> <service>_server::<Service> for
<Service>TonicAdapter<T> where T: <Service>`. `T` is unbounded on the struct so the type can be named
without the service trait in scope; the bound rides on the impl. `inner` is an `Arc` so one
implementation instance can be served over several transports at once rather than moved into the
adapter.

One method per rpc, in all four shapes:

| Shape | Request | Response |
|---|---|---|
| unary | `tonic::Request<In>` | `tonic::Response<Out>` |
| server-streaming | `tonic::Request<In>` | `tonic::Response<Self::…Stream>`, plus the associated `type …Stream` the tonic trait declares beside the method |
| client-streaming | `tonic::Request<tonic::Streaming<In>>`, re-wrapped as the `tddy-rpc` inbound stream | `tonic::Response<Out>` |
| bidirectional | `tonic::Request<tonic::Streaming<In>>` | `tonic::Response<Self::…Stream>` |

Two names come off two different fields, and confusing them silently produces a trait impl that does
not match: the **method** name is prost's `Method::name` verbatim, because tonic-build declares its
trait methods from that; the **associated stream type** is built from the *proto* method name, because
that is what tonic-build builds it from.

A server-streaming method also gets `T::<Method>Stream: 'static` added to the impl's `where` clause —
the `tddy-rpc` trait bounds its stream associated types `Send + Unpin` but not `'static`, which boxing
them into the tonic trait's `Pin<Box<dyn Stream + Send>>` requires.

## Refusals go through the shared conversion

Every generated body converts a refusal with `tddy_service::to_tonic_status` (and, for a
client-streaming inbound half, `to_rpc_status`) rather than constructing its own mapping. That path is
not configurable, deliberately: the hand-written adapters in `tddy-daemon` use the same pair, and a
generated adapter with a conversion of its own is exactly how one refusal reaches two transports as
two different gRPC codes.

`tddy-rpc` carries its own conversion, but it pins **tonic 0.11** against everything else's **0.12**,
which is why the shared pair lives in `tddy-service` instead.

## Tests

`generator.rs`'s `tonic_adapter_tests` module asserts the emitted text per shape: a unary method
generates a delegating `async fn`; a server-streaming one generates its associated `…Stream` type; a
bidirectional one takes a `tonic::Streaming` request and answers with a stream; a client-streaming one
takes a `tonic::Streaming` request and answers with a plain message; and every body calls the shared
status conversion.

The production evidence is `terminal_session.TerminalSessionService`: its generated adapter is what
the daemon mounts on its local UDS socket, and it is what `tddy-sandbox-app` — running inside every
jail — dials for its terminal stream, including the bidirectional method.

## See also

- [server-streaming.md](./server-streaming.md) — the teardown contract every server-streaming rpc
  depends on
- [terminal-session-service.md](../../tddy-terminal-rpc/docs/terminal-session-service.md) — the
  service the generated adapter serves in production
- [`docs/dev/todo/`](../../../docs/dev/todo/) — replacing the two remaining hand-written adapters
