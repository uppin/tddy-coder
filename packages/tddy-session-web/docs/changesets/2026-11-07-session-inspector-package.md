# 2026-11-07 — The inspector, agent layers, code pane and SessionMainPane move here (#web-split 17/17, PR #861)

60 modules from `tddy-web` (`components/sessions/*` flat into `src/`, `components/session/*` into
`src/code/`, plus `useKeyedActionFailure` and `codeLanguage`) with their unit tests, and the
`SessionMainPane` code-issue record. 24 specs run in the package on `cy.mountWithClients`
(`SessionAgentRosterSplitSession` through per-host clients instead of a per-daemon LiveKit mount);
the support they read is copied. New dependencies: `tddy-chat-web`, `tddy-hosts-web`,
`tddy-prstack-web`, `tddy-rpc-web`, `tddy-workspace-web` (workspace) and `react-resizable-panels`,
`react-syntax-highlighter` (+ types), moved from `tddy-web`'s manifest. The code-pane, agent-tree,
agent-conversation and indexing-indicator docs moved here.

At wrap: the package's own typecheck errors are fixed (16 → 0 local; the one left is
`tddy-tauri-web`'s): the specs build RPC responses with `create(method.output, …)`, the conversation-
pane spec passes `callerLogin`, `SessionToolsTab.cy.tsx` drops a `sessionToken` prop the tab never
took. `SessionMainPane.tsx` 504 → 504 lines.
