# 2026-11-07 — `tddy-session-create-web`: the create-session flow in its own package (#web-split 7/17, PR #844)

**Type:** Architecture change

Adds `packages/tddy-session-create-web` (workspace entry, `bun.lock`, `web-tddy-session-create-web.yml`)
with the whole create-session flow — pane, dialog, project-home setup, fields, hooks, request builder,
attachment staging, start progress — and 35 of its Cypress specs. `tddy-client-web` gains the upload,
failure-text, id and file-listing helpers both sides use; `tddy-ui-web` gains `WorktreeFileTree`.
The passphrase prompt a cloning start raises became a slot the host application fills
(`StartPassphraseProvider`; `tddy-web` mounts `HostStartPassphraseScope` at its root), because the
dialog belongs to the Hosts screen.
Deviations from the plan: `baseBranchChoice()` stays with PR-stack (only its types moved); three specs
stay in `tddy-web` (two pin the Hosts dialog, one the drawer) and one was split; the planned
`buildCreateSessionRequest` never existed (`buildStartSessionRequest`).
Package detail: [tddy-session-create-web](../../packages/tddy-session-create-web/docs/changesets/2026-11-07-create-session-package.md),
[tddy-web](../../packages/tddy-web/docs/changesets/2026-11-07-create-session-package.md),
[tddy-client-web](../../packages/tddy-client-web/docs/changesets/2026-11-07-create-session-package.md),
[tddy-ui-web](../../packages/tddy-ui-web/docs/changesets/2026-11-07-create-session-package.md).
Product description: [create-session-package.md](../ft/web/create-session-package.md).
Deferred: [package support copies `tddy-web`'s](../todo/2026-11-07-create-session-cypress-support-duplicates-tddy-web.md),
[package typecheck errors](../todo/2026-11-07-create-session-package-typecheck-errors.md).
