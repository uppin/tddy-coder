# 2026-06-21 — Demo goal Phase 2: DemoVmControls

- `DemoVmControls` component: polls `GetDemoVmStatus` every 3 s; "Launch Demo VM" → `StartDemoVm`, "Stop VM" → `StopDemoVm`, booting badge, running state with "Open demo" share-URL link, error with "Retry"
- Wired into `ConnectionScreen.tsx` for sessions with `workflowGoal === "demo"` alongside the session token guard
- Feature: [coder/demo-goal.md](../../coder/demo-goal.md). Cross-package: [docs/dev/changesets/](../../../dev/changesets/).
