# 2026-08-14 — No LiveKit RPC call has a client-side deadline

**Category:** Future enhancement
**Source:** remote-managed-worktree changeset, 2026-08-14

Neither `tddy_livekit::RpcClient` nor `tddy_rpc`'s `ClientEngine` bounds how long a call may wait for
a response. A request published to a participant that is not listening — a daemon restarted since the
room was joined, say — never completes and never errors. The caller hangs.

This surfaced while caching the LiveKit room in `tddy-tools`: holding one connection moves the 10 s
participant wait from every call to first connect, so a cached client can outlive the peer it
addresses. That case is mitigated by re-checking participant presence per call
(`LiveKitSession::peer_present`), but presence can lapse between the check and the publish, and
nothing bounds a call already in flight.

The same missing deadline is what makes the chunking hazard silent: `packages/tddy-livekit/src/chunking.rs`
documents that reassembly is best-effort and index-keyed, so a lost frame wedges a call permanently —
"deadlines are the only escape", and there are none on the client side. `forward_to_peer` added one
for the *daemon→daemon* hop (`PEER_FORWARD_TIMEOUT`) after exactly this bug; the client side never got
the equivalent.

A deadline on `RpcClient` would cover both. It is a policy decision affecting every LiveKit RPC in the
repo — including long-lived streams, which must not inherit a unary timeout — so it needs its own
change rather than riding along with a feature.
