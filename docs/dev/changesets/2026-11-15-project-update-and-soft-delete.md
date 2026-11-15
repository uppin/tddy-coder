# 2026-11-15 — Rename, soft-delete and restore a project on every host owning it

**Type:** Feature
**Packages:** `tddy-service` (proto), `tddy-projects`, `tddy-daemon-rpc`, `tddy-daemon-kernel`,
`tddy-daemon-livekit`, `tddy-session-split`, `tddy-agent-launch`, `tddy-telegram-control`,
`tddy-session-create-web`, `tddy-session-web`, `tddy-workspace-web`, `tddy-client-web` (generated)

## State A

`project.ProjectService` could create a project, add it to another host and set its default branch,
accounts and orchestrator defaults. A project could not be renamed, and it could not be removed.

The brief assumed that `SetProjectDefaultBranch` and `SetProjectAccounts` fan out to every host that
owns the `project_id`. **They do not.** Each one routes to the single host its `daemon_instance_id`
names, and the web always sends to `group.hosts[0]`. This is the open defect
[2026-10-14-project-account-assignment-reaches-one-host](../todo/2026-10-14-project-account-assignment-reaches-one-host.md).
So there was no logical-project fan-out to reuse.

## State B

Three new RPCs: `UpdateProject`, `DeleteProject` and `RestoreProject`. All three have
**logical-project scope with a daemon fan-out**. A deleted project is a tombstone, and nothing is
removed from disk.

### Decisions (developer, 2026-11-15)

| Question | Decision |
|---|---|
| How a logical-project write reaches every host | **The daemon fans out.** The receiving daemon writes its own row, then forwards the request with `local_only` to every eligible peer. A peer without the row answers `not_found` and is skipped. Default branch and accounts stay single-host; that TODO stays open. |
| A create or add that matches a deleted project | **Restore automatically.** |

### Data model (`projects.yaml`)

`ProjectData.lifecycle: ProjectLifecycle` is flattened onto the row, so its keys sit beside `name`:

| Key | Meaning |
|---|---|
| `deleted_at` | RFC 3339 time of the soft delete. Present means deleted. |
| `deleted_by` | The GitHub user who deleted it. |
| `restored_at` | RFC 3339 time of the last restore. |

All three are optional and skipped when absent. A row written by an older daemon loads as live, and
a live row writes none of these keys.

### Fan-out (`project/fan_out.rs`)

1. The receiving daemon applies the change to its own row, if it has one.
2. It forwards the same request, with `local_only = true`, to every eligible peer concurrently.
   - `not_found` means the peer does not own the project, so it is skipped.
   - Any other failure, or no answer, lists the peer under `unconfirmed_hosts` with a reason.
