# 2026-11-21 — A sandboxed session's agent tab runs in a jail of its own

**Type:** Feature · **Stack:** `#session-liveness` 7/8 (PR #966)

`GhCliStub::build` writes the fake `gh` through `write_executable`, which warms it up, so its first exec cannot eat a probe deadline.

Full record: [`docs/dev/changesets/2026-11-21-session-liveness-sandbox-agent-tab.md`](../../../../docs/dev/changesets/2026-11-21-session-liveness-sandbox-agent-tab.md); feature: [`session-liveness.md`](../../../../docs/ft/daemon/session-liveness.md#a-sandboxed-sessions-tabs).
