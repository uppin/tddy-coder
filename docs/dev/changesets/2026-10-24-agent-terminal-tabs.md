# 2026-10-24 — Agent terminal tabs: any CLI harness beside bash, in the session's worktree

**Type:** Feature · PR [#718](https://github.com/uppin/tddy-coder/pull/718) (`#warp-agent` 2/2)

A terminal tab can run any registered CLI harness instead of the login shell.
`StartTerminalSessionRequest.kind = 4` reaches the host's `TerminalRoster` as a `TerminalLaunch`; the
daemon builds the harness's fresh interactive argv from its descriptor and spawns it in the session's
worktree, the coder refuses every agent kind, and the web `+` is a menu of Bash plus every harness,
with tabs labelled by kind. Product docs: [Session terminal tabs](../../ft/web/session-terminal-tabs.md),
[Terminal sessions § Agent terminals](../../ft/daemon/terminal-sessions.md#agent-terminals).

Decisions:

- **Any registered harness, not Warp only**, and a kind is a harness name, never an argv.
- **Every harness is offered before the session's type is known** (developer consent, 2026-10-24;
  "Bash only until known" declined). The host refuses a kind it does not run and the web shows the
  refusal.
- **The `+` sits in its own wrapper outside the tab scroller** (accepted deviation from the plan's
  in-scroller button), so the menu is not clipped.

Deferred: [`2026-10-24-claude-agent-tab-shares-session-hooks`](../todo/2026-10-24-claude-agent-tab-shares-session-hooks.md)
— a `claude-cli` tab in a `claude-cli` session shares the worktree's hooks file.

- `tddy-terminal-rpc` — proto `kind`, `TerminalLaunch` / `TerminalKind`, ports module:
  [entry](../../../packages/tddy-terminal-rpc/docs/changesets/2026-10-24-agent-terminal-tabs.md)
- `tddy-session-lifecycle` — daemon roster dispatches agent kinds:
  [entry](../../../packages/tddy-session-lifecycle/docs/changesets/2026-10-24-agent-terminal-tabs.md)
- `tddy-cli-sessions` — `start_agent_terminal`:
  [entry](../../../packages/tddy-cli-sessions/docs/changesets/2026-10-24-agent-terminal-tabs.md)
- `tddy-coder` — the coder roster refuses agent kinds:
  [entry](../../../packages/tddy-coder/docs/changesets/2026-10-24-agent-terminal-tabs.md)
- `tddy-web` — the `+` menu and kind-aware labels:
  [entry](../../../packages/tddy-web/docs/changesets/2026-10-24-agent-terminal-tabs.md)
