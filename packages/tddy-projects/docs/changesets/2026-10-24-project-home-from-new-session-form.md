# 2026-10-24 — ProjectData.home_session_id replaces home_agent
**Type:** Fix

`ProjectData.home_session_id` (serde default, skipped when none) records the project's main;
`project_storage::set_project_home_session(dir, id, Option<&str>)` records or clears it. `home_agent`,
`set_project_home_agent` and the `SetProjectHomeAgent` handler / service / re-export are removed; an
older row's `home_agent` key is ignored on read and not written back. Tests:
`tests/project_home_session_storage.rs` (replaces `project_home_agent_storage.rs`). See
[project service § Home session](../project-service.md#home-session).

Code issues, re-measured with `/analyze-code-issues` at `4348f160a` against `c1df8e142`: no finding
is attributable to this change. Two first detections, both pre-existing and never entered by this
crate's tests, touched here only by the field rename:
`crap-project-provision-resolve-remote-git-repo-path` (CRAP 182, complexity 13) and
`crap-project-provision-ensure-project-available-from-facilitator` (CRAP 90, complexity 9). No record
closed; none deleted.

The branch-wide re-measure (`/analyze-code-issues` on tddy-agent-launch, tddy-daemon-rpc and tddy-projects at `4348f160a` against `c1df8e142`, commit `f3f1c93ac`) attributes no finding to this change; `start_session_on_request` in tddy-agent-launch improved from 393 to 384 lines.
