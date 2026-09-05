# 2026-09-05 — The daemon as a library, and its own settings

- **tddy-daemon**: the bootstrap moves out of `main()` into **`tddy_daemon::runtime`** (959 → 183 lines); `build` is assembly — it binds no socket, joins no room, spawns no task — so the binary and an embedding process assemble the **same roster**. Feature **[tddy-desktop-tauri.md](../../desktop/tddy-desktop-tauri.md)**.
- **tddy-daemon**: **`daemon_config.DaemonConfigService`** reads and writes the daemon's YAML — secrets redacted, validation before any write, atomic rename, `restart_required` for what cannot apply live — with a supervisor that genuinely reconnects the LiveKit common room. Feature **[daemon-settings.md](../daemon-settings.md)**.
- **tddy-core**: `LogConfig` and its nested types gained `Serialize` so `DaemonConfig` can be written back.
