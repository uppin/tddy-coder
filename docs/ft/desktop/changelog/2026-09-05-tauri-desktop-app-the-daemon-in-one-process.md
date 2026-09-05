# 2026-09-05 — Tauri desktop app: the daemon in one process

- **tddy-desktop**: Electrobun replaced by a Tauri application (`src-tauri`) that hosts `tddy-daemon` on its own runtime — no child process, no binary resolution, no port wait, and **no listening socket**. Two IPC commands (`tddy_rpc_connect`, `tddy_rpc_send`) carry `rpc_envelope` frames as raw bytes. Feature **[tddy-desktop-tauri.md](../tddy-desktop-tauri.md)** (supersedes **[tddy-desktop-electrobun.md](../tddy-desktop-electrobun.md)**). **Cross-package**: [docs/dev/changesets/](../../../dev/changesets/).