3. The response carries:
   - `project`: the row from the first host that applied the change (this host's row, when it has one);
   - `applied_hosts`;
   - `unconfirmed_hosts`.
4. If no host applied the change, the call fails with `not_found`.

`DeleteProject` and `RestoreProject` stamp their time once, on the coordinating host. The stamp is
carried on the forwarded call (`deleted_at` / `deleted_by` / `restored_at`), so every host stores
the same tombstone.

### The tombstone wins over a host that missed it

A peer listing (`EligibleDaemonSource::peer_project_entries`, which is LiveKit's
`peer_listing_request`) now asks each peer for `include_deleted`. `ListProjects` then handles the
merged rows as follows:

- It groups them by `project_id` and takes the **newest delete or restore event** across the rows
  (`ProjectLifecycle::newest`).
  - Any event beats a row that never saw one.
  - If a delete and a restore have the same time, the delete wins.
- Every listed row of that project shows the winning lifecycle.
- This host **adopts** the winner into its own row when the winner is newer
  (`adopt_project_lifecycle`). A host that was offline during a delete is healed the next time a
  listing reaches it.
- Deleted projects are filtered out unless `include_deleted` is set.

Name divergence is **not** reconciled. A host that missed a rename keeps its old name until the
rename is repeated (`unconfirmed_hosts` tells the caller which hosts those are). TODO:
[2026-11-15-project-rename-reaches-offline-hosts-only-on-retry](../todo/2026-11-15-project-rename-reaches-offline-hosts-only-on-retry.md).

### Sessions of a project being deleted

- **Refused while running.** `DeleteProject` first sends a `preflight_only` round to this host and
  every peer. A host answers `failed_precondition` when it is running one of the project's sessions,
  except the project-home session. "Running" means `.session.yaml` records a live pid, the same test
  `ListSessions` uses. Orchestrator sessions count as running work. The refusal names each session
  and its host, and no host is tombstoned. Unreachable hosts cannot be checked; their sessions keep
  running until stopped.
- **The project-home session is stopped, not removed.** Each host that applies the tombstone sends
  SIGTERM to every running project-home session of the project, the same path as the project-home
  `TerminateSession` tool. The session directory and `home_session_id` are kept, so a restore brings
  main back with `EnsureProjectHomeSession`. The stopped ids are returned in `stopped_session_ids`.
- **Existing sessions of a deleted project** stay on disk and are hidden by the drawer. They cannot
  be resumed (`failed_precondition`, "… is deleted; restore it …") until the project is restored.

### New work on a deleted project is refused

`project_storage::refuse_if_deleted` returns `failed_precondition` naming the project. It is called
from:

- `tddy_session_split::service_util::find_registered_project`. This covers `EnsureProjectHomeSession`,
  orchestrator start and resume, tool sessions, CLI and workspace worktrees, split and workspace
  starts, and the stack seed.
- `ensure_project_available_locally` / `ensure_project_available_from_facilitator`, for a locally
  registered deleted row. A peer's deleted row is never offered to the peer lookup, so a host that
  never held the project cannot provision it from a tombstone.
- `ResumeSession`, for a session of a deleted project.
- The project-home exec tools, through the caller's project.
- Telegram: the project pickers leave deleted projects out, and a direct spawn is refused.

The worktree service's read paths, such as listing worktrees, are deliberately left open.

### Create and add restore a deleted project

- `CreateProject` matches a deleted project among this host's rows: first by `git_url` (trimmed),
  which needs no clone destination, then by the destination it would clone to. On a match it restores the project with the fan-out and returns the
  restored row. It does not clone again, and the project keeps its old name.
- `AddProjectToHost` restores the project when the target host holds its tombstone.

### Web

- **Project settings pane** (`ProjectSettingsPane`, drawer gear):
  - A name field with **Save** (`UpdateProject`).
  - **Delete project**, behind a confirmation. The confirmation says that the checkout, worktrees,
    sessions and stacks are kept on disk and that the project can be restored from the Projects
    screen.
  - A refusal, such as running sessions, is shown in place.
- **Projects screen:** a **Show deleted projects** checkbox lists deleted projects as deleted cards
  (who, when, "its data is kept") with **Restore** (`RestoreProject`).
- **Drawer:** it lists projects with `includeDeleted`. Deleted projects get no group, and their
  sessions are dropped rather than moved to **Other**. They are also left out of the bulk-delete
  selection, so **Select all** cannot hard-delete a deleted project's kept sessions.
- **Pickers:** every other `ListProjects` caller (create-session, orchestrator composer, worktrees,
  chat workspace) uses the default and so never sees a deleted project.

### Modules

- `tddy-projects`: `project_storage.rs` holds the tombstone type (`ProjectLifecycle`) and every
  lifecycle and rename write.
- `tddy-daemon-rpc/src/project/`:
  - `fan_out.rs`: the peer fan-out.
  - `update.rs`, `soft_delete.rs`: the handlers.
  - `project_sessions.rs`: the running and project-home sessions.
  - `lifecycle_listing.rs`: the `ListProjects` reconciliation.
  - `recreate.rs`: create and add meeting a deleted project.

### Not in scope

- No `tddy-tools` tool to update, delete or restore a project (per the brief).
- No hard delete, anywhere.

## Docs

- [docs/ft/daemon/project-concept.md](../../ft/daemon/project-concept.md):
  - § Data model;
  - § Create project behavior;
  - § Renaming, deleting and restoring a project;
  - § API.
- [docs/ft/web/projects-screen-multi-host.md](../../ft/web/projects-screen-multi-host.md):
  § Deleted projects and § Cross-host visibility.
- [docs/ft/web/session-drawer.md](../../ft/web/session-drawer.md): § Project settings pane.
- [packages/tddy-projects/docs/project-service.md](../../../packages/tddy-projects/docs/project-service.md):
  § The surface and § Soft delete.
- [packages/tddy-daemon-rpc/docs/architecture.md](../../../packages/tddy-daemon-rpc/docs/architecture.md):
  § Module layout.
- Backlog: [2026-11-15-project-rename-reaches-offline-hosts-only-on-retry](../todo/2026-11-15-project-rename-reaches-offline-hosts-only-on-retry.md)
  opened.
- [2026-10-14-project-account-assignment-reaches-one-host](../todo/2026-10-14-project-account-assignment-reaches-one-host.md)
  stays open. Default branch and accounts are still single-host.

## Code issues

`complexity-project-coordinate-handlers-add-project-to-host-at-project-coordinate` went from
120 lines to **126**. The restore branch is a single call into `recreate.rs`, and the issue stays
open.

## Tests (red first)

| Suite | What it pins |
|---|---|
| `tddy-projects/tests/project_soft_delete_storage.rs` | The tombstone round-trip, old rows load, no keys written for live rows, restore, rename, name rules, the `newest` rule, adoption, deleted-project matching, `refuse_if_deleted` |
| `tddy-daemon-rpc/tests/project_update_delete_acceptance.rs` | Two in-process daemons and a silent host on the in-memory peer transport: rename and delete everywhere, data retained, listing filter, refusal naming a remote running session, main stopped, a stale host does not resurrect a delete, a restore beats a stale tombstone, create and add auto-restore, start, main, resume and project-home tools refused, no provisioning from a tombstone |
| `tddy-daemon-livekit` unit test `a_peer_listing_asks_for_the_peers_own_rows_tombstones_included` | The peer listing asks for tombstones |
| `tddy-telegram-control/tests/telegram_deleted_project_picker.rs` | The project picker leaves a deleted project out |
| `tddy-session-web/src/utils/sessionProjectGroups.test.ts` | The drawer split, and a deleted project's sessions dropped |
| `tddy-session-create-web/cypress/component/ProjectSettingsPane.cy.tsx` | Rename, delete confirmation wording, cancel, confirmed delete, refused delete |
| `tddy-workspace-web/cypress/component/ProjectsScreenDeletedProjects.cy.tsx` | Hidden by default, Show deleted, Restore, refused restore |
