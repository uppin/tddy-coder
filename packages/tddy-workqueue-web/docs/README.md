# tddy-workqueue-web documentation

The project stack's screens:

- **`WorkQueueView`** — one project stack, loaded and kept current by `useWorkQueueWatch` over
  `WatchWorkQueue` on `watchClient` (required: `WorkQueueService` on the host that holds the queue, or
  `null` when it is unreachable), keeping the newest copy by revision (`queueRevision`) across snapshots
  and mutation answers, with its load-error and live notices in `WorkQueueLiveStatus`: the
  planned-PR list (`WorkItemList`, the row, badges and details, `AddWorkItemForm`,
  `DirtyWorktreeDialog`), "New stack", reorder and repoint, and the Start-session flow, which opens
  `tddy-session-create-web`'s dialog pre-filled from the node (`startSessionInitialValues`). `StartPassphraseProvider` is the host
  application's to mount (as for `CreateSessionPane`).
- **Live status** — `useQueryBranch`, `branchQueries`, `baseSyncStatus`, `orderWorkItems`,
  `parentTitles`, `boundChildSession`, `nodeChildSession`, `isNodeOrphaned`, `resolveNodeSession`,
  `workItemSessions` (joins children through `StackChildMetadata`, the three fields it reads),
  `startBlockers`, `deriveStackBaseBranch`, `baseBranchChoice`, `prioritiseBaseBranchOptions`,
  `hydrateWorkItems`, `stackPlan`.
- **Workflow chat** — `WorkflowChatScreen`, every tddy-coder workflow session's main pane, which opens
  its own presenter room through `MediaPort.usePresenterRoom` and renders `tddy-chat-web`'s `AgentChat`.

The package imports nothing from `tddy-web` and nothing from LiveKit (`src/packageBoundary.test.ts`).

Product behaviour: [docs/ft/coder/pr-stacking.md](../../../docs/ft/coder/pr-stacking.md),
[docs/ft/coder/pr-stack-live-status.md](../../../docs/ft/coder/pr-stack-live-status.md),
[docs/ft/web/session-drawer.md § Project stack view](../../../docs/ft/web/session-drawer.md#work-queue-view).

- **Changeset history**: [changesets/](changesets/)

## Tests

`bun test src` (the live-status helpers, stack plan and children, the boundary rule; builders in
`test-utils/builders.ts`) and `bun run cypress:component` — 4 specs: the project stack view, the
planned-PR list's Start-session flow, and the workflow chat's resume and presenter room. The
drawer-driven planned-PR row specs live in `tddy-session-web` and open the stack view at its route.
CI: `.github/workflows/web.yml` (`Web lint` typechecks it; the `Web tests` shards run its suites).
