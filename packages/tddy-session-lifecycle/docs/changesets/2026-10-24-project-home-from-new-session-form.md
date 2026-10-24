# 2026-10-24 — Project-home acceptance suites set main up from a start spec
**Type:** Fix

`tests/ensure_project_home_session_acceptance.rs` pins the ensure contract with and without a
start spec (refusals by type, placement, SSH, agent clone, split agent and role; replace; stale-id
clearing; concurrent set-ups), and `tests/project_home_idle_stop_acceptance.rs` sets main up with a
claude-cli spec. Their shared daemon, repo and stub-`claude` fixture is `tests/project_home_support/mod.rs`.
Two unit-test `ProjectData` literals renamed. See
[project concept § Project-home session](../../../../docs/ft/daemon/project-concept.md#project-home-session).

Code issues: no production code in this package changed; no record touched, none deleted.

The branch-wide re-measure (`/analyze-code-issues` on tddy-agent-launch, tddy-daemon-rpc and tddy-projects at `4348f160a` against `c1df8e142`, commit `f3f1c93ac`) attributes no finding to this change; `start_session_on_request` in tddy-agent-launch improved from 393 to 384 lines.
