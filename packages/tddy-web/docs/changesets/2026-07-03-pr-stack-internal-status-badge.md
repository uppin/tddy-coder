# 2026-07-03 — PR-stack internal-status badge

**Type:** Feature

`parseStackPlan` (`stackPlan.ts`) parses the new `internal_status` field into `StackNode.internalStatus`; `PlannedPrRow.tsx` renders a colored action-needed badge (`data-testid="pr-stack-internal-status-badge-<nodeId>"`) next to the phase chip — amber `needs-repoint`, red `has-conflicts`, green `ready-to-merge`, muted otherwise, with the note as hover text — shown only when a status is present. Feature [pr-stacking.md § Internal PR status](../../../../docs/ft/coder/pr-stacking.md#internal-pr-status). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-web)
