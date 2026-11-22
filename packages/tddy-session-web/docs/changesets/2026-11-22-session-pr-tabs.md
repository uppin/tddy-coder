# 2026-11-22 — The inspector's PR tab and its default tab

**Type:** Feature · **Stack:** `#workqueue` 4/14 (PR #1005)

`InspectorTabs` gains **PR** and `SessionInspectorDrawer` renders `SessionPullRequestTab` for it.
`defaultInspectorTab(session)` picks PR when `prNumber > 0`, otherwise Worktree. The drawer uses it
when no `?inspector=` tab is named, and so does every opening of the inspector (toggle, or an
attachment error). An explicit tab wins. Specs that act through Details controls open Details first.
See [docs/ft/web/session-pr-inspector.md § Default tab](../../../../docs/ft/web/session-pr-inspector.md#default-tab).
Full record: [`docs/dev/changesets/2026-11-22-session-pr-tabs.md`](../../../../docs/dev/changesets/2026-11-22-session-pr-tabs.md).
