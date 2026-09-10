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

## Re-read 2026-09-10 — the daemon's calls are now all in one crate

`#unbundle` node 4 ([#473](https://github.com/uppin/tddy-coder/pull/473)) moved `session_room`,
`livekit_peer_discovery`, `common_room_supervisor` and `livekit_rooms_stream` into
`packages/tddy-daemon-livekit`. **Every daemon-side LiveKit call is now behind one crate boundary**,
so a single deadline policy is finally expressible in one place instead of being scattered across a
23,000-line module's neighbours.

That changes the *cost* of the fix, not its shape. It is still not done here, and node 4
deliberately did not do it: adding a deadline changes live behaviour under load and belongs in its
own PR with its own test, and the move introduced no new un-deadlined call — the existing ones
crossed verbatim.

What the consolidation makes possible that was not before: the crate can carry the policy itself
(a default deadline applied where it constructs its clients, with the streaming calls exempted
explicitly rather than by omission), so the decision is made once and visible in one file. The
client-side half in `tddy_livekit::RpcClient` / `tddy_rpc::ClientEngine` is still the broader
change, and still affects callers outside the daemon.

See also [2026-09-06-the-livekit-room-creation-call-has-no-timeout.md](./2026-09-06-the-livekit-room-creation-call-has-no-timeout.md),
which is the same gap on one specific control-plane call.
