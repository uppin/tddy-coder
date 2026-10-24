# 2026-10-24 — EnsureProjectHomeSession start spec; ProjectEntry.home_session_id
**Type:** Fix

`session.proto`: `EnsureProjectHomeSessionRequest.start = 4` (a `StartSessionRequest`; absent =
reach the recorded main) and the outcome `"replaced"`. `project.proto`:
`ProjectService.SetProjectHomeAgent` and its messages removed, `ProjectEntry` field 10 and the name
`home_agent` reserved, `string home_session_id = 11` (empty = main not set up).
`tests/unbundle_service_split.rs`: `RESIDUAL_METHODS` 20 → 19. See
[project concept § Project-home session](../../../../docs/ft/daemon/project-concept.md#project-home-session).

Code issues: this package has no code in the change beyond the protos; no record touched, none
deleted.

The branch-wide re-measure (`/analyze-code-issues` on tddy-agent-launch, tddy-daemon-rpc and tddy-projects at `4348f160a` against `c1df8e142`, commit `f3f1c93ac`) attributes no finding to this change; `start_session_on_request` in tddy-agent-launch improved from 393 to 384 lines.
