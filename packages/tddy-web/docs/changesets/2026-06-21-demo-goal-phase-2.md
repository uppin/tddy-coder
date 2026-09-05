# 2026-06-21 — **Demo goal Phase 2

**Type:** Feature

DemoVmControls UI** — `DemoVmControls.tsx`: polls `GetDemoVmStatus` every 3 s; shows "Launch Demo VM" (stopped/unknown), "VM booting…" badge, "VM running" + "Open demo" share-URL link + "Stop VM" (running), "VM error" + "Retry" (error); wired into `ConnectionScreen.tsx` for sessions with `workflowGoal === "demo"` alongside the session token guard. Feature [coder/demo-goal.md](../../../../../docs/ft/coder/demo-goal.md). (tddy-web)
