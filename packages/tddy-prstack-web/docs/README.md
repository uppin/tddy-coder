# tddy-prstack-web documentation

The PR-stack orchestrator's screens:

- **`PrStackScreen`** — the planned-PR list and panel (`PlannedPrList`, `PlannedPrPanel`, the row,
  badges and details, `AddPlannedPrForm`, `DirtyWorktreeDialog`), reorder and repoint, and the
  Start-session flow, which opens `tddy-session-create-web`'s dialog pre-filled from the node.
  `StartPassphraseProvider` is the host application's to mount (as for `CreateSessionPane`).
- **Live status** — `useQueryBranch`, `branchQueries`, `baseSyncStatus`, `orderStackNodes`,
  `parentTitles`, `boundChildSession`, `nodeChildSession`, `isNodeOrphaned`, `resolveNodeSession`,
  `stackChildSessions` (joins children through `StackChildMetadata`, the three fields it reads),
  `startBlockers`, `deriveStackBaseBranch`, `baseBranchChoice`, `prioritiseBaseBranchOptions`,
  `hydrateStackNodes`, `stackPlan`, `stackDocAttachments`.
- **Chat** — `PrStackChat` (the orchestrator's `AgentChat` from `tddy-chat-web`) and
  `WorkflowChatScreen`, which opens its own presenter room through `MediaPort.usePresenterRoom`.

The package imports nothing from `tddy-web` and nothing from LiveKit (`src/packageBoundary.test.ts`).

Product behaviour: [docs/ft/coder/pr-stacking.md](../../../docs/ft/coder/pr-stacking.md),
[docs/ft/coder/pr-stack-live-status.md](../../../docs/ft/coder/pr-stack-live-status.md),
[docs/ft/web/session-drawer.md § PR-Stack Chat Screen](../../../docs/ft/web/session-drawer.md).

- **Changeset history**: [changesets/](changesets/)
- **Open code issues**: [code-issues/](code-issues/) (`PrStackScreen.tsx` is over the 500-line budget)

## Tests

`bun test src` (the live-status helpers, stack plan and children, the boundary rule; builders in
`test-utils/builders.ts`) and `bun run cypress:component` — 6 specs: the connecting overlay, workflow
chat resume and presenter room, and three that mount `PrStackScreen` / `PrStackChat` / the Start-session
flow directly. The drawer-driven PR-stack specs stay in `tddy-web` until the session package takes the
drawer. CI: `.github/workflows/web.yml` (`Web lint` typechecks it; the `Web tests` shards run its suites).
