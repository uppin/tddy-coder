# Tonic gRPC Method Routing

How tonic routes an HTTP/2 request to a user-implemented service method. Based on analysis of [hyperium/tonic](https://github.com/hyperium/tonic).

## Request Flow

```
TCP connection
  └─ hyper (HTTP/2 frame parsing → http::Request<Incoming>)
       └─ MakeSvc (per-connection service factory, tower middleware chain)
            └─ Routes (axum::Router)
                 ├─ "/{pkg.SvcA}/{*rest}" → SvcAServer<T>::call()
                 │     └─ match req.uri().path()
                 │          ├─ "/pkg.SvcA/Foo" → FooSvc<T> → grpc.unary()       → T::foo()
                 │          ├─ "/pkg.SvcA/Bar" → BarSvc<T> → grpc.streaming()   → T::bar()
                 │          └─ _ → Unimplemented
                 ├─ "/{pkg.SvcB}/{*rest}" → SvcBServer<T>::call() ...
                 └─ fallback → Unimplemented
```

## Two-Level Routing

### Level 1 — Service selection (axum prefix match)

`Routes::add_service` registers each service into an `axum::Router` using a wildcard prefix route:

```rust
// tonic/src/service/router.rs
self.router = self.router.route_service(
    &format!("/{}/{{*rest}}", S::NAME),  // e.g. "/helloworld.Greeter/{*rest}"
    svc.map_request(|req: Request<axum::body::Body>| req.map(Body::new)),
);
```

`S::NAME` is a `&'static str` from the `NamedService` trait, set by codegen to the fully-qualified protobuf service name (e.g. `"helloworld.Greeter"`). Axum's trie-based router efficiently matches the service prefix and forwards the full `http::Request` to the generated server struct.

### Level 2 — Method selection (generated `match` on URI path)

The generated `XxxServer<T>` implements `tower::Service<http::Request<B>>`. Its `call()` does a compile-time literal string match on the full gRPC path:

```rust
// generated code (tonic-build/src/server.rs template)
fn call(&mut self, req: http::Request<B>) -> Self::Future {
    match req.uri().path() {
        "/helloworld.Greeter/SayHello" => {
            // ... handle unary ...
        }
        "/helloworld.Greeter/StreamGreetings" => {
            // ... handle server streaming ...
        }
        _ => {
            // return gRPC Unimplemented status
        }
    }
}
```

The path format follows the [gRPC HTTP/2 spec](https://github.com/grpc/grpc/blob/master/doc/PROTOCOL-HTTP2.md): `/{package.Service}/{Method}`.

Constructed at codegen time by:
```rust
// tonic-build/src/lib.rs
fn format_method_path(service, method, emit_package) -> String {
    format!("/{}/{}", format_service_name(service, emit_package), method.identifier())
}
```

## Per-Method Handler Pattern

Inside each match arm, codegen produces a per-method inner struct that bridges the protocol handler with the user trait:

```rust
// generated for each method
struct SayHelloSvc<T: Greeter>(pub Arc<T>);

impl<T: Greeter> tonic::server::UnaryService<HelloRequest> for SayHelloSvc<T> {
    type Response = HelloReply;
    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

    fn call(&mut self, request: tonic::Request<HelloRequest>) -> Self::Future {
        let inner = Arc::clone(&self.0);
        Box::pin(async move {
            <T as Greeter>::say_hello(&inner, request).await
        })
    }
}
```

Then this service is passed to `tonic::server::Grpc`:

```rust
let method = SayHelloSvc(inner);
let codec = ProstCodec::default();
let mut grpc = tonic::server::Grpc::new(codec)
    .apply_compression_config(/* ... */)
    .apply_max_message_size_config(/* ... */);
let res = grpc.unary(method, req).await;
```

Four method types map to four service traits and four `Grpc` handler methods:

| Streaming type | Generated trait impl | `Grpc` method |
|---|---|---|
| Unary | `UnaryService<Req>` | `grpc.unary()` |
| Server streaming | `ServerStreamingService<Req>` | `grpc.server_streaming()` |
| Client streaming | `ClientStreamingService<Req>` | `grpc.client_streaming()` |
| Bidi streaming | `StreamingService<Req>` | `grpc.streaming()` |

## Protocol Handling (`Grpc`)

`tonic::server::Grpc<T: Codec>` handles gRPC-over-HTTP/2 framing:

1. **Decode**: Creates a `Streaming` decoder from the HTTP body (handles gRPC length-prefixed framing, decompression)
2. **Dispatch**: Calls `service.call(request)` — which invokes the user's trait method
3. **Encode**: Wraps the response in `EncodeBody` (handles length-prefix framing, compression) and returns an `http::Response`

For unary: decodes a single message, calls the service, encodes a single response.
For streaming: wraps the body as a `Streaming<T>` (async stream of decoded messages) and/or wraps the response as an encoded stream.

## Transport Layer

`tonic::transport::Server` manages the TCP/TLS listener and creates per-connection service stacks:

```
Server::serve(addr, routes)
  └─ TcpIncoming (accept connections)
       └─ MakeSvc::call(&io) per connection
            └─ ServiceBuilder chain:
                 RecoverErrorLayer → LoadShedLayer → ConcurrencyLimitLayer
                   → GrpcTimeout → ConnectInfoLayer → Svc(inner)
                        └─ hyper_util::TowerToHyperService
                             └─ hyper serves HTTP/2 on the connection
```

## Key Design Properties

- **String-based, zero-cost routing**: Method dispatch uses compile-time literal `match` arms — no runtime reflection or hash maps.
- **Per-method typed wrappers**: Each method gets its own struct implementing the appropriate service trait, giving full type safety through the codec.
- **Pluggable codecs**: The codec is not hardwired to protobuf. Codegen uses `method.codec_path()` to determine which codec to instantiate (prost, protobuf, or custom).
- **Separation of concerns**: axum handles service-level prefix routing (trie-based, efficient), generated code handles method-level routing (simple match). The transport layer knows nothing about individual methods.
- **`NamedService` as the seam**: The only thing the router needs from a service is its `NAME` constant — the rest is opaque `tower::Service` dispatch.

## Source References

| File | Role |
|------|------|
| `tonic-build/src/server.rs` | Codegen: generates `XxxServer<T>`, `Service` impl with `match`, per-method svc structs |
| `tonic-build/src/lib.rs` | `Service`/`Method` traits, `format_method_path()`, `NamedService` codegen |
| `tonic/src/service/router.rs` | `Routes` — wraps `axum::Router`, `add_service()` with prefix route |
| `tonic/src/server/grpc.rs` | `Grpc<T: Codec>` — gRPC protocol handler (decode, dispatch, encode) |
| `tonic/src/server/service.rs` | `UnaryService`, `ServerStreamingService`, `ClientStreamingService`, `StreamingService` traits |
| `tonic/src/server/mod.rs` | `NamedService` trait definition |
| `tonic/src/transport/server/mod.rs` | `Server`, `Router`, `MakeSvc` — TCP/TLS listener, per-connection middleware |
