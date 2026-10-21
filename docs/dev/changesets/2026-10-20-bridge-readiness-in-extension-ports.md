# 2026-10-20 — The bridge readiness protocol lives in the extension ports

**Type:** Architecture

`#host-desktops` 6/8 ([#658](https://github.com/uppin/tddy-coder/pull/658)); correction to
[2026-10-20-bridge-ready](2026-10-20-bridge-ready.md). The readiness line's types and parser move
from `tddy-screenshare` to `tddy-daemon-extensions` (`bridge_readiness`), with their tests and
doc, unchanged on the wire. `tddy-screen-sharing` reads it from there and no longer depends on
`tddy-screenshare`, a LiveKit plugin crate, which had failed
`livekit_is_a_satellite::only_plugins_wiring_and_known_offenders_name_a_livekit_crate`.
`tddy-screenshare` re-exports the items, so `tddy-vnc` and `tddy-rdp` are unchanged.
`tddy-daemon-extensions` gains `serde` and `serde_json` (already in the lockfile). Package entries:
[tddy-daemon-extensions](../../packages/tddy-daemon-extensions/docs/changesets/2026-10-20-bridge-readiness-in-extension-ports.md),
[tddy-screenshare](../../packages/tddy-screenshare/docs/changesets/2026-10-20-bridge-readiness-in-extension-ports.md),
[tddy-screen-sharing](../../packages/tddy-screen-sharing/docs/changesets/2026-10-20-bridge-readiness-in-extension-ports.md).
