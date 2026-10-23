# 2026-10-23 — A per-project home agent, stored and set from the Projects screen

**Type:** Feature

`#project-home` 1/7 ([#696](https://github.com/uppin/tddy-coder/pull/696)). A project can name the agent
its project-home session runs (nothing reads it yet; `#project-home` 4/7 does).

- **tddy-service**: `ProjectEntry.home_agent` (field 10), `ProjectService.SetProjectHomeAgent`; joins
  the closed `RESIDUAL_METHODS` list in `tests/unbundle_service_split.rs`.
- **tddy-projects**: `ProjectData.home_agent: Option<String>` (absent key = unset, never written when
  unset), `project_storage::set_project_home_agent`, the `ProjectHandler` / `ProjectServiceImpl` method.
- **tddy-daemon-kernel**: `peer_calls::forward_set_project_home_agent`.
- **tddy-daemon-rpc**: `project/home_agent_handler.rs` routes like the default branch, checks the id
  against `catalog::listed_agents` (the `ListAgents` row assembly, moved unchanged into
  `catalog/agent_rows.rs` so the picker and the check read the same rows) and writes;
  `ProjectRpcHandler` holds the model registry (field budget 6 → 7). The peer-routing block the four
  per-host project RPCs each carried is now one `coordinate_route` (`project/coordinate_route.rs`):
  `coordinate_handlers.rs` 652 → 586 production lines.
- **tddy-web**: `ProjectHomeAgent` picker on each project card, fed by `ListAgents`; a refusal is shown
  on the card. `ProjectsScreen.tsx` now imports `ProjectEntry` from `project_pb`.

Decisions: a stored id the catalog no longer lists is offered as its own *(not listed by this daemon)*
option rather than shown as **Not set** (the PRD named only "Not set" plus listed agents); the write
reaches one host, like the other project setters
([todo](../todo/2026-10-14-project-account-assignment-reaches-one-host.md)).

Code issues: `oversized-file-project-coordinate-handlers` 652 → 586 and
`complexity-…-add-project-to-host-at-project-coordinate` 175 → 153 lines — both better, both still open.
