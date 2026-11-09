# 2026-11-07 — `tddy-terminal-web`: the browser terminal in its own package (#web-split 8/17, PR #845)

**Type:** Architecture change

Adds `packages/tddy-terminal-web` (workspace entry, `bun.lock`, `web-tddy-terminal-web.yml`) with the
browser terminal — `GhosttyTerminal`, `GhosttyTerminalSession`, the connection chrome, the
transport-agnostic feed and input delivery, and the terminal helpers — and 21 of its Cypress specs.
The two reaches back into `tddy-web` (session runtime registry, LiveKit status presentation) became a
package-owned `ByteDelta` type and a prop.
Deviations from the plan: 21 specs moved, not ~35 — nine terminal-named specs mount session, chat or
host components and travel with `feature/web-split/session`; the terminal types had already moved to
`tddy-client-web` in #842, so only the implementations moved here.
Package detail: [tddy-terminal-web](../../packages/tddy-terminal-web/docs/changesets/2026-11-07-terminal-package.md),
[tddy-web](../../packages/tddy-web/docs/changesets/2026-11-07-terminal-package.md).
Product description: [terminal-package.md](../ft/web/terminal-package.md).
Deferred: [package typecheck errors](../todo/2026-11-07-terminal-package-typecheck-errors.md).
