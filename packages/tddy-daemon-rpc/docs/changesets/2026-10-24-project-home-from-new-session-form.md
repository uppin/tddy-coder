# 2026-10-24 — SetProjectHomeAgent removed; ListProjects reports home_session_id
**Type:** Fix

`project/home_agent_handler.rs`, its `ProjectHandler` port and its test double are removed;
`ProjectRpcHandler` loses `model_registry` (field budget 7 → 6), and `catalog/agent_rows.rs`'s
`listed_agents` narrows to `pub(super)`. `project/entries.rs` maps the recorded main to
`ProjectEntry.home_session_id`. Tests: `tests/project_home_session_entry.rs`;
`tests/set_project_home_agent_acceptance.rs` deleted. See [architecture](../architecture.md).

Code issues, re-measured with `/analyze-code-issues` at `4348f160a` against `c1df8e142` (287 tests,
join 66.8%): no finding is attributable to this change. `oversized-file-project-coordinate-handlers`
is touched by two field renames and unchanged at 586 production lines (the ⚠ DURING constraint held:
the file did not grow). `complexity-project-coordinate-handlers-add-project-to-host-at-project-coordinate`
is unchanged at 153 lines (now entered, CRAP 23, owed to an existing suite). First detections, both
pre-existing: `crap-project-coordinate-handlers-create-project-at-project-coordinate` (CRAP 240) and
`crap-project-coordinate-handlers-list-project-branches-at-project-coordinate` (CRAP 110). The
`exec_tool/ports.rs` and `oversized-file-pr-stack-ports` records are not touched. No record closed;
none deleted.

The branch-wide re-measure (`/analyze-code-issues` on tddy-agent-launch, tddy-daemon-rpc and tddy-projects at `4348f160a` against `c1df8e142`, commit `f3f1c93ac`) attributes no finding to this change; `start_session_on_request` in tddy-agent-launch improved from 393 to 384 lines.
