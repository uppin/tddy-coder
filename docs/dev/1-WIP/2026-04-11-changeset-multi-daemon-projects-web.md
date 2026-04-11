# Changeset: multi-daemon project listing (web + daemon)

## Plan context (summary)

The connection flow treats **`ListProjects`** as a **registry of rows**: each **`ProjectEntry`** includes **`daemon_instance_id`** for the owning daemon. The web connection screen renders **one accordion and session table per row**, uses composite automation keys when **`daemon_instance_id`** is present, and scopes session assignment (scoped and unscoped) to the session’s host. The daemon builds the project list from the local registry and concatenates optional peer-supplied rows via **`EligibleDaemonSource::peer_project_entries`**; the default source returns no peer rows.

## Product documentation

- **[docs/ft/web/web-terminal.md](../../ft/web/web-terminal.md)** — projects, bulk selection test ids, eligible daemons / **`ListProjects`**.
- **[docs/ft/web/changelog.md](../../ft/web/changelog.md)** — dated release note.
- **[docs/ft/daemon/changelog.md](../../ft/daemon/changelog.md)** — daemon / proto surface.

## Cross-package index

- **[docs/dev/changesets.md](../changesets.md)** — one-line cross-package bullet.

## Affected packages (implementation)

- `packages/tddy-service` — **`connection.proto`** **`ProjectEntry.daemon_instance_id`**.
- `packages/tddy-daemon` — **`connection_service`**: merge local **`list_projects`** with **`peer_project_entries`**; trait default empty peer list; integration test **`list_projects_multi_daemon_aggregation`**.
- `packages/tddy-web` — **`ConnectionScreen`**, **`sessionProjectTable`**, generated **`connection_pb`**; Cypress **`ConnectionScreen.cy.tsx`**; Bun **`sessionProjectTableMultiHost.test.ts`**.

## Deferred / out of scope (documented in feature doc)

- **`WorktreesAppPage`** alignment with composite project+host identity.
- Live cross-daemon **`StartSession`** routing when the user selects a non-local host (daemon rejection until routing exists).
- Production **`EligibleDaemonSource`** implementations that supply non-empty **`peer_project_entries`** and their partial-failure behavior.

## Validation (representative)

- `cargo test -p tddy-daemon list_projects_multi_daemon_aggregation` (or full package test as run in CI).
- `bun test` / Cypress component tests for **`tddy-web`** as configured in the workspace.
