# 2026-09-18 — `./release --desktop` owns the desktop build order

**Type:** Fix

Building the application is a three-step sequence whose order is not interchangeable — CLI binaries,
`tddy-web` bundle, `tauri build` — because the application embeds the bundle at build time. That
sequence lives in `./release --desktop`, and `./install --desktop --build` delegates to it, so
building without installing produces exactly what the install pre-flights for.

`./release --desktop` also separates a failed application build from a failed *later* bundle target:
when the `.app` is present it names it and points at `./install --desktop`, since the macOS `dmg`
target fails without Automation permission while the application itself is valid. It exits non-zero
either way.

See [config-resolution-and-install.md](../config-resolution-and-install.md) and the cross-package
entry [2026-09-18-release-desktop.md](../../../../docs/dev/changesets/2026-09-18-release-desktop.md).
