# Request transport — how a request reached its handler

**Modules**: [`src/message.rs`](../src/message.rs) (`RequestTransport`, `RequestMetadata`),
[`src/types.rs`](../src/types.rs) (`Request`), [`src/server_engine.rs`](../src/server_engine.rs)
(`ServerEngine`), [`src/bridge.rs`](../src/bridge.rs) (`RpcService::start_bidi_stream`)

Every `Request<T>` a handler receives carries a `RequestMetadata`, and every `RequestMetadata`
names the **transport** the request arrived on. The transport is stamped by the **host** that
received the request — the engine or server it built for one kind of channel — and is never read
from the request's own bytes. Nothing a caller sends can choose it.

That makes it usable for a decision the caller must not be able to influence. The one such decision
today is first-login enrolment on Tddy Desktop: only a login completed from the desktop's own
window (`InProcess`) may enrol, and a caller in a LiveKit room or on a socket cannot claim to be
that window (`tddy-daemon-auth`'s `FirstLoginEnrolment` —
[auth-service.md](../../tddy-daemon-auth/docs/auth-service.md#first-login-enrolment)).

## `RequestTransport`

```rust
pub enum RequestTransport { InProcess, LiveKit, UnixSocket, Pipe, Http, Grpc, Direct }
```

| Variant | What it is |
|---|---|
| `InProcess` | The host application's own webview over its in-process IPC bridge (Tauri IPC) — the person at the machine running this process |
| `LiveKit` | A LiveKit room's data channel: the common room or a session room. Any participant the room admits can send one, peer daemons forwarding a call included |
| `UnixSocket` | Framed `tddy-rpc` over a Unix-domain socket: the embedded daemon's agent tool socket, a sandbox's tool socket, a session's toolcall listener. A separate, co-located process |
| `Pipe` | Framed `tddy-rpc` over a pipe pair to a parent or child process — its stdin/stdout, or a jail's piped stdio |
| `Http` | Connect-RPC over HTTP (the daemon's `/rpc` route) |
| `Grpc` | gRPC through a tonic server (the daemon's local Unix-domain socket, the index daemon) |
| `Direct` | Received over no transport: a call one component makes on another in code, carrying no transport claim |

**`Direct` is not an unknown transport.** It covers a call built in code (`Request::direct`), a
request the process builds from its own state, and a payload re-dispatched in code after the request
it arrived in was decoded and its metadata is no longer in scope — the sandbox host bridge, which
receives only a method name and raw bytes. A relay that **does** still hold an inbound request's
metadata passes it through with `Request::with_metadata`, so the inner service sees the stamp the
receiving host gave it; `DaemonBspService`'s relays do this. Nothing may treat `Direct` as
`InProcess`.

## No default metadata

`RequestMetadata` has **no `Default`**, and its `transport` field is private. It is built only
through `RequestMetadata::over(transport)`, optionally followed by `with_sender_identity(..)`, and
read through `transport()`. A struct literal cannot produce one, and a new host cannot forget to
stamp a transport and inherit someone else's.

`sender_identity` is the identity the sender's envelope named (e.g. a LiveKit participant
identity). The sender writes it, so it is a routing hint, never an authority.

A `Request<T>` is built one of three ways:

| Constructor | Metadata |
|---|---|
| `Request::from_rpc_message(inner, &message)` | the message's, as its host stamped it |
| `Request::with_metadata(inner, metadata)` | the given metadata — a host's stamp, or a relay passing an inbound request's through |
| `Request::direct(inner)` | `RequestMetadata::over(RequestTransport::Direct)` |

There is no `Request::new`. A handler-level test calls `Request::direct`; a test that is about the
transport stamps the transport it models.

## Hosts stamp, envelopes cannot

`ServerEngine::new(service, transport)` takes the transport **once**, from its host, and stamps
every request it dispatches with it. The single stamping point is `ServerEngine::metadata_of`,
which builds `RequestMetadata::over(self.transport)` plus the envelope's `sender_identity`, and it
covers every message the engine hands the bridge: a unary call, each fragment of a client-streaming
call, a bidi stream's opening message and each continuation, and the bidi session's own metadata.
The envelope's `sender_identity` and `metadata` map — the fields a sender writes — are never read
for the transport, and the envelope proto has no transport field.

A generic endpoint that cannot tell what its byte channel is takes the transport from whoever
opened the channel: `tddy-stdio`'s `StdioEndpoint::from_duplex(reader, writer, service, transport)`
is the case ([tddy-stdio](../../tddy-stdio/docs/stdio-endpoint.md)).

**Forwarding needs no special case.** A daemon forwarding a call to a peer sends only the request
bytes, and the receiving daemon's `LiveKitParticipant` stamps it `LiveKit`. A login forwarded from a
desktop's window is, on the peer, a login from the room.

## Bidi streams carry their session's stamp

```rust
async fn start_bidi_stream(
    &self, service: &str, method: &str,
    metadata: RequestMetadata,
    input_rx: mpsc::Receiver<RpcMessage>,
) -> Result<BidiStreamOutput, Status>;
```

A bidi handler is handed its `Request` before any message has arrived, so its metadata cannot be read
off the first one. `metadata` is the session's, as the host that opened it stamped it, and the
generated handler (`tddy-codegen`) builds the handler's request from it.

## Where each transport is stamped

| Transport | Host | Stamping site |
|---|---|---|
| `InProcess` | Tauri IPC — the desktop's window | `tddy-tauri-rpc`: `WebviewRpcHost` (`src/host.rs`) and `MultiConnectionHost` (`src/multi_host.rs`), each `ServerEngine::new(.., InProcess)`. The only constructions of `InProcess` in production code |
| `LiveKit` | the common room, session rooms, peer forwards | `tddy-livekit`: `LiveKitParticipant::connect` and `::join` (`src/participant.rs`) |
| `UnixSocket` | `StdioEndpoint::from_duplex` over a Unix socket | `tddy-daemon/src/agent_tool_socket.rs`; `tddy-sandbox-runner/src/runner.rs`; `tddy-sandbox-app/src/sandboxed_session.rs`; `tddy-toolcall/src/toolcall/listener.rs`; `tddy-session-lifecycle/src/session_toolcall.rs` and `connection_service/svc_start_claude_cli_session.rs`; `tddy-supervisor/src/server.rs`; `tddy-coder/src/run.rs`. Clients hosting a no-callback service: `tddy-session-tool-client/src/lib.rs`, `tddy-toolcall/src/toolcall/client.rs`, `tddy-supervisor/src/client.rs` |
| `Pipe` | a parent's or child's stdio | `tddy-stdio`: `StdioEndpoint::from_process_stdio`, `from_child_stdio` (`src/endpoint.rs`); a jail's piped stdio in `tddy-daemon-sandbox/src/sandbox_session.rs` |
| `Http` | the Connect-RPC `/rpc` router | `tddy-connectrpc/src/router.rs`, `over_http()` |
| `Grpc` | tonic | the codegen'd `*TonicAdapter` (`tddy-codegen/src/generator.rs`, unary and streaming), the exec-tool supplement (`tddy-service/exec_tool_tonic_adapter_supplement.rs`), and `impl From<tonic::Request<T>> for Request<T>` (`src/types.rs`) |
| `Direct` | none | `Request::direct` — every in-process delegation and handler-level test |

## Tests

| Suite | Covers |
|---|---|
| `tests/server_engine_stamps_transport.rs` | a unary request stamped with its host's transport; an envelope claiming `InProcess` stamped with the host's instead; every fragment of a client-streaming call; a bidi handler handed its session's stamp before any message; a bidi continuation claiming `InProcess` stamped by the host |
| `tddy-stdio/tests/stamps_the_transport_its_opener_names.rs` | `from_duplex` stamping the transport it was opened as, and ignoring a frame's claim |
| `tddy-tauri-rpc/tests/stamps_the_in_process_transport.rs` | both Tauri hosts stamping `InProcess` |
| `tddy-daemon-livekit/tests/forwarded_rpc_is_stamped_by_the_receiver.rs` | a forwarded RPC stamped `LiveKit` by the daemon that received it |

The generated bidi handler's metadata and the `Http` and `Grpc` stamps have no dedicated test; none
of those transports can enrol a login.

## Related

- [`tddy-github` device-flow.md](../../tddy-github/docs/device-flow.md) — `LoginAdmission::admit(github_login, transport)`
- [`tddy-daemon-auth` auth-service.md](../../tddy-daemon-auth/docs/auth-service.md) — `FirstLoginEnrolment`, the one reader of the transport
- [Multi-transport RPC (product)](../../../docs/ft/coder/rpc-multi-transport.md)
- [changesets/](./changesets/)
