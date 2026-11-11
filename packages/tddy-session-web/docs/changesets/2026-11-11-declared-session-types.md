# 2026-11-11 — Session types come from YAML declarations

**Type:** Fix

- The terminal "+" menu and tmux listing, the drawer's mobile shortcuts and the attach rules (`sessionPaneIsWorkflowView`) read the served launch kind (`knownSessionLaunch`); a legacy record's empty session type still reads as a tool session. The drawer registers session-type options from `sessionTypes`.

Cross-package: [declared session types](../../../../docs/dev/changesets/2026-11-11-declared-session-types.md). Product: [Session types are declared](../../../../docs/ft/daemon/session-agent-roster.md#session-types-are-declared).
