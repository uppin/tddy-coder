# 2026-09-05 — The host list no longer requires a LiveKit common room

Which daemons the selector offers is now a **host directory** merged from sources, rather than the
roster of a LiveKit common room. The common room is one source. The daemon that served the page is
another, so a build that joins no room still has a host to offer — previously the selector was empty
and disabled, not even naming the daemon it was running against.

An unconfigured common room contributes nothing and reports **idle, not an error**. No room is
constructed and no token is minted. An operator who deliberately did not configure LiveKit is no
longer shown a connection failure for it on every screen, and a room that *is* configured but
unreachable degrades the peers rather than the host in front of them.

Presence is now requested rather than ambient. A screen that wants the participant roster asks for it
by host and can be told it is unavailable, instead of reaching a `Room` off shared context. Nothing
visible changes yet — that refusal is the seam the media and presence surfaces are gated on next.

**Naming a host is not yet reaching it.** This node makes the directory offer the serving daemon, but
no wire that can reach it is registered here, so on a page whose common room is down that daemon is
selected and each screen reports it has no connection — where before the selector said the room was
unreachable. The wire follows later in the stack.

Node 2 of the `optional-livekit` stack, on
[#437](https://github.com/uppin/tddy-coder/pull/437). Session connections follow in
[#439](https://github.com/uppin/tddy-coder/pull/439), capability gating of the media and presence
surfaces in [#440](https://github.com/uppin/tddy-coder/pull/440).

Feature [daemon-selector-livekit-rpc.md](../daemon-selector-livekit-rpc.md), technical
[host-directory.md](../../../../packages/tddy-web/docs/host-directory.md).
