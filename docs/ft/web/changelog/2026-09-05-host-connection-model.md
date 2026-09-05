# 2026-09-05 — Daemon RPC reaches a host over whichever wire can reach it

Daemon-level RPC is addressed to a **host**, not to a LiveKit participant. A call site asks for a
connection to a host id and a registered provider supplies it, so a build that reaches a daemon some
other way contributes that wire without any screen learning which one it got. LiveKit is the provider
today and behaves exactly as before — same transport, same auth gate, same traffic meter.

A build with no provider registered resolves every host to `null` rather than failing, which is the
guard every call site already had; that is what makes the common room optional rather than assumed.

Who the hosts *are* is still read from common-room participants, so this alone does not let a
room-less build reach anything — it removes the reason it could not. Node 1 of the `optional-livekit`
stack; the host directory follows in [#438](https://github.com/uppin/tddy-coder/pull/438), session
connections in [#439](https://github.com/uppin/tddy-coder/pull/439), and capability gating of the
media and presence surfaces in [#440](https://github.com/uppin/tddy-coder/pull/440).

Feature [daemon-selector-livekit-rpc.md](../daemon-selector-livekit-rpc.md), technical
[host-connections.md](../../../../packages/tddy-web/docs/host-connections.md).
