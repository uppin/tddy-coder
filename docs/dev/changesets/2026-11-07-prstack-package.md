# 2026-11-07 — `tddy-prstack-web`: the PR-stack screens in their own package (#web-split 14/17, PR #850)

**Type:** Architecture change

Adds `packages/tddy-prstack-web` (workspace entry, `bun.lock`) with `PrStackScreen`, the planned-PR
list and panel, the live-status helpers, `PrStackChat` and `WorkflowChatScreen`, their suites and the
`PrStackScreen` code-issue record. The package joins `.github/workflows/web.yml` by workspace discovery
(caller workflow deleted), pre-bundles through `scripts/vite-prebundle.ts`, and typechecks clean.
Deviations from the plan: 3 specs moved and 3 direct-mount specs were written instead of ~40 moving —
every other PR-stack spec drives the session drawer, which stays in `tddy-web` until the session node;
`PrStackInspector` and a `WorkflowChat` module did not exist; the passphrase scope and the session
metadata type were cut as upward reaches.
Measured: `PrStackScreen.tsx` 702 → 701 lines (record open, moved with the file).
Package detail: [tddy-prstack-web](../../packages/tddy-prstack-web/docs/changesets/2026-11-07-prstack-package.md),
[tddy-web](../../packages/tddy-web/docs/changesets/2026-11-07-prstack-package.md).
Product description: [prstack-package.md](../ft/web/prstack-package.md).
