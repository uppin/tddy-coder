# Projects screen & multi-host projects

## Purpose

Give **tddy-web** a first-class, dedicated **Projects** screen (route **`/projects`**)
instead of the projects table embedded in the old sessions view
(**`ConnectionScreen`**). From this screen an operator can create a project and —
new capability — **add an existing project to a different host**, reusing the same
logical **`project_id`** so it is recognizably the same project across hosts.

A "host" is a **daemon instance** (`tddy-daemon`). The set of hosts an operator can
target is exactly the set of **connected `tddy-daemon` LiveKit participants at that
moment** — the same source that already powers **`ListEligibleDaemons`** (LiveKit
common-room participant discovery).

## Screen

**`/projects`** renders via **`ProjectsAppPage`** (data container) + **`ProjectsScreen`**
(presentational), following the existing screen shell pattern
(`VmsAppPage`/`WorktreesAppPage`) with the shared **`DaemonNavMenu`**. A
**Projects** entry is added to the nav menu.

The screen:

- Lists projects **grouped by logical `project_id`**. Because a project may now live
  on multiple hosts, each project shows one **host row** per hosting daemon
  (`daemonInstanceId`), with that host's checkout path (`mainRepoPath`).
- Provides the **create-project** form relocated from `ConnectionScreen`
  (name, git URL, optional user-relative path) → **`CreateProject`**.
- Provides a per-project **"Add to host"** control: a host selector populated from
  **`ListEligibleDaemons`**, **excluding hosts that already host that project**, plus
  a submit that calls **`AddProjectToHost`** and then refreshes the list.

The projects table and create-project form are **removed** from `ConnectionScreen`;
the session-to-project/host association helpers (`sessionProjectTable.ts`) remain,
as the sessions views still use them.

## Adding a project to a host

**`AddProjectToHost`** makes an existing project available on a target host while
**reusing its `project_id`**:

- Request carries `session_token`, the existing `project_id`, `name`, `git_url`,
  optional `main_branch_ref`, the target `daemon_instance_id`, and an optional
  `user_relative_path` for the per-host checkout location.
- The daemon receiving the RPC routes by target host, reusing the same
  routing/forwarding path as **`StartSession`**: if the target is a **peer**, the RPC
  is forwarded over the LiveKit common room to that daemon; if the target is
  **local**, it is handled locally.
- The handling daemon **clones the repo** to the destination and writes a
  `projects.yaml` row with the **given `project_id`** (not a freshly minted UUID),
  tagging the returned `ProjectEntry` with that daemon's `daemon_instance_id`.
- The action is **idempotent**: if the target host already has a row for that
  `project_id`, the existing row is returned and no re-clone occurs.
- Only hosts reported by **`ListEligibleDaemons`** are valid targets; an
  unreachable/unknown target is rejected (`failed_precondition`).

## Cross-host visibility

For a project added to a remote host to appear in the list, aggregated
**`ListProjects`** must include rows from peer daemons. The existing merge
(`merge_listed_projects_with_peers`) is completed by implementing the real
LiveKit fan-out for **`EligibleDaemonSource::peer_project_entries`**: it calls each
discovered peer's **`ListProjects`** (with **`local_only = true`** to prevent recursive
fan-out) and tags returned rows with the peer's `daemon_instance_id`. A new
**`local_only`** flag on **`ListProjectsRequest`** returns only the local registry's rows
and skips the merge.

## Trust model

Consistent with existing multi-host behavior: any participant that can join the
LiveKit common room appears as an eligible daemon and can receive forwarded RPCs
(including the caller's `session_token`). The common room is treated as a trusted
peer group, not a cryptographically authenticated one.

## Related documentation

- **[LiveKit common room: owned project count](livekit-participant-owned-projects.md)** — participant discovery + project registry presence
- **[Web terminal / common room](web-terminal.md#shared-livekit-room-livekitcommon_room)** — the shared LiveKit room
- **[Daemon project concept](../daemon/project-concept.md)** — per-user project registry (`projects.yaml`)
