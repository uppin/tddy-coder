# 2026-11-10 — Effort picker

**Type:** Feature

- `CreateSessionEffortField` (`create-session-effort-select`) beside Model, fed by `useCreateSessionEffort` from the served `effortLevels` / `defaultEffort`; hidden for an agent without levels. `CreateSessionFormValues.effort` is sent for CLI agents only.
- `cliAgentFeaturesOf` (`codingAgentMetadata.ts`) replaces the pane's inline feature object.

Cross-package: [effort on session start](../../../../docs/dev/changesets/2026-11-10-start-effort.md). Product: [Session drawer § Fields](../../../../docs/ft/web/session-drawer.md#fields).
