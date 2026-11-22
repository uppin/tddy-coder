# 2026-11-21 — Tab hygiene

**Type:** Bug Fix · **Stack:** `#tab-hardening` 2/4 (PR #990)

`tmux_agent_end::end_tmux_run_agent` (delete ends a tmux-run session's agent: start-checked TERM → grace → KILL), the agent's start recorded on the tmux main's roster node (`record_tmux_main(…, start)`, `recorded_tmux_main_start`), `OwnedChild` signalling through the spawn's `ToolSignal`, the one tmux kill helper, and the `ToolHost` fixtures from `tddy-test-hosts`.

Full record: [`docs/dev/changesets/2026-11-21-tab-hygiene.md`](../../../../docs/dev/changesets/2026-11-21-tab-hygiene.md); feature: [`session-liveness.md`](../../../../docs/ft/daemon/session-liveness.md#deleting-a-tmux-run-session).
