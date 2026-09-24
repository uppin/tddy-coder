# `StdioEndpoint` — `tddy-rpc` over a framed byte channel

**Module**: [`src/endpoint.rs`](../src/endpoint.rs) · re-exported as
`tddy_stdio::{StdioEndpoint, spawn_child_endpoint, ChildEndpoint}`

A `StdioEndpoint<S>` layers `tddy-rpc`'s `ServerEngine` and `ClientEngine` over one duplex byte
channel carrying length-prefixed frames. It hosts `service` for requests from the peer and returns a
`StdioRpcClient` for calling into the peer, over the same channel, so either side can call the
other. One writer task owns the write half, so a request and a response never interleave
mid-frame.

## Constructors, and the transport each stamps

The endpoint's `ServerEngine` stamps every request it hosts with a `RequestTransport`
([`tddy-rpc` request-transport.md](../../tddy-rpc/docs/request-transport.md)). The endpoint knows
what its channel is in two of its three constructors. In the third it does not, so the caller names
it:

| Constructor | Channel | Transport stamped |
|---|---|---|
| `from_process_stdio(service)` | this process's own stdin/stdout, to the peer that spawned it | `Pipe` |
| `spawn_child_endpoint(..)` (through `from_child_stdio`) | a child this crate spawns with `tokio::process::Command` | `Pipe` |
| `from_duplex(reader, writer, service, transport)` | any already-open `AsyncRead` / `AsyncWrite` pair the caller owns | **`transport`, as the caller names it** |

`from_duplex` wraps a channel somebody else opened: a Unix socket an accept loop handed over, or a
jailed child's stdio that platform-specific spawn code (Seatbelt `sandbox-exec`, Linux namespaces)
converted into async handles. The endpoint cannot tell a socket from a pipe, so **whoever opened
the channel names it**, and every request hosted over it carries that transport. A frame's own
bytes cannot change it.

In practice a Unix-socket opener passes `RequestTransport::UnixSocket` — the agent tool socket, the
sandbox tool socket, the toolcall listener and its client, the supervisor socket, the host-session
socket — and a jail's piped stdio passes `RequestTransport::Pipe`. The full list is the stamping
table in [request-transport.md](../../tddy-rpc/docs/request-transport.md#where-each-transport-is-stamped).

## Tests

`tests/stamps_the_transport_its_opener_names.rs`: a request over `from_duplex` is stamped with the
transport the channel was opened as, and a frame claiming the in-process bridge is stamped with the
channel's transport instead.

## Related

- [Multi-transport RPC (product)](../../../docs/ft/coder/rpc-multi-transport.md)
- [`tddy-toolcall` architecture](../../tddy-toolcall/docs/architecture.md) — the toolcall listener and client, both over `from_duplex`
- [changesets/](./changesets/)
