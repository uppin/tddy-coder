# 2026-09-06 — Turn the common room off without losing its configuration

A daemon's LiveKit common room now has an explicit switch, `livekit.enabled`, edited from the
Settings screen as a checkbox above the LiveKit fields. Switching it off preserves the url, key,
secret and room name, so switching back on is one checkbox rather than four credentials retyped —
previously the only way to express "off" was to delete a working block.

**Breaking:** `enabled` defaults to **`false`**. A deployment with a working `livekit:` block stops
joining the common room on upgrade until `enabled: true` is added. This is intended — the common room
is opt-in, and a daemon that has not been told to join does not join — but it changes the behaviour of
every existing configuration.

Switched off, the daemon joins no common room, publishes no advertisement, and mints no token into
it. Its own authentication is unaffected: `livekit.api_secret` still signs session tokens, so a daemon
with the common room off still serves its gated RPCs — including the settings RPC an operator uses to
switch it back on. Per-session rooms, screen sharing and the `#/livekit` rooms panel read the same
LiveKit block for their own purposes and keep working.

Saving the toggle applies live — switching off disconnects a running room, switching on rejoins it,
neither needing a restart. The web app is told the state and joins nothing when it is off, with the
host directory reporting LiveKit as idle rather than as a connection error.

See [daemon-settings.md](../daemon-settings.md) and
[livekit-peer-discovery.md](../livekit-peer-discovery.md).
