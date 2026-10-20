# 2026-10-21 — `SessionReach::open_now`, `OpenedRoom`, and the ratchet closes

`#satellite` 13/13 ([#635](https://github.com/uppin/tddy-coder/pull/635)); cross-package entry:
[2026-10-21-satellite-13-lifecycle-launch.md](../../../../docs/dev/changesets/2026-10-21-satellite-13-lifecycle-launch.md).

`SessionReach` gains `open_now(session, host) -> Result<Option<OpenedRoom>, Status>` (`NoSessionReach`:
`Ok(None)`). `OpenedRoom` replaces `session_rooms::OpenedSessionRoom`. The ratchet's
`KNOWN_OFFENDERS` and `every_known_offender_still_names_a_livekit_crate` are deleted;
`only_plugins_and_wiring_name_a_livekit_crate` is the closing assertion. See
[extension ports](../extension-ports.md#wiring-the-session-host).
