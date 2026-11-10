# 2026-11-07 — The browser terminal is its own package (#web-split 8/17, PR #845)

- New package: `GhosttyTerminal`, `GhosttyTerminalSession`, the terminal chrome (`src/connection/`),
  the terminal feed and input delivery (`src/rpc/{terminal,terminalFeed,inputDelivery}`),
  `useTerminalControl`, `useEnqueuedInput`, upload progress and the `lib/` terminal helpers — all
  `git mv`'d from `tddy-web`; `ghostty-web` is this package's dependency.
- The two upward reaches were cut: `ByteDelta` (the `onBytes` event) is defined here
  (`lib/byteDelta.ts`) instead of in `tddy-web`'s session runtime registry, and the LiveKit status
  label is computed by the caller and passed in.
- 21 Cypress specs and their drivers run from the package's config; nine terminal-named specs that
  mount session, chat or host components stay in `tddy-web`. `src/packageBoundary.test.ts` pins the
  import rule; `.github/workflows/web-tddy-terminal-web.yml`.
- Docs `terminal-{session,connection-chrome,presentation,zoom,osc52-clipboard,file-upload}.md` moved
  here from `tddy-web`. The `GhosttyTerminal.tsx` (1491) and `GhosttyTerminalSession.tsx` (1004)
  oversized-file records moved with the files, unchanged.
- Deferred: [package typecheck errors](../../../../docs/dev/todo/2026-11-07-terminal-package-typecheck-errors.md).
