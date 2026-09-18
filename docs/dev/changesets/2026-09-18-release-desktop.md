# 2026-09-18 — `./release --desktop` builds what `./install --desktop` installs

**Type:** Fix

`./release` accepts `--desktop`, which builds the three artifacts a desktop install ships, in the one
order that works: the CLI binaries the embedded daemon spawns, then the `tddy-web` bundle, then
`tauri build`. The application *embeds* the bundle (`frontendDist` in `tauri.conf.json` points at
`packages/tddy-web/dist` and `tauri build` reads it rather than building it), so building the
application first bakes in a stale dashboard and reports nothing.

`./install --desktop --build` delegates to that script instead of carrying its own copy of the
sequence, and the install's preflight names `./release --desktop` as the command to run. The two
paths therefore cannot drift into producing different artifacts, and
[install_contract.rs](../../../packages/tddy-e2e/src/install_contract.rs) asserts the delegation.

Two smaller corrections in the same script:

- An unrecognised argument is an error with a usage line. `./release` previously ignored every
  argument it was given, so `./release --desktop` would have silently built only the CLI binaries.
- It builds only the bundle target the install consumes: `--bundles app` on macOS, `--no-bundle` on
  Linux. `tauri.conf.json` lists four targets because it also describes distribution, and a local
  install uses one of them per platform — the macOS `.app` copied into `~/Applications`, or the
  Linux binary put on `PATH`. The Linux `.desktop` entry and icon come from
  `packages/tddy-desktop/src-tauri/icons/`, not from a `deb` or an AppImage. This also removes a
  failure rather than tolerating one: the `dmg` target runs `bundle_dmg.sh`, which drives Finder
  over AppleScript and fails without Automation permission
  ([2026-09-05-…-tauri-desktop-single-process-daemon.md](../todo/2026-09-05-from-2026-09-05-tauri-desktop-single-process-daemon.md))
  — after the `.app` is already complete, so the build reported failure while holding a valid
  application.
- A `tauri build` that fails **after** the artifact exists reports which of the two happened, names
  the artifact and points at `./install --desktop`. The script still exits non-zero — it reports the
  distinction rather than swallowing the failure.

The script also resolves its own directory and `cd`s there, so it no longer depends on the caller's
working directory being the repo root.

Follows [2026-09-18-install-desktop.md](./2026-09-18-install-desktop.md).
