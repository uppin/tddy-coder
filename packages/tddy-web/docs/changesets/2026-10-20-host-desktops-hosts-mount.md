# 2026-10-20 — The Hosts screen mounts the tooling section

**Type:** Feature

`HostRowToolingCell` and `useHostTooling` mount `HostRowTooling` on every online row of `HostsScreen`, fed by one `GetHostTooling` per row per visit through `useHostClient(HostService, online ? instanceId : null)`; an offline row resolves no client and asks nothing. Loading, failed ("Tooling could not be read") and answered states. A new Tooling column. Closes the backlog entry *No code path issues `GetHostTooling` yet* (2026-09-06): the assembled path row → RPC → probe → cells now runs under `HostsScreenToolingMountAcceptance.cy.tsx`. See [Hosts screen](../hosts-screen.md).
