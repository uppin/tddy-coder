# 2026-09-24 — Every request carries the transport its host stamped

**Type:** Feature

`#keyring` 2/9, PR [#509](https://github.com/uppin/tddy-coder/pull/509). Cross-package entry: [`docs/dev/changesets/2026-09-24-keyring-desktop-login.md`](../../../../docs/dev/changesets/2026-09-24-keyring-desktop-login.md)

`RequestTransport { InProcess, LiveKit, UnixSocket, Pipe, Http, Grpc, Direct }`. `RequestMetadata` has no `Default` and a private `transport`; it is built only through `RequestMetadata::over(transport)` and read through `transport()`. `Request::new` is gone: a request is stamped by its host (`from_rpc_message`, `with_metadata`) or says `Request::direct`. `ServerEngine::new(service, transport)` stamps every dispatched message in `metadata_of` — unary, client-streaming fragments, bidi open and continuations, and the bidi session — and never reads the envelope for it. `RpcService::start_bidi_stream` takes the session's `metadata`. `From<tonic::Request>` stamps `Grpc`. Pinned by `tests/server_engine_stamps_transport.rs`. Detail: [request-transport.md](../request-transport.md).
