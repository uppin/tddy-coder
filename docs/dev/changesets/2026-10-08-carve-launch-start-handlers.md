# 2026-10-08 — The session start, resume and coordinate handlers run over `LaunchSessions`

**Type:** Architecture

`#carve` 20/21 ([#535](https://github.com/uppin/tddy-coder/pull/535), plan label 16e). The last host-bound topic code of
`tddy-session-lifecycle` (session start and resume, project provisioning, the session coordinate handlers) is
`impl LaunchSessions`, in place. That completes the in-place conversion: no topic module names `DaemonSessionHost`, and
the three callback traits are each defined and implemented once. No behaviour change, no crate move, no consumer edit
(`tddy-daemon-rpc`, `tddy-daemon`, `tddy-telegram-control`, `tddy-desktop`), no new crate edge. Follows
[`2026-10-07-carve-launch-sessions-handle`](./2026-10-07-carve-launch-sessions-handle.md).

Durable description: [`module-layout.md`](../../packages/tddy-session-lifecycle/docs/module-layout.md#launch-sessions).
State B, decisions, numbers and acceptance:
[`2026-10-08-carve-launch-start-handlers`](../../packages/tddy-session-lifecycle/docs/changesets/2026-10-08-carve-launch-start-handlers.md).

- Baseline held: 575 passed, 22 failed, 1 ignored, the same 22 by name; `tddy-session-agents` 75 passed.
- One engine refusal (`retarget_impl` S6) closed by a consented one-line import respelling; the defect stays in the
  backlog as `2026-10-07-restructure-retarget-impl-refuses-a-relative-import-of-its-target-type`.
- Backlog entry resolved and deleted: `2026-09-24-lifecycle-session-entry-from-listing-not-started`.
- Next: the move node (`#carve` 21/21, #536) moves the converted topics into their receivers with the engine.
