# 2026-11-07 — `tddy-ui-web` UI kit package (#web-split 3/16, PR #840)

**Type:** Architecture Change

Adds `packages/tddy-ui-web` (root `workspaces`, `bun.lock`, `web-tddy-ui-web.yml`) and the shared
package-boundary checker `scripts/web-package-boundary.ts`. `tddy-web` imports its primitives, `cn`,
theme and the two viewport hooks from `tddy-ui-web` (~90 files; `components.json` aliases point at
the package). Package detail:
[packages/tddy-ui-web/docs/changesets](../../packages/tddy-ui-web/docs/changesets/2026-11-07-ui-kit-package.md).
