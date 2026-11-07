# 2026-11-07 — The `tddy-desktop` web check does not follow the tddy-web bundle it ships

**Category:** Missing test trigger (CI gate)
**Source:** `#web-split` 1/16 (PR #838), `/pr-wrap`

`tddy-desktop`'s front end is tddy-web's built bundle (`frontendDist` in
`packages/tddy-desktop/src-tauri/tauri.conf.json`), but its `package.json` declares no
`workspace:*` dependency on `tddy-web`, so `scripts/web-affected.ts` does not count a tddy-web change
as affecting `tddy-desktop`.

**Why deferred:** harmless today — the desktop check runs no tests (no `test:ci`, no `tauri build` in
CI), so triggering it would only install the workspace.

**What closing it takes:** when the desktop check gains a real test or build, either declare
`"tddy-web": "workspace:*"` in `packages/tddy-desktop/package.json` (the detector then follows it with
no code change) or add an explicit extra-dependency map to `scripts/web-affected.ts` with a test.
