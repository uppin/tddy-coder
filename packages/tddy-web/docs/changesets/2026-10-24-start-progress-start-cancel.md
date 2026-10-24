# 2026-10-24 — Cancel during a session start

**Type:** Feature

`#start-progress` 8/8 (#741). `useCreateSessionSubmit` keeps the task the stream names first and returns `startCancel`; `CreateSessionActions` turns Cancel into **Cancelling…** and shows **Start cancelled**. `taskClient` reaches the drawer's create pane and the PR-Stack Start-session dialog. See [session-start-and-indexing-progress § Cancelling a start](../session-start-and-indexing-progress.md#cancelling-a-start). Oversized-file records: `CreateSessionPane.tsx` 502 → 506, `SessionsDrawerScreen.tsx` 497 → 504 (new), `PrStackScreen.tsx` 698 → 704 (new) — deferred with the developer's consent. Cross-package entry: [changeset](../../../../docs/dev/changesets/2026-10-24-start-progress-start-cancel.md).
