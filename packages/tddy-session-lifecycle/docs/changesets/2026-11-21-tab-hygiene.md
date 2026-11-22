# 2026-11-21 — Tab hygiene

**Type:** Bug Fix · **Stack:** `#tab-hardening` 2/4 (PR #990)

The recorded-tabs kill goes through `kill_tmux_session_logged`; `test_util` and the suites take their tool hosts from `tddy-test-hosts`; the tmux-agent daemon fixture and `AnAgentRun` move to shared `tests/` support; stub scripts are written warmed (`write_executable`); `delete_session` pins the delete of a tmux-run session.

Full record: [`docs/dev/changesets/2026-11-21-tab-hygiene.md`](../../../../docs/dev/changesets/2026-11-21-tab-hygiene.md); feature: [`session-liveness.md`](../../../../docs/ft/daemon/session-liveness.md#deleting-a-tmux-run-session).
