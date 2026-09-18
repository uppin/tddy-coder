# 2026-09-18 — `./release --desktop` owns the desktop build order

**Type:** Fix

Building the application is a three-step sequence whose order is not interchangeable — CLI binaries,
`tddy-web` bundle, `tauri build` — because the application embeds the bundle at build time. That
sequence lives in `./release --desktop`, and `./install --desktop --build` delegates to it, so
building without installing produces exactly what the install pre-flights for.

It builds one bundle target per platform — `--bundles app` on macOS, `--no-bundle` on Linux —
because that is what `./install --desktop` consumes. `tauri.conf.json` keeps all four targets for
distribution; a local install needs the `.app` or the bare binary and nothing else, and the Linux
`.desktop` entry and icon come from `src-tauri/icons/` rather than from a packaged bundle. Skipping
`dmg` also removes a failure: `bundle_dmg.sh` drives Finder over AppleScript and needs Automation
permission, and it runs after the `.app` is already valid.

If a build fails after the artifact exists, `./release --desktop` names the artifact and points at
`./install --desktop`. It exits non-zero either way.

See [config-resolution-and-install.md](../config-resolution-and-install.md) and the cross-package
entry [2026-09-18-release-desktop.md](../../../../docs/dev/changesets/2026-09-18-release-desktop.md).
