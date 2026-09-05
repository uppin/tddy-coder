# 2026-04-11 — Bundled tddy-daemon (macOS)

- **tddy-desktop**: **`embedded-daemon`** resolves **`TDDY_DAEMON_CONFIG`** (or **`dev.desktop.yaml`** in dev), loads repo **`.env`**, spawns **`tddy-daemon`** from **`TDDY_DAEMON_BINARY`** / **`resources/bin/`** / **`target/{release,debug}`**; **`prebuild`** builds and copies release binary; **`electrobun.config.ts`** **`build.copy`** includes the binary; teardown on app exit. Feature **[tddy-desktop-electrobun.md](../tddy-desktop-electrobun.md)** (bundled daemon section). **Cross-package**: [docs/dev/changesets/](../../../dev/changesets/).
