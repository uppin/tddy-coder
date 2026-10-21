# 2026-10-20 — Desktop starts report sign-in failures and the real size

**Type:** Fix

`#host-desktops` 6/8 ([#658](https://github.com/uppin/tddy-coder/pull/658)). Bridges write one readiness line to stdout
(`tddy-screenshare`), both bridges tag a refused sign-in by type (`tddy-vnc` now answers VNC
Authentication and reads the SecurityResult itself, with the approved `des` dependency; `tddy-rdp`
tags CredSSP failures), the daemon waits for the line, maps it to a status, and stops a failed bridge
(`tddy-screen-sharing`), and the web viewer shows why a start failed (`tddy-web`). The 1920×1080
guess is gone: replies carry the bridge's size. Package entries:
[tddy-screenshare](../../packages/tddy-screenshare/docs/changesets/2026-10-20-bridge-ready.md),
[tddy-screen-sharing](../../packages/tddy-screen-sharing/docs/changesets/2026-10-20-bridge-ready.md),
[tddy-vnc](../../packages/tddy-vnc/docs/changesets/2026-10-20-bridge-ready.md),
[tddy-rdp](../../packages/tddy-rdp/docs/changesets/2026-10-20-bridge-ready.md),
[tddy-web](../../packages/tddy-web/docs/changesets/2026-10-20-bridge-ready.md). Feature doc:
[Screen sharing sessions](../ft/web/screen-sharing-sessions.md#the-bridges-start-report).

Every file stays under 500 production lines; `spawn_bridge` went from 102 to 34 lines. No backlog
entries or code-issue records were resolved. Added to the backlog:
`2026-10-18-screen-share-mid-session-resize`, `2026-10-20-session-start-without-a-bridge-succeeds`,
`2026-10-20-screen-sharing-two-stand-in-bridges`, `2026-10-20-screen-sharing-start-functions-over-length`.
