# 2026-11-07 — `tddy-chat-web`: agent chat in its own package (#web-split 10/17, PR #847)

**Type:** Architecture change

Adds `packages/tddy-chat-web` (workspace entry, `bun.lock`) with agent chat, the ACP session, replay
and tool-call hooks, the agent activity registry and panes, and the subagent conversation and
permission UI — 13 Cypress specs and 7 `bun test` suites with it. Moving the activity registry with
chat removes the chat → sessions import cycle. The package joins `.github/workflows/web.yml` by
workspace discovery (no caller workflow) and typechecks clean.
Also here: `tddy-client-web` gets `@types/debug` (developer consent, 2026-11-07), which takes
`tddy-rpc-playground-web` back into `Web lint` and narrows the terminal and create-session exclusions.
Deviations from the plan: 13 specs moved, not ~21 — chat-named specs that mount session components
stay in `tddy-web` for `feature/web-split/session`; `useAnswerAgentPermission`, `pendingPermission`,
`sessionRosterHalf` and `scrollFollow` moved too (only chat uses them, or chat cannot import them from
sessions).
Closed: `oversized-file-use-acp-session` (536 → 449 lines after the frame builders moved to
`acpSessionMessages.ts`).
Package detail: [tddy-chat-web](../../packages/tddy-chat-web/docs/changesets/2026-11-07-chat-package.md),
[tddy-web](../../packages/tddy-web/docs/changesets/2026-11-07-chat-package.md),
[tddy-client-web](../../packages/tddy-client-web/docs/changesets/2026-11-07-chat-package.md).
Product description: [chat-package.md](../ft/web/chat-package.md).
