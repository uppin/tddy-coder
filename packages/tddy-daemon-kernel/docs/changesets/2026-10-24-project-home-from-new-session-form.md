# 2026-10-24 — forward_set_project_home_agent removed
**Type:** Fix

`peer_calls::forward_set_project_home_agent` is removed with `SetProjectHomeAgent`. See
[project concept § Project-home session](../../../../docs/ft/daemon/project-concept.md#project-home-session).

Code issues: no record touched, none deleted.

The branch-wide re-measure (`/analyze-code-issues` on tddy-agent-launch, tddy-daemon-rpc and tddy-projects at `4348f160a` against `c1df8e142`, commit `f3f1c93ac`) attributes no finding to this change; `start_session_on_request` in tddy-agent-launch improved from 393 to 384 lines.
