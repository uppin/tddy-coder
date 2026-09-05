# 2026-07-01 — Per-workflow session views + PR-Stack Chat Screen

**Type:** Feature

new `resolveWorkflowView` registry (`workflowViews.tsx`) wired into `SessionMainPane` before the terminal branch; `recipe === "pr-stack"` renders `PrStackScreen` (planned-PR list parsed from `stackPlanJson`, "Start session" CTA reusing the existing `StartSession` `recipe`/`stackParent` fields, chat window over `TddyRemote.Stream` via `usePresenterChat`) instead of the terminal, regardless of attachment status. `CreateSessionPane`'s recipe dropdown now offers `pr-stack` in place of the two legacy entries; `stackParents.ts` recognizes it as a valid orchestrator recipe. Feature [session-drawer.md](../../../../docs/ft/web/session-drawer.md#per-workflow-session-views). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-web)
