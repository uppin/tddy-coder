# 2026-07-21 — Host stats footer (disk + per-core CPU; traffic relocated)

- The sessions drawer gains a persistent bottom **Host Stats Footer**. The byte-traffic readout moves out of the top header into this footer; the top header now holds only the daemon selector.
- The footer adds two host-level indicators for the currently selected daemon: **available disk space** on the filesystem holding the daemon's default project directory (refreshed every 60 s), and a row of **per-core CPU** mini bars (refreshed every 5 s). Switching the selected daemon re-fetches both for the new host.
- Backed by two new `ConnectionService` RPCs (`GetHostCpuStats` / `GetHostDiskStats`, sourced from `sysinfo` on the daemon). See [host-stats-footer.md](../host-stats-footer.md).
