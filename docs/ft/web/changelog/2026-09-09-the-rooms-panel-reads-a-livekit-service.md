# 2026-09-09 — The rooms panel reads a LiveKit service of its own

The LiveKit rooms panel subscribes to `livekit.LiveKitService.StreamLiveKitRooms` instead of
reaching the same method on `connection.ConnectionService`. Nothing the panel shows or does is
different: the same handler answers, over the same connection, with the same snapshot-then-changes
contract and the same 3-second cadence.

The one consequence worth knowing is a compatibility one — a bundle from before this change asks
the old coordinate and gets `unimplemented` for the rooms panel alone, so a bundle and a daemon
have to come from the same side of it.

Where the stream is served from, and why the daemon serves several services rather than one:
[livekit-rooms-panel.md § RPC surface](../livekit-rooms-panel.md#rpc-surface),
[auth-livekit-services.md](../../daemon/auth-livekit-services.md).
