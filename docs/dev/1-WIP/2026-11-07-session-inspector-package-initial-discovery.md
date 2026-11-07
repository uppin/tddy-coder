# Initial Discovery: Session inspector, agents and code pane in `tddy-session-web`

**Changeset**: [2026-11-07-session-inspector-package.md](./2026-11-07-session-inspector-package.md)
**Date**: 2026-11-07
**Passes**: 1

## Combined Conclusions

Inspector + agents + code pane slice of the #851 split.

## Exploration 1: #851's boundary measurement — 2026-11-07

**Agent**: #851's [G] subagent (report relayed by the orchestrator)
**Scope**: what the single session node would have to move.

- 150 source files / 19,076 lines under `components/sessions` + `components/session`; 156 specs.
- ~40 reaches outside the moved dirs: `routing/useAppLocation`, `routing/appRoutes`, `shell/AppShell`,
  `HostStartPassphrasePrompt`, 14 `lib/`, 7 `utils/`, 8 `rpc/` modules, `hooks/authProvider`,
  `GhosttyTerminalSession`, `EnqueuedInputOverlay`, `test-utils`.
- 10 specs import `tddy-livekit-web` / `tddy-livekit-rpc-web` testkits.
