# 2026-04-11 — Connection service: project entries with owning daemon and peer row hook

- **`connection.proto`**: **`ProjectEntry.daemon_instance_id`** identifies the registry row’s owning daemon.
- **`tddy-daemon`**: **`list_projects`** merges local disk projects with **`EligibleDaemonSource::peer_project_entries(session_token)`**; the default **`EligibleDaemonSource`** supplies an empty peer list. Integration test **`list_projects_multi_daemon_aggregation`** exercises merge and per-row **`daemon_instance_id`**. Cross-package note: **[docs/dev/changesets/](../../../dev/changesets/)**; web feature doc: **[web-terminal.md](../../web/web-terminal.md)** (eligible daemons / **`ListProjects`**).
