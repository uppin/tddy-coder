# 2026-07-25 — pr-stack-live-status: the PR-Stack Chat Screen now shows live per-node status. `resolveNodeSession(node, sessions)` resolves the in-progress child session by matching `node.branch` against `SessionEntry.branch` (in-progress indicator); `usePrStatus` polls `GetPrStatus` every `POLL_INTERVAL_MS` (5s) per rendered branch to render the PR number as a link + its state (open/merged/closed/draft); a Repoint control (shown when a parent PR is merged) calls `RepointPlannedPr` and applies the returned stack. `SessionsDrawerScreen` treats a pr-stack orchestrator as active while a child it owns is live, so it stays reachable in the drawer mid-flight. Feature [pr-stack-live-status.md](../../../../docs/ft/coder/pr-stack-live-status.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-web)

**Type:** Feature


